//! Linker symbols

/// The start address where everything is linked to.
///
/// Keep in sync with number in linker script.
pub const LINK_START: usize = 0x8020_0000;

macro_rules! linker_section {
    ($fn:ident, $start:ident, $end:ident) => {
        #[inline(always)]
        pub fn $fn() -> (*mut u8, *mut u8) {
            let start: *mut u8;
            let end: *mut u8;

            unsafe {
                asm!(
                    concat!("lla {}, ", stringify!($start)),
                    concat!("lla {}, ", stringify!($end)),
                    out(reg) start,
                    out(reg) end,
                );
            }

            (start, end)
        }
    };
}

linker_section!(kernel_range, __kernel_start, __kernel_end);
linker_section!(text_range, __text_start, __text_end);
linker_section!(rodata_range, __rodata_start, __rodata_end);
linker_section!(data_range, __data_start, __data_end);
linker_section!(tdata_range, __tdata_start, __tdata_end);
linker_section!(bss_range, __bss_start, __bss_end);
linker_section!(stack_range, __stack_start, __stack_end);

linker_section!(rel_dyn_range, __rel_dyn_start, __rel_dyn_end);
