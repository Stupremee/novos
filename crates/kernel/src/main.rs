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

mod static_cell;
pub use static_cell::StaticCell;

use devicetree::DeviceTree;

/// The kernel entrypoint for the booting hart. At this point paging is set up.
pub fn main(_hart: usize, _fdt: &DeviceTree<'_>) {
    log::info!("hey from main");
}
