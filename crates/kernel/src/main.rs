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
    use page::PageTable;

    log::info!("{}", pmem::alloc_stats());

    // 0x9cfc5000

    for idx in 0..1_00_000_000 {
        use core::ptr::NonNull;
        let mut addr = [NonNull::dangling(); 256];
        for idx in 0..256 {
            let p = pmem::alloc_order(0).unwrap();
            addr[idx] = p;
            log::debug!("alloc {:p}", p);
        }

        for idx in 0..256 {
            log::debug!("free {:p}", addr[idx]);
            unsafe {
                pmem::free_order(addr[idx], 0).unwrap();
            }
            log::debug!("free done");
        }

        //log::info!("at {}", idx);
        //let c = (1 * unit::MIB) / 4096;
        //page::root()
        //.map_alloc(
        //0x0000_AA00_0000.into(),
        //c,
        //page::PageSize::Kilopage,
        //page::Perm::READ | page::Perm::WRITE,
        //)
        //.unwrap();
        ////let mut x = Vec::<u8>::with_capacity(128 * 1024 * 1024);
        ////assert!(x.len() == 0);
        //unsafe {
        //page::root().free(0x0000_AA00_0000.into(), c).unwrap();
        //}
    }

    log::info!("{}", pmem::alloc_stats());
    sbi::system::shutdown();
}

/// The entry point for each new hart that is not the boot hart.
pub fn hmain() {}
