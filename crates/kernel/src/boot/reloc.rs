//! Relocate the kernel by overwriting all RISCV_RELATIVE relocations.

use crate::symbols;

mod elf {
    pub const DT_NULL: isize = 0;
    pub const DT_RELA: isize = 7;
    pub const DT_RELAENT: isize = 9;
    pub const DT_RELACOUNT: isize = 0x6ffffff9;

    pub const R_RISCV_RELATIVE: usize = 3;

    #[repr(C)]
    pub struct Dyn {
        pub tag: isize,
        pub val: usize,
    }

    #[repr(C)]
    pub struct Rela {
        pub offset: usize,
        pub info: usize,
        pub addend: isize,
    }
}

#[naked]
pub(super) unsafe extern "C" fn reloc_and_run() -> ! {
    asm!(
        "
            # Store hart id and FDT pointer because we need them later
            mv t4, a0
            mv t5, a1

            # First we need to relocate the kernel to the base 0x8020_0000,
            # because that's where we are first loaded
            la sp, __stack_end

            # Load the actual kernel start and offset to the dynamic section
            lla a0, __kernel_start
            lla a1, __dynamic_start
            sub a1, a1, a0

            # `a2` contains the address that needs to be added to every linker address
            # to get the real address
            li a2, {}
            sub a2, a0, a2
            call {}

            # Zero bss section
            la t0, __bss_start
            la t1, __bss_end

        zero_bss:
            bgeu t0, t1, zero_bss_done
            sd zero, (t0)
            addi t0, t0, 8
            j zero_bss

        zero_bss_done:
            # Jump into rust code
            mv a0, t4
            mv a1, t5
            j {}",
        const symbols::LINK_START,
        sym reloc,
        sym super::before_main,
        options(noreturn)
    )
}

/// Relocate the kernel to the given base address.
pub unsafe extern "C" fn reloc(base: *mut u8, dynamic_offset: usize, offset: *mut u8) {
    // set the load start address
    symbols::set_load_start(base as usize);

    // get a pointer to the `.dynamic` section by adding the offset
    let mut dynamic = base.add(dynamic_offset).cast::<elf::Dyn>();

    // create an iterator that will iterate over all entries in the dynamic section
    let entries = core::iter::from_fn(|| {
        let entry = ((*dynamic).tag != elf::DT_NULL).then(|| &*dynamic)?;
        dynamic = dynamic.offset(1);
        Some(entry)
    });

    // each of these variables need to be found inside the dynamic array
    let mut rela_offset = None;
    let mut rela_entry_size = 0;
    let mut rela_count = 0;

    // loop over every entry in the dynamic array
    for &elf::Dyn { tag, val } in entries {
        match tag {
            elf::DT_RELA => {
                rela_offset = Some(val);
            }
            elf::DT_RELAENT => {
                rela_entry_size = val;
            }
            elf::DT_RELACOUNT => {
                rela_count = val;
            }
            _ => {}
        }
    }

    // if we found a `DT_RELA` entry, parse it and perform the relocations
    if let Some(rela_offset) = rela_offset {
        // check if the entry size entry is valid
        if rela_entry_size != core::mem::size_of::<elf::Rela>() {
            return;
        }

        // get the base address of the RELA section
        let rela_base = offset.add(rela_offset).cast::<elf::Rela>();

        // go through each RELA relocation
        for idx in 0..rela_count {
            // get reference to the relocation
            let rela = &*rela_base.add(idx);

            // perform relocation if it's a RISC-V relocation
            if rela.info == elf::R_RISCV_RELATIVE {
                let to_write = offset.offset(rela.addend);
                *(offset.add(rela.offset).cast::<usize>()) = to_write as usize;
            } else {
                // invalid relocation => error
                return;
            }
        }
    }
}
