//! Hand-encoded `oneof` layouts: every member shape, last-wins across members, and members
//! interleaved with other fields.

use proto_codec::{DecodeError, WireType};

use crate::fixtures::oneof::Choice;
use crate::fixtures::oneof::choice::{Mode, Value};

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn key(field: u32, wire_type: u8, out: &mut Vec<u8>) {
    varint(u64::from(field << 3 | u32::from(wire_type)), out);
}

fn uint(field: u32, value: u64, out: &mut Vec<u8>) {
    key(field, 0, out);
    varint(value, out);
}

fn len_delim(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
    key(field, 2, out);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

/// `before = 1`, `after = "z"`, `maybe = 5`, and `mode.fast = 7`, around which each test
/// places the `value` member it is checking.
fn with_neighbors(member: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut msg = Vec::new();
    uint(1, 1, &mut msg);
    member(&mut msg);
    uint(10, 5, &mut msg);
    key(11, 5, &mut msg);
    msg.extend_from_slice(&7u32.to_le_bytes());
    len_delim(13, b"z", &mut msg);
    msg
}

fn assert_neighbors(choice: &Choice<&[u8]>) {
    assert_eq!(choice.before(), 1);
    assert_eq!(choice.maybe(), Some(5));
    assert!(matches!(choice.mode(), Some(Mode::Fast(7))));
    assert_eq!(choice.after().unwrap(), "z");
}

#[test]
fn scalar_members() {
    let msg = with_neighbors(|m| uint(2, 300, m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Number(300))));
    assert_neighbors(&choice);

    let msg = with_neighbors(|m| uint(3, 3, m)); // zig-zag 3 = -2
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Signed(-2))));

    let msg = with_neighbors(|m| {
        key(4, 1, m);
        m.extend_from_slice(&2.5f64.to_le_bytes());
    });
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Ratio(r)) if r == 2.5));

    let msg = with_neighbors(|m| uint(5, 1, m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Flag(true))));
}

#[test]
fn default_valued_member_is_still_present() {
    // A oneof member has explicit presence: `number = 0` on the wire is `Some`.
    let msg = with_neighbors(|m| uint(2, 0, m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Number(0))));

    let msg = with_neighbors(|m| len_delim(6, b"", m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Text(Ok("")))));
}

#[test]
fn length_delimited_members() {
    let msg = with_neighbors(|m| len_delim(6, "héllo".as_bytes(), m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Text(Ok("héllo")))));
    assert_neighbors(&choice);

    let msg = with_neighbors(|m| len_delim(6, &[0xff], m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Text(Err(_)))));

    let msg = with_neighbors(|m| len_delim(7, &[0xde, 0xad], m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Blob(&[0xde, 0xad]))));
}

#[test]
fn message_members_from_this_and_another_package() {
    let mut item = Vec::new();
    uint(1, 42, &mut item);
    let msg = with_neighbors(|m| len_delim(8, &item, m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    match choice.value() {
        Some(Value::Item(item)) => assert_eq!(item.id(), 42),
        _ => panic!("expected Value::Item"),
    }
    assert_neighbors(&choice);

    let mut inner = Vec::new();
    len_delim(3, b"inner-text", &mut inner); // fixtures.nested.Inner.text
    let msg = with_neighbors(|m| len_delim(9, &inner, m));
    let choice = Choice::parse(msg.as_slice()).unwrap();
    match choice.value() {
        Some(Value::Foreign(inner)) => assert_eq!(inner.text().unwrap(), "inner-text"),
        _ => panic!("expected Value::Foreign"),
    }
}

#[test]
fn absent_oneof_is_none() {
    let msg = with_neighbors(|_| {});
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(choice.value().is_none());
    assert_neighbors(&choice);

    let choice = Choice::parse(&[][..]).unwrap();
    assert!(choice.value().is_none());
    assert!(choice.mode().is_none());
}

#[test]
fn last_member_on_the_wire_wins() {
    // Across members: a later member replaces an earlier one, whichever field it is.
    let mut msg = Vec::new();
    uint(2, 1, &mut msg);
    len_delim(13, b"between", &mut msg);
    len_delim(6, b"text wins", &mut msg);
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Text(Ok("text wins")))));
    assert_eq!(choice.after().unwrap(), "between");

    let mut msg = Vec::new();
    len_delim(6, b"text", &mut msg);
    uint(2, 9, &mut msg);
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Number(9))));

    // Within one member: the last occurrence wins too.
    let mut msg = Vec::new();
    uint(2, 1, &mut msg);
    uint(2, 2, &mut msg);
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Number(2))));
}

#[test]
fn two_oneofs_are_independent() {
    let mut msg = Vec::new();
    uint(12, 77, &mut msg); // mode.slow
    uint(2, 5, &mut msg); // value.number
    key(11, 5, &mut msg); // mode.fast replaces mode.slow
    msg.extend_from_slice(&3u32.to_le_bytes());
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(matches!(choice.value(), Some(Value::Number(5))));
    assert!(matches!(choice.mode(), Some(Mode::Fast(3))));
}

#[test]
fn unknown_fields_do_not_select_a_member() {
    let mut msg = Vec::new();
    uint(99, 1, &mut msg);
    len_delim(98, b"?", &mut msg);
    let choice = Choice::parse(msg.as_slice()).unwrap();
    assert!(choice.value().is_none());
}

#[test]
fn members_are_validated() {
    // A message member is validated recursively, like any nested message.
    let msg = with_neighbors(|m| len_delim(8, &[0x08], m)); // Item.id tag with no value
    assert_eq!(
        Choice::parse(msg.as_slice()).err(),
        Some(DecodeError::UnexpectedEof)
    );

    // A member's wire type is checked against its declared type.
    let msg = with_neighbors(|m| len_delim(2, b"x", m)); // `number` sent length-delimited
    assert_eq!(
        Choice::parse(msg.as_slice()).err(),
        Some(DecodeError::UnexpectedWireType {
            field: 2,
            wire_type: WireType::LengthDelimited
        })
    );
}
