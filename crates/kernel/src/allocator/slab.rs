use crate::{allocator, vmem};
use core::alloc::Layout;
use core::ptr::NonNull;

/// The number of pages to allocate when growing.
pub const GROW_PAGES_COUNT: usize = 4;

/// A slab holds a bunch of objects with a fixed size.
///
/// It will allocate new memory on the fly if there's no memory
/// left in this slab.
pub struct Slab {
    free_list: Option<NonNull<usize>>,
    // the size of each object inside this slab
    size: usize,
}

impl Slab {
    /// Create a new slab that is able to hold `size` big objects.
    const fn new(size: usize) -> Self {
        Self {
            free_list: None,
            size,
        }
    }

    /// Grow this slab by allocating a bunch of physical memory and adding it to
    /// this slab.
    pub unsafe fn grow(&mut self, page: NonNull<u8>) -> Result<(), vmem::Error> {
        let page = page.as_ptr() as usize;
        let size = GROW_PAGES_COUNT * allocator::PAGE_SIZE;

        // loop through every object that fits in the allocated memory
        // and push it to this slab
        for obj in (page..page + size).step_by(self.size) {
            self.push(NonNull::new(obj as *mut _).unwrap());
        }

        Ok(())
    }

    /// Push a pointer to this slabs freelist.
    unsafe fn push(&mut self, ptr: NonNull<usize>) {
        *ptr.as_ptr() = self.free_list.map(|x| x.as_ptr() as usize).unwrap_or(0);
        self.free_list = Some(ptr.cast());
    }

    /// Pop a pointer to this slabs freelist.
    unsafe fn pop(&mut self) -> Option<NonNull<usize>> {
        let head = self.free_list?;
        self.free_list = NonNull::new(*head.as_ptr() as *mut _);
        Some(head)
    }

    /// Get one block of this block, remove it from the list and return the pointer.
    pub fn allocate(&mut self) -> Option<NonNull<u8>> {
        unsafe { self.pop().map(|x| x.cast()) }
    }

    /// Free a block of memory that was previously allocated by this slab.
    pub unsafe fn deallocate(&mut self, ptr: NonNull<u8>) {
        self.push(ptr.cast())
    }
}

/// A pool of slabs that manages multiple slabs
/// and allows to allocate / deallocate memory from all
/// the slabs.
pub struct SlabPool {
    slab_32: Slab,
    slab_64: Slab,
    slab_128: Slab,
    slab_256: Slab,
    slab_512: Slab,
    slab_1024: Slab,
    slab_2048: Slab,
}

impl SlabPool {
    /// Construct an empty pool of slabs.
    pub const fn new() -> Self {
        Self {
            slab_32: Slab::new(32),
            slab_64: Slab::new(64),
            slab_128: Slab::new(128),
            slab_256: Slab::new(256),
            slab_512: Slab::new(512),
            slab_1024: Slab::new(1024),
            slab_2048: Slab::new(2048),
        }
    }

    /// Find a slab that is able to hold the given layout inside this slab pool.
    pub fn slab_for_layout(&mut self, layout: Layout) -> Option<&mut Slab> {
        let slab = match (layout.size(), layout.align()) {
            (0..=32, 0..=32) => &mut self.slab_32,
            (0..=64, 0..=64) => &mut self.slab_64,
            (0..=128, 0..=128) => &mut self.slab_128,
            (0..=256, 0..=256) => &mut self.slab_256,
            (0..=512, 0..=512) => &mut self.slab_512,
            (0..=1024, 0..=1024) => &mut self.slab_1024,
            (0..=2048, 0..=2048) => &mut self.slab_2048,
            _ => return None,
        };
        Some(slab)
    }
}
