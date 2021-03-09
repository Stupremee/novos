//! Logging system for the kernel.

#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![feature(unsize, ptr_metadata)]
#![no_std]

mod value;
pub use value::Value;

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use super::Value;

    #[test]
    fn it_works() {
        let x = 32u32;
        let y = Value::<dyn core::fmt::Display, 2>::new(x).unwrap();
        assert_eq!(y.to_string(), "32");
    }
}
