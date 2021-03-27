#![allow(unused)]

macro_rules! write_csr {
    (pub $number:expr) => {
        /// Writes the raw value into this CSR.
        #[inline(always)]
        pub unsafe fn write(bits: usize) {
            asm!("csrw {}, {}", const $number, in(reg) bits);
        }
    };

    ($number:expr) => {
        /// Writes the raw value into this CSR.
        #[inline(always)]
        unsafe fn _write(bits: usize) {
            asm!("csrw {}, {}", const $number, in(reg) bits);
        }
    };
}

macro_rules! read_csr {
    (pub $number:expr) => {
        /// Read the raw bits out of this CSR.
        #[inline(always)]
        pub unsafe fn read() -> usize {
            let bits;
            asm!("csrr {}, {}", out(reg) bits, const $number);
            bits
        }
    };

    ($number:expr) => {
        /// Read the raw bits out of this CSR.
        #[inline(always)]
        unsafe fn _read() -> usize {
            let bits;
            asm!("csrr {}, {}", out(reg) bits, const $number);
            bits
        }
    };
}

macro_rules! set_csr {
    (pub $number:expr) => {
        /// Set all bits specified by the mask to one inside this CSR.
        #[inline(always)]
        pub unsafe fn set(mask: usize) {
            asm!("csrs {}, {}", const $number, in(reg) mask);
        }
    };

    ($number:expr) => {
        #[inline(always)]
        unsafe fn _set(mask: usize) {
            asm!("csrs {}, {}", const $number, in(reg) mask);
        }
    };
}

macro_rules! clear_csr {
    (pub $number:expr) => {
        clear_csr!($number);

        /// Clear all bits specified by the mask inside this CSR.
        #[inline(always)]
        pub unsafe fn clear(mask: usize) {
            asm!("csrc {}, {}", const $number, in(reg) mask);
        }
    };

    ($number:expr) => {
        #[inline(always)]
        unsafe fn _clear(mask: usize) {
            asm!("csrc {}, {}", const $number, in(reg) mask);
        }
    };
}

macro_rules! csr_mod {
    (rw, $name:ident, $num:expr) => {
        #[doc = concat!("The `", stringify!($name), "` CSR.")]
        pub mod $name {
            read_csr!(pub $num);

            write_csr!(pub $num);
            set_csr!(pub $num);
            clear_csr!(pub $num);
        }
    };

    (r, $name:ident, $num:expr) => {
        #[doc = concat!("The `", stringify!($name), "` CSR.")]
        pub mod $name {
            read_csr!(pub $num);
        }
    };
}
