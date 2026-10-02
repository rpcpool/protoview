//! [`SharedBytes`]: byte buffers that can hand out owned sub-buffers of themselves.

use core::ops::Range;

/// A byte buffer that can create an owned handle to part of itself, without copying.
///
/// A generated view over `B: SharedBytes` can return nested views that own a slice of
/// the same buffer (its `*_owned` getters), instead of borrowing from the parent. That
/// lets them be sent to other threads or stored without keeping the root view alive.
///
/// Implemented for `&[u8]` (slicing a shared reference) and, with the `bytes` feature, for
/// `bytes::Bytes` (a reference-counted slice). A `Vec<u8>` cannot implement it without
/// copying.
pub trait SharedBytes: AsRef<[u8]> + Sized {
    /// Returns a container holding exactly the bytes at `range` of `self`.
    ///
    /// # Arguments
    ///
    /// * `range` - The byte range to keep, relative to `self.as_ref()`.
    ///
    /// # Returns
    ///
    /// A container over `self.as_ref()[range]`.
    ///
    /// # Panics
    ///
    /// If `range` is out of bounds of `self.as_ref()`.
    fn subslice(&self, range: Range<usize>) -> Self;

    /// Returns a container holding `sub`, which must lie within `self.as_ref()`.
    ///
    /// Generated getters call this with a slice they just read out of `self`. An empty
    /// `sub`, or one that does not lie within `self`, yields an empty container rather than
    /// panicking.
    ///
    /// # Arguments
    ///
    /// * `sub` - A subslice of `self.as_ref()`.
    ///
    /// # Returns
    ///
    /// A container over the same bytes as `sub`.
    fn slice_ref(&self, sub: &[u8]) -> Self {
        let buf = self.as_ref();
        let start = (sub.as_ptr() as usize).wrapping_sub(buf.as_ptr() as usize);
        match start.checked_add(sub.len()) {
            Some(end) if !sub.is_empty() && end <= buf.len() => self.subslice(start..end),
            _ => self.subslice(0..0),
        }
    }
}

impl<'a> SharedBytes for &'a [u8] {
    /// Slices the borrowed buffer; the result keeps the original lifetime.
    fn subslice(&self, range: Range<usize>) -> Self {
        let buf: &'a [u8] = self;
        &buf[range]
    }
}

#[cfg(feature = "bytes")]
impl SharedBytes for bytes::Bytes {
    /// Shares the reference-counted allocation.
    fn subslice(&self, range: Range<usize>) -> Self {
        self.slice(range)
    }
}

#[cfg(test)]
mod tests {
    use super::SharedBytes;

    #[test]
    fn slice_ref_keeps_the_same_lifetime_for_borrowed_slices() {
        let data = [1u8, 2, 3, 4, 5];
        let whole: &[u8] = &data;
        let part = whole.slice_ref(&whole[1..4]);
        assert_eq!(part, [2, 3, 4]);
        assert_eq!(part.as_ptr(), data[1..].as_ptr());
    }

    #[test]
    fn empty_or_foreign_slices_give_an_empty_container() {
        let data = [1u8, 2, 3];
        let other = [9u8, 9];
        let whole: &[u8] = &data;
        assert!(whole.slice_ref(&[]).is_empty());
        assert!(whole.slice_ref(&data[1..1]).is_empty());
        assert!(whole.slice_ref(&other).is_empty());
    }

    #[cfg(feature = "bytes")]
    #[test]
    fn bytes_slices_share_the_allocation() {
        let bytes = bytes::Bytes::from(vec![1u8, 2, 3, 4, 5]);
        let part = bytes.slice_ref(&bytes[1..4]);
        assert_eq!(part.as_ref(), [2, 3, 4]);
        assert_eq!(part.as_ptr(), bytes[1..].as_ptr());
    }
}
