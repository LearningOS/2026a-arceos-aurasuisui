//! An unordered map backed by a flat, open-addressed hash table.
//!
//! `alloc` only ships ordered collections, and ArceOS must not gain new
//! dependencies (the workspace lock file is frozen and CI builds with
//! `--locked`), so this module implements `HashMap` directly on top of
//! `alloc::vec::Vec`.
//!
//! # Representation
//!
//! A map is made of three arrays:
//!
//! * `ctrl` holds one control byte per bucket: `EMPTY`, `DELETED`, or the
//!   top 7 bits of the hash of the key stored in that bucket, tagged with
//!   `0x80`. Probing only reads this array, so a lookup usually touches a
//!   few cache lines instead of chasing keys through the heap.
//! * `slots` holds one entry index per bucket, matching `ctrl` one to one.
//! * `entries` holds the `(key, value)` pairs densely, in insertion order
//!   (removals compact it with `swap_remove`). Iteration and rehashing walk
//!   this array front to back, which keeps the keys (and the heap blocks they
//!   point into) in roughly the order they were allocated.
//!
//! Collisions are resolved by linear probing. Removal writes a tombstone in
//! `ctrl` instead of an empty bucket, so it never breaks the probe sequence
//! of another key, and compacts `entries` with `swap_remove`. Tombstones are
//! discarded whenever the table is rebuilt, which happens as soon as the
//! number of occupied-or-deleted buckets would exceed 7/8 of the capacity.
//!
//! Keys are bounded by `Eq + Hash`, exactly like the standard library's
//! `HashMap`; no `Ord` bound is required. Hashing is done by an internal
//! FNV-1a hasher whose output is run through a 64-bit finalizer, because
//! FNV-1a alone has weak low bits and the bucket index is taken from the low
//! bits of the hash.

use core::borrow::Borrow;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::iter::FusedIterator;

use alloc::vec::Vec;

/// The number of buckets a map starts with. Must be a power of two.
const INITIAL_CAPACITY: usize = 8;

/// The table is rebuilt once `used / buckets` would exceed `7 / 8`.
const LOAD_FACTOR_NUM: usize = 7;
const LOAD_FACTOR_DEN: usize = 8;

/// Control byte of a bucket that has never been used (or was cleared).
const EMPTY: u8 = 0;
/// Control byte of a bucket whose entry was removed.
const DELETED: u8 = 1;
/// Bit set in every control byte that carries a hash tag.
const TAG_BIT: u8 = 0x80;

/// Entry index stored in a bucket that has no live entry.
const NO_ENTRY: u32 = u32::MAX;

/// `FNV-1a` offset basis and prime.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A small FNV-1a hasher followed by a 64-bit avalanche finalizer.
struct FnvHasher {
    state: u64,
}

impl FnvHasher {
    const fn new() -> Self {
        Self {
            state: FNV_OFFSET_BASIS,
        }
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        // splitmix64 finalizer: spread the entropy of the FNV state into the
        // lower bits, which are the ones used for the bucket index.
        let mut x = self.state;
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^= x >> 33;
        x
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut state = self.state;
        for &byte in bytes {
            state ^= u64::from(byte);
            state = state.wrapping_mul(FNV_PRIME);
        }
        self.state = state;
    }
}

