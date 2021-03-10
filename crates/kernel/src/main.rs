#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]
#![no_main]
#![feature(asm, naked_functions, panic_info_message, exclusive_range_pattern)]

pub mod drivers;
pub mod pmem;
pub mod unit;

mod boot;
mod panic;
