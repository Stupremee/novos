//! Virtual memory allocator.

use crate::allocator::slab::SlabPool;
use crate::boot::KERNEL_VMEM_ALLOC_BASE;
use crate::page::{PageSize, PageTable, Perm};
use crate::{
    allocator::{self, PAGE_SIZE},
    page,
};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};

static VMEM_ALLOC_ADDR: AtomicUsize = AtomicUsize::new(KERNEL_VMEM_ALLOC_BASE);

displaydoc_lite::displaydoc! {
    /// Any error that can happen while allocating or deallocating virtual memory.
    #[derive(Debug)]
    pub enum Error {
        /// {_0}
        Alloc(allocator::Error),
        /// {_0}
        Page(page::Error),
    }
}

/// Allocate a single, virtual memory page.
pub fn valloc() -> Result<NonNull<u8>, Error> {
    valloc_pages(1)
}

/// Allocate `n` pages of virtual memory.
pub fn valloc_pages(n: usize) -> Result<NonNull<u8>, Error> {
    // compute the size of memory that will be mapped,
    // we add one additional page, because there will be guard
    // pages between the allocations
    let size = n * PAGE_SIZE;

    // get the virtual address where the memory will be mapped
    let vaddr = VMEM_ALLOC_ADDR.fetch_add(size + PAGE_SIZE, Ordering::AcqRel);

    // map the virtual memory
    let mut table = page::root();
    table
        .map_alloc(
            vaddr.into(),
            n,
            PageSize::Kilopage,
            Perm::READ | Perm::WRITE,
        )
        .map_err(Error::Page)?;

    // map the guard page
    table
        .map_alloc((vaddr + size).into(), 1, PageSize::Kilopage, Perm::EXEC)
        .map_err(Error::Page)?;

    Ok(NonNull::new(vaddr as *mut _).unwrap())
}

/// The global allocator that is used inside the kernel to allocate anything.
pub struct VirtualAllocator {
    _slabs: SlabPool,
}

unsafe impl GlobalAlloc for VirtualAllocator {
    unsafe fn alloc(&self, _: Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {
        todo!()
    }
}
