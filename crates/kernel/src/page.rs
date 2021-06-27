//! Implementation of the different page table structures
//! and related types.

mod types;
pub use types::*;

pub mod modes;

use crate::{
    allocator,
    boot::KERNEL_PHYS_MEM_BASE,
    pmem::{self, GlobalPhysicalAllocator},
};
use core::{fmt, marker::PhantomData, ptr::NonNull};

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::modes::Sv39 {}
}

type Box<T> = alloc::boxed::Box<T, GlobalPhysicalAllocator>;
type Vec<T> = alloc::vec::Vec<T, GlobalPhysicalAllocator>;
type RawPageTable = [Entry; 512];

pub type Result<T> = core::result::Result<T, Error>;
pub type KernelPageTable = PageTable<modes::Sv39>;

/// Errors that are related to paging.
#[derive(Debug)]
pub enum Error {
    InvalidAddress,
    UnalignedAddress,
    AlreadyMapped,
    Alloc(allocator::Error),
}

/// Convert a physical address into a virtual address.
pub fn phys2virt(paddr: impl Into<PhysAddr>) -> VirtAddr {
    let paddr: usize = paddr.into().into();

    // FIXME: This is currently safe, since this is the only access to satp.
    // However in the future there must be some global lock to provide
    // safe access the the global page_table.
    let mode = unsafe { riscv::csr::satp::read().mode };

    // if paging is not enabled, return the physical address.
    if matches!(mode, riscv::csr::satp::Mode::Bare) {
        return paddr.into();
    }

    VirtAddr::from(paddr + KERNEL_PHYS_MEM_BASE)
}

/// A trait that represents any supported paging mode, and is used in a [`PageTable`] to specify
/// the paging mode to use.
pub unsafe trait PagingMode: sealed::Sealed {
    /// The number of page table levels this paging mode supports.
    const LEVELS: usize;

    /// The largest page size this mode supports.
    const TOP_LEVEL_SIZE: PageSize;

    /// Check if the two addresses are valid to be mapped for the given page size.
    fn validate_addresses(paddr: PhysAddr, vaddr: VirtAddr, size: PageSize) -> Result<()>;

    /// Get the VPN with the given index.
    fn vpn(vaddr: VirtAddr, idx: usize) -> usize;

    /// Set the VPN in the given virtual and return the new address.
    fn set_vpn(vaddr: VirtAddr, idx: usize, val: usize) -> VirtAddr;
}

/// Generic representation of a page table that can support any paging mode.
pub struct PageTable<M> {
    entries: Box<[Entry; 512]>,
    subtables: Vec<NonNull<[Entry; 512]>>,
    _mode: PhantomData<M>,
}

impl<M> PageTable<M> {
    /// Create a new, empty page table.
    pub fn new() -> Self {
        Self {
            entries: Box::new_in([Entry::ZERO; 512], GlobalPhysicalAllocator),
            subtables: Vec::new_in(GlobalPhysicalAllocator),
            _mode: PhantomData,
        }
    }
}

