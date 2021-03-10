//! Implementation of the different page table structures
//! and related types.

mod types;
pub use types::*;

pub mod sv39;

displaydoc_lite::displaydoc! {
    /// Errors that are related to paging.
    #[derive(Debug)]
    pub enum Error {
        /// tried to map an address that is not aligned to the page size
        UnalignedAddress,
        /// tried to map a page size that is not supported
        UnsupportedPageSize,
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

    /// Try to unmap the given virtual address.
    ///
    /// The bool indicates if there was a virtual address that was unmapped.
    /// It's `false` if `vaddr` is not mapped.
    fn unmap(&mut self, vaddr: VirtAddr) -> Result<bool, Error>;

    /// Translate the virtual address and return the physical address it's pointing to, and the
    /// size of the mapped page.
    fn translate(&self, vaddr: VirtAddr) -> Option<(PhysAddr, PageSize)>;
}
