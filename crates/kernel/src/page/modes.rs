use super::{PageSize, PagingMode};
use riscv::csr::satp;

/// The Sv39 paging mode which supports 39-bit virtual addresses.
pub enum Sv39 {}

unsafe impl PagingMode for Sv39 {
    const LEVELS: usize = 3;
    const TOP_LEVEL_SIZE: PageSize = PageSize::Gigapage;
    const SATP_MODE: satp::Mode = satp::Mode::Sv39;
    const MAX_ADDRESS: usize = 0x7F_FFFF_FFFF;
}

/// The Sv48 paging mode which supports 48-bit virtual addresses.
pub enum Sv48 {}

unsafe impl PagingMode for Sv48 {
    const LEVELS: usize = 4;
    const TOP_LEVEL_SIZE: PageSize = PageSize::Terapage;
    const SATP_MODE: satp::Mode = satp::Mode::Sv48;
    const MAX_ADDRESS: usize = 0xFFFF_FFFF_FFFF;
}
