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

use alloc::vec::Vec;
use devicetree::DeviceTree;

/// The kernel entrypoint for the booting hart. At this point paging is set up.
pub fn main(_fdt: &DeviceTree<'_>) {
    log::info!("{}", pmem::alloc_stats());

    for i in 0..10 {
        log::info!("start {}", i);
        let mut x = Vec::<u8>::with_capacity(9 * 1024 * 1024);
        let mut y = Vec::<u8>::with_capacity(16 * 1024);

        x.push(i);
        assert!(x[0] == i);
        y.push(i + 1);
        assert!(y[0] == i + 1);
    }

    log::info!("{}", pmem::alloc_stats());
    sbi::system::shutdown();
}

/// The entry point for each new hart that is not the boot hart.
pub fn hmain() {}
