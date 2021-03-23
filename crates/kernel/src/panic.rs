use crate::hart;
use core::panic::PanicInfo;

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    log::error!("=========================");
    log::error!("KERNEL PANIC ON HART {:3}", hart::current().id());
    log::error!("=========================");

    match (info.location(), info.message()) {
        (Some(loc), Some(msg)) => {
            log::error!("line {}, file {}: {}", loc.line(), loc.file(), msg)
        }
        (None, Some(msg)) => {
            log::error!("{}", msg)
        }
        (Some(loc), None) => {
            log::error!("line {}, file {}", loc.line(), loc.file())
        }
        (None, None) => log::error!("no information available."),
    }

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
