#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]
#![no_main]
#![feature(
    asm,
    naked_functions,
    panic_info_message,
    exclusive_range_pattern,
    int_bits_const,
    alloc_error_handler,
    allocator_api
)]
#![allow(clippy::missing_safety_doc, clippy::empty_loop)]

extern crate alloc;

pub mod allocator;
pub mod boot;
pub mod drivers;
pub mod hart;
pub mod interrupt;
pub mod page;
pub mod pmem;
pub mod unit;
pub mod vmem;

mod panic;

mod static_cell;
pub use static_cell::StaticCell;

use devicetree::DeviceTree;

/// The kernel entrypoint for the booting hart. At this point paging is set up.
pub fn main(fdt: &DeviceTree<'_>) {
    // initialize all device drivers
    let mut devices = drivers::DeviceManager::from_devicetree(fdt);
    devices.init();

    sbi::system::shutdown();
}

/// The entry point for each new hart that is not the boot hart.
pub fn hmain() {}
