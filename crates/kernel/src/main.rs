#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]
#![no_main]
#![feature(asm, naked_functions, panic_info_message, exclusive_range_pattern)]
#![allow(clippy::missing_safety_doc)]

pub mod drivers;
pub mod page;
pub mod pmem;
pub mod unit;

mod boot;
mod panic;

/// The base virtual addresses where the stacks for every hart are located.
pub const KERNEL_STACK_BASE: usize = 0x000A_AAA0_0000;

/// The stack size for each hart.
pub const KERNEL_STACK_SIZE: usize = 1024 * 1024;

/// The virtual address at which the physical memory is mapped in, such that adding
/// this constant to any "real" physaddr returns the new physaddr which can be used if
/// paging is activaed.
pub const KERNEL_PHYS_MEM_BASE: usize = 0x001F_FF00_0000;
