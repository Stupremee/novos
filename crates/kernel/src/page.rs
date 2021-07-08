//! Implementation of the different page table structures
//! and related types.

mod types;
pub use types::*;

pub mod modes;

use crate::{
    allocator,
    memmap::phys2virt,
    pmem::{self, Box, GlobalPhysicalAllocator, Vec},
};
use core::{fmt, marker::PhantomData, ops, ptr::NonNull};
use riscv::{csr::satp, sync::MutexGuard};

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::modes::Sv39 {}
    impl Sealed for super::modes::Sv48 {}
}

pub type Result<T> = core::result::Result<T, Error>;
pub type KernelPageTable = PageTable<modes::Sv48>;

/// Errors that are related to paging.
#[derive(Debug)]
pub enum Error {
    UnsupportedPageSize,
    InvalidAddress,
    UnalignedAddress,
    AlreadyMapped,
    Alloc(allocator::Error),
}

/// A trait that represents any supported paging mode, and is used in a [`PageTable`] to specify
/// the paging mode to use.
pub unsafe trait PagingMode: sealed::Sealed {
    /// The number of page table levels this paging mode supports.
    const LEVELS: usize;

    /// The largest page size this mode supports.
    const TOP_LEVEL_SIZE: PageSize;

    /// The mode that will be put into the satp CSR.
    const SATP_MODE: satp::Mode;

    /// The maximum address that is possible to map.
    const MAX_ADDRESS: usize;
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
        entries: Box<[Entry; 512]>,
        subtables: Vec<NonNull<[Entry; 512]>>,
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
        *mut [Entry; 512],
        (*mut NonNull<[Entry; 512]>, usize, usize),
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
    /// Construct a value that is ready to be written into the satp CSR.
    pub fn satp(&self) -> satp::Satp {
        satp::Satp {
            asid: 0,
            mode: M::SATP_MODE,
            root_table: self.entries.as_ptr() as u64,
        }
    }

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
        // check if this paging mode supports the given pagesize
        if size.vpn_idx() >= M::LEVELS {
            return Err(Error::UnsupportedPageSize);
        }

        // validate the addresses
        if usize::from(vaddr) >= M::MAX_ADDRESS || usize::from(vaddr) & !M::MAX_ADDRESS != 0 {
            return Err(Error::InvalidAddress);
        }

        // verify the given addresses
        if !size.is_aligned(paddr.into()) || !size.is_aligned(vaddr.into()) {
            return Err(Error::UnalignedAddress);
        }

        // go through each page level in the virtual address
        let mut table = &mut *self.entries;
        for vpn_i in (size.vpn_idx() + 1..M::LEVELS).rev() {
            // get the current Vpn and the according entry
            let vpn = Self::vpn(vaddr, vpn_i);
            let entry = &mut table[vpn];

            match entry.kind() {
                // the address is already mapped, throw an error
                Some(EntryKind::Leaf) => return Err(Error::AlreadyMapped),
                // this entry points to the next table, so traverse the next level
                Some(EntryKind::Branch(next)) => {
                    let next = phys2virt(next);
                    table = unsafe { next.as_ptr::<[Entry; 512]>().as_mut().unwrap() };
                }
                // this entry is empty, so we need to point it to a new table
                None => {
                    // the entry is empty, so we allocate a new table and turn this entry
                    // into a branch to the new table
                    let table_ptr = pmem::zalloc_order(0)
                        .map_err(Error::Alloc)?
                        .as_ptr()
                        .cast::<[Entry; 512]>();

                    // update the current entry to point to the new page
                    entry.0 = ((table_ptr as usize as u64) >> 2) | Entry::VALID;
                    self.subtables
                        .push(unsafe { NonNull::new_unchecked(table_ptr) });

                    // traverse the newly allocated table
                    table = unsafe {
                        phys2virt(table_ptr)
                            .as_ptr::<[Entry; 512]>()
                            .as_mut()
                            .unwrap()
                    };
                }
            }
        }

        // get the entry which we need to overwrite
        let last_vpn = Self::vpn(vaddr, size.vpn_idx());
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
            _ => unimplemented!(),
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
        let (_, page_size, _) = self.translate(vaddr).ok_or(Error::InvalidAddress)?;
        let end = usize::from(vaddr) + (page_size.size() * count);

        // the order that will be used for the buddy allocator for freeing the pages
        let order = match page_size {
            PageSize::Kilopage => 0,
            PageSize::Megapage => 9,
            _ => unimplemented!(),
        };

        // loop through the rest of the pages and deallocate them too
        for page in (usize::from(vaddr)..end).step_by(page_size.size()) {
            // translate the address to find the physaddr which we need for deallocation
            let (paddr, _, _) = self.translate(page.into()).unwrap();

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
    pub fn translate(&self, vaddr: VirtAddr) -> Option<(PhysAddr, PageSize, Flags)> {
        self.traverse(vaddr).map(|Mapping { entry, size, .. }| {
            // read the PTE from the found address
            let entry = unsafe { entry.as_ref().unwrap() };

            // get the page offset from the virtual address
            let off = usize::from(vaddr);
            let off = match size {
                PageSize::Kilopage => off & 0xFFF,
                PageSize::Megapage => off & 0x1F_FFFF,
                PageSize::Gigapage => off & 0x3FFF_FFFF,
                PageSize::Terapage => off & 0x7F_FFFF_FFFF,
            };

            // get the physical page number specified by the PTE
            // and return the PPN plus the page offset
            let ppn = PhysAddr::from((entry.0 as usize >> 10) << 12);
            (ppn.offset(off), size, entry.flags())
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
            let entry = &table[Self::vpn(vaddr, idx)];

            match entry.kind()? {
                // we found a mapped address, so break the loop
                EntryKind::Leaf => break PhysAddr::from(entry as *const _),
                EntryKind::Branch(new_table_ptr) => {
                    // this entry points to the next level, so traverse the next level
                    let new_table = phys2virt(new_table_ptr.as_ptr::<u8>());
                    table = unsafe { new_table.as_ptr::<[Entry; 512]>().as_ref().unwrap() };

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

    fn vpn(vaddr: VirtAddr, idx: usize) -> usize {
        (usize::from(vaddr) >> (12 + idx * 9)) & 0x1FF
    }
}

fn set_vpn(vaddr: VirtAddr, idx: usize, val: usize) -> VirtAddr {
    let vaddr = usize::from(vaddr);

    let mask = (1 << 9) - 1;

    let shamt = idx * 9;
    let val = (val & mask) << (shamt + 12);
    let mask = mask << (shamt + 12);

    let vaddr = (vaddr & !mask) | val;
    VirtAddr::from(vaddr)
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
    table: &'page [Entry; 512],
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
                            PageSize::Terapage => 'T',
                        },
                        set_vpn(self.addr, self.size.vpn_idx(), idx),
                        (entry.0 >> 10 << 12) as usize as *const u8,
                        entry.flags(),
                    )?;
                }
                Some(EntryKind::Branch(next)) => {
                    // get access to the next table
                    let table =
                        unsafe { phys2virt(next).as_ptr::<[Entry; 512]>().as_ref().unwrap() };

                    // walk down the table by deubg printing the new table
                    let debug = DebugPageTable {
                        table,
                        size: self.size.step().unwrap(),
                        addr: set_vpn(self.addr, self.size.vpn_idx(), idx),
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
