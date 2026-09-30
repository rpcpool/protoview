use crate::error::{DecodeError, MAX_DEPTH, MAX_MESSAGE_LEN};
use crate::varint::read_varint;

/// The protobuf wire types this crate supports.
///
/// Groups (wire types 3 and 4) are deliberately absent; encountering one yields
/// [`DecodeError::InvalidWireType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireType {
    /// Base-128 varint: `int32`, `int64`, `uint32`, `uint64`, `sint*`, `bool`, enums.
    Varint,
    /// Eight little-endian bytes: `fixed64`, `sfixed64`, `double`.
    Fixed64,
    /// A varint length followed by that many bytes: `string`, `bytes`, messages, packed
    /// repeated scalars.
    LengthDelimited,
    /// Four little-endian bytes: `fixed32`, `sfixed32`, `float`.
    Fixed32,
}

impl WireType {
    /// Converts the low three bits of a tag into a [`WireType`].
    ///
    /// # Arguments
    ///
    /// * `bits` - The wire type bits taken from a tag.
    ///
    /// # Returns
    ///
    /// The matching [`WireType`].
    ///
    /// # Errors
    ///
    /// [`DecodeError::InvalidWireType`] for group markers and for values above 5.
    pub const fn from_bits(bits: u8) -> Result<Self, DecodeError> {
        match bits {
            0 => Ok(Self::Varint),
            1 => Ok(Self::Fixed64),
            2 => Ok(Self::LengthDelimited),
            5 => Ok(Self::Fixed32),
            wire_type => Err(DecodeError::InvalidWireType { wire_type }),
        }
    }
}

/// One field located by a [`Scanner`], identified by number and by where its payload
/// begins.
///
/// `payload` points just past the tag, so it addresses the varint bits, the fixed-width
/// bytes, or the length prefix depending on `wire_type`. A single offset is therefore
/// enough to read the field back later, which is what lets a generated index store one
/// `u32` per field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// The field number carried by the tag.
    pub number: u32,
    /// How the payload is encoded.
    pub wire_type: WireType,
    /// Byte offset of the payload, relative to the start of the scanned buffer.
    pub payload: u32,
}

/// A forward-only walk over the fields of an encoded message.
///
/// This is the structural pass a generated `parse` runs: it reads tags and lengths and
/// skips payloads, without interpreting field contents. Everything it yields has been
/// bounds-checked, which is what allows generated getters to be infallible.
pub struct Scanner<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Scanner<'a> {
    /// Creates a scanner positioned at the start of `buf`.
    ///
    /// # Arguments
    ///
    /// * `buf` - The encoded message to walk.
    ///
    /// # Returns
    ///
    /// A [`Scanner`] over the whole buffer.
    ///
    /// # Errors
    ///
    /// [`DecodeError::MessageTooLarge`] if `buf` is longer than [`MAX_MESSAGE_LEN`], as
    /// its offsets would not fit the `u32` fields of [`Field`].
    pub fn new(buf: &'a [u8]) -> Result<Self, DecodeError> {
        if buf.len() > MAX_MESSAGE_LEN {
            return Err(DecodeError::MessageTooLarge);
        }
        Ok(Self { buf, pos: 0 })
    }

    /// Creates a scanner over `buf[start..end]`, reporting offsets relative to `buf`.
    ///
    /// Used to re-walk a span a previous validating walk already covered, so neither
    /// bound is checked against [`MAX_MESSAGE_LEN`]. An `end` past the buffer is clamped.
    ///
    /// # Arguments
    ///
    /// * `buf` - The whole encoded message.
    /// * `start` - Offset of the first tag to read.
    /// * `end` - Offset just past the last payload to read.
    ///
    /// # Returns
    ///
    /// A [`Scanner`] positioned at `start` that stops at `end`.
    pub(crate) fn within(buf: &'a [u8], start: usize, end: usize) -> Self {
        let end = end.min(buf.len());
        Self {
            buf: &buf[..end],
            pos: start,
        }
    }

    /// Returns the current read offset.
    ///
    /// # Returns
    ///
    /// The offset of the next tag [`Scanner::next_field`] will read, or, right after a
    /// call to it, the offset just past the payload it returned.
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// Advances to the next field.
    ///
    /// # Returns
    ///
    /// The next [`Field`], or [`None`] once the buffer is exhausted.
    ///
    /// # Errors
    ///
    /// [`DecodeError::InvalidFieldNumber`] for a zero field number,
    /// [`DecodeError::InvalidWireType`] for a group or unknown wire type, and any error
    /// from reading the tag or skipping the payload.
    pub fn next_field(&mut self) -> Result<Option<Field>, DecodeError> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        let (tag, after_tag) = read_varint(self.buf, self.pos)?;
        let number = u32::try_from(tag >> 3).map_err(|_| DecodeError::InvalidFieldNumber)?;
        if number == 0 {
            return Err(DecodeError::InvalidFieldNumber);
        }
        let wire_type = WireType::from_bits((tag & 7) as u8)?;
        self.pos = skip_payload(self.buf, after_tag, wire_type)?;
        Ok(Some(Field {
            number,
            wire_type,
            payload: after_tag as u32,
        }))
    }
}

