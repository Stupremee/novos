//! Fast, safe, zero-allocation and panic-free parsing of binary data.
#![forbid(unsafe_code)]
#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]

mod slice;
pub use slice::Slice;

use bytemuck::Pod;
use core::mem::size_of;

/// A read-only cursor for reading data from raw binary data.
#[derive(Debug)]
pub struct Reader<'input> {
    input: Slice<'input>,
    idx: usize,
}

impl<'input> Reader<'input> {
    /// Create a new [`Reader`] that will read data from the given byte array.
    #[inline]
    pub const fn new(input: Slice<'input>) -> Self {
        Self { input, idx: 0 }
    }

    /// Check if this reader has any bytes left to read.
    #[inline]
    pub fn at_end(&self) -> bool {
        self.idx == self.input.len()
    }

    /// Read a [`Pod`] from this reader, without advancing it.
    #[inline]
    pub fn peek<T: Pod>(&mut self) -> Result<&T, Error> {
        let end = self
            .idx
            .checked_add(size_of::<T>())
            .ok_or(Error::IntegerOverflow)?;

        match self.input.get_range(self.idx..end) {
            Some(x) => bytemuck::try_from_bytes(x.as_slice()).map_err(Error::PodCast),
            None => Err(Error::EndOfInput),
        }
    }

    /// Read a [`Pod`] from this reader, advancing it by the size of the given type.
    #[inline]
    pub fn read<T: Pod>(&mut self) -> Result<T, Error> {
        let elem = *self.peek::<T>()?;
        self.idx = self
            .idx
            .checked_add(size_of::<T>())
            .ok_or(Error::IntegerOverflow)?;
        Ok(elem)
    }

    /// Read `count` bytes and return a slice containing the next `count` bytes.
    #[inline]
    pub fn read_bytes(&mut self, count: usize) -> Result<Slice<'_>, Error> {
        let end = self.idx.checked_add(count).ok_or(Error::IntegerOverflow)?;
        match self.input.get_range(self.idx..end) {
            Some(x) => {
                self.idx = end;
                Ok(x)
            }
            None => Err(Error::EndOfInput),
        }
    }

    /// Skip the next `count` bytes of this reader.
    #[inline]
    pub fn skip(&mut self, count: usize) -> Result<(), Error> {
        self.read_bytes(count).map(|_| ())
    }
}

/// Any error that can happen while reading data from a [`Reader`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Tried to read data, but the end of the reader was already reached.
    EndOfInput,
    /// An error occurred while casting bytes to a Pod.
    PodCast(bytemuck::PodCastError),
    /// The index of a reader overflowed.
    IntegerOverflow,
}
