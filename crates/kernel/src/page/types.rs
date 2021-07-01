use crate::unit;
use core::fmt;

macro_rules! addr_type {
    ($(#[$attr:meta])* $pub:vis struct $name:ident;) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(transparent)]
        $pub struct $name(usize);

        impl $name {
            /// Interpret this physical address as a pointer to a `T`.
            pub fn as_ptr<T>(self) -> *mut T {
                self.0 as *mut T
            }

            /// Calculates the wrapping offset from this physical address.
            pub fn offset(self, off: usize) -> Self {
                $name::from(self.0.wrapping_add(off))
            }
        }

        impl From<usize> for $name {
            fn from(addr: usize) -> Self {
                Self(addr)
            }
        }

        impl<T> From<*const T> for $name {
            fn from(x: *const T) -> Self {
                Self::from(x as usize)
            }
        }

        impl<T> From<*mut T> for $name {
            fn from(x: *mut T) -> Self {
                Self::from(x as usize)
            }
        }

        impl From<$name> for usize {
            fn from(x: $name) -> usize {
                x.0
            }
        }

        impl core::fmt::Pointer for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Pointer::fmt(&self.as_ptr::<u8>(), f)
            }
        }
    };
}

addr_type! {
    /// A Virtual address
    pub struct VirtAddr;
}

addr_type! {
    /// A Physical address
    pub struct PhysAddr;
}

/// Represents the different kinds of pages that can be mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    Kilopage,
    Megapage,
    Gigapage,
    Terapage,
}

impl PageSize {
    /// Check if a given address is aligned to the boundary of this page size.
    pub fn is_aligned(self, addr: usize) -> bool {
        let align = self.size();
        addr % align == 0
    }

    /// Return the number of bytes this page size covers.
    ///
    /// This will return the sizes for the Sv39 addressing mode.
    #[allow(clippy::identity_op)]
    pub const fn size(self) -> usize {
        match self {
            PageSize::Kilopage => 4 * unit::KIB,
            PageSize::Megapage => 2 * unit::MIB,
            PageSize::Gigapage => 1 * unit::GIB,
            PageSize::Terapage => 512 * unit::GIB,
        }
    }

    /// Return the index of the VPN that specifies this page size.
    pub fn vpn_idx(self) -> usize {
        match self {
            PageSize::Kilopage => 0,
            PageSize::Megapage => 1,
            PageSize::Gigapage => 2,
            PageSize::Terapage => 3,
        }
    }

    /// Return the pagesize that comes after going through a branch at this level.
    pub fn step(self) -> Option<Self> {
        match self {
            PageSize::Kilopage => None,
            PageSize::Megapage => Some(PageSize::Kilopage),
            PageSize::Gigapage => Some(PageSize::Megapage),
            PageSize::Terapage => Some(PageSize::Gigapage),
        }
    }
}

bitflags::bitflags! {
    pub struct Flags: u8 {
        const READ =     1 << 1;
        const WRITE =    1 << 2;
        const EXEC =     1 << 3;
        const USER =     1 << 4;
        const GLOBAL =   1 << 5;
        const ACCESSED = 1 << 6;
        const DIRTY =    1 << 7;
    }
}
impl fmt::Display for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use owo_colors::OwoColorize;

        const BITS: &[(Flags, &str)] = &[
            (Flags::DIRTY, "D"),
            (Flags::ACCESSED, "A"),
            (Flags::GLOBAL, "G"),
            (Flags::USER, "U"),
            (Flags::EXEC, "X"),
            (Flags::WRITE, "W"),
            (Flags::READ, "R"),
        ];

        for (bit, c) in BITS {
            match self.contains(*bit) {
                true => write!(f, "{}", c.green())?,
                false => write!(f, "{}", "-".red())?,
            }
        }

        Ok(())
    }
}
