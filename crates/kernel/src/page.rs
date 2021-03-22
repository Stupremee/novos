//! Implementation of the different page table structures
//! and related types.

mod types;
pub use types::*;

pub mod sv39;

use crate::boot::KERNEL_PHYS_MEM_BASE;
use crate::{allocator, pmem};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use riscv::sync::{Mutex, MutexGuard};

displaydoc_lite::displaydoc! {
    /// Errors that are related to paging.
    #[derive(Debug)]
    pub enum Error {
        /// address not aligned
        UnalignedAddress,
        /// address already mapped
        AlreadyMapped,
        /// {_0}
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
            PageSize::Megapage => 9,
            PageSize::Gigapage => unimplemented!(),
        };

        // loop through the whole mapping and map every required page
        for vaddr in (vaddr.into()..end).step_by(page_size.size()) {
            // alloc the page
            let page = pmem::alloc_order(order).map_err(Error::Alloc)?;
            let paddr = PhysAddr::from(page.as_ptr());

            // map the new page
            self.map(paddr, vaddr.into(), page_size, perm)?;

            riscv::asm::sfence(vaddr, None);
        }

        Ok(())
    }

    /// Free a virtual allocation that was previously allocated by the `map_alloc` method.
    ///
    /// The count *must* be the same number used for allocation.
    unsafe fn free(&mut self, vaddr: VirtAddr, count: usize) -> Result<(), Error> {
        // translate the first page manually, to get the page size
        let (_, page_size) = self.translate(vaddr).unwrap();
        let end = usize::from(vaddr) + (page_size.size() * count);

        // the order that will be used for the buddy allocator for freeing the pages
        let order = match page_size {
            PageSize::Kilopage => 0,
            PageSize::Megapage => 9,
            PageSize::Gigapage => unimplemented!(),
        };

        // loop through the rest of the pages and deallocate them too
        for page in (usize::from(vaddr)..end).step_by(page_size.size()) {
            // translate the address to find the physaddr which we need for deallocation
            let (paddr, _) = self.translate(page.into()).unwrap();

            // unmap the page
            assert!(self.unmap(page.into())?);

            riscv::asm::sfence(usize::from(vaddr), None);

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

    // FIXME: This is currently safe, since this is the only access to satp.
    // However in the future there must be some global lock to provide
    // safe access the the global page_table.
    let mode = riscv::csr::satp::read().mode;

    // if paging is not enabled, return the physical address.
    if matches!(mode, riscv::csr::satp::Mode::Bare) {
        return paddr.into();
    }

    VirtAddr::from(paddr + KERNEL_PHYS_MEM_BASE)
}

// A dummy mutex that ensures exclusive access to the page table
// stored in `satp`.
static ROOT_TABLE_LOCK: Mutex<()> = Mutex::new(());

/// Check if paging is enabled.
pub fn enabled() -> bool {
    !matches!(riscv::csr::satp::read().mode, riscv::csr::satp::Mode::Bare)
}

/// Get exclusive access to the global page table.
pub fn root() -> TableGuard {
    let table = riscv::csr::satp::read().root_table;
    TableGuard {
        table: unsafe { &mut *(table as *mut _) },
        _guard: ROOT_TABLE_LOCK.lock(),
    }
}

/// Structure that protects access to the global page table.
pub struct TableGuard {
    table: &'static mut sv39::Table,
    _guard: MutexGuard<'static, ()>,
}

impl Deref for TableGuard {
    type Target = sv39::Table;

    fn deref(&self) -> &Self::Target {
        self.table
    }
}

impl DerefMut for TableGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.table
    }
}
