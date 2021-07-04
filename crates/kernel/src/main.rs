#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]
#![no_main]
#![feature(
    asm,
    naked_functions,
    panic_info_message,
    exclusive_range_pattern,
    alloc_error_handler,
    allocator_api,
    fn_align,
    thread_local,
    vec_into_raw_parts
)]
#![allow(clippy::missing_safety_doc, clippy::empty_loop)]

extern crate alloc;

pub mod allocator;
pub mod boot;
pub mod drivers;
pub mod hart;
pub mod page;
pub mod pmem;
pub mod symbols;
pub mod trap;
pub mod unit;

mod panic;

mod static_cell;
pub use static_cell::StaticCell;

use core::fmt;
use devicetree::DeviceTree;

/// The kernel entrypoint for the booting hart. At this point paging is set up.
pub fn main(fdt: &DeviceTree<'_>) -> ! {
    // print the hello message with some statistics
    let cores = fdt.find_nodes("/cpus/cpu@").count();
    log::info!(
        "{} starting with {} cores and {} physical memory",
        "NovOS".green(),
        cores,
        unit::bytes(pmem::alloc_stats().total),
    );

    for _ in 0..10_000 {}

    unsafe {
        *(0x1234 as *mut _) = 1u8;
    }

    log_core_online();

    log::debug!("{}", pmem::alloc_stats());

    sbi::system::shutdown()
}

/// The entrypoint for all other harts.
pub fn hmain() -> ! {
    log_core_online();

    loop {
        riscv::asm::wfi();
    }
}

fn log_core_online() {
    // get a human readable representation of the ISA
    let fdt = hart::current().fdt();
    let node = fdt
        .cpus()
        .children()
        .find(|node| node.unit_address() == Some(hart::current().id()));

    let isa = node
        .and_then(|n| n.prop("riscv,isa")?.as_str())
        .unwrap_or("Unknown");

    // get the architecture name
    let arch = match sbi::base::marchid() {
        Ok(0) => "Qemu/SiFive",
        Ok(1) => "Rocket",
        Ok(2) => "BOOM",
        Ok(3) => "Ariane",
        Ok(4) => "RI5CY",
        Ok(5) => "Spike",
        Ok(6) => "E-Class",
        Ok(7) => "ORCA",
        Ok(8) => "SCR1",
        Ok(9) => "YARVI",
        Ok(10) => "RVBS",
        Ok(11) => "SweRV EH1",
        Ok(12) => "MSCC",
        Ok(13) => "BlackParrot",
        Ok(14) => "BaseJump Manycore",
        Ok(15) => "C-Class",
        Ok(16) => "SweRV EL2",
        Ok(17) => "SweRV EH2",
        Ok(18) => "SERV",
        Ok(19) => "NEORV32",
        Ok(_) => "Unknown",
        Err(_) => "Error",
    };

    struct PrintCore;
    impl fmt::Display for PrintCore {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if hart::current().is_bsp() {
                use owo_colors::OwoColorize;
                write!(f, "{} ", "Physical Core".red())
            } else {
                f.write_str("Physical Core ")
            }
        }
    }

    log::info!(
        "{} {} ({} on {}) online",
        PrintCore,
        hart::current().id().green(),
        isa.magenta(),
        arch.blue(),
    );
}

use alloc::alloc::{GlobalAlloc, Layout};

struct MyAllocator;

unsafe impl GlobalAlloc for MyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static A: MyAllocator = MyAllocator;
