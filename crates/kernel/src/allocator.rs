//! Implementation of all different allocators that are used inside the kernel.

pub mod buddy;
pub use buddy::{order_for_size, size_for_order, BuddyAllocator};

pub mod linked_list;
pub use linked_list::LinkedList;

pub mod rangeset;
pub use rangeset::RangeSet;

pub mod slab;

use displaydoc_lite::displaydoc;

/// The size of a single page in memory.
///
/// This is also used as the order-0 size inside
/// the buddy allocator.
pub const PAGE_SIZE: usize = 4096;

/// Result for every memory allocation operation.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Aligns the given `addr` upwards to `align`.
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

displaydoc! {
    /// Any error that can happen while allocating or deallocating memory.
    #[derive(Debug)]
    pub enum Error {
        /// tried to add a region to an allocator that was too small.
        RegionTooSmall,
        /// the `end` pointer of a memory region was before the `start` pointer.
        InvalidRegion,
        /// tried to allocate an order that exceeded the maximum order.
        OrderTooLarge,
        /// tried to allocate, but there was no free memory left.
        NoMemoryAvailable,
        /// tried to allocate zero pages using `alloc_pages`
        AllocateZeroPages,
        /// Tried to create a `NonNull` from a null pointer.
        ///
        /// Mostly just a safety mechanism to avoid UB.
        NullPointer,
    }
}
