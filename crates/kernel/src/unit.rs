//! Utilities for working with raw byte units.

use core::fmt;

/// `1 KiB`
pub const KIB: usize = 1 << 10;
/// `1 MiB`
pub const MIB: usize = 1 << 20;
/// `1 GiB`
pub const GIB: usize = 1 << 30;
/// `1 TiB`
pub const TIB: usize = 1 << 40;

/// Return a formattable type that will pretty-print the given amount of bytes.
pub fn bytes<I: Into<usize> + Copy>(x: I) -> impl fmt::Display {
    ByteUnit(x)
}

/// Wrapper around raw byte that pretty-prints
/// them using the [`Display`](core::fmt::Display)
/// implementation.
#[derive(Debug, Clone, Copy)]
pub struct ByteUnit<I>(I);

impl<I> fmt::Display for ByteUnit<I>
where
    I: Into<usize> + Copy,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = Into::<usize>::into(self.0);

        match count {
            0..KIB => write!(f, "{} B", count)?,
            KIB..MIB => write!(f, "{:.2} KiB", count / KIB)?,
            MIB..GIB => write!(f, "{:.2} MiB", count / MIB)?,
            GIB..TIB => write!(f, "{:.2} GiB", count / GIB)?,
            _ => write!(f, "{:.2} TiB", count / TIB)?,
        };

        Ok(())
    }
}
