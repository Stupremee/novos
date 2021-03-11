//! Kernel entrypoint and everything related to boot into the kernel

use crate::drivers::{ns16550a, DeviceTreeDriver};
use crate::{
    page::{self, PageSize, PageTable, Perm},
    pmem, unit, KERNEL_PHYS_MEM_BASE, KERNEL_STACK_BASE, KERNEL_STACK_SIZE,
};
use allocator::{order_for_size, size_for_order};
use core::slice;
use devicetree::DeviceTree;

/// The code that sets up memory stuff,
/// allocates a new stack and then runs the real main function.
#[no_mangle]
unsafe extern "C" fn _before_main(_hart_id: usize, fdt2: *const u8) -> ! {
    let fdt = DeviceTree::from_ptr(fdt2).unwrap();

    // try to find a uart device, and then set it as the global logger
    if let Some(uart) = ns16550a::Device::from_chosen(fdt.chosen()) {
        match log::init_log(uart) {
            Ok(_) => log::info!(
                "{} the global logging system using UART.",
                "Initialized".green()
            ),
            // special case, we will instantly shutdown if we failed to initialize the logger
            // this is a pseudo panic since panic wont print anything
            Err(mut uart) => {
                use core::fmt::Write;

                // FIXME: Use colors here
                write!(
                    uart,
                    "Failed to initialize the global logger. Shutting down..."
                )
                .unwrap();
                sbi::system::fail_shutdown();
            }
        }
    }

    // initialize the physmem allocator
    pmem::init(&fdt).unwrap();

    // copy the devicetree to a newly allocated physical page
    let fdt_order = order_for_size(fdt.total_size() as usize);
    let new_fdt = pmem::alloc_order(fdt_order).unwrap();
    assert_ne!(
        (new_fdt.as_ptr() as usize) >> 12,
        (&fdt as *const _ as usize) >> 12,
    );

    let new_fdt = slice::from_raw_parts_mut(new_fdt.as_ptr(), size_for_order(fdt_order));
    let fdt = fdt.copy_to_slice(new_fdt);

    let mut table = page::sv39::Table::new();

    // get all available physical memory from the devicetree and map it
    // at the physmem base
    let phys_mem = fdt.memory().regions().next().unwrap();
    for page in (phys_mem.start()..phys_mem.end()).step_by(2 * unit::MIB) {
        let vaddr = page + KERNEL_PHYS_MEM_BASE;
        log::info!("Mapping physmem {:#x?} to {:#x?}", page, vaddr);
        table
            .map(
                page.into(),
                vaddr.into(),
                PageSize::Megapage,
                Perm::READ | Perm::WRITE,
            )
            .unwrap();
    }

    table
        .map_alloc(
            KERNEL_STACK_BASE.into(),
            KERNEL_STACK_SIZE / allocator::PAGE_SIZE,
            PageSize::Kilopage,
            Perm::READ,
        )
        .unwrap();

    log::info!("{:#x?}", table.translate(KERNEL_STACK_BASE.into()));

    sbi::system::shutdown()
}

/// The entrypoint for the whole kernel.
///
/// `a0` = hart id
/// `a1` = pointer to device tree
#[naked]
#[no_mangle]
#[link_section = ".text.init"]
unsafe extern "C" fn _boot() -> ! {
    asm!(
        // ---------------------------------
        // Load the global pointer into
        // the `gp` register
        // ---------------------------------
        ".option push",
        ".option norelax",
        "    la gp, __global_pointer$",
        ".option pop",
        // ---------------------------------
        // Disable interrupts
        // ---------------------------------
        "csrw sie, zero",
        "csrci sstatus, 2",
        // ---------------------------------
        // Set `bss` to zero
        // ---------------------------------
        "    la t0, __bss_start",
        "    la t1, __bss_end",
        "    bgeu t0, t1, zero_bss_done",
        "zero_bss:",
        "    sd zero, (t0)",
        "    addi t0, t0, 8",
        "zero_bss_done:",
        // ---------------------------------
        // Initialize stack.
        // ---------------------------------
        "    la sp, __stack_end",
        // ---------------------------------
        // Jump into rust code
        // ---------------------------------
        "j _before_main",
        options(noreturn)
    )
}
