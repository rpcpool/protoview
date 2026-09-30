use crate::error::DecodeError;

/// The maximum number of bytes a base-128 varint encoding a 64-bit value may occupy.
pub const MAX_VARINT_LEN: usize = 10;

/// Reads a base-128 varint starting at `start`.
///
/// # Arguments
///
/// * `buf` - The buffer to read from.
/// * `start` - Byte offset of the varint's first byte.
///
/// # Returns
///
/// The decoded value and the offset of the first byte after it.
///
/// # Errors
///
/// [`DecodeError::UnexpectedEof`] if the buffer ends before a terminating byte, or
/// [`DecodeError::VarintOverflow`] if no byte in [`MAX_VARINT_LEN`] clears the
/// continuation bit.
pub fn read_varint(buf: &[u8], start: usize) -> Result<(u64, usize), DecodeError> {
    let mut value = 0u64;
    let mut pos = start;
    for shift in (0..64).step_by(7) {
        let byte = *buf.get(pos).ok_or(DecodeError::UnexpectedEof)?;
        pos += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Ok((value, pos));
        }
    }
    Err(DecodeError::VarintOverflow)
}

/// Reads a varint and truncates it to 32 bits.
///
/// Protobuf encodes 32-bit fields as full 64-bit varints, and a conformant decoder
/// discards the upper bits rather than rejecting them.
///
/// # Arguments
///
/// * `buf` - The buffer to read from.
/// * `start` - Byte offset of the varint's first byte.
///
/// # Returns
///
/// The truncated value and the offset of the first byte after the varint.
///
/// # Errors
///
/// Propagates any error from [`read_varint`].
pub fn read_varint32(buf: &[u8], start: usize) -> Result<(u32, usize), DecodeError> {
    let (value, pos) = read_varint(buf, start)?;
    Ok((value as u32, pos))
}

/// Reverses the zig-zag encoding used by protobuf's `sint32` fields.
///
/// # Arguments
///
/// * `value` - The raw varint bits, already truncated to 32 bits.
///
/// # Returns
///
/// The signed value the encoding represents.
pub const fn zigzag_decode32(value: u32) -> i32 {
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

/// Reverses the zig-zag encoding used by protobuf's `sint64` fields.
///
/// # Arguments
///
/// * `value` - The raw varint bits.
///
/// # Returns
///
/// The signed value the encoding represents.
pub const fn zigzag_decode64(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}
