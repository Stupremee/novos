//! Management of physical memory, including initialization and global allocation.

use core::ptr::NonNull;
use core::{array, mem, slice};

use crate::unit;
use allocator::{
    rangeset::{self, Range},
    BuddyAllocator, RangeSet,
};
use devicetree::DeviceTree;
use riscv::sync::Mutex;

displaydoc_lite::displaydoc! {
    /// Any error that is related to physical memory.
    #[derive(Debug)]
    pub enum Error {
        /// {_0}
        RangeSet(rangeset::Error),
        /// {_0}
        Alloc(allocator::Error),
        /// tried to add a region of memory that starts at address zero
        NullRegion,
    }
}

/// Initialize the global physical memory allocator by adding all regions specified
/// in the `/memory` node of the given device tree.
pub unsafe fn init(tree: &DeviceTree<'_>) -> Result<(), Error> {
    // prepare a list of ranges that will be used to track all memory regions
    let mut mem = RangeSet::new();

    // read the memory regions specified in the devicetree
    tree.memory()
        .regions()
        .try_for_each(|region| {
            // add each region to the rangeset, subtract 1 because rangeset uses inclusive ranges
            let range = Range::new(region.start(), region.end() - 1);
            mem.insert(range)
        })
        .map_err(Error::RangeSet)?;

    // remove the ranges that should not be allocated
    array::IntoIter::new(get_blocked_ranges())
        .into_iter()
        .try_for_each(|region| {
            mem.remove_range(region)?;
            Ok(())
        })
        .map_err(Error::RangeSet)?;

    let mut alloc = PHYS_MEM.0.lock();

    // add each region to the global allocator and get the amount of total bytes
    let bytes = mem
        .as_slice()
        .iter()
        .try_fold(0usize, |acc, &Range { start, end }| {
            let start = NonNull::new(start as *mut u8).ok_or(Error::NullRegion)?;
            let end = NonNull::new(end as *mut u8).ok_or(Error::NullRegion)?;
            let bytes = alloc.add_region(start, end).map_err(Error::Alloc)?;

            Ok::<_, Error>(acc + bytes)
        })?;

    log::info!(
        "{} the physical memory allocator with {} free memory",
        "Initialized".green(),
        unit::bytes(bytes),
    );

    Ok(())
}

/// Get a list of memory ranges that must not be used for memory allocation,
/// like the kernel itself and OpenSBI.
fn get_blocked_ranges() -> [Range; 2] {
    let (kernel_start, kernel_end) = riscv::symbols::kernel_range();

    [
        // this range contains the OpenSBI firmware
        Range::new(0x8000_0000, 0x801F_FFFF),
        // the kernel itself
        Range::new(kernel_start as _, kernel_end as usize - 1),
    ]
}

static PHYS_MEM: PhysicalAllocator = PhysicalAllocator(Mutex::new(BuddyAllocator::new()));

struct PhysicalAllocator(Mutex<BuddyAllocator>);

unsafe impl Send for PhysicalAllocator {}
unsafe impl Sync for PhysicalAllocator {}

/// Allocate a single page of physical memory.
#[inline]
pub fn alloc() -> Result<NonNull<u8>, allocator::Error> {
    alloc_order(0)
}

/// Allocate a region of memory with the given order.
#[inline]
pub fn alloc_order(order: usize) -> Result<NonNull<u8>, allocator::Error> {
    PHYS_MEM.0.lock().allocate(order)
}

/// Allocate a single page of physical memory and zero it.
#[inline]
pub fn zalloc() -> Result<NonNull<u8>, allocator::Error> {
    zalloc_order(0)
}

/// Allocate a region of memory with the given order.
#[inline]
pub fn zalloc_order(order: usize) -> Result<NonNull<u8>, allocator::Error> {
    let page = alloc_order(order)?;

    let slice = unsafe {
        slice::from_raw_parts_mut(
            page.as_ptr().cast::<u64>(),
            allocator::size_for_order(order) / mem::size_of::<u64>(),
        )
    };
    slice.fill(0u64);

    Ok(page)
}

/// Free a single page that was allocated using the order 0.
///
/// # Safety
///
/// The pointer *must* be allocated through one of the allocation methods in this module.
#[inline]
pub unsafe fn free(ptr: NonNull<u8>) -> Result<(), allocator::Error> {
    free_order(ptr, 0)
}

/// Free a single page that was allocated using the given order.
///
/// # Safety
///
/// The pointer *must* be allocated through one of the allocation methods in this module.
/// The order *must* be the same as the order that the pointer was allocated with.
#[inline]
pub unsafe fn free_order(ptr: NonNull<u8>, order: usize) -> Result<(), allocator::Error> {
    PHYS_MEM.0.lock().deallocate(ptr, order)
}
