//! Collection types.
//!
//! This module mirrors `alloc::collections` (so `BTreeMap`, `BTreeSet`,
//! `BinaryHeap`, `LinkedList` and `VecDeque` keep working), and additionally
//! provides a `HashMap` that `alloc` itself does not offer.

#[doc(no_inline)]
pub use alloc::collections::*;

pub mod hash_map;

pub use self::hash_map::HashMap;
