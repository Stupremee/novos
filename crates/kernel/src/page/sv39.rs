//! Implementation of the Sv39 virtual memory mode.

use super::{phys2virt, Error, PageSize, Perm, PhysAddr, VirtAddr};
use crate::pmem;
use core::ptr::NonNull;

/// The central structure for managing a page table.
#[repr(C, align(4096))]
pub struct Table {
    entries: [Entry; 512],
}

impl Table {
    /// Create a new, empty page table.
    pub const fn new() -> Self {
        Self {
            entries: [Entry::EMPTY; 512],
        }
    }

    /// Traverse the page table and search for the given virtual address.
    fn traverse(&self, vaddr: VirtAddr) -> Option<Mapping> {
        // get the virtual page numbers
        let vpns = vpns_of_vaddr(vaddr);

        // represent the current table that is walked.
        let mut table = self;
        let mut idx = vpns.len() - 1;

        // we store the level 1 and 2 tables to return them
        let mut table_mib = None;
        let mut table_kib = None;

        let entry = loop {
            // get the entry at this level
            let entry = &table.entries[vpns[idx]];

            match entry.kind()? {
                // we found a mapped address, so break the loop
                EntryKind::Leaf => break PhysAddr::from(entry as *const _),
                EntryKind::Branch(new_table_ptr) => {
                    // this entry points to the next level, so traverse the next level
                    let new_table = phys2virt(new_table_ptr.as_ptr::<u8>());
                    table = unsafe { new_table.as_ptr::<Table>().as_mut().unwrap() };

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

    // Check if all entries in this table are invalid / empty.
    fn is_empty(&self) -> bool {
        self.entries.iter().all(|ent| ent.kind().is_none())
    }

    /// Deallocate the underlying memory of this table through the reference.
    unsafe fn free_mem(&self) -> Result<(), Error> {
        let page = NonNull::new(self as *const _ as *mut u8).unwrap();
        pmem::free(page).map_err(Error::Alloc)
    }
}

impl super::PageTable for Table {
    fn map(
        &mut self,
        paddr: PhysAddr,
        vaddr: VirtAddr,
        size: PageSize,
        perm: Perm,
    ) -> Result<(), Error> {
        // check if the virtual address is below the maximum
        assert!(
            usize::from(vaddr) <= 0x7F_FFFF_FFFF,
            "virtual address exceeded 39 bits"
        );

        // verify the given addresses
        if !size.is_aligned(paddr.into()) || !size.is_aligned(vaddr.into()) {
            return Err(Error::UnalignedAddress);
        }

        // get the virtual page numbers and physical page number
        let vpns = vpns_of_vaddr(vaddr);
        let ppn = ppn_of_paddr(paddr);

        // get the list of indices inside the different levels
        let (indices, last_idx) = match size {
            PageSize::Kilopage => (&vpns[1..], vpns[0]),
            PageSize::Megapage => (&vpns[2..], vpns[1]),
            PageSize::Gigapage => (&vpns[3..], vpns[2]),
        };

        // represent the current table that is walked.
        let mut table = self;

        for &idx in indices.iter().rev() {
            // get the entry at this level
            let entry = &mut table.entries[idx];

            match entry.kind() {
                // the address is already mapped, so return an error
                Some(EntryKind::Leaf) => return Err(Error::AlreadyMapped),
                Some(EntryKind::Branch(new_table)) => {
                    // this entry points to the next level, so traverse the next level
                    let new_table = phys2virt(new_table);
                    table = unsafe { new_table.as_ptr::<Table>().as_mut().unwrap() };
                }
                None => {
                    // the entry is empty, so we allocate a new table and turn this entry
                    // into a branch to the new table
                    let page_ptr = pmem::zalloc().map_err(Error::Alloc)?.as_ptr();
                    let page = page_ptr as u64;

                    // update the current entry to point to the new page
                    entry.set((page >> 2) | Entry::VALID);

                    // traverse the newly allocated table
                    table = unsafe { phys2virt(page_ptr).as_ptr::<Table>().as_mut().unwrap() };
                }
            }
        }

        // if the entry is a leaf, aka already mapped, return an error
        let entry = &mut table.entries[last_idx];
        if matches!(entry.kind(), Some(EntryKind::Leaf)) {
            return Err(Error::AlreadyMapped);
        }

        // if we reach this point, `table` is the table where the mapping should be created,
        // and `last_idx` is the index inside the table where the mapping should be placed
        //
        // so just construct the new entry, and insert it
        let new_entry = (ppn << 10) | (usize::from(perm) << 1) | Entry::VALID as usize;
        entry.set(new_entry as u64);

        Ok(())
    }

    fn unmap(&mut self, vaddr: VirtAddr) -> Result<bool, Error> {
        // we may need the vpns later for freeing the table
        let vpn = vpns_of_vaddr(vaddr);

        let Mapping {
            table_kib,
            table_mib,
            entry,
            ..
        } = match self.traverse(vaddr) {
            Some(x) => x,
            // there's no mapping the for given address
            None => return Ok(false),
        };

        // clear the entry by zeroing it
        unsafe {
            core::ptr::write_volatile(entry, Entry::EMPTY);
        }

        // if we have a level 2 table, check if we can free the table
        if let Some(table) = table_kib {
            // get a rust reference to the table
            let table_ref = unsafe { &mut *phys2virt(table).as_ptr::<Table>() };

            if table_ref.is_empty() {
                // if we free this table,
                // we also need to remove the entry from the level 1 table
                let table_mib = unsafe { &mut *phys2virt(table_mib.unwrap()).as_ptr::<Table>() };
                table_mib.entries[vpn[1]].set(0);

                unsafe {
                    pmem::free(NonNull::new(table.as_ptr()).unwrap()).map_err(Error::Alloc)?;
                }
            }
        }

        // now try to free the level 1 table
        if let Some(table) = table_mib {
            // get a rust reference to the table
            let table_ref = unsafe { &mut *phys2virt(table).as_ptr::<Table>() };

            if table_ref.is_empty() {
                // if we free this table,
                // we also need to remove the entry from the level 0 table
                self.entries[vpn[2]].set(0);

                unsafe {
                    pmem::free(NonNull::new(table.as_ptr()).unwrap()).map_err(Error::Alloc)?;
                }
            }
        }

        Ok(true)
    }

    fn translate(&self, vaddr: VirtAddr) -> Option<(PhysAddr, PageSize)> {
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
}

/// Structure that stores the result of a traverse operation.
///
/// The tables are used inside unmap to free them if neccessary.
#[derive(Debug)]
struct Mapping {
    table_mib: Option<PhysAddr>,
    table_kib: Option<PhysAddr>,

    entry: *mut Entry,
    size: PageSize,
}

/// A Sv39 page table entry.
#[derive(Debug)]
#[repr(transparent)]
pub struct Entry(u64);

impl Entry {
    /// An empty PTE.
    pub const EMPTY: Entry = Entry(0);

    /// The `V` bit of a PTE.
    pub const VALID: u64 = 1 << 0;
    /// The `U` bit of a PTE.
    pub const USER: u64 = 1 << 4;
    /// The `G` bit of a PTE.
    pub const GLOBAL: u64 = 1 << 5;

    /// Get the kind of this entry.
    pub fn kind(&self) -> Option<EntryKind> {
        match (self.valid(), self.branch()) {
            (true, true) => {
                let next = ((self.0 as usize >> 10) & 0x0FFF_FFFF_FFFF) << 12;
                let next = PhysAddr::from(next);
                Some(EntryKind::Branch(next))
            }
            (true, false) => Some(EntryKind::Leaf),
            _ => None,
        }
    }

    /// Check if this PTE is a branch to the next level.
    #[inline]
    pub fn branch(&self) -> bool {
        self.perm() == Perm::from(0u8) && self.valid()
    }

    /// Set the raw value of this entry to the given value.
    #[inline]
    pub fn set(&mut self, x: u64) {
        self.0 = x;
    }

    /// Get the raw value of this entry.
    #[inline]
    pub fn get(&self) -> u64 {
        self.0
    }

    /// Check the `V` bit of this PTE.
    #[inline]
    pub fn valid(&self) -> bool {
        self.0 & Entry::VALID != 0
    }

    /// Return the permissions for this PTE.
    #[inline]
    pub fn perm(&self) -> Perm {
        let perm = (self.0 >> 1) & 0b111;
        Perm::from(perm as u8)
    }

    /// Check if this PTE is accessible from U-Mode.
    #[inline]
    pub fn user(&self) -> bool {
        self.0 & Entry::USER != 0
    }

    /// Check if this PTE is global mapping.
    #[inline]
    pub fn global(&self) -> bool {
        self.0 & Entry::GLOBAL != 0
    }
}

/// Represents the different kinds of page table entries.
#[derive(Debug)]
pub enum EntryKind {
    /// This entry points to the entry in the next level.
    Branch(PhysAddr),
    /// This entry is a leaf and can directly be used to translate an address.
    Leaf,
}

fn vpns_of_vaddr(vaddr: VirtAddr) -> [usize; 3] {
    const MASK: usize = 0x1FF;

    let vaddr = usize::from(vaddr);
    [
        (vaddr >> 12) & MASK,
        (vaddr >> 21) & MASK,
        (vaddr >> 30) & MASK,
    ]
}

fn ppn_of_paddr(paddr: PhysAddr) -> usize {
    (usize::from(paddr) >> 12) & 0x0FFF_FFFF_FFFF
}
