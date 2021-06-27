use super::{Error, PageSize, PagingMode, PhysAddr, Result, VirtAddr};

pub enum Sv39 {}

unsafe impl PagingMode for Sv39 {
    const LEVELS: usize = 3;

    const TOP_LEVEL_SIZE: PageSize = PageSize::Gigapage;

    fn validate_addresses(paddr: PhysAddr, vaddr: VirtAddr, size: PageSize) -> Result<()> {
        // check if the virtual address is below the maximum
        if usize::from(vaddr) > 0x7F_FFFF_FFFF {
            return Err(Error::InvalidAddress);
        }

        // verify the given addresses
        if !size.is_aligned(paddr.into()) || !size.is_aligned(vaddr.into()) {
            return Err(Error::UnalignedAddress);
        }

        Ok(())
    }

    fn vpn(vaddr: VirtAddr, idx: usize) -> usize {
        usize::from(vaddr) >> (12 + idx * 9) & 0x1FF
    }

    fn set_vpn(vaddr: VirtAddr, idx: usize, val: usize) -> VirtAddr {
        let vaddr = usize::from(vaddr);
        let mut vpns = [
            (vaddr >> 12) & 0x1FF,
            (vaddr >> 21) & 0x1FF,
            (vaddr >> 30) & 0x1FF,
        ];

        vpns[idx] = val;

        let vaddr = (vpns[0] << 12) | (vpns[1] << 21) | (vpns[2] << 30);
        VirtAddr::from(vaddr)
    }
}
