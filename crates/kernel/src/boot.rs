//! Kernel entrypoint and everything related to boot into the kernel

use crate::drivers::{ns16550a, DeviceTreeDriver};
use devicetree::DeviceTree;

/// The code that sets up memory stuff,
/// allocates a new stack and then runs the real main function.
#[no_mangle]
unsafe extern "C" fn _before_main(_hart_id: usize, fdt: *const u8) -> ! {
    let fdt = DeviceTree::from_ptr(fdt).expect("failed to parse device tree");

    // try to find a uart device, and then set it as the global logger
    if let Some(uart) = ns16550a::Uart::from_chosen(fdt.chosen()) {
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
                sbi::system::shutdown();
            }
        }
    }

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