/// Hashes one key with the map's internal hasher.
fn hash_key<Q: Hash + ?Sized>(key: &Q) -> u64 {
    let mut hasher = FnvHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Returns the control byte that marks a bucket holding a key with `hash`.
fn tag_of(hash: u64) -> u8 {
    TAG_BIT | (hash >> 57) as u8
}

/// Returns the smallest power-of-two bucket count that holds `capacity`
/// entries at the 7/8 load factor.
fn capacity_to_buckets(capacity: usize) -> usize {
    let mut buckets = INITIAL_CAPACITY;
    while buckets / LOAD_FACTOR_DEN * LOAD_FACTOR_NUM < capacity {
        buckets = buckets.checked_mul(2).expect("capacity overflow");
    }
    buckets
}

/// One live key-value pair of the map.
struct Entry<K, V> {
    key: K,
    value: V,
    /// The bucket that points at this entry.
    slot: u32,
}

/// A hash map implemented with open addressing and linear probing.
///
/// The type parameter `K` is bounded by `Eq + Hash`, just like the standard
/// library's map; no `Ord` bound is required.
pub struct HashMap<K, V> {
    /// One control byte per bucket: `EMPTY`, `DELETED`, or a hash tag.
    ctrl: Vec<u8>,
    /// One entry index per bucket; `NO_ENTRY` for unused buckets.
    slots: Vec<u32>,
    /// The live entries, stored densely.
    entries: Vec<Entry<K, V>>,
    /// Number of occupied or deleted buckets.
    used: usize,
}

impl<K, V> HashMap<K, V> {
    /// Creates an empty map.
    ///
    /// The map does not allocate until the first entry is inserted.
    pub const fn new() -> Self {
        Self {
            ctrl: Vec::new(),
            slots: Vec::new(),
            entries: Vec::new(),
            used: 0,
        }
    }

    /// Creates an empty map with room for at least `capacity` entries.
    pub fn with_capacity(capacity: usize) -> Self {
        let buckets = capacity_to_buckets(capacity);
        Self {
            ctrl: new_ctrl(buckets),
            slots: new_slots(buckets),
            entries: Vec::new(),
            used: 0,
        }
    }

    /// Returns the number of entries in the map.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the map contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Removes all entries from the map, keeping the allocated memory.
    pub fn clear(&mut self) {
        self.ctrl.fill(EMPTY);
        self.slots.fill(NO_ENTRY);
        self.entries.clear();
        self.used = 0;
    }

    /// Returns an iterator over the entries of the map.
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            inner: self.entries.iter(),
        }
    }

    /// Returns an iterator over the entries of the map, with mutable values.
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            inner: self.entries.iter_mut(),
        }
    }

    /// Probes for `key`, returning `(found, insert_at)`.
    ///
    /// `found` is the bucket holding `key`, or `usize::MAX` if it is absent.
    /// `insert_at` is the first tombstone of the probe sequence, or the first
    /// empty bucket if the sequence has no tombstone.
    ///
    /// Only the control bytes are consulted; the keys are compared just when
    /// the 7-bit hash tag matches, which makes the common case a scan over a
    /// small array. `K: Borrow<Q>` lets a `HashMap<String, V>` be queried
    /// with a `&str`, like the standard library's map.
    fn find<Q>(&self, hash: u64, key: &Q) -> (usize, usize)
    where
        K: Borrow<Q>,
        Q: Eq + ?Sized,
    {
        if self.ctrl.is_empty() {
            return (usize::MAX, usize::MAX);
        }
        let mask = self.ctrl.len() - 1;
        let tag = tag_of(hash);
        let mut index = hash as usize & mask;
        let mut first_deleted = usize::MAX;
        loop {
            let ctrl = self.ctrl[index];
            if ctrl == EMPTY {
                let insert_at = if first_deleted == usize::MAX {
                    index
                } else {
                    first_deleted
                };
                return (usize::MAX, insert_at);
            } else if ctrl == DELETED {
                if first_deleted == usize::MAX {
                    first_deleted = index;
                }
            } else if ctrl == tag {
                let entry = &self.entries[self.slots[index] as usize];
                if entry.key.borrow() == key {
                    return (index, usize::MAX);
                }
            }
            index = (index + 1) & mask;
        }
    }

    /// Returns the entry index of a bucket known to hold a live entry.
    fn entry_index(&self, bucket: usize) -> usize {
        self.slots[bucket] as usize
    }
}

