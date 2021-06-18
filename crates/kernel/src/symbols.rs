//! Linker symbols

use crate::StaticCell;

/// The start address where everything is linked to.
///
/// Keep in sync with number in linker script.
pub const LINK_START: usize = 0x8020_0000;

/// The address where the kernel is laoded at.
static LOAD_START: StaticCell<usize> = StaticCell::new(LINK_START);

/// Get the address at which the kernel is loaded.
pub fn load_start() -> usize {
    unsafe { *LOAD_START.get() }
}

/// Set the address at which the kernel is loaded.
pub unsafe fn set_load_start(x: usize) {
    LOAD_START.get().write(x);
}

macro_rules! linker_section {
    ($fn:ident, $start:ident, $end:ident) => {
        pub fn $fn() -> (*mut u8, *mut u8) {
            extern "C" {
                static mut $start: Symbol;
                static mut $end: Symbol;
            }

            unsafe {
                (
                    $start.ptr().add(load_start() - LINK_START),
                    $end.ptr().add(load_start() - LINK_START),
                )
            }
        }
    };
}

/// Helper struct to make handling with Linker Symbols easier.
#[repr(transparent)]
pub struct Symbol(u8);

impl Symbol {
    /// Treats this symbol as a mutable pointer to a byte.
    pub fn ptr(&mut self) -> *mut u8 {
        self as *mut _ as *mut _
    }

    /// Treats this symbol as a value, that is retrieved by
    /// using the value of the address where this symbol points to.
    pub fn value(&self) -> usize {
        self as *const _ as usize
    }
}

linker_section!(text_range, __text_start, __text_end);
linker_section!(rodata_range, __rodata_start, __rodata_end);
linker_section!(data_range, __data_start, __data_end);
linker_section!(bss_range, __bss_start, __bss_end);
linker_section!(stack_range, __stack_start, __stack_end);

linker_section!(kernel_range, __kernel_start, __kernel_end);
linker_section!(tdata_range, __tdata_start, __tdata_end);
