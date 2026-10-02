use crate::error::DecodeError;
use crate::repeated::{Records, Scalars, Width, validate_numeric};
use crate::varint::{read_varint, zigzag_decode32, zigzag_decode64};
use crate::wire::{Field, Scanner, WireType, read_fixed32, read_fixed64, read_length_delimited};

#[test]
fn varint_single_byte() {
    assert_eq!(read_varint(&[0x00], 0), Ok((0, 1)));
    assert_eq!(read_varint(&[0x01], 0), Ok((1, 1)));
    assert_eq!(read_varint(&[0x7f], 0), Ok((127, 1)));
}

#[test]
fn varint_multi_byte() {
    assert_eq!(read_varint(&[0x96, 0x01], 0), Ok((150, 2)));
    assert_eq!(read_varint(&[0xac, 0x02], 0), Ok((300, 2)));
}

#[test]
fn varint_max_u64() {
    let encoded = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
    assert_eq!(read_varint(&encoded, 0), Ok((u64::MAX, 10)));
}

#[test]
fn varint_reads_from_offset() {
    assert_eq!(read_varint(&[0xff, 0xff, 0x96, 0x01], 2), Ok((150, 4)));
}

#[test]
fn varint_rejects_truncated() {
    assert_eq!(read_varint(&[0x96], 0), Err(DecodeError::UnexpectedEof));
    assert_eq!(read_varint(&[], 0), Err(DecodeError::UnexpectedEof));
}

#[test]
fn varint_rejects_unterminated() {
    let encoded = [0x80; 11];
    assert_eq!(read_varint(&encoded, 0), Err(DecodeError::VarintOverflow));
}

#[test]
fn zigzag_round_trips_known_pairs() {
    assert_eq!(zigzag_decode32(0), 0);
    assert_eq!(zigzag_decode32(1), -1);
    assert_eq!(zigzag_decode32(2), 1);
    assert_eq!(zigzag_decode32(4294967294), i32::MAX);
    assert_eq!(zigzag_decode32(4294967295), i32::MIN);
    assert_eq!(zigzag_decode64(3), -2);
    assert_eq!(zigzag_decode64(u64::MAX), i64::MIN);
}

/// `{ uint64 slot = 1 (150); string blockhash = 2 ("testing"); fixed32 n = 3 (7) }`
const MESSAGE: &[u8] = &[
    0x08, 0x96, 0x01, // field 1, varint, 150
    0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g', // field 2, len 7
    0x1d, 0x07, 0x00, 0x00, 0x00, // field 3, fixed32, 7
];

#[test]
fn scanner_walks_every_field() {
    let mut scanner = Scanner::new(MESSAGE).unwrap();
    assert_eq!(
        scanner.next_field(),
        Ok(Some(Field {
            number: 1,
            wire_type: WireType::Varint,
            payload: 1
        }))
    );
    assert_eq!(
        scanner.next_field(),
        Ok(Some(Field {
            number: 2,
            wire_type: WireType::LengthDelimited,
            payload: 4
        }))
    );
    assert_eq!(
        scanner.next_field(),
        Ok(Some(Field {
            number: 3,
            wire_type: WireType::Fixed32,
            payload: 13
        }))
    );
    assert_eq!(scanner.next_field(), Ok(None));
}

#[test]
fn payload_offsets_address_their_values() {
    assert_eq!(read_varint(MESSAGE, 1), Ok((150, 3)));
    assert_eq!(read_length_delimited(MESSAGE, 4), Ok(&b"testing"[..]));
    assert_eq!(read_fixed32(MESSAGE, 13), Ok(7));
}

#[test]
fn scanner_yields_nothing_for_empty_message() {
    assert_eq!(Scanner::new(&[]).unwrap().next_field(), Ok(None));
}

#[test]
fn scanner_rejects_groups() {
    // Field 1 with wire type 3 (group start).
    let mut scanner = Scanner::new(&[0x0b]).unwrap();
    assert_eq!(
        scanner.next_field(),
        Err(DecodeError::InvalidWireType { wire_type: 3 })
    );
}

#[test]
fn scanner_rejects_field_number_zero() {
    let mut scanner = Scanner::new(&[0x00, 0x01]).unwrap();
    assert_eq!(scanner.next_field(), Err(DecodeError::InvalidFieldNumber));
}

