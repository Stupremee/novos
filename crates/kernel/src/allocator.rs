//! Implementation of all different allocators that are used inside the kernel.

pub mod buddy;
pub use buddy::{order_for_size, size_for_order, BuddyAllocator};

pub mod rangeset;
pub use rangeset::RangeSet;

pub mod slab;

use crate::unit;
use core::fmt;
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
        /// this is not a real error and should never be thrown somewhere
        NoSlabForLayout,
        /// Tried to create a `NonNull` from a null pointer.
        ///
        /// Mostly just a safety mechanism to avoid UB.
        NullPointer,
    }
}

/// Statistics for a memory allocator.
#[derive(Debug, Clone)]
pub struct AllocStats {
    /// The name of the allocator that collected these stat.s
    pub name: &'static str,
    /// The number of size that were allocated.
    pub allocated: usize,
    /// The number of bytes that are left for allocation.
    pub free: usize,
    /// The total number of bytes that this allocator has available for allocation.
    pub total: usize,
}

impl AllocStats {
    /// Create a new [`AllocStats`] instance for the given allocator name.
    pub const fn with_name(name: &'static str) -> Self {
        Self {
            name,
            free: 0,
            allocated: 0,
            total: 0,
        }
    }
}

impl fmt::Display for AllocStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.name)?;
        self.name.chars().try_for_each(|_| write!(f, "~"))?;

        writeln!(f, "\n{:<11} {}", "Allocated:", unit::bytes(self.allocated))?;
        writeln!(f, "{:<11} {}", "Free:", unit::bytes(self.free))?;
        writeln!(f, "{:<11} {}", "Total:", unit::bytes(self.total))?;

        self.name.chars().try_for_each(|_| write!(f, "~"))?;
        Ok(())
    }
}
