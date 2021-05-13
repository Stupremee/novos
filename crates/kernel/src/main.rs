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
    thread_local
)]
#![allow(clippy::missing_safety_doc, clippy::empty_loop)]

extern crate alloc;

pub mod allocator;
pub mod boot;
pub mod drivers;
pub mod hart;
pub mod page;
pub mod pmem;
pub mod trap;
pub mod unit;
pub mod vmem;

mod panic;

mod static_cell;
pub use static_cell::StaticCell;

use core::fmt;
use devicetree::DeviceTree;

/// The kernel entrypoint for the booting hart. At this point paging is set up.
pub fn main(fdt: &DeviceTree<'_>) -> ! {
    // initialize all devices
    unsafe {
        hart::current().devices().init();
    }

    // initialize the global logging system, if there's a logging device available
    if hart::current().devices().logger().is_some() {
        log::init_log(GlobalLog).map_err(|_| ()).unwrap();
    }

    // print the hello message with some statistics
    let cores = fdt.find_nodes("/cpus/cpu@").count();
    log::info!(
        "{} starting with {} cores and {} physical memory",
        "NovOS".green(),
        cores,
        unit::bytes(pmem::alloc_stats().total),
    );

    log_core_online();

    const ROUNDS: u64 = 1_00_0000;
    log::debug!("{}", pmem::alloc_stats());

    let mut allocated = 0u64;
    let start = riscv::asm::time();
    for _ in 0..ROUNDS {
        unsafe {
            let orig_page = pmem::alloc().unwrap();
            let page = page::phys2virt(orig_page.as_ptr());
            core::ptr::write_volatile(page.as_ptr::<u8>(), 123u8);
            pmem::free(orig_page).unwrap();
        }
        allocated += 1;
    }

    let elapsed = riscv::asm::time() - start;
    let rate = ROUNDS / elapsed.as_secs();
    log::info!(
        "Allocated and freed {} pages in {:?}: {} pages / sec",
        allocated,
        elapsed,
        rate
    );
    log::debug!("{}", pmem::alloc_stats());

    sbi::system::shutdown();
    //loop {
    //riscv::asm::wfi();
    //}
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
    let node = hart::current()
        .fdt()
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

/// A global logger that uses the hart local context to access the Logger.
pub struct GlobalLog;

impl log::Logger for GlobalLog {
    fn write_str(&self, x: &str) -> fmt::Result {
        let devices = hart::current().devices();
        if let Some(dev) = devices.logger() {
            dev.write_str(x)?;
        }

        Ok(())
    }
}
