//! Function to access the SBI IPI extension functionality.

use super::{Error, HartMask, SbiResult};

/// The unique id of the IPI extension.
pub const EXTENSION_ID: u32 = 0x735049;

/// Send an inter-process interrupt to all harts defined by the mask.
pub fn send_ipi(mask: HartMask) -> SbiResult<()> {
    let err_code: usize;
    unsafe {
        asm!("ecall",
            inout("a7") EXTENSION_ID => _,
            inout("a6") 0x00 => _,

            inout("a1") mask.base => _,
            inout("a0") mask.mask => err_code,
        );
    }
    Error::from_sbi_call((), err_code as isize)
}
