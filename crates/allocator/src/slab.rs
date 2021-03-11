use crate::LinkedList;
use core::ptr::NonNull;

/// A slab holds a bunch of objects with a fixed size.
///
/// It will allocate new memory on the fly if there's no memory
/// left in this slab.
pub struct Slab {
    free_list: LinkedList,
    // the size of each object inside this slab
    #[allow(dead_code)]
    size: usize,
}

impl Slab {
    /// Create a new slab that is able to hold `size` big objects.
    pub const fn new(size: usize) -> Self {
        Self {
            free_list: LinkedList::new(),
            size,
        }
    }

    /// Get one block of this block, remove it from the list and return the pointer.
    pub fn allocate(&mut self) -> Option<NonNull<u8>> {
        self.free_list.pop().map(|x| x.cast())
    }

    /// Free a block of memory that was previously allocated by this slab.
    ///
    /// # Safety
    ///
    /// The given pointer *must* came from the [`Self::allocate`] method.
    pub unsafe fn free(&mut self, ptr: NonNull<u8>) {
        self.free_list.push(ptr.cast())
    }
}
