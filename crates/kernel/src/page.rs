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
use core::{fmt, marker::PhantomData, ops, ptr::NonNull};
use riscv::sync::MutexGuard;

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
    pub entries: Box<[Entry; 512]>,
    pub subtables: Vec<NonNull<[Entry; 512]>>,
    _mode: PhantomData<M>,
}

unsafe impl<M> Send for PageTable<M> {}
unsafe impl<M> Sync for PageTable<M> {}

impl<M> PageTable<M> {
    /// Create a new pagetable from the raw entries pointer and a subtables pointer.
    pub unsafe fn from_raw_parts(
        entries: Box<RawPageTable>,
        subtables: Vec<NonNull<RawPageTable>>,
    ) -> Self {
        Self {
            entries,
            subtables,
            _mode: PhantomData,
        }
    }

    /// Create a new, empty page table.
    pub fn new() -> Self {
        Self {
            entries: Box::new_in([Entry::ZERO; 512], GlobalPhysicalAllocator),
            subtables: Vec::new_in(GlobalPhysicalAllocator),
            _mode: PhantomData,
        }
    }

    /// Turn this pagetable into the raw underlying parts.
    pub fn into_raw_parts(
        self,
    ) -> (
        *mut RawPageTable,
        (*mut NonNull<RawPageTable>, usize, usize),
    ) {
        let mut me = core::mem::ManuallyDrop::new(self);
        (
            me.entries.as_mut_ptr().cast(),
            (
                me.subtables.as_mut_ptr(),
                me.subtables.len(),
                me.subtables.capacity(),
            ),
        )
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

    /// Return the pointer to the root page table that can be inserted into the `satp` register.
    pub fn root_ptr(&self) -> *const () {
        self.entries.as_ptr().cast()
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
                    let table_ptr = pmem::zalloc_order(0)
                        .map_err(Error::Alloc)?
                        .as_ptr()
                        .cast::<RawPageTable>();

                    // update the current entry to point to the new page
                    entry.0 = ((table_ptr as usize as u64) >> 2) | Entry::VALID;
                    self.subtables
                        .push(unsafe { NonNull::new_unchecked(table_ptr) });

                    // traverse the newly allocated table
                    table = unsafe {
                        phys2virt(table_ptr)
                            .as_ptr::<RawPageTable>()
                            .as_mut()
                            .unwrap()
                    };
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
            riscv::asm::sfence(vaddr, None);
        }

        Ok(())
    }

    /// Free a virtual allocation that was previously allocated by the `map_alloc` method.
    ///
    /// The count *must* be the same number used for allocation.
    pub unsafe fn free(&mut self, vaddr: VirtAddr, count: usize) -> Result<()> {
        // translate the first page manually, to get the page size
        let (_, page_size) = self.translate(vaddr).ok_or(Error::InvalidAddress)?;
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

            // flush tlb for this page
            riscv::asm::sfence(usize::from(vaddr), None);

            // deallocate the page
            pmem::free_order(NonNull::new(paddr.as_ptr()).unwrap(), order)
                .map_err(Error::Alloc)?;
        }

        Ok(())
    }

    /// Translate the virtual address and return the physical address it's pointing to, and the
    /// size of the mapped page.
    pub fn translate(&self, vaddr: VirtAddr) -> Option<(PhysAddr, PageSize)> {
        self.traverse(vaddr).map(|Mapping { entry, size, .. }| {
            // read the PTE from the found address
            let entry = unsafe { entry.as_ref().unwrap() };

            // get the page offset from the virtual address
            let off = usize::from(vaddr);
            let off = match size {
                PageSize::Kilopage => off & 0xFFF,
                PageSize::Megapage => off & 0x1FFFFF,
                PageSize::Gigapage => off & 0x3FFFFFFF,
            };

            // get the physical page number specified by the PTE
            // and return the PPN plus the page offset
            let ppn = PhysAddr::from(((entry.0 as usize >> 10) & 0x0FFF_FFFF_FFFF) << 12);
            (ppn.offset(off), size)
        })
    }

    /// Try to unmap the given virtual address.
    ///
    /// The bool indicates if there was a virtual address that was unmapped.
    /// It's `false` if `vaddr` is not mapped.
    pub fn unmap(&mut self, vaddr: VirtAddr) -> Result<bool> {
        let Mapping { entry, .. } = match self.traverse(vaddr) {
            Some(x) => x,
            // there's no mapping the for given address
            None => return Ok(false),
        };

        // clear the entry by zeroing it
        unsafe {
            core::ptr::write_volatile(entry, Entry::ZERO);
        }
        Ok(true)
    }

    /// Traverse the page table and search for the given virtual address.
    fn traverse(&self, vaddr: VirtAddr) -> Option<Mapping> {
        // represent the current table that is walked.
        let mut table = &*self.entries;
        let mut idx = M::LEVELS - 1;

        // we store the level 1 and 2 tables to return them
        let mut table_mib = None;
        let mut table_kib = None;

        let entry = loop {
            // get the entry at this level
            let entry = &table[M::vpn(vaddr, idx)];

            match entry.kind()? {
                // we found a mapped address, so break the loop
                EntryKind::Leaf => break PhysAddr::from(entry as *const _),
                EntryKind::Branch(new_table_ptr) => {
                    // this entry points to the next level, so traverse the next level
                    let new_table = phys2virt(new_table_ptr.as_ptr::<u8>());
                    table = unsafe { new_table.as_ptr::<RawPageTable>().as_ref().unwrap() };

                    // update the level 1 and 2 table variable to return them later
                    match idx {
                        1 => table_kib = Some(new_table_ptr),
                        2 => table_mib = Some(new_table_ptr),
                        _ => {}
                    }
                }
            }

            // check if we reached the last table
            if idx == 0 {
                return None;
            }

            // go to the next level
            idx -= 1;
        };

        Some(Mapping {
            table_mib,
            table_kib,
            entry: entry.as_ptr(),
            size: match idx {
                0 => PageSize::Kilopage,
                1 => PageSize::Megapage,
                2 => PageSize::Gigapage,
                _ => unreachable!(),
            },
        })
    }
}

impl<M> Drop for PageTable<M> {
    fn drop(&mut self) {
        for ptr in self.subtables.drain(..) {
            let _ = unsafe { pmem::free(ptr.cast()) };
        }
    }
}

#[derive(Debug)]
struct Mapping {
    table_mib: Option<PhysAddr>,
    table_kib: Option<PhysAddr>,

    entry: *mut Entry,
    size: PageSize,
}

#[derive(Debug)]
#[repr(transparent)]
pub struct Entry(u64);

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

/// Get exclusive access to the global page table, if there is one.
pub fn root() -> TableGuard {
    TableGuard {
        guard: crate::boot::PAGE_TABLE.lock(),
    }
}

/// Structure that protects access to the global page table.
pub struct TableGuard {
    guard: MutexGuard<'static, Option<KernelPageTable>>,
}

impl ops::Deref for TableGuard {
    type Target = KernelPageTable;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_ref()
            .expect("there is no global page table to take")
    }
}

impl ops::DerefMut for TableGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard
            .as_mut()
            .expect("there is no global page table to take")
    }
}
