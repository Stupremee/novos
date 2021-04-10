//! All legacy extensions and calls.

use crate::{Error, SbiResult};

fn sbi_call(ext: u32, arg: Option<usize>) -> SbiResult<usize> {
    let (value, err_code): (usize, isize);

    unsafe {
        asm!("ecall",
            inout("a7") ext => _,
            inout("a0") arg.unwrap_or(0) => err_code,
            out("a1") value,
        );
    }

    Error::from_sbi_call(value, err_code)
}

/// Prints a byte to the output console.
pub fn put_char(c: char) -> SbiResult<()> {
    sbi_call(0x01, Some(c as u32 as usize)).map(|_| ())
}