#[test]
fn scanner_rejects_length_past_end() {
    // Field 1, length-delimited, declares 99 bytes but supplies 2.
    let mut scanner = Scanner::new(&[0x0a, 0x63, 0x00, 0x00]).unwrap();
    assert_eq!(scanner.next_field(), Err(DecodeError::LengthOverflow));
}

#[test]
fn scanner_rejects_truncated_fixed64() {
    let mut scanner = Scanner::new(&[0x09, 0x00, 0x00]).unwrap();
    assert_eq!(scanner.next_field(), Err(DecodeError::UnexpectedEof));
}

#[test]
fn scanner_skips_unknown_fields() {
    // A field 4 the schema does not know sits between fields 1 and 3.
    let buf = [0x08, 0x01, 0x22, 0x02, 0xff, 0xff, 0x18, 0x05];
    let mut scanner = Scanner::new(&buf).unwrap();
    let numbers: [u32; 3] = core::array::from_fn(|_| scanner.next_field().unwrap().unwrap().number);
    assert_eq!(numbers, [1, 4, 3]);
    assert_eq!(scanner.next_field(), Ok(None));
}

#[test]
fn scanner_accepts_high_field_numbers() {
    // Field 536870911 (2^29 - 1), the largest legal number, wire type 0.
    let buf = [0xf8, 0xff, 0xff, 0xff, 0x0f, 0x01];
    let mut scanner = Scanner::new(&buf).unwrap();
    let field = scanner.next_field().unwrap().unwrap();
    assert_eq!(field.number, (1 << 29) - 1);
}

#[test]
fn fixed_readers_reject_truncation() {
    assert_eq!(read_fixed32(&[0x00; 3], 0), Err(DecodeError::UnexpectedEof));
    assert_eq!(read_fixed64(&[0x00; 7], 0), Err(DecodeError::UnexpectedEof));
    assert_eq!(read_fixed64(&[0x00; 8], 0), Ok(0));
}

/// Records the span a generated `parse` would for field `number`: from the tag of its
/// first occurrence to the end of its last.
fn span(buf: &[u8], number: u32) -> (u32, u32) {
    let mut scanner = Scanner::new(buf).unwrap();
    let (mut start, mut end) = (0, 0);
    loop {
        let tag = scanner.position() as u32;
        let Some(field) = scanner.next_field().unwrap() else {
            break;
        };
        if field.number == number {
            if end == 0 {
                start = tag;
            }
            end = scanner.position() as u32;
        }
    }
    (start, end)
}

fn scalars(buf: &[u8], number: u32, width: Width) -> Vec<u64> {
    let (start, end) = span(buf, number);
    Scalars::new(buf, Records::new(buf, start, end, number), width).collect()
}

#[test]
fn records_skip_interleaved_and_unknown_fields() {
    let buf = [
        0x08, 0x01, // field 1 = 1
        0x1a, 0x01, b'a', // field 3 = "a"
        0x10, 0x02, // field 2 = 2
        0x1a, 0x01, b'b', // field 3 = "b"
        0x3a, 0x01, 0xff, // unknown field 7
        0x1a, 0x01, b'c', // field 3 = "c"
        0x08, 0x09, // field 1 again, after the last element
    ];
    let (start, end) = span(&buf, 3);
    assert_eq!((start, end), (2, 16));
    let items: Vec<&[u8]> = Records::new(&buf, start, end, 3)
        .map(|f| read_length_delimited(&buf, f.payload as usize).unwrap())
        .collect();
    assert_eq!(items, [&b"a"[..], b"b", b"c"]);
}

#[test]
fn records_absent_field_is_empty() {
    let buf = [0x08, 0x01];
    assert_eq!(Records::new(&buf, 0, 0, 3).count(), 0);
}

#[test]
fn records_start_at_offset_zero() {
    let buf = [0x08, 0x05, 0x10, 0x00, 0x08, 0x06];
    assert_eq!(span(&buf, 1), (0, 6));
    assert_eq!(scalars(&buf, 1, Width::Varint), [5, 6]);
}

#[test]
fn scalars_merge_packed_and_unpacked_runs() {
    let buf = [
        0x0a, 0x03, 0x01, 0x96, 0x01, // field 1 packed [1, 150]
        0x10, 0x07, // field 2, interleaved
        0x08, 0x02, // field 1 unpacked 2
        0x0a, 0x00, // field 1 empty packed run
        0x0a, 0x01, 0x03, // field 1 packed [3]
    ];
    assert_eq!(scalars(&buf, 1, Width::Varint), [1, 150, 2, 3]);
}

