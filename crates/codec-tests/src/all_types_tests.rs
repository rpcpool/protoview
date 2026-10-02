//! `AllTypes` round trips, encoded by `prost` as an independent reference encoder.

use prost::Message as _;

use crate::fixtures::all_types::{AllTypes, Leaf};

/// Hand-derived `prost` mirrors of `proto/all_types.proto`, so no `protoc` is needed.
mod pb {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Leaf {
        #[prost(uint32, tag = "1")]
        pub id: u32,
        #[prost(string, tag = "2")]
        pub name: String,
        #[prost(message, repeated, tag = "3")]
        pub children: Vec<Leaf>,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct AllTypes {
        #[prost(double, tag = "1")]
        pub f_double: f64,
        #[prost(double, optional, tag = "21")]
        pub o_double: Option<f64>,
        #[prost(double, repeated, tag = "41")]
        pub r_double: Vec<f64>,
        #[prost(float, tag = "2")]
        pub f_float: f32,
        #[prost(float, optional, tag = "22")]
        pub o_float: Option<f32>,
        #[prost(float, repeated, tag = "42")]
        pub r_float: Vec<f32>,
        #[prost(int32, tag = "3")]
        pub f_int32: i32,
        #[prost(int32, optional, tag = "23")]
        pub o_int32: Option<i32>,
        #[prost(int32, repeated, tag = "43")]
        pub r_int32: Vec<i32>,
        #[prost(int64, tag = "4")]
        pub f_int64: i64,
        #[prost(int64, optional, tag = "24")]
        pub o_int64: Option<i64>,
        #[prost(int64, repeated, tag = "44")]
        pub r_int64: Vec<i64>,
        #[prost(uint32, tag = "5")]
        pub f_uint32: u32,
        #[prost(uint32, optional, tag = "25")]
        pub o_uint32: Option<u32>,
        #[prost(uint32, repeated, tag = "45")]
        pub r_uint32: Vec<u32>,
        #[prost(uint64, tag = "6")]
        pub f_uint64: u64,
        #[prost(uint64, optional, tag = "26")]
        pub o_uint64: Option<u64>,
        #[prost(uint64, repeated, tag = "46")]
        pub r_uint64: Vec<u64>,
        #[prost(sint32, tag = "7")]
        pub f_sint32: i32,
        #[prost(sint32, optional, tag = "27")]
        pub o_sint32: Option<i32>,
        #[prost(sint32, repeated, tag = "47")]
        pub r_sint32: Vec<i32>,
        #[prost(sint64, tag = "8")]
        pub f_sint64: i64,
        #[prost(sint64, optional, tag = "28")]
        pub o_sint64: Option<i64>,
        #[prost(sint64, repeated, tag = "48")]
        pub r_sint64: Vec<i64>,
        #[prost(fixed32, tag = "9")]
        pub f_fixed32: u32,
        #[prost(fixed32, optional, tag = "29")]
        pub o_fixed32: Option<u32>,
        #[prost(fixed32, repeated, tag = "49")]
        pub r_fixed32: Vec<u32>,
        #[prost(fixed64, tag = "10")]
        pub f_fixed64: u64,
        #[prost(fixed64, optional, tag = "30")]
        pub o_fixed64: Option<u64>,
        #[prost(fixed64, repeated, tag = "50")]
        pub r_fixed64: Vec<u64>,
        #[prost(sfixed32, tag = "11")]
        pub f_sfixed32: i32,
        #[prost(sfixed32, optional, tag = "31")]
        pub o_sfixed32: Option<i32>,
        #[prost(sfixed32, repeated, tag = "51")]
        pub r_sfixed32: Vec<i32>,
        #[prost(sfixed64, tag = "12")]
        pub f_sfixed64: i64,
        #[prost(sfixed64, optional, tag = "32")]
        pub o_sfixed64: Option<i64>,
        #[prost(sfixed64, repeated, tag = "52")]
        pub r_sfixed64: Vec<i64>,
        #[prost(bool, tag = "13")]
        pub f_bool: bool,
        #[prost(bool, optional, tag = "33")]
        pub o_bool: Option<bool>,
        #[prost(bool, repeated, tag = "53")]
        pub r_bool: Vec<bool>,
        #[prost(string, tag = "14")]
        pub f_string: String,
        #[prost(string, optional, tag = "34")]
        pub o_string: Option<String>,
        #[prost(string, repeated, tag = "54")]
        pub r_string: Vec<String>,
        #[prost(bytes = "vec", tag = "15")]
        pub f_bytes: Vec<u8>,
        #[prost(bytes = "vec", optional, tag = "35")]
        pub o_bytes: Option<Vec<u8>>,
        #[prost(bytes = "vec", repeated, tag = "55")]
        pub r_bytes: Vec<Vec<u8>>,
        #[prost(message, optional, tag = "16")]
        pub f_message: Option<Leaf>,
        #[prost(message, optional, tag = "36")]
        pub o_message: Option<Leaf>,
        #[prost(message, repeated, tag = "56")]
        pub r_message: Vec<Leaf>,
    }
}

fn assert_leaf<B: AsRef<[u8]>>(lens: &Leaf<B>, expected: &pb::Leaf) {
    assert_eq!(lens.id(), expected.id);
    assert_eq!(lens.name().unwrap(), expected.name);
    assert_leaves(lens.children(), &expected.children);
}

fn assert_leaf_opt<B: AsRef<[u8]>>(lens: Option<Leaf<B>>, expected: Option<&pb::Leaf>) {
    match (lens, expected) {
        (Some(lens), Some(expected)) => assert_leaf(&lens, expected),
        (None, None) => {}
        (lens, expected) => panic!(
            "presence mismatch: lens {}, expected {}",
            lens.is_some(),
            expected.is_some()
        ),
    }
}

fn assert_leaves<B: AsRef<[u8]>>(lens: impl Iterator<Item = Leaf<B>>, expected: &[pb::Leaf]) {
    let lens: Vec<_> = lens.collect();
    assert_eq!(lens.len(), expected.len(), "element count");
    for (lens, expected) in lens.iter().zip(expected) {
        assert_leaf(lens, expected);
    }
}

/// Checks every getter of `lens` against the message it was encoded from.
fn assert_all(lens: &AllTypes<&[u8]>, expected: &pb::AllTypes) {
    assert_eq!(lens.f_double(), expected.f_double, "f_double");
    assert_eq!(lens.o_double(), expected.o_double, "o_double");
    assert_eq!(
        lens.r_double().collect::<Vec<_>>(),
        expected.r_double,
        "r_double"
    );
    assert_eq!(lens.f_float(), expected.f_float, "f_float");
    assert_eq!(lens.o_float(), expected.o_float, "o_float");
    assert_eq!(
        lens.r_float().collect::<Vec<_>>(),
        expected.r_float,
        "r_float"
    );
    assert_eq!(lens.f_int32(), expected.f_int32, "f_int32");
    assert_eq!(lens.o_int32(), expected.o_int32, "o_int32");
    assert_eq!(
        lens.r_int32().collect::<Vec<_>>(),
        expected.r_int32,
        "r_int32"
    );
    assert_eq!(lens.f_int64(), expected.f_int64, "f_int64");
    assert_eq!(lens.o_int64(), expected.o_int64, "o_int64");
    assert_eq!(
        lens.r_int64().collect::<Vec<_>>(),
        expected.r_int64,
        "r_int64"
    );
    assert_eq!(lens.f_uint32(), expected.f_uint32, "f_uint32");
    assert_eq!(lens.o_uint32(), expected.o_uint32, "o_uint32");
    assert_eq!(
        lens.r_uint32().collect::<Vec<_>>(),
        expected.r_uint32,
        "r_uint32"
    );
    assert_eq!(lens.f_uint64(), expected.f_uint64, "f_uint64");
    assert_eq!(lens.o_uint64(), expected.o_uint64, "o_uint64");
    assert_eq!(
        lens.r_uint64().collect::<Vec<_>>(),
        expected.r_uint64,
        "r_uint64"
    );
    assert_eq!(lens.f_sint32(), expected.f_sint32, "f_sint32");
    assert_eq!(lens.o_sint32(), expected.o_sint32, "o_sint32");
    assert_eq!(
        lens.r_sint32().collect::<Vec<_>>(),
        expected.r_sint32,
        "r_sint32"
    );
    assert_eq!(lens.f_sint64(), expected.f_sint64, "f_sint64");
    assert_eq!(lens.o_sint64(), expected.o_sint64, "o_sint64");
    assert_eq!(
        lens.r_sint64().collect::<Vec<_>>(),
        expected.r_sint64,
        "r_sint64"
    );
    assert_eq!(lens.f_fixed32(), expected.f_fixed32, "f_fixed32");
    assert_eq!(lens.o_fixed32(), expected.o_fixed32, "o_fixed32");
    assert_eq!(
        lens.r_fixed32().collect::<Vec<_>>(),
        expected.r_fixed32,
        "r_fixed32"
    );
    assert_eq!(lens.f_fixed64(), expected.f_fixed64, "f_fixed64");
    assert_eq!(lens.o_fixed64(), expected.o_fixed64, "o_fixed64");
    assert_eq!(
        lens.r_fixed64().collect::<Vec<_>>(),
        expected.r_fixed64,
        "r_fixed64"
    );
    assert_eq!(lens.f_sfixed32(), expected.f_sfixed32, "f_sfixed32");
    assert_eq!(lens.o_sfixed32(), expected.o_sfixed32, "o_sfixed32");
    assert_eq!(
        lens.r_sfixed32().collect::<Vec<_>>(),
        expected.r_sfixed32,
        "r_sfixed32"
    );
    assert_eq!(lens.f_sfixed64(), expected.f_sfixed64, "f_sfixed64");
    assert_eq!(lens.o_sfixed64(), expected.o_sfixed64, "o_sfixed64");
    assert_eq!(
        lens.r_sfixed64().collect::<Vec<_>>(),
        expected.r_sfixed64,
        "r_sfixed64"
    );
    assert_eq!(lens.f_bool(), expected.f_bool, "f_bool");
    assert_eq!(lens.o_bool(), expected.o_bool, "o_bool");
    assert_eq!(lens.r_bool().collect::<Vec<_>>(), expected.r_bool, "r_bool");
    assert_eq!(lens.f_string().unwrap(), expected.f_string);
    assert_eq!(
        lens.o_string().map(Result::unwrap),
        expected.o_string.as_deref()
    );
    assert_eq!(
        lens.r_string().collect::<Result<Vec<_>, _>>().unwrap(),
        expected.r_string
    );
    assert_eq!(lens.f_bytes(), expected.f_bytes.as_slice());
    assert_eq!(lens.o_bytes(), expected.o_bytes.as_deref());
    assert_eq!(lens.r_bytes().collect::<Vec<_>>(), expected.r_bytes);
    assert_leaf_opt(lens.f_message(), expected.f_message.as_ref());
    assert_leaf_opt(lens.o_message(), expected.o_message.as_ref());
    assert_leaves(lens.r_message(), &expected.r_message);
}

fn leaf(id: u32, name: &str, children: Vec<pb::Leaf>) -> pb::Leaf {
    pb::Leaf {
        id,
        name: name.to_owned(),
        children,
    }
}

/// Every field set, with values chosen to hit sign handling, multi-byte varints, and
/// extremes of each type.
fn populated() -> pb::AllTypes {
    pb::AllTypes {
        f_double: -1.5,
        f_float: 2.25,
        f_int32: -7,
        f_int64: i64::MIN,
        f_uint32: u32::MAX,
        f_uint64: u64::MAX,
        f_sint32: i32::MIN,
        f_sint64: -300,
        f_fixed32: 0xdead_beef,
        f_fixed64: 1 << 63,
        f_sfixed32: -1,
        f_sfixed64: i64::MAX,
        f_bool: true,
        f_string: "héllo".to_owned(),
        f_bytes: vec![0, 1, 0xff],
        f_message: Some(leaf(1, "single", vec![leaf(2, "grandchild", vec![])])),

        o_double: Some(3.0),
        o_float: Some(-0.5),
        o_int32: Some(i32::MIN),
        o_int64: Some(-1),
        o_uint32: Some(300),
        o_uint64: Some(1 << 40),
        o_sint32: Some(-64),
        o_sint64: Some(i64::MAX),
        o_fixed32: Some(1),
        o_fixed64: Some(u64::MAX),
        o_sfixed32: Some(i32::MAX),
        o_sfixed64: Some(i64::MIN),
        o_bool: Some(true),
        o_string: Some("optional".to_owned()),
        o_bytes: Some(vec![9; 200]),
        o_message: Some(leaf(3, "optional", vec![])),

        r_double: vec![0.0, -1.0, f64::MAX],
        r_float: vec![1.0, f32::MIN_POSITIVE],
        r_int32: vec![-1, 0, i32::MAX],
        r_int64: vec![i64::MIN, 5],
        r_uint32: vec![0, 127, 128, u32::MAX],
        r_uint64: vec![u64::MAX, 1],
        r_sint32: vec![-1, 1, i32::MIN],
        r_sint64: vec![i64::MIN, i64::MAX],
        r_fixed32: vec![1, 2, 3],
        r_fixed64: vec![u64::MAX],
        r_sfixed32: vec![-2, 2],
        r_sfixed64: vec![-9, 9],
        r_bool: vec![true, false, true],
        r_string: vec!["a".to_owned(), String::new(), "ü".to_owned()],
        r_bytes: vec![vec![], vec![1, 2]],
        r_message: vec![
            leaf(10, "first", vec![leaf(11, "nested", vec![])]),
            leaf(0, "", vec![]),
            leaf(
                20,
                "third",
                vec![leaf(21, "a", vec![]), leaf(22, "b", vec![])],
            ),
        ],
    }
}

fn round_trip(expected: &pb::AllTypes) {
    let bytes = expected.encode_to_vec();
    let lens = AllTypes::parse(bytes.as_slice()).unwrap();
    assert_all(&lens, expected);
}

#[test]
fn every_field_populated() {
    round_trip(&populated());
}

#[test]
fn empty_message_yields_defaults_none_and_empty() {
    let lens = AllTypes::parse(&[][..]).unwrap();
    assert_all(&lens, &pb::AllTypes::default());
    assert_eq!(lens.f_string().unwrap(), "");
    assert_eq!(lens.o_uint64(), None);
    assert!(lens.f_message().is_none());
    assert_eq!(lens.r_message().count(), 0);
}

#[test]
fn optional_fields_set_to_defaults_are_present() {
    // Explicit presence: `Some(0)` is encoded on the wire and must read back as `Some`,
    // unlike the implicit-presence twin, which is indistinguishable from absent.
    let expected = pb::AllTypes {
        o_double: Some(0.0),
        o_float: Some(0.0),
        o_int32: Some(0),
        o_int64: Some(0),
        o_uint32: Some(0),
        o_uint64: Some(0),
        o_sint32: Some(0),
        o_sint64: Some(0),
        o_fixed32: Some(0),
        o_fixed64: Some(0),
        o_sfixed32: Some(0),
        o_sfixed64: Some(0),
        o_bool: Some(false),
        o_string: Some(String::new()),
        o_bytes: Some(Vec::new()),
        o_message: Some(pb::Leaf::default()),
        ..Default::default()
    };
    round_trip(&expected);
}

#[test]
fn repeated_messages_from_concatenated_encodings_interleave() {
    // Concatenating two encodings is a protobuf merge: repeated fields append (so the
    // second message's elements follow other fields of the first), singular fields take
    // the last value, and each packed field arrives as two separate runs.
    let first = pb::AllTypes {
        f_uint32: 1,
        r_uint32: vec![1, 2],
        r_message: vec![leaf(1, "a", vec![])],
        f_string: "first".to_owned(),
        ..Default::default()
    };
    let second = pb::AllTypes {
        f_uint32: 2,
        r_uint32: vec![3],
        r_message: vec![
            leaf(2, "b", vec![leaf(3, "c", vec![])]),
            leaf(4, "d", vec![]),
        ],
        ..Default::default()
    };
    let mut bytes = first.encode_to_vec();
    bytes.extend(second.encode_to_vec());

    let mut merged = first.clone();
    merged.merge(second.encode_to_vec().as_slice()).unwrap();
    assert_eq!(merged.r_message.len(), 3, "prost agrees on merge semantics");

    let lens = AllTypes::parse(bytes.as_slice()).unwrap();
    assert_all(&lens, &merged);
    assert_eq!(lens.f_uint32(), 2);
    assert_eq!(lens.r_uint32().collect::<Vec<_>>(), [1, 2, 3]);
}

#[test]
fn malformed_grandchild_in_repeated_message_fails_parse() {
    let mut bytes = pb::AllTypes {
        r_message: vec![leaf(1, "ok", vec![])],
        ..Default::default()
    }
    .encode_to_vec();
    // An r_message (56) element that is itself well-formed — one `children` (3) entry,
    // in bounds — but whose child is a lone varint tag with no value. Only a walk that
    // descends two levels can see it.
    let element = [0x1a, 0x01, 0x08];
    bytes.extend([0xc2, 0x03, element.len() as u8]);
    bytes.extend(element);

    assert_eq!(
        AllTypes::parse(bytes.as_slice()).err(),
        Some(proto_codec::DecodeError::UnexpectedEof)
    );
}
