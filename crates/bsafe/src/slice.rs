use core::ops::Range;

/// A panic-free, immutable wrapper around a `&[u8]`.
#[derive(Debug)]
pub struct Slice<'a> {
    inner: &'a [u8],
}

impl<'a> Slice<'a> {
    /// Get a reference to the element at the given index.
    #[inline]
    pub fn get(&self, idx: usize) -> Option<&u8> {
        self.inner.get(idx)
    }

    /// Get a subslice of this slice in the given range.
    #[inline]
    pub fn get_range(&self, r: Range<usize>) -> Option<Slice<'_>> {
        self.inner.get(r).map(Slice::from)
    }

    /// Return the length of this slice.
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if this slice is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return the inner Rust slice.
    #[inline]
    pub fn as_slice(&self) -> &'a [u8] {
        self.inner
    }
}

impl<'a> From<&'a [u8]> for Slice<'a> {
    fn from(x: &'a [u8]) -> Self {
        Self { inner: x }
    }
}
