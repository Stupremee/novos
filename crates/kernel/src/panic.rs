use core::panic::PanicInfo;

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    log::error!("============");
    log::error!("KERNEL PANIC");
    log::error!("============");

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

    sbi::system::fail_shutdown();
}
