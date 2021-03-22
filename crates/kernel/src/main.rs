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

    let start = riscv::asm::time();
    let mut iters = 0usize;

    const SIZE: usize = 8 * unit::MIB;
    loop {
        let mut v = Vec::with_capacity(SIZE / 8);
        v.push(iters);
        assert_eq!(v[0], iters);
        drop(v);

        iters += 1;
        if iters % 1_000_000 == 0 {
            let end = riscv::asm::time();
            let elapsed = end - start;

            log::info!(
                "size {} | iters: {:08} | {} / s",
                unit::bytes(SIZE),
                iters,
                unit::bytes((iters * SIZE) / elapsed.as_secs().max(1) as usize)
            );
        }
    }

    log::info!("{}", pmem::alloc_stats());
    sbi::system::shutdown();
}

/// The entry point for each new hart that is not the boot hart.
pub fn hmain() {}
