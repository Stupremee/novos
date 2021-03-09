use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    unsafe {
        core::ptr::write_volatile(0x1000_0000 as *mut u8, b'*');
    }
    loop {}
}
