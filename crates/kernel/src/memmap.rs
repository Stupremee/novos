//! Module for working with the virtual memory map of the kernel.

use crate::page::{PhysAddr, VirtAddr};
use core::sync::atomic::{AtomicUsize, Ordering};

/// The address at which the higher half of the address space begins, and which is used as the base
/// for everything.
pub const HIGHER_HALF_START: usize = 0x4000_0000_0000;

/// The virtual address at which the physical memory is mapped in, such that adding
/// this constant to any "real" physaddr returns the new physaddr which can be used if
/// paging is activaed.
pub const KERNEL_PHYS_MEM_BASE: usize = HIGHER_HALF_START + 0x0A00_0000_0000;

/// The base virtual addresses where the stack for every hart is located.
pub const KERNEL_STACK_BASE: usize = HIGHER_HALF_START + 0x0B00_0000_0000;
/// The stack size for each hart.
pub const KERNEL_STACK_SIZE: usize = 1024 * 1024;

/// The base virtual address where the allocator will start allocating virtual memory.
pub const KERNEL_VMEM_ALLOC_BASE: usize = HIGHER_HALF_START + 0x0C00_0000_0000;

static PHYSICAL_MEMORY_OFFSET: AtomicUsize = AtomicUsize::new(0);

/// Set the global physical memory offset that is used for converting virt addresses to physical
/// addresses.
pub unsafe fn set_phymem_offset(offset: usize) {
    PHYSICAL_MEMORY_OFFSET.store(offset, Ordering::Relaxed);
}

/// Convert a physical address into a virtual address using the physical memory offset.
pub fn phys2virt(paddr: impl Into<PhysAddr>) -> VirtAddr {
    let paddr: usize = paddr.into().into();
    VirtAddr::from(paddr + PHYSICAL_MEMORY_OFFSET.load(Ordering::Relaxed))
}
