//! Fast, safe, zero-allocation and panic-free parsing of binary data.
#![forbid(unsafe_code)]
#![deny(rust_2018_idioms, rustdoc::broken_intra_doc_links)]
#![no_std]

mod slice;
pub use slice::Slice;

use bytemuck::Pod;
use core::fmt;
use core::mem::size_of;

/// A read-only cursor for reading data from raw binary data.
#[derive(Debug)]
pub struct Reader<'input> {
    input: Slice<'input>,
    idx: usize,
    context: Option<&'static str>,
}

impl<'input> Reader<'input> {
    /// Create a new [`Reader`] that will read data from the given byte array.
    #[inline]
    pub const fn new(input: Slice<'input>) -> Self {
        Self {
            input,
            idx: 0,
            context: None,
        }
    }

    fn error(&self, kind: ErrorKind) -> Error {
        Error {
            kind,
            context: self.context,
        }
    }

    /// Set the context of this reader for the given closure to provide. This allows to have better
    /// error messages.
    pub fn context<T, F: FnOnce(&mut Self) -> Result<T, Error>>(
        &mut self,
        context: &'static str,
        parse: F,
    ) -> Result<T, Error> {
        self.context = Some(context);
        let item = parse(self);
        self.context = None;
        item
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
            .ok_or_else(|| self.error(ErrorKind::IntegerOverflow))?;

        match self.input.get_range(self.idx..end) {
            Some(x) => bytemuck::try_from_bytes(x.as_slice())
                .map_err(|err| self.error(ErrorKind::PodCast(err))),
            None => Err(self.error(ErrorKind::EndOfInput)),
        }
    }

    /// Read a [`Pod`] from this reader, advancing it by the size of the given type.
    #[inline]
    pub fn read<T: Pod>(&mut self) -> Result<T, Error> {
        let elem = *self.peek::<T>()?;
        self.idx = self
            .idx
            .checked_add(size_of::<T>())
            .ok_or_else(|| self.error(ErrorKind::IntegerOverflow))?;
        Ok(elem)
    }

    /// Read `count` bytes and return a slice containing the next `count` bytes.
    #[inline]
    pub fn read_bytes(&mut self, count: usize) -> Result<Slice<'_>, Error> {
        let end = self
            .idx
            .checked_add(count)
            .ok_or_else(|| self.error(ErrorKind::IntegerOverflow))?;
        match self.input.get_range(self.idx..end) {
            Some(x) => {
                self.idx = end;
                Ok(x)
            }
            None => Err(self.error(ErrorKind::EndOfInput)),
        }
    }

    /// Skip the next `count` bytes of this reader.
    #[inline]
    pub fn skip(&mut self, count: usize) -> Result<(), Error> {
        self.read_bytes(count).map(|_| ())
    }
}

/// An error that contains the [`ErrorKind`] and an optional context, which indicates
/// where in the bytestream the error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub context: Option<&'static str>,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.context {
            Some(ctx) => write!(f, "error at {}: {}", ctx, self.kind),
            None => fmt::Display::fmt(&self.kind, f),
        }
    }
}

/// Any error that can happen while reading data from a [`Reader`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Tried to read data, but the end of the reader was already reached.
    EndOfInput,
    /// An error occurred while casting bytes to a Pod.
    PodCast(bytemuck::PodCastError),
    /// The index of a reader overflowed.
    IntegerOverflow,
    /// A custom error with an individual message.
    Custom(&'static str),
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::EndOfInput => f.write_str("reached end of input while parsing"),
            ErrorKind::PodCast(err) => fmt::Display::fmt(err, f),
            ErrorKind::IntegerOverflow => f.write_str("the index of the reader overflowed"),
            ErrorKind::Custom(msg) => f.write_str(msg),
        }
    }
}