impl<M: PagingMode> PageTable<M> {
    /// Return a debug printable version of this table.
    pub fn debug(&self) -> impl fmt::Debug + '_ {
        DebugPageTable {
            table: &*self.entries,
            size: M::TOP_LEVEL_SIZE,
            addr: VirtAddr::from(0),
            _mode: PhantomData::<M>,
        }
    }

    /// Map a physical address to a virtual address using a given page size, with the given flags.
    pub fn map(
        &mut self,
        paddr: PhysAddr,
        vaddr: VirtAddr,
        size: PageSize,
        flags: Flags,
    ) -> Result<()> {
        // validate the addresses
        M::validate_addresses(paddr, vaddr, size)?;

        // go through each page level in the virtual address
        let mut table = &mut *self.entries;
        for vpn_i in (size.vpn_idx() + 1..M::LEVELS).rev() {
            // get the current Vpn and the according entry
            let vpn = M::vpn(vaddr, vpn_i);
            let entry = &mut table[vpn];

            match entry.kind() {
                // the address is already mapped, throw an error
                Some(EntryKind::Leaf) => return Err(Error::AlreadyMapped),
                // this entry points to the next table, so traverse the next level
                Some(EntryKind::Branch(next)) => {
                    let next = phys2virt(next);
                    table = unsafe { next.as_ptr::<RawPageTable>().as_mut().unwrap() };
                }
                // this entry is empty, so we need to point it to a new table
                None => {
                    // the entry is empty, so we allocate a new table and turn this entry
                    // into a branch to the new table
                    let new_table = Box::new_in([Entry::ZERO; 512], GlobalPhysicalAllocator);
                    let table_ptr = Box::into_raw(new_table);

                    // update the current entry to point to the new page
                    entry.0 = ((table_ptr as usize as u64) >> 2) | Entry::VALID;
                    self.subtables
                        .push(unsafe { NonNull::new_unchecked(table_ptr) });

                    // traverse the newly allocated table
                    table = unsafe { table_ptr.as_mut().unwrap() };
                }
            }
        }

        // get the entry which we need to overwrite
        let last_vpn = M::vpn(vaddr, size.vpn_idx());
        let entry = &mut table[last_vpn];

        // if the entry is a leaf, aka already mapped, return an error
        if matches!(entry.kind(), Some(EntryKind::Leaf)) {
            return Err(Error::AlreadyMapped);
        }

        // if we reach this point, `table` is the table where the mapping should be created,
        // and `last_idx` is the index inside the table where the mapping should be placed
        //
        // so just construct the new entry, and insert it
        let ppn = usize::from(paddr) as u64 >> 12;
        let new_entry = (ppn << 10) | flags.bits() as u64 | Entry::VALID;
        entry.0 = new_entry as u64;

        // flush tlb for this page
        riscv::asm::sfence(usize::from(vaddr), None);

        Ok(())
    }

    /// Create a new virtual mapping at `vaddr` for `count` pages of the given page size.
    pub fn map_alloc(
        &mut self,
        vaddr: VirtAddr,
        count: usize,
        page_size: PageSize,
        flags: Flags,
    ) -> Result<()> {
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
            self.map(paddr, vaddr.into(), page_size, flags)?;

            // flush tlb for this page
            riscv::asm::sfence(usize::from(vaddr), None);
        }

        Ok(())
    }

    /// Free a virtual allocation that was previously allocated by the `map_alloc` method.
    ///
    /// The count *must* be the same number used for allocation.
    pub unsafe fn free(&mut self, _vaddr: VirtAddr, _count: usize) -> Result<()> {
        todo!()
    }

    /// Translate the virtual address and return the physical address it's pointing to, and the
    /// size of the mapped page.
    pub fn translate(&self, _vaddr: VirtAddr) -> Option<(PhysAddr, PageSize)> {
        todo!()
    }

    /// Try to unmap the given virtual address.
    ///
    /// The bool indicates if there was a virtual address that was unmapped.
    /// It's `false` if `vaddr` is not mapped.
    pub fn unmap(&mut self, _vaddr: VirtAddr) -> Result<bool> {
        todo!()
    }
}

#[derive(Debug)]
#[repr(transparent)]
struct Entry(u64);

impl Entry {
    const VALID: u64 = 1 << 0;
    const ZERO: Entry = Entry(0);

    fn kind(&self) -> Option<EntryKind> {
        let valid = self.0 & Entry::VALID != 0;
        match (valid, valid && self.flags() == Flags::empty()) {
            (true, true) => {
                let next = ((self.0 as usize >> 10) & 0x0FFF_FFFF_FFFF) << 12;
                let next = PhysAddr::from(next);
                Some(EntryKind::Branch(next))
            }
            (true, false) => Some(EntryKind::Leaf),
            _ => None,
        }
    }

    #[inline]
    fn flags(&self) -> Flags {
        // set the V bit to 0 because it's not part of the flags
        let flags = self.0 as u8 >> 1 << 1;
        Flags::from_bits_truncate(flags)
    }
}

#[derive(Debug)]
enum EntryKind {
    Branch(PhysAddr),
    Leaf,
}

struct DebugPageTable<'page, M: PagingMode> {
    table: &'page RawPageTable,
    size: PageSize,
    addr: VirtAddr,
    _mode: PhantomData<M>,
}

impl<M: PagingMode> fmt::Debug for DebugPageTable<'_, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, entry) in self.table.iter().enumerate() {
            match entry.kind() {
                // invalid entry, just skip this entry
                None => continue,
                Some(EntryKind::Leaf) => {
                    // write out the pagetable entry
                    writeln!(
                        f,
                        "[{}] {:#p} -> {:#p} | {}",
                        match self.size {
                            PageSize::Kilopage => 'K',
                            PageSize::Megapage => 'M',
                            PageSize::Gigapage => 'G',
                        },
                        M::set_vpn(self.addr, self.size.vpn_idx(), idx),
                        (entry.0 >> 10 << 12) as usize as *const u8,
                        entry.flags(),
                    )?;
                }
                Some(EntryKind::Branch(next)) => {
                    // get access to the next table
                    let table =
                        unsafe { phys2virt(next).as_ptr::<RawPageTable>().as_ref().unwrap() };

                    // walk down the table by deubg printing the new table
                    let debug = DebugPageTable {
                        table,
                        size: self.size.step().unwrap(),
                        addr: M::set_vpn(self.addr, self.size.vpn_idx(), idx),
                        _mode: PhantomData::<M>,
                    };
                    fmt::Debug::fmt(&debug, f)?;
                }
            }
        }

        Ok(())
    }
}

// A dummy mutex that ensures exclusive access to the page table
/// Get exclusive access to the global page table.
pub fn root() -> &'static mut PageTable<modes::Sv39> {
    panic!()
}
