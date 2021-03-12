//! Implementation of the different page table structures
//! and related types.

mod types;
pub use types::*;

pub mod sv39;

use crate::{allocator, pmem};
use core::ptr::NonNull;

displaydoc_lite::displaydoc! {
    /// Errors that are related to paging.
    #[derive(Debug)]
    pub enum Error {
        /// tried to map an address that is not aligned to the page size
        UnalignedAddress,
        /// tried to identity map a range using a page size that can't fit into the range
        RangeTooSmall,
        /// tried to map an address which was already mapped
        AlreadyMapped,
        /// failed to allocate a new page: `{_0}`
        Alloc(allocator::Error),
    }
}

/// Represents any kind of page table. Namely, Sv39 and Sv48 page tables at the moment.
pub trait PageTable {
    /// Map a page with the given size, from the physical address `paddr`, to the virtual address
    /// `vaddr`.
    ///
    /// This method will not overwrite any existing mapping, and will fail if this case happens.
    fn map(
        &mut self,
        paddr: PhysAddr,
        vaddr: VirtAddr,
        size: PageSize,
        perm: Perm,
    ) -> Result<(), Error>;

    /// Create a new virtual mapping at `vaddr` for `count` pages of the given page size.
    fn map_alloc(
        &mut self,
        vaddr: VirtAddr,
        count: usize,
        page_size: PageSize,
        perm: Perm,
    ) -> Result<(), Error> {
        // get the end address of the mapping
        let size = page_size.size() * count;
        let end = usize::from(vaddr) + size;

        // the order that will be used for the buddy allocator for ech page size
        let order = match page_size {
            PageSize::Kilopage => 0,
            PageSize::Megapage => 8,
            PageSize::Gigapage => unimplemented!(),
        };

        // loop through the whole mapping and map every required page
        for vaddr in (vaddr.into()..end).step_by(page_size.size()) {
            // alloc the page
            let page = pmem::alloc_order(order).map_err(Error::Alloc)?;
            let paddr = PhysAddr::from(page.as_ptr());

            // map the new page
            self.map(paddr, vaddr.into(), page_size, perm)?;
        }

        Ok(())
    }

    /// Free a virtual allocation that was previously allocated by the `map_alloc` method.
    ///
    /// The count *must* be the same number used for allocation.
    unsafe fn free(&mut self, vaddr: VirtAddr, count: usize) -> Result<(), Error> {
        // translate the first page manually, to get the page size
        let (first_page, page_size) = self.translate(vaddr).unwrap();
        let end = usize::from(vaddr) + (page_size.size() * count);

        // the order that will be used for the buddy allocator for freeing the pages
        let order = match page_size {
            PageSize::Kilopage => 0,
            PageSize::Megapage => 8,
            PageSize::Gigapage => unimplemented!(),
        };

        // unmap and deallocate the now free'd page
        assert!(self.unmap(vaddr)?);
        pmem::free_order(NonNull::new(first_page.as_ptr()).unwrap(), order)
            .map_err(Error::Alloc)?;

        // loop through the rest of the pages and deallocate them too
        for page in (usize::from(vaddr) + page_size.size()..end).step_by(page_size.size()) {
            // translate the address to find the physaddr which we need for deallocation
            let (paddr, _) = self.translate(page.into()).unwrap();

            // unmap the page
            assert!(self.unmap(page.into())?);

            // deallocate the page
            pmem::free_order(NonNull::new(paddr.as_ptr()).unwrap(), order)
                .map_err(Error::Alloc)?;
        }

        Ok(())
    }

    /// Try to unmap the given virtual address.
    ///
    /// The bool indicates if there was a virtual address that was unmapped.
    /// It's `false` if `vaddr` is not mapped.
    fn unmap(&mut self, vaddr: VirtAddr) -> Result<bool, Error>;

    /// Translate the virtual address and return the physical address it's pointing to, and the
    /// size of the mapped page.
    fn translate(&self, vaddr: VirtAddr) -> Option<(PhysAddr, PageSize)>;
}

/// Convert a physical address into a virtual address.
pub fn phys2virt(paddr: impl Into<PhysAddr>) -> VirtAddr {
    let paddr: usize = paddr.into().into();
    VirtAddr::from(paddr + crate::boot::KERNEL_PHYS_MEM_BASE)
}
