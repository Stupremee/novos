//! Management of physical memory, including initialization and global allocation.

use core::alloc::{AllocError, Allocator, Layout};
use core::ptr::NonNull;
use core::{array, mem, ptr, slice};

use crate::allocator::{
    self,
    rangeset::{self, Range},
    BuddyAllocator, RangeSet,
};
use crate::page;
use devicetree::DeviceTree;
use riscv::sync::Mutex;

/// Any error that is related to physical memory.
#[derive(Debug)]
pub enum Error {
    RangeSet(rangeset::Error),
    Alloc(allocator::Error),
    NullRegion,
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
    mem.as_slice()
        .iter()
        .try_for_each(|&Range { start, end }| {
            let start = NonNull::new(start as *mut u8).ok_or(Error::NullRegion)?;
            let end = NonNull::new(end as *mut u8).ok_or(Error::NullRegion)?;

            alloc.add_region(start, end).map_err(Error::Alloc)?;
            Ok(())
        })?;

    Ok(())
}

/// Get a list of memory ranges that must not be used for memory allocation,
/// like the kernel itself and OpenSBI.
fn get_blocked_ranges() -> [Range; 2] {
    let (kernel_start, kernel_end) = crate::symbols::kernel_range();

    [
        // this range contains the OpenSBI firmware
        Range::new(0x8000_0000, 0x801F_FFFF),
        // the kernel itself
        Range::new(kernel_start as _, kernel_end as usize - 1),
    ]
}

static PHYS_MEM: PhysicalAllocator = PhysicalAllocator(Mutex::new(BuddyAllocator::new()));

/// The global allocator that is responsible for allocating phyical memory.
pub struct PhysicalAllocator(Mutex<BuddyAllocator>);

unsafe impl Send for PhysicalAllocator {}
unsafe impl Sync for PhysicalAllocator {}

unsafe impl Allocator for PhysicalAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // the buddy allocator could technically allocate larger alignments,
        // but we don't need that at the moment
        if layout.align() > allocator::PAGE_SIZE {
            log::warn!(
                "{} to allocate physical memory: requested alignment was too big",
                "Failed".yellow()
            );
            return Err(AllocError);
        }

        // get the order for the requested size
        let order = allocator::order_for_size(layout.size());
        let size = allocator::size_for_order(order);

        // perform the allocation
        match self.0.lock().allocate(order) {
            Ok(ptr) => {
                let ptr = page::phys2virt(ptr.as_ptr());
                let slice = ptr::slice_from_raw_parts_mut(ptr.as_ptr(), size);
                Ok(unsafe { NonNull::new_unchecked(slice) })
            }
            Err(err) => {
                log::warn!(
                    "{} to allocate physical memory (order: {}): {:?}",
                    "Failed".yellow(),
                    order,
                    err
                );
                Err(AllocError)
            }
        }
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // get the order for the requested size
        let order = allocator::order_for_size(layout.size());
        let ptr = page::root()
            .translate(ptr.as_ptr().into())
            .map(|(a, _)| NonNull::new(a.as_ptr()).unwrap())
            .unwrap_or(ptr);

        // perform the deallocation
        match self.0.lock().deallocate(ptr, order) {
            Ok(()) => {}
            Err(err) => {
                log::warn!(
                    "{} to free physical memory (order: {}): {:?}",
                    "Failed".yellow(),
                    order,
                    err
                );
            }
        }
    }
}

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
    let page_ptr = page::phys2virt(page.as_ptr());

    let slice = unsafe {
        slice::from_raw_parts_mut(
            page_ptr.as_ptr::<u64>(),
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

/// Return the statistics of the global physmem allocator.
pub fn alloc_stats() -> allocator::AllocStats {
    PHYS_MEM.0.lock().stats()
}

/// Empty struct that can be used as an [`Allocator`], which will allocate from the global physical
/// memory allocator.
#[derive(Clone, Default)]
pub struct GlobalPhysicalAllocator;

unsafe impl Send for GlobalPhysicalAllocator {}
unsafe impl Sync for GlobalPhysicalAllocator {}

unsafe impl Allocator for GlobalPhysicalAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        PHYS_MEM.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        PHYS_MEM.deallocate(ptr, layout)
    }
}
