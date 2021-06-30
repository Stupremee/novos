//! Implementation of dynamic relocation of the kernel itself.

use crate::symbols::{kernel_range, LINK_START};

mod elf {
    pub const R_RISCV_RELATIVE: usize = 3;

    #[repr(C)]
    pub struct Rela {
        pub offset: usize,
        pub info: usize,
        pub addend: isize,
    }
}

/// Relocate the kernel to the given base address.
pub unsafe extern "C" fn relocate(base: usize) -> u32 {
    let (start, _) = kernel_range();
    let current_offset = start.sub(LINK_START);

    // get the `rela.dyn` section to look for relocations
    let (start, end) = crate::symbols::rel_dyn_range();
    let (mut start, end) = (start.cast::<elf::Rela>(), end.cast::<elf::Rela>());

    // get an iterator that goes through each entry of the section
    let entries = core::iter::from_fn(|| {
        (start != end).then(|| {
            let res = start.as_ref().unwrap();
            start = start.add(1);
            res
        })
    });

    // go through each relocation entry
    for entry in entries {
        // we only support R_RISCV_RELATIVE entries atm
        if entry.info != elf::R_RISCV_RELATIVE {
            return 1;
        }

        // perform the relocation
        let new = base.wrapping_add((entry.addend as usize) - LINK_START);
        current_offset.add(entry.offset).cast::<usize>().write(new);
    }

    0
}
