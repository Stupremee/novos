use super::{align_up, AllocStats, Error, Result};
use crate::page::phys2virt;
use core::{cmp, ptr::NonNull};

/// The maximum order for the buddy allocator (inclusive).
///
/// This means, that orders 0..MAX_ORDER are available.
pub const MAX_ORDER: usize = 12;

/// The size of the order `0`, thus it's the smallest possible size
/// that can be allocated.
pub const MIN_ORDER_SIZE: usize = super::PAGE_SIZE;

/// Calculates the size in bytes for the given order.
pub fn size_for_order(order: usize) -> usize {
    (1 << order) * MIN_ORDER_SIZE
}

/// Calculates the first order where the given `size` would fit in.
///
/// This function may return an order that is larger than [`MAX_ORDER`].
pub fn order_for_size(size: usize) -> usize {
    let size = cmp::max(size, MIN_ORDER_SIZE);
    let size = size.next_power_of_two() / MIN_ORDER_SIZE;
    size.trailing_zeros() as usize
}

/// Calculate the address of the other buddy for the given block.
fn buddy_of(block: NonNull<usize>, order: usize) -> Result<NonNull<usize>> {
    let buddy = block.as_ptr() as usize ^ size_for_order(order);
    NonNull::new(buddy as *mut _).ok_or(Error::NullPointer)
}

/// The central structure that is responsible for allocating
/// memory using the buddy allocation algorithm.
pub struct BuddyAllocator {
    orders: [Option<NonNull<ListNode>>; MAX_ORDER],
    stats: AllocStats,
}

impl BuddyAllocator {
    /// Create a empty and uninitialized buddy allocator.
    pub const fn new() -> Self {
        Self {
            orders: [None; MAX_ORDER],
            stats: AllocStats::with_name("Physical Memory"),
        }
    }

    /// Adds a region of memory to this allocator and makes it available for allocation.
    ///
    /// This method will add as many orders as possible, meaning that a region of size
    /// `size_for_order(4) + 4KiB` will add one order `4` page and one order `0` page.
    /// If the region size is not a multiple of the [pagesize](super::PAGE_SIZE),
    /// the memory that is leftover will stay untouuched.
    ///
    /// If the `start` pointer is not aligned to the page size it will be aligned
    /// correctly before added to this allocator.
    ///
    /// Returns the total number of bytes that were added to this allocator.
    pub unsafe fn add_region(&mut self, start: NonNull<u8>, end: NonNull<u8>) -> Result<usize> {
        // align the pointer to the page size
        let start = start.as_ptr();
        let mut start = align_up(start as _, MIN_ORDER_SIZE) as *mut u8;
        let end = end.as_ptr();

        // check if there's enough memory for at least
        // one page
        if (end as usize).saturating_sub(start as usize) < MIN_ORDER_SIZE {
            return Err(Error::RegionTooSmall);
        }

        // check if the memory region is invalid
        if end < start {
            return Err(Error::InvalidRegion);
        }

        // loop until there's not enough memory left to allocate a single page
        let mut total = 0;
        while (end as usize).saturating_sub(start as usize) >= MIN_ORDER_SIZE {
            let order = self.add_single_region(start, end)?;
            let size = size_for_order(order);

            start = start.add(size);
            total += size;
        }

        Ok(total)
    }

    /// Tries to add a single order to this allocator from the given range.
    ///
    /// Returns the order which was inserted into this allocator.
    unsafe fn add_single_region(&mut self, start: *mut u8, end: *mut u8) -> Result<usize> {
        // TODO: Optimize so it doesn't need a loop
        let start_addr = start as usize;

        // loop until we reached the maximum order
        let mut order = 0;
        while order < (MAX_ORDER - 1) {
            // calculate the size for the next order,
            // so we can break if another order doesn't fit.
            let size = size_for_order(order + 1);

            // check if there's enough memory left for the size of
            // the next order
            let new_end = match start_addr.checked_add(size) {
                Some(num) if num <= end as usize => num,
                _ => break,
            };

            // if there is enough place, try the next order,
            // otherwise we break. we also need to check if the buddy of this
            // block would fit into the range.
            let buddy = buddy_of(NonNull::new(start as *mut _).unwrap(), order + 1)?.as_ptr();
            if new_end <= end as usize && (start.cast() <= buddy && buddy <= end.cast()) {
                order += 1;
            } else {
                break;
            }
        }

        // push the block to the list for the given order
        self.order_push(order, NonNull::new(start).unwrap().cast());

        // update statistics
        let size = size_for_order(order);
        self.stats.total += size;
        self.stats.free += size;

        Ok(order)
    }

