#![no_std]
#![no_main]

#[panic_handler]
fn _p(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
