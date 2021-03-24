use crate::hart;
use core::panic::PanicInfo;

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    // we use an extra guard for printing, to print the whole
    // panic message without interference
    let mut _guard = log::global_log();
    let guard = &mut _guard;

    log::error!(guard = guard; "=========================");
    log::error!(guard = guard; "KERNEL PANIC ON HART {:3}", hart::current().id());
    log::error!(guard = guard; "=========================");

    match (info.location(), info.message()) {
        (Some(loc), Some(msg)) => {
            log::error!(guard = guard; "line {}, file {}: {}", loc.line(), loc.file(), msg)
        }
        (None, Some(msg)) => {
            log::error!(guard = guard; "{}", msg)
        }
        (Some(loc), None) => {
            log::error!(guard = guard; "line {}, file {}", loc.line(), loc.file())
        }
        (None, None) => log::error!(guard = guard; "no information available."),
    }

    drop(_guard);
    if hart::current().is_bsp() {
        sbi::system::fail_shutdown();
    } else {
        loop {
            riscv::asm::wfi()
        }
    }
}

#[alloc_error_handler]
fn alloc_handler(layout: core::alloc::Layout) -> ! {
    panic!(
        "memory allocation of {} bytes and {} alignment failed",
        layout.size(),
        layout.align()
    )
}