/// Returns the offset just past a payload beginning at `payload`.
///
/// # Arguments
///
/// * `buf` - The buffer being walked.
/// * `payload` - Offset of the payload's first byte.
/// * `wire_type` - How the payload is encoded.
///
/// # Returns
///
/// The offset of the next tag.
///
/// # Errors
///
/// [`DecodeError::UnexpectedEof`] if a fixed-width payload runs past the end,
/// [`DecodeError::LengthOverflow`] if a declared length does, or any error from reading
/// a varint.
fn skip_payload(buf: &[u8], payload: usize, wire_type: WireType) -> Result<usize, DecodeError> {
    match wire_type {
        WireType::Varint => Ok(read_varint(buf, payload)?.1),
        WireType::Fixed64 => bounded(buf, payload, 8).ok_or(DecodeError::UnexpectedEof),
        WireType::Fixed32 => bounded(buf, payload, 4).ok_or(DecodeError::UnexpectedEof),
        WireType::LengthDelimited => {
            let (len, after_len) = read_varint(buf, payload)?;
            let len = usize::try_from(len).map_err(|_| DecodeError::LengthOverflow)?;
            bounded(buf, after_len, len).ok_or(DecodeError::LengthOverflow)
        }
    }
}

/// Adds `len` to `start`, rejecting sums that overflow or run past the buffer.
///
/// # Arguments
///
/// * `buf` - The buffer being bounded against.
/// * `start` - The starting offset.
/// * `len` - The number of bytes to advance.
///
/// # Returns
///
/// The resulting offset, or [`None`] if it would leave the buffer.
fn bounded(buf: &[u8], start: usize, len: usize) -> Option<usize> {
    let end = start.checked_add(len)?;
    (end <= buf.len()).then_some(end)
}

/// Reads the bytes of a length-delimited payload.
///
/// # Arguments
///
/// * `buf` - The buffer to read from.
/// * `payload` - Offset of the length prefix.
///
/// # Returns
///
/// The payload bytes, excluding the length prefix.
///
/// # Errors
///
/// [`DecodeError::LengthOverflow`] if the declared length runs past the end of `buf`, or
/// any error from reading the length varint.
pub fn read_length_delimited(buf: &[u8], payload: usize) -> Result<&[u8], DecodeError> {
    let (len, after_len) = read_varint(buf, payload)?;
    let len = usize::try_from(len).map_err(|_| DecodeError::LengthOverflow)?;
    let end = bounded(buf, after_len, len).ok_or(DecodeError::LengthOverflow)?;
    Ok(&buf[after_len..end])
}

/// Reads four little-endian bytes.
///
/// # Arguments
///
/// * `buf` - The buffer to read from.
/// * `payload` - Offset of the first byte.
///
/// # Returns
///
/// The raw 32-bit value, which callers reinterpret as `fixed32`, `sfixed32` or `float`.
///
/// # Errors
///
/// [`DecodeError::UnexpectedEof`] if fewer than four bytes remain.
pub fn read_fixed32(buf: &[u8], payload: usize) -> Result<u32, DecodeError> {
    let end = bounded(buf, payload, 4).ok_or(DecodeError::UnexpectedEof)?;
    let bytes: [u8; 4] = buf[payload..end].try_into().expect("bounded to 4 bytes");
    Ok(u32::from_le_bytes(bytes))
}

/// Reads eight little-endian bytes.
///
/// # Arguments
///
/// * `buf` - The buffer to read from.
/// * `payload` - Offset of the first byte.
///
/// # Returns
///
/// The raw 64-bit value, which callers reinterpret as `fixed64`, `sfixed64` or `double`.
///
/// # Errors
///
/// [`DecodeError::UnexpectedEof`] if fewer than eight bytes remain.
pub fn read_fixed64(buf: &[u8], payload: usize) -> Result<u64, DecodeError> {
    let end = bounded(buf, payload, 8).ok_or(DecodeError::UnexpectedEof)?;
    let bytes: [u8; 8] = buf[payload..end].try_into().expect("bounded to 8 bytes");
    Ok(u64::from_le_bytes(bytes))
}

/// Checks that a known field arrived with the wire type its declared type uses.
///
/// # Arguments
///
/// * `field` - The field as located by a [`Scanner`].
/// * `expected` - The wire type the schema requires.
///
/// # Returns
///
/// `Ok(())` when the wire types match.
///
/// # Errors
///
/// [`DecodeError::UnexpectedWireType`] when they differ.
pub fn expect_wire_type(field: &Field, expected: WireType) -> Result<(), DecodeError> {
    if field.wire_type == expected {
        Ok(())
    } else {
        Err(DecodeError::UnexpectedWireType {
            field: field.number,
            wire_type: field.wire_type,
        })
    }
}

/// Returns the depth to validate a nested message at, one below `depth`.
///
/// # Arguments
///
/// * `depth` - The depth of the message containing the nested one; `0` at the root.
///
/// # Returns
///
/// `depth + 1`.
///
/// # Errors
///
/// [`DecodeError::RecursionLimitExceeded`] if that would exceed [`MAX_DEPTH`].
pub fn descend(depth: u32) -> Result<u32, DecodeError> {
    if depth < MAX_DEPTH {
        Ok(depth + 1)
    } else {
        Err(DecodeError::RecursionLimitExceeded)
    }
}
