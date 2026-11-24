#![no_std]

use allocator::{AllocError, AllocResult, BaseAllocator, ByteAllocator, PageAllocator};
use core::alloc::Layout;
use core::ptr::NonNull;

/// Aligns pos up to align (which must be a power of two).
const fn align_up(pos: usize, align: usize) -> usize {
    (pos + align - 1) & !(align - 1)
}

/// Aligns pos down to align (which must be a power of two).
const fn align_down(pos: usize, align: usize) -> usize {
    pos & !(align - 1)
}

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    /// The left bound of the whole memory range.
    start: usize,
    /// The right bound (exclusive) of the whole memory range.
    end: usize,
    /// The bump pointer of the byte area. Bytes are allocated from start towards end.
    b_pos: usize,
    /// The bump pointer of the page area. Pages are allocated from end towards start.
    /// The free area is always [b_pos, p_pos).
    p_pos: usize,
    /// Number of live byte allocations. When it drops to zero, the whole byte area
    /// is reclaimed by resetting b_pos back to start.
    b_count: usize,
    /// Number of pages handed out by the page allocator (never reclaimed).
    p_count: usize,
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
            b_count: 0,
            p_count: 0,
        }
    }

    /// Returns whether the allocator has been initialized with a non-empty range.
    const fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        self.start = start;
        self.end = start + size;
        self.b_pos = start;
        self.p_pos = self.end;
        self.b_count = 0;
        self.p_count = 0;
    }

    /// Adds a new memory region.
    ///
    /// The double-end layout only tracks one contiguous range, so a new region can
    /// only be merged when it is adjacent to the whole range and the corresponding
    /// end (byte area for the low side, page area for the high side) is still
    /// untouched. Otherwise NoMemory is returned.
    fn add_memory(&mut self, start: usize, size: usize) -> AllocResult {
        if size == 0 {
            return Ok(());
        }
        let new_end = start.checked_add(size).ok_or(AllocError::InvalidParam)?;

        if self.is_empty() {
            self.init(start, size);
            return Ok(());
        }
        if start < self.end && self.start < new_end {
            return Err(AllocError::MemoryOverlap);
        }
        if start == self.end && self.p_count == 0 {
            // Extend upwards: the page area is still untouched, so the whole
            // range can simply grow.
            self.end = new_end;
            self.p_pos = new_end;
            return Ok(());
        }
        if new_end == self.start && self.b_count == 0 {
            // Extend downwards: the byte area is still untouched.
            self.start = start;
            self.b_pos = start;
            return Ok(());
        }
        Err(AllocError::NoMemory)
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(&mut self, layout: Layout) -> AllocResult<NonNull<u8>> {
        let start = align_up(self.b_pos, layout.align());
        let end = start.checked_add(layout.size()).ok_or(AllocError::NoMemory)?;
        // Do not overlap with the page area: return OOM instead of panicking so
        // that the upper layer can handle it.
        if end > self.p_pos {
            return Err(AllocError::NoMemory);
        }
        self.b_pos = end;
        self.b_count += 1;
        NonNull::new(start as *mut u8).ok_or(AllocError::NoMemory)
    }

    fn dealloc(&mut self, _pos: NonNull<u8>, _layout: Layout) {
        if self.b_count == 0 {
            return;
        }
        self.b_count -= 1;
        if self.b_count == 0 {
            // All byte allocations are gone, reclaim the whole byte area.
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        (self.b_pos - self.start) + (self.end - self.p_pos)
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(&mut self, num_pages: usize, align_pow2: usize) -> AllocResult<usize> {
        if num_pages == 0 {
            return Err(AllocError::InvalidParam);
        }
        // align_pow2 is the required alignment in bytes. Every page allocation
        // is at least page-aligned.
        let align = align_pow2.max(SIZE);
        if !align.is_power_of_two() || align % SIZE != 0 {
            return Err(AllocError::InvalidParam);
        }
        let size = num_pages.checked_mul(SIZE).ok_or(AllocError::InvalidParam)?;
        let start = align_down(
            self.p_pos.checked_sub(size).ok_or(AllocError::NoMemory)?,
            align,
        );
        if start < self.b_pos {
            return Err(AllocError::NoMemory);
        }
        self.p_pos = start;
        self.p_count += num_pages;
        Ok(start)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
        // The page area grows backwards and is never reclaimed.
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / SIZE
    }

    fn used_pages(&self) -> usize {
        self.p_count
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / SIZE
    }
}