impl<K: Eq + Hash, V> HashMap<K, V> {
    /// Inserts a key-value pair into the map.
    ///
    /// If the map already contained this key, the value is updated and the old
    /// value is returned. The existing key is kept.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if self.ctrl.is_empty() {
            self.grow();
        }
        let hash = hash_key(&key);
        let (found, mut insert_at) = self.find(hash, &key);
        if found != usize::MAX {
            let entry = self.entry_index(found);
            return Some(core::mem::replace(&mut self.entries[entry].value, value));
        }
        if (self.used + 1) * LOAD_FACTOR_DEN > self.ctrl.len() * LOAD_FACTOR_NUM {
            self.grow();
            insert_at = self.find(hash, &key).1;
        }
        debug_assert!(insert_at != usize::MAX);
        let entry = u32::try_from(self.entries.len())
            .expect("HashMap cannot hold more than u32::MAX entries");
        let reusing_tombstone = self.ctrl[insert_at] == DELETED;
        self.ctrl[insert_at] = tag_of(hash);
        self.slots[insert_at] = entry;
        self.entries.push(Entry {
            key,
            value,
            slot: insert_at as u32,
        });
        if !reusing_tombstone {
            self.used += 1;
        }
        None
    }

    /// Rebuilds the table, dropping all tombstones and growing it when needed
    /// to respect the load factor.
    ///
    /// The entries are not moved: only the bucket arrays are rebuilt.
    #[cold]
    fn grow(&mut self) {
        let new_capacity = if self.ctrl.is_empty() {
            INITIAL_CAPACITY
        } else {
            // A table whose load is mostly tombstones only needs a rebuild at
            // the same size; otherwise grow to the smallest suitable size.
            core::cmp::max(self.ctrl.len(), capacity_to_buckets(self.entries.len() + 1))
        };
        let mut ctrl = new_ctrl(new_capacity);
        let mut slots = new_slots(new_capacity);
        let mask = new_capacity - 1;
        for (index, entry) in self.entries.iter_mut().enumerate() {
            let hash = hash_key(&entry.key);
            let mut bucket = hash as usize & mask;
            while ctrl[bucket] != EMPTY {
                bucket = (bucket + 1) & mask;
            }
            ctrl[bucket] = tag_of(hash);
            slots[bucket] = index as u32;
            entry.slot = bucket as u32;
        }
        self.ctrl = ctrl;
        self.slots = slots;
        self.used = self.entries.len();
    }

    /// Returns a reference to the value associated with `key`.
    ///
    /// `key` may be any borrowed form of the map's key type, e.g. a `&str`
    /// for a `HashMap<String, V>`.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (found, _) = self.find(hash_key(key), key);
        if found == usize::MAX {
            return None;
        }
        Some(&self.entries[self.entry_index(found)].value)
    }

    /// Returns a mutable reference to the value associated with `key`.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (found, _) = self.find(hash_key(key), key);
        if found == usize::MAX {
            return None;
        }
        let entry = self.entry_index(found);
        Some(&mut self.entries[entry].value)
    }

    /// Removes a key from the map, returning its value if the key was present.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let (found, _) = self.find(hash_key(key), key);
        if found == usize::MAX {
            return None;
        }
        let entry = self.slots[found] as usize;
        self.ctrl[found] = DELETED;
        self.slots[found] = NO_ENTRY;
        let removed = self.entries.swap_remove(entry);
        if entry < self.entries.len() {
            let moved_slot = self.entries[entry].slot as usize;
            self.slots[moved_slot] = entry as u32;
        }
        Some(removed.value)
    }

    /// Returns `true` if the map contains `key`.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.find(hash_key(key), key).0 != usize::MAX
    }
}

impl<K, V> Default for HashMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for HashMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// Returns a freshly allocated control array of `buckets` empty buckets.
fn new_ctrl(buckets: usize) -> Vec<u8> {
    let mut ctrl = Vec::with_capacity(buckets);
    ctrl.resize(buckets, EMPTY);
    ctrl
}

/// Returns a freshly allocated bucket-to-entry array with no entries.
fn new_slots(buckets: usize) -> Vec<u32> {
    let mut slots = Vec::with_capacity(buckets);
    slots.resize(buckets, NO_ENTRY);
    slots
}

/// An iterator over the entries of a `HashMap`.
///
/// Created by `HashMap::iter`.
pub struct Iter<'a, K, V> {
    inner: core::slice::Iter<'a, Entry<K, V>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|entry| (&entry.key, &entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}
impl<K, V> FusedIterator for Iter<'_, K, V> {}

/// A mutable iterator over the entries of a `HashMap`.
///
/// Created by `HashMap::iter_mut`.
pub struct IterMut<'a, K, V> {
    inner: core::slice::IterMut<'a, Entry<K, V>>,
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.inner.next()?;
        Some((&entry.key, &mut entry.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<K, V> ExactSizeIterator for IterMut<'_, K, V> {}
impl<K, V> FusedIterator for IterMut<'_, K, V> {}

impl<'a, K, V> IntoIterator for &'a HashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut HashMap<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