    /// Allocates a chunk of memory that has the given order.
    ///
    /// The size for returned chunk can be calculated using [`size_for_order`].
    pub fn allocate(&mut self, order: usize) -> Result<NonNull<u8>> {
        // check if we exceeded the maximum order
        if order >= MAX_ORDER {
            return Err(Error::OrderTooLarge);
        }

        // fast path: if there's a block with the given order,
        // return it
        if let Some(block) = self.order_pop(order) {
            // update statistics
            let size = size_for_order(order);
            self.alloc_stats(size);

            return NonNull::new(block.as_ptr().cast()).ok_or(Error::NullPointer);
        }

        // slow path: walk up the order list and split required buddies.
        //
        // we map the error to no memory available, because if there's no block
        // in the order above, we don't have any memory available
        let block = self
            .allocate(order + 1)
            .map_err(|_| Error::NoMemoryAvailable)?;

        // this is one of the big advanteges of the buddy system.
        //
        // the addresses of two buddies only differe in one bit, thus we
        // can easily get the address of a buddy, if we have the other buddy.
        let buddy = buddy_of(block.cast(), order)?;

        // push the second buddy to the free list
        self.order_push(order, buddy.cast());

        // update statistics
        let size = size_for_order(order);
        self.dealloc_stats(size);

        Ok(block)
    }

    /// Deallocates a block of memory, that was allocated using the given order.
    ///
    /// # Safety
    ///
    /// The poitner must be allocated by `self` using the [`Self::allocate`] method
    /// with the same order as given here.
    pub unsafe fn deallocate(&mut self, block: NonNull<u8>, order: usize) -> Result<()> {
        // get the buddy of the block to deallocate
        let buddy_addr = buddy_of(block.cast(), order)?;

        // check if the buddy is free, if so remove it and
        // free walk up to try merge again
        if self.order_remove(order, buddy_addr.cast()) {
            // update statistics
            let size = size_for_order(order);
            self.alloc_stats(size);

            // ...and then go to the next level and merge both buddies
            let new_block = cmp::min(buddy_addr.cast(), block);
            self.deallocate(new_block, order + 1)?;
        } else {
            // if the buddy is not free, just insert the block to deallocate
            // into the free-list
            self.order_push(order, block.cast());

            // update statistics
            let size = size_for_order(order);
            self.dealloc_stats(size);
        }

        Ok(())
    }

    /// Return a copy of the statistics of this buddy allocator.
    pub fn stats(&self) -> AllocStats {
        self.stats.clone()
    }

    /// Push the given block onto the free list of the given order.
    fn order_push(&mut self, order: usize, ptr: NonNull<ListNode>) {
        let head = self.orders[order];

        unsafe {
            let vptr = phys2virt(ptr.as_ptr());
            vptr.as_ptr::<ListNode>().write(ListNode { next: head });
        }

        self.orders[order] = Some(ptr.cast());
    }

    /// Pop an entry from the free list for the given order.
    fn order_pop(&mut self, order: usize) -> Option<NonNull<ListNode>> {
        let head = self.orders[order]?;
        let vhead = phys2virt(head.as_ptr());

        unsafe {
            self.orders[order] = (*vhead.as_ptr::<ListNode>()).next;
        }

        Some(head)
    }

    /// Try to remove the given ptr from the free list for the given order.
    ///
    /// Returns whether the remove was successful.
    fn order_remove(&mut self, order: usize, to_remove: NonNull<ListNode>) -> bool {
        let mut cur: *mut Option<NonNull<ListNode>> = &mut self.orders[order];

        // walk through the linked list
        while let Some(ptr) = unsafe { *cur } {
            let vptr = phys2virt(ptr.as_ptr()).as_ptr::<ListNode>();

            // if this is the block to remove...
            if ptr == to_remove {
                // ...remove it and return
                unsafe {
                    *cur = (*vptr).next;
                }
                return true;
            }

            // go to the next entry
            unsafe {
                cur = &mut (*vptr).next;
            }
        }

        false
    }

    fn alloc_stats(&mut self, size: usize) {
        self.stats.free -= size;
        self.stats.allocated += size;
    }

    fn dealloc_stats(&mut self, size: usize) {
        self.stats.free += size;
        self.stats.allocated -= size;
    }
}

struct ListNode {
    next: Option<NonNull<ListNode>>,
}
