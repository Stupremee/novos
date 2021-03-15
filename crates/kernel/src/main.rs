#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]
#![no_main]
#![feature(
    asm,
    naked_functions,
    panic_info_message,
    exclusive_range_pattern,
    int_bits_const,
    alloc_error_handler
)]
#![allow(clippy::missing_safety_doc, clippy::empty_loop)]

extern crate alloc;

pub mod allocator;
pub mod boot;
pub mod drivers;
pub mod hart;
pub mod page;
pub mod pmem;
pub mod unit;
pub mod vmem;

mod panic;

mod static_cell;
pub use static_cell::StaticCell;

use alloc::vec::Vec;
use devicetree::DeviceTree;

/// The kernel entrypoint for the booting hart. At this point paging is set up.
pub fn main(_fdt: &DeviceTree<'_>) {
    log::info!("hey from main");

    //let mut x = Vec::with_capacity(1024 * 1024 * 1024);
    //x.push(42u32);
    //log::info!("v: {:?}", x);
    loop {
        pmem::alloc().unwrap();
    }
}

/// The entry point for each new hart that is not the boot hart.
pub fn hmain() {}
