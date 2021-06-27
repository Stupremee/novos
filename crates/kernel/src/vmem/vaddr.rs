use crate::boot::KERNEL_VMEM_ALLOC_BASE;
use crate::page::{PageSize, VirtAddr};
use crate::pmem::GlobalPhysicalAllocator;
use alloc::vec::Vec;
use riscv::sync::{Mutex, MutexGuard};

/// Indicating that an entry inside the bitmap is completely filled.
const FULL_ENTRY: u64 = u64::MAX;

/// The size of the memory region a single bit specifies.
const BIT_SIZE: usize = PageSize::Megapage.size();

static VADDR_ALLOC: Mutex<VirtualAddressAllocator> =
    Mutex::new(VirtualAddressAllocator::new(KERNEL_VMEM_ALLOC_BASE));

/// An "allocator" that is responsible for giving out virtual addresses
/// that can be used. The allocator operates on 2 MiB large megapages.
pub struct VirtualAddressAllocator {
    // the bitmap stores a bit for each page, indicating if it's used or free.
    // there must be a bit for every page until `head`
    bitmap: Vec<u64, GlobalPhysicalAllocator>,
    // the start address of the vaddr allocator
    start: usize,
}

impl VirtualAddressAllocator {
    /// Create a new vaddr allocator that starts at the given address.
    pub const fn new(start: usize) -> Self {
        Self {
            bitmap: Vec::new_in(GlobalPhysicalAllocator),
            start,
        }
    }

    /// Allocate a new virtaddr, that is aligned to the mega page size
    /// and is able to hold `n` mega pages.
    pub fn next_vaddr(&mut self, n: usize) -> VirtAddr {
        assert_ne!(n, 0, "requested to alloc 0 vaddrs");

        // if we request more than 64 pages, we will not check for single bits,
        // instead use the full entries as the bitmap
        if n >= 64 {
            // get the number of entries that are required to be free
            let num_entries = (n + 63) / 64;

            // go through the bitmap in windows of the required entry count
            for (idx, window) in self.bitmap().windows(num_entries).enumerate() {
                // if one of the entries in this window, is not empty,
                // we can't fit the requested size
                if window.iter().any(|entry| *entry != 0) {
                    continue;
                }

                // set the entries to full
                self.bitmap()[idx..idx + num_entries]
                    .iter_mut()
                    .for_each(|entry| *entry = FULL_ENTRY);

                // return the address
                let addr = self.start + (idx * (64 * BIT_SIZE));
                return addr.into();
            }

            // if we hit this, there are no entries, so grow the bitmap
            // and try again
            let bitmap = self.bitmap();
            bitmap.resize(bitmap.len() * 2, 0);
            return self.next_vaddr(n);
        }

        // get a mask, that can check for `n` pages that are free
        let mask = (1 << n) - 1;

        // check each entry, that is not full, to see if it has the
        // requested amount of pages free
        for (idx, entry) in self.bitmap().iter_mut().enumerate() {
            // if the entry is full, skip it
            if *entry == FULL_ENTRY {
                continue;
            }

            // get the first bit that is free
            let bit = entry.trailing_ones() as usize;

            // check that there's enough bits free, to fit the requested amount
            if (*entry >> bit) & mask != 0 {
                continue;
            }

            // the entry can fit enough pages, so set the entry as used
            // and return the address
            *entry |= mask << bit;

            let addr = self.start + (idx * (64 * BIT_SIZE)) + (bit * BIT_SIZE);
            return addr.into();
        }

        // if we hit this, there are no entries, so grow the bitmap
        // and try again
        let bitmap = self.bitmap();
        bitmap.resize(bitmap.len() * 2, 0);
        self.next_vaddr(n)
    }

    /// Mark the given virtual address, that was previously allocated with `n` pages, as free.
    pub unsafe fn free_vaddr(&mut self, addr: VirtAddr, n: usize) {
        assert_ne!(n, 0, "requested to free 0 vaddrs");

        let addr = usize::from(addr);

        // if we request to free more than 64 pages, we need to free whole entries
        if n >= 64 {
            // get the index of the entry, `addr` is marked by, and the number
            // of entries that were allocated
            let entry = (addr - self.start) / (64 * BIT_SIZE);
            let num_entries = (n + 63) / 64;

            // mark the entries as free
            self.bitmap()[entry..entry + num_entries]
                .iter_mut()
                .for_each(|entry| *entry = 0);

            return;
        }

        // get the entry and bit that is responsible for `addr`
        let entry = (addr - self.start) / (64 * BIT_SIZE);
        let bit = ((addr - self.start) / BIT_SIZE) % 64;
        let mask = !((1 << bit) - 1) as u64;

        // mark the pages as free
        let entry = &mut self.bitmap()[entry];
        *entry &= mask;
    }

    #[inline]
    fn bitmap(&mut self) -> &mut Vec<u64, GlobalPhysicalAllocator> {
        &mut self.bitmap
    }
}

/// Return an exclusive reference to the global vaddr allocator.
pub fn global() -> MutexGuard<'static, VirtualAddressAllocator> {
    VADDR_ALLOC.lock()
}
