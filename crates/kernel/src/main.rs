#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![allow(clippy::missing_safety_doc, clippy::identity_op)]
#![no_std]
#![no_main]
#![feature(asm, naked_functions)]

mod boot;

#[panic_handler]
fn _p(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
