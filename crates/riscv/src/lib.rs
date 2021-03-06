//! Common components that are used across multiple crates
//! of windy.
#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]
#![feature(asm)]
#![allow(clippy::missing_safety_doc, clippy::empty_loop)]

pub mod asm;
pub mod csr;
pub mod sync;
pub mod trap;