#[test]
fn scalars_read_fixed_widths() {
    let mut buf = vec![0x0a, 0x08];
    buf.extend_from_slice(&1u32.to_le_bytes());
    buf.extend_from_slice(&2u32.to_le_bytes());
    buf.push(0x0d); // field 1, fixed32, unpacked
    buf.extend_from_slice(&3u32.to_le_bytes());
    assert_eq!(scalars(&buf, 1, Width::Fixed32), [1, 2, 3]);

    let mut buf = vec![0x09];
    buf.extend_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(scalars(&buf, 1, Width::Fixed64), [u64::MAX]);
}

#[test]
fn scalars_skip_mismatched_wire_types_and_truncated_runs() {
    let buf = [
        0x0d, 0x00, 0x00, 0x00, 0x00, // field 1 as fixed32: wrong width, skipped
        0x0a, 0x02, 0x04, 0x80, // packed run whose second varint is truncated
        0x08, 0x05, // field 1 unpacked 5
    ];
    assert_eq!(scalars(&buf, 1, Width::Varint), [4, 5]);
}

fn first_field(buf: &[u8]) -> Field {
    Scanner::new(buf).unwrap().next_field().unwrap().unwrap()
}

#[test]
fn validate_numeric_accepts_well_formed_records() {
    for (buf, width) in [
        (&[0x08, 0x05][..], Width::Varint),
        (&[0x0a, 0x03, 0x01, 0x96, 0x01], Width::Varint),
        (&[0x0a, 0x00], Width::Varint),
        (&[0x0a, 0x08, 0, 0, 0, 0, 0, 0, 0, 0], Width::Fixed32),
        (&[0x0a, 0x08, 0, 0, 0, 0, 0, 0, 0, 0], Width::Fixed64),
    ] {
        assert_eq!(
            validate_numeric(buf, &first_field(buf), width),
            Ok(()),
            "{buf:?}"
        );
    }
}

#[test]
fn validate_numeric_rejects_malformed_records() {
    let buf = [0x0d, 0, 0, 0, 0]; // fixed32 where a varint is declared
    assert_eq!(
        validate_numeric(&buf, &first_field(&buf), Width::Varint),
        Err(DecodeError::UnexpectedWireType {
            field: 1,
            wire_type: WireType::Fixed32
        })
    );

    let buf = [0x0a, 0x02, 0x04, 0x80];
    assert_eq!(
        validate_numeric(&buf, &first_field(&buf), Width::Varint),
        Err(DecodeError::MalformedPackedField { field: 1 })
    );

    let buf = [0x0a, 0x03, 0, 0, 0];
    assert_eq!(
        validate_numeric(&buf, &first_field(&buf), Width::Fixed32),
        Err(DecodeError::MalformedPackedField { field: 1 })
    );
}

#[test]
fn map_entry_offsets_take_the_last_key_and_value() {
    use crate::map::entry_offsets;
    // value = "a", unknown field 3, key = 7, value = "b"
    let entry = [0x12, 0x01, b'a', 0x18, 0x00, 0x08, 0x07, 0x12, 0x01, b'b'];
    let (key, value) = entry_offsets(&entry);
    assert_eq!(read_varint(&entry, key as usize), Ok((7, 7)));
    assert_eq!(read_length_delimited(&entry, value as usize), Ok(&b"b"[..]));

    assert_eq!(entry_offsets(&[]), (0, 0));
    assert_eq!(entry_offsets(&[0x08, 0x05]).1, 0);
}

#[test]
fn fixed_bytes_length_is_checked_exactly() {
    use crate::wire::{expect_fixed_len, read_fixed_bytes};
    let buf = [0x0a, 0x03, 1, 2, 3];
    let field = Scanner::new(&buf).unwrap().next_field().unwrap().unwrap();
    assert_eq!(expect_fixed_len(&buf, &field, 3), Ok(()));
    assert_eq!(
        expect_fixed_len(&buf, &field, 4),
        Err(DecodeError::FixedBytesLenMismatch {
            field: 1,
            expected: 4,
            actual: 3
        })
    );
    assert_eq!(read_fixed_bytes::<3>(&buf, 1), Some([1, 2, 3]));
    assert_eq!(read_fixed_bytes::<2>(&buf, 1), None);
    assert_eq!(read_fixed_bytes::<4>(&buf, 1), None);
}
