//! Iteration over `repeated` fields.
//!
//! A repeated field's elements are separate records on the wire, and the encoding
//! permits other fields to sit between them. A generated `parse` therefore records a
//! span per repeated field, from the tag of its first element to the end of its last,
//! and the iterators here re-walk that span, yielding the matching records in wire order
//! and skipping everything else.

use crate::varint::read_varint;
use crate::error::DecodeError;
use crate::wire::{Field, Scanner, WireType, read_fixed32, read_fixed64, read_length_delimited};

/// The records of one repeated field, in wire order.
///
/// Yields every [`Field`] in the span whose number matches, whatever its wire type;
/// callers filter on [`Field::wire_type`] as their element type requires.
pub struct Records<'a> {
    scanner: Scanner<'a>,
    number: u32,
}

impl<'a> Records<'a> {
    /// Creates an iterator over the records of field `number` within a span.
    ///
    /// # Arguments
    ///
    /// * `buf` - The whole encoded message the span was recorded against.
    /// * `start` - Offset of the first element's tag.
    /// * `end` - Offset just past the last element's payload, or `0` if the field is
    ///   absent.
    /// * `number` - The field number to yield.
    ///
    /// # Returns
    ///
    /// A [`Records`] iterator, empty when `end` is `0`.
    pub fn new(buf: &'a [u8], start: u32, end: u32, number: u32) -> Self {
        Self {
            scanner: Scanner::within(buf, start as usize, end as usize),
            number,
        }
    }
}

impl Iterator for Records<'_> {
    type Item = Field;

    /// Advances to the next record of this field, skipping any other field in between.
    ///
    /// # Returns
    ///
    /// The next matching [`Field`], or [`None`] at the end of the span. The span was
    /// validated by `parse`, so a decode error here cannot occur in practice and is
    /// treated as the end of iteration.
    fn next(&mut self) -> Option<Field> {
        loop {
            let field = self.scanner.next_field().ok()??;
            if field.number == self.number {
                return Some(field);
            }
        }
    }
}

/// How the values of a numeric repeated field are encoded, which decides both the
/// unpacked wire type to accept and how a packed run is split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    /// Varint values: `int*`, `uint*`, `sint*`, `bool`, enums.
    Varint,
    /// Four-byte values: `fixed32`, `sfixed32`, `float`.
    Fixed32,
    /// Eight-byte values: `fixed64`, `sfixed64`, `double`.
    Fixed64,
}

impl Width {
    /// Returns the wire type an unpacked element of this width is encoded with.
    ///
    /// # Returns
    ///
    /// The matching [`WireType`].
    const fn wire_type(self) -> WireType {
        match self {
            Self::Varint => WireType::Varint,
            Self::Fixed32 => WireType::Fixed32,
            Self::Fixed64 => WireType::Fixed64,
        }
    }

    /// Reads one value of this width.
    ///
    /// # Arguments
    ///
    /// * `buf` - The buffer to read from.
    /// * `pos` - Offset of the value's first byte.
    ///
    /// # Returns
    ///
    /// The raw value widened to 64 bits and the offset just past it, or [`None`] if the
    /// value runs past the end of `buf`.
    fn read(self, buf: &[u8], pos: usize) -> Option<(u64, usize)> {
        match self {
            Self::Varint => read_varint(buf, pos).ok(),
            Self::Fixed32 => read_fixed32(buf, pos).ok().map(|v| (u64::from(v), pos + 4)),
            Self::Fixed64 => read_fixed64(buf, pos).ok().map(|v| (v, pos + 8)),
        }
    }
}

/// Validates one record of a numeric repeated field: either a single value of `width`,
/// or a packed run that divides exactly into such values.
///
/// # Arguments
///
/// * `buf` - The whole encoded message.
/// * `field` - The record, as located by a [`Scanner`] over `buf`.
/// * `width` - How each value is encoded.
///
/// # Returns
///
/// `Ok(())` if the record is well-formed.
///
/// # Errors
///
/// [`DecodeError::UnexpectedWireType`] if the record is neither length-delimited nor of
/// `width`'s wire type, and [`DecodeError::MalformedPackedField`] if a packed run ends
/// mid-value. A single value was already bounds-checked by the [`Scanner`].
pub fn validate_numeric(buf: &[u8], field: &Field, width: Width) -> Result<(), DecodeError> {
    if field.wire_type != WireType::LengthDelimited {
        return crate::wire::expect_wire_type(field, width.wire_type());
    }
    let run = read_length_delimited(buf, field.payload as usize)?;
    let malformed = DecodeError::MalformedPackedField {
        field: field.number,
    };
    match width {
        Width::Fixed32 if run.len() % 4 != 0 => Err(malformed),
        Width::Fixed64 if run.len() % 8 != 0 => Err(malformed),
        Width::Fixed32 | Width::Fixed64 => Ok(()),
        Width::Varint => {
            let mut pos = 0;
            while pos < run.len() {
                pos = read_varint(run, pos).map_err(|_| malformed)?.1;
            }
            Ok(())
        }
    }
}

/// The values of a numeric repeated field, merging packed and unpacked records.
///
/// A conformant decoder must accept either encoding for any numeric repeated field, and
/// a message may carry several packed runs, single values, or a mix, for one field. All
/// of them are yielded as one stream in wire order. Values are raw bits widened to
/// `u64`; generated code narrows and reinterprets them for the field's declared type.
pub struct Scalars<'a> {
    buf: &'a [u8],
    records: Records<'a>,
    width: Width,
    /// The unread part of the packed run currently being drained, as `(pos, end)`.
    packed: Option<(usize, usize)>,
}

impl<'a> Scalars<'a> {
    /// Creates an iterator over the values of a numeric repeated field.
    ///
    /// # Arguments
    ///
    /// * `buf` - The whole encoded message; must be the buffer `records` walks.
    /// * `records` - The field's records, from [`Records::new`].
    /// * `width` - How each value is encoded.
    ///
    /// # Returns
    ///
    /// A [`Scalars`] iterator.
    pub fn new(buf: &'a [u8], records: Records<'a>, width: Width) -> Self {
        Self {
            buf,
            records,
            width,
            packed: None,
        }
    }
}

impl Iterator for Scalars<'_> {
    type Item = u64;

    /// Advances to the next value, draining the current packed run before moving on to
    /// the next record.
    ///
    /// # Returns
    ///
    /// The next raw value, or [`None`] once every record is exhausted. `parse` has
    /// already rejected records of the wrong wire type and packed runs ending mid-value
    /// (see [`validate_numeric`]); should one occur anyway, it is skipped.
    fn next(&mut self) -> Option<u64> {
        loop {
            if let Some((pos, end)) = self.packed {
                if pos < end
                    && let Some((value, next)) = self.width.read(&self.buf[..end], pos)
                {
                    self.packed = Some((next, end));
                    return Some(value);
                }
                self.packed = None;
            }

            let field = self.records.next()?;
            let payload = field.payload as usize;
            if field.wire_type == WireType::LengthDelimited {
                let (len, start) = read_varint(self.buf, payload).ok()?;
                let len = usize::try_from(len).unwrap_or(usize::MAX);
                let end = start.saturating_add(len).min(self.buf.len());
                self.packed = Some((start, end));
            } else if field.wire_type == self.width.wire_type()
                && let Some((value, _)) = self.width.read(self.buf, payload)
            {
                return Some(value);
            }
        }
    }
}
