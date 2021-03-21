//! Virtual memory allocator.

mod vaddr;

use crate::allocator::slab::SlabPool;
use crate::page::{PageSize, PageTable, Perm};
use crate::{
    allocator::{self, buddy::MAX_ORDER, slab, PAGE_SIZE},
    page,
};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::NonNull;
use riscv::sync::Mutex;

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

/// The freelist is used to cache the pages for every order.
///
/// If you try to pop from an empty freelist, a new page with the correct
/// size will be allocated and returned.
struct FreeList {
    order: usize,
    head: Option<NonNull<usize>>,
}

impl FreeList {
    const fn new(order: usize) -> Self {
        Self { order, head: None }
    }

    /// Pop a page from this free list. If no page is available, allocate a bunch of
    /// new pages for this free list.
    fn pop(&mut self) -> Result<NonNull<u8>, Error> {
        // if this list is empty, allocate new pages
        if self.head.is_none() {
            // get the size of each page inside this freelist
            let size = allocator::size_for_order(self.order);

            // get a new vaddr and allocate the new memory
            let vaddr = vaddr::global().next_vaddr_4k(size / PAGE_SIZE);
            page::root()
                .map_alloc(
                    vaddr,
                    size / PAGE_SIZE,
                    PageSize::Kilopage,
                    Perm::READ | Perm::WRITE,
                )
                .map_err(Error::Page)?;

            // push the new allocated page to this linked list
            unsafe {
                self.push(NonNull::new(vaddr.as_ptr()).unwrap());
            }
        }

        // pop a page from this list
        let head = self
            .head
            .ok_or(Error::Alloc(allocator::Error::NoMemoryAvailable))?;
        self.head = NonNull::new(unsafe { *head.as_ptr() as *mut _ });
        Ok(head.cast())
    }

    /// "Free" a page of memory by adding it back to this free list.
    unsafe fn push(&mut self, ptr: NonNull<u8>) {
        let ptr = ptr.cast::<usize>();
        *ptr.as_ptr() = self.head.map(|x| x.as_ptr() as usize).unwrap_or(0);
        self.head = Some(ptr);
    }
}

/// The global allocator that is used inside the kernel to allocate anything.
pub struct VirtualAllocator {
    slabs: SlabPool,
    free_lists: [FreeList; MAX_ORDER],
}

impl VirtualAllocator {
    /// Create a new virtual allocator.
    pub const fn new() -> Self {
        Self {
            slabs: SlabPool::new(),
            free_lists: [
                FreeList::new(0),
                FreeList::new(1),
                FreeList::new(2),
                FreeList::new(3),
                FreeList::new(4),
                FreeList::new(5),
                FreeList::new(6),
                FreeList::new(7),
                FreeList::new(8),
                FreeList::new(9),
                FreeList::new(10),
                FreeList::new(11),
            ],
        }
    }

    fn alloc(&mut self, layout: Layout) -> Result<NonNull<u8>, Error> {
        // FIXME
        assert!(
            layout.align() <= 4096,
            "allocating >4096 alignment not supported currently"
        );

        // first, we try to find a slab that is able to hold the object
        if let Some(slab) = self.slabs.slab_for_layout(layout) {
            // if the slab allocation succeeds, return
            if let Some(ptr) = slab.allocate() {
                return Ok(ptr);
            }

            // otherwise grow the slab and try again
            let order = allocator::order_for_size(slab::GROW_PAGES_COUNT * PAGE_SIZE);
            let page = self.free_lists[order].pop()?;
            unsafe {
                slab.grow(page)?;
            }

            return slab
                .allocate()
                .ok_or(Error::Alloc(allocator::Error::NoMemoryAvailable));
        }

        // the next step is, to look at the list of free-lists for the order
        // the layout needs
        let order = allocator::order_for_size(layout.size());

        // if there's a list for this order, pop a page from it.
        if let Some(list) = self.free_lists.get_mut(order) {
            return list.pop();
        }

        // last step, if there's no free lsit for the requested size, manually allocate
        // the memory required for the allocation
        let size = allocator::align_up(layout.size(), PAGE_SIZE);
        let vaddr = vaddr::global().next_vaddr_4k(size / PAGE_SIZE);
        page::root()
            .map_alloc(
                vaddr,
                size / PAGE_SIZE,
                PageSize::Kilopage,
                Perm::READ | Perm::WRITE,
            )
            .map_err(Error::Page)?;

        // return the fresh allocated memory
        Ok(NonNull::new(vaddr.as_ptr()).unwrap())
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) -> Result<(), Error> {
        // FIXME
        assert!(
            layout.align() <= 4096,
            "allocating >4096 alignment not supported currently"
        );

        // first, we try to find a slab that may allocated the object
        if let Some(slab) = self.slabs.slab_for_layout(layout) {
            // if we found a slab, the block came from this slab so push it back
            // to the slab
            slab.deallocate(ptr);
            return Ok(());
        }

        // the next step is, to look at the list of free-lists for the order
        // the layout needs
        let order = allocator::order_for_size(layout.size());

        // if there's a list for this order, push the page from it.
        if let Some(list) = self.free_lists.get_mut(order) {
            list.push(ptr);
            return Ok(());
        }

        // last step, if there's no free lsit for the requested size, manually free the memory
        let size = allocator::align_up(layout.size(), PAGE_SIZE);
        page::root()
            .free(ptr.as_ptr().into(), size / PAGE_SIZE)
            .map_err(Error::Page)?;

        // tell the vaddr allocator that the address is free to use again
        vaddr::global().free_vaddr_4k(ptr.as_ptr().into(), size / PAGE_SIZE);

        // successfully freed memory
        Ok(())
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator(Mutex::new(VirtualAllocator::new()));

struct GlobalAllocator(Mutex<VirtualAllocator>);

unsafe impl Send for GlobalAllocator {}
unsafe impl Sync for GlobalAllocator {}

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut res = self.0.lock();
        match res.alloc(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(err) => {
                log::warn!(
                    "{} to allocate memory (size: {} align: {}): {}",
                    "Failed".yellow(),
                    layout.size(),
                    layout.align(),
                    err
                );
                core::ptr::null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut res = self.0.lock();
        match res.dealloc(NonNull::new(ptr).unwrap(), layout) {
            Ok(()) => (),
            Err(err) => {
                log::warn!(
                    "{} to free memory (size: {} align: {}): {}",
                    "Failed".yellow(),
                    layout.size(),
                    layout.align(),
                    err
                );
            }
        }
    }
}
