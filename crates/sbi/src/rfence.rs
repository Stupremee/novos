//! Functions to access the SBI RFENCE extension functionality.

use super::{Error, HartMask, SbiResult};

/// The unique id of the IPI extension.
pub const EXTENSION_ID: u32 = 0x52464E43;

fn sbi_call(fid: u32, mask: HartMask, start: usize, size: usize) -> SbiResult<()> {
    let err_code: usize;
    unsafe {
        asm!("ecall",
            inout("a7") EXTENSION_ID => _,
            inout("a6") fid => _,

            inout("a0") mask.mask => err_code,
            inout("a1") mask.base => _,
            inout("a2") start => _,
            inout("a3") size => _,
        );
    }
    Error::from_sbi_call((), err_code as isize)
}

/// Instructs the harts specified by `mask` to execute a FENCE.I instruction.
pub fn fence_i(mask: HartMask) -> SbiResult<()> {
    sbi_call(0x00, mask, 0, 0)
}

/// Instructs the harts specified by `mask` to execute SFENCE.VMA instructions, covering the range
/// from `start` with `size` bytes.
pub fn sfence_vma(mask: HartMask, start: usize, size: usize) -> SbiResult<()> {
    sbi_call(0x00, mask, start, size)
}
