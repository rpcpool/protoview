use core::fmt;

use crate::wire::WireType;

/// The maximum buffer size a view can index, imposed by the `u32` offsets stored in the
/// generated index tables.
pub const MAX_MESSAGE_LEN: usize = u32::MAX as usize;

/// The deepest message nesting a generated `parse` will validate, matching `prost`'s
/// default recursion limit. Bounds stack use on adversarial input.
pub const MAX_DEPTH: u32 = 100;

/// A failure encountered while validating or reading a protobuf message.
///
/// Produced during the structural walk performed by a generated `parse`, and by the
/// low-level readers in [`crate::varint`] and [`crate::wire`]. Field getters on a
/// generated view do not return this type: the walk has already rejected anything
/// malformed by the time a view exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The buffer ended in the middle of a tag, length, or payload.
    UnexpectedEof,
    /// A varint did not terminate within the 10 bytes a 64-bit value can occupy.
    VarintOverflow,
    /// A length-delimited field declared a length that does not fit in the buffer.
    LengthOverflow,
    /// A tag carried a wire type that is not one of varint, 64-bit, length-delimited or
    /// 32-bit. Groups (wire types 3 and 4) are reported here, as they are unsupported.
    InvalidWireType {
        /// The wire type bits read from the tag.
        wire_type: u8,
    },
    /// A tag carried field number zero, which is not a legal protobuf field number.
    InvalidFieldNumber,
    /// A `bytes` field configured as a fixed-size array carried a different length.
    FixedBytesLenMismatch {
        /// The field number the mismatch was found on.
        field: u32,
        /// The length declared by the `fixed_bytes` build configuration.
        expected: u32,
        /// The length actually present on the wire.
        actual: u32,
    },
    /// The buffer is longer than [`MAX_MESSAGE_LEN`], so its offsets cannot be indexed.
    MessageTooLarge,
    /// A field known to the schema arrived with a wire type its declared type cannot be
    /// encoded with.
    UnexpectedWireType {
        /// The field number carried by the tag.
        field: u32,
        /// The wire type the tag carried.
        wire_type: WireType,
    },
    /// Nested messages went deeper than [`MAX_DEPTH`].
    RecursionLimitExceeded,
    /// A packed repeated field's payload did not divide evenly into whole values.
    MalformedPackedField {
        /// The field number the malformed run was found on.
        field: u32,
    },
}

impl fmt::Display for DecodeError {
    /// Writes a human-readable description of this error.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write into.
    ///
    /// # Returns
    ///
    /// [`fmt::Result`] propagated from the underlying writes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("buffer ended mid-field"),
            Self::VarintOverflow => f.write_str("varint exceeded 10 bytes"),
            Self::LengthOverflow => f.write_str("length-delimited field runs past end of buffer"),
            Self::InvalidWireType { wire_type } => {
                write!(f, "invalid or unsupported wire type {wire_type}")
            }
            Self::InvalidFieldNumber => f.write_str("field number 0 is not legal"),
            Self::FixedBytesLenMismatch {
                field,
                expected,
                actual,
            } => write!(
                f,
                "field {field} configured as [u8; {expected}] but wire length is {actual}"
            ),
            Self::MessageTooLarge => write!(f, "message exceeds {MAX_MESSAGE_LEN} bytes"),
            Self::UnexpectedWireType { field, wire_type } => {
                write!(f, "field {field} cannot be encoded as {wire_type:?}")
            }
            Self::RecursionLimitExceeded => {
                write!(f, "message nesting exceeds {MAX_DEPTH} levels")
            }
            Self::MalformedPackedField { field } => {
                write!(f, "packed field {field} ends mid-value")
            }
        }
    }
}

impl core::error::Error for DecodeError {}
