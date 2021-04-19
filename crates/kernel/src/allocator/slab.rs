use crate::allocator::PAGE_SIZE;
use crate::{allocator, vmem};
use core::alloc::Layout;
use core::ptr::NonNull;

/// The number of pages to allocate when growing.
pub const GROW_PAGES_COUNT: usize = 4;

/// A slab holds a bunch of objects with a fixed size.
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

    /// Return the number of bytes each grow operation will add.
    pub fn grow_size(&self) -> usize {
        self.size.max(PAGE_SIZE) * GROW_PAGES_COUNT
    }

    /// Grow this slab by allocating a bunch of physical memory and adding it to
    /// this slab. The size of `page` must be equal to the size returned by `grow_size`.
    pub unsafe fn grow(&mut self, page: NonNull<u8>) -> Result<(), vmem::Error> {
        let size = self.grow_size();
        let start = page.as_ptr() as usize;

        // go through each page and add it to this slab
        for page in (start..start + size).step_by(self.size) {
            self.push(
                NonNull::new(page as *mut _)
                    .ok_or(vmem::Error::Alloc(allocator::Error::NullPointer))?,
            )
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

macro_rules! gen_slab_pool {
    ($($size:literal => $name:ident,)+) => {
        /// A pool of slabs that manages multiple slabs
        /// and allows to allocate / deallocate memory from all
        /// the slabs.
        pub struct SlabPool {
            $($name: Slab,)+
        }

        impl SlabPool {
            /// Construct an empty pool of slabs.
            pub const fn new() -> Self {
                Self {
                    $($name: Slab::new($size),)+
                }
            }

            /// Find a slab that is able to hold the given layout inside this slab pool.
            pub fn slab_for_layout(&mut self, layout: Layout) -> Option<&mut Slab> {
                let slab = match (layout.size(), layout.align()) {
                    $((0..=$size, 0..=$size) => &mut self.$name,)+
                    _ => return None,
                };
                Some(slab)
            }
        }
    };
}

gen_slab_pool! {
    32 => slab_32,
    64 => slab_64,
    128 => slab_128,
    256 => slab_256,
    512 => slab_512,
    1024 => slab_1024,
    2048 => slab_2048,
}
