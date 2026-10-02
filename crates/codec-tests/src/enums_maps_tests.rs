//! Enums and maps: round trips through hand-derived `prost` mirrors of
//! `proto/enums.proto` and `proto/maps.proto`, plus hand-encoded layouts `prost` never
//! emits (duplicate keys, entries missing their key or value, unknown entry fields).

use std::collections::HashMap;

use prost::Message as _;
use proto_codec::{DecodeError, WireType};

use crate::fixtures::enums::palette::{self, Shade, Swatch};
use crate::fixtures::enums::{Aliased, Color, HasUnknown, Level, Palette};
use crate::fixtures::maps::Maps;

/// Hand-derived `prost` mirrors, so no `protoc` is needed.
mod pb {
    use std::collections::HashMap;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
    #[repr(i32)]
    pub enum Color {
        Unspecified = 0,
        Red = 1,
        Green = 2,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
    #[repr(i32)]
    pub enum Level {
        Low = 0,
        High = 5,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Label {
        #[prost(string, tag = "1")]
        pub text: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Swatch {
        #[prost(enumeration = "Color", tag = "1")]
        pub color: i32,
        #[prost(int32, tag = "2")]
        pub shade: i32,
        #[prost(message, optional, tag = "3")]
        pub label: Option<Label>,
    }

    // Mirrors the proto's member names, prefix and all.
    #[derive(Clone, PartialEq, prost::Oneof)]
    #[allow(clippy::enum_variant_names)]
    pub enum Pick {
        #[prost(enumeration = "Color", tag = "7")]
        PickedColor(i32),
        #[prost(enumeration = "Level", tag = "8")]
        PickedLevel(i32),
        #[prost(message, tag = "9")]
        PickedLabel(Label),
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Palette {
        #[prost(enumeration = "Color", tag = "1")]
        pub primary: i32,
        #[prost(enumeration = "Color", optional, tag = "2")]
        pub accent: Option<i32>,
        #[prost(enumeration = "Color", repeated, tag = "3")]
        pub history: Vec<i32>,
        #[prost(int32, tag = "4")]
        pub shade: i32,
        #[prost(message, optional, tag = "5")]
        pub swatch: Option<Swatch>,
        #[prost(message, repeated, tag = "6")]
        pub swatches: Vec<Swatch>,
        #[prost(oneof = "Pick", tags = "7, 8, 9")]
        pub pick: Option<Pick>,
        #[prost(int32, tag = "10")]
        pub aliased: i32,
        #[prost(int32, tag = "11")]
        pub flagged: i32,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Value {
        #[prost(uint32, tag = "1")]
        pub id: u32,
        #[prost(string, tag = "2")]
        pub name: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Inner {
        #[prost(double, tag = "1")]
        pub ratio: f64,
        #[prost(uint32, tag = "2")]
        pub scale: u32,
        #[prost(string, tag = "3")]
        pub text: String,
        #[prost(string, tag = "4")]
        pub alt_text: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Maps {
        #[prost(map = "string, string", tag = "1")]
        pub labels: HashMap<String, String>,
        #[prost(map = "int32, message", tag = "2")]
        pub by_id: HashMap<i32, Value>,
        #[prost(map = "uint64, bytes", tag = "3")]
        pub blobs: HashMap<u64, Vec<u8>>,
        #[prost(map = "sint64, double", tag = "4")]
        pub scores: HashMap<i64, f64>,
        #[prost(map = "bool, enumeration(Color)", tag = "5")]
        pub flags: HashMap<bool, i32>,
        #[prost(map = "fixed32, message", tag = "6")]
        pub foreign: HashMap<u32, Inner>,
        #[prost(map = "string, int64", tag = "7")]
        pub counters: HashMap<String, i64>,
        #[prost(uint32, tag = "8")]
        pub between: u32,
        #[prost(map = "sfixed64, uint32", tag = "9")]
        pub wide: HashMap<i64, u32>,
    }
}

// ---------------------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------------------

#[test]
fn enum_names_follow_prost() {
    // Each of these is a compile-time check as much as a runtime one.
    assert_eq!(Color::Red.to_i32(), 1);
    assert_eq!(Color::Unspecified.as_str_name(), Some("COLOR_UNSPECIFIED"));
    assert_eq!(Level::High.to_i32(), 5);
    assert_eq!(Level::from_i32(0), Level::Low);
    assert_eq!(Shade::Dark.to_i32(), 1);
    // Aliases keep the first name for a number.
    assert_eq!(Aliased::from_i32(0), Aliased::First);
    assert_eq!(Aliased::Second.as_str_name(), Some("SECOND"));
    // A declared UNKNOWN takes the name; the catch-all moves aside.
    assert_eq!(HasUnknown::from_i32(1), HasUnknown::Unknown);
    assert_eq!(HasUnknown::from_i32(9), HasUnknown::Unrecognized(9));
}

#[test]
fn undeclared_enum_values_are_kept() {
    for value in [3, 42, -1, i32::MIN, i32::MAX] {
        let color = Color::from_i32(value);
        assert_eq!(color, Color::Unknown(value));
        assert_eq!(color.to_i32(), value);
        assert_eq!(color.as_str_name(), None);
    }
    assert_eq!(Color::default(), Color::Unspecified);
    assert_eq!(Level::default(), Level::Low);
}

fn label(text: &str) -> pb::Label {
    pb::Label {
        text: text.to_owned(),
    }
}

fn swatch(color: pb::Color, shade: i32, text: Option<&str>) -> pb::Swatch {
    pb::Swatch {
        color: color as i32,
        shade,
        label: text.map(label),
    }
}

fn assert_swatch(lens: &Swatch<&[u8]>, expected: &pb::Swatch) {
    let pb::Swatch {
        color,
        shade,
        label,
    } = expected;
    assert_eq!(lens.color().to_i32(), *color);
    assert_eq!(lens.shade().to_i32(), *shade);
    match (lens.label(), label) {
        (Some(lens), Some(expected)) => assert_eq!(lens.text().unwrap(), expected.text),
        (None, None) => {}
        _ => panic!("label presence mismatch"),
    }
}

fn assert_palette(lens: &Palette<&[u8]>, expected: &pb::Palette) {
    let pb::Palette {
        primary,
        accent,
        history,
        shade,
        swatch,
        swatches,
        pick,
        aliased,
        flagged,
    } = expected;
    assert_eq!(lens.primary().to_i32(), *primary, "primary");
    assert_eq!(lens.accent().map(Color::to_i32), *accent, "accent");
    assert_eq!(
        lens.history().map(Color::to_i32).collect::<Vec<_>>(),
        *history,
        "history"
    );
    assert_eq!(lens.shade().to_i32(), *shade, "shade");
    match (lens.swatch(), swatch) {
        (Some(lens), Some(expected)) => assert_swatch(&lens, expected),
        (None, None) => {}
        _ => panic!("swatch presence mismatch"),
    }
    let lens_swatches: Vec<_> = lens.swatches().collect();
    assert_eq!(lens_swatches.len(), swatches.len());
    for (lens, expected) in lens_swatches.iter().zip(swatches) {
        assert_swatch(lens, expected);
    }
    match (lens.pick(), pick) {
        (None, None) => {}
        (Some(palette::Pick::PickedColor(lens)), Some(pb::Pick::PickedColor(expected))) => {
            assert_eq!(lens.to_i32(), *expected)
        }
        (Some(palette::Pick::PickedLevel(lens)), Some(pb::Pick::PickedLevel(expected))) => {
            assert_eq!(lens.to_i32(), *expected)
        }
        (Some(palette::Pick::PickedLabel(lens)), Some(pb::Pick::PickedLabel(expected))) => {
            assert_eq!(lens.text().unwrap(), expected.text)
        }
        _ => panic!("pick mismatch"),
    }
    assert_eq!(lens.aliased().to_i32(), *aliased, "aliased");
    assert_eq!(lens.flagged().to_i32(), *flagged, "flagged");
}

fn round_trip_palette(expected: &pb::Palette) {
    let bytes = expected.encode_to_vec();
    let lens = Palette::parse(bytes.as_slice()).unwrap();
    assert_palette(&lens, expected);
}

#[test]
fn palette_round_trips_through_prost() {
    for pick in [
        None,
        Some(pb::Pick::PickedColor(pb::Color::Green as i32)),
        Some(pb::Pick::PickedLevel(pb::Level::High as i32)),
        Some(pb::Pick::PickedLevel(77)),
        Some(pb::Pick::PickedLabel(label("picked"))),
    ] {
        round_trip_palette(&pb::Palette {
            primary: pb::Color::Red as i32,
            accent: Some(pb::Color::Unspecified as i32),
            history: vec![1, 2, 0, 42, -1],
            shade: 1,
            swatch: Some(swatch(pb::Color::Green, 1, Some("main"))),
            swatches: vec![
                swatch(pb::Color::Red, 0, None),
                swatch(pb::Color::Unspecified, 1, Some("")),
            ],
            pick,
            aliased: 2,
            flagged: 1,
        });
    }
    round_trip_palette(&pb::Palette::default());
}

#[test]
fn undeclared_values_survive_every_position() {
    let expected = pb::Palette {
        primary: 42,
        accent: Some(-7),
        history: vec![99],
        shade: 3,
        swatch: Some(swatch(pb::Color::Red, 9, None)),
        flagged: 9,
        ..Default::default()
    };
    let bytes = expected.encode_to_vec();
    let lens = Palette::parse(bytes.as_slice()).unwrap();
    assert_eq!(lens.primary(), Color::Unknown(42));
    assert_eq!(lens.accent(), Some(Color::Unknown(-7)));
    assert_eq!(lens.history().collect::<Vec<_>>(), [Color::Unknown(99)]);
    assert_eq!(lens.shade(), Shade::Unknown(3));
    assert_eq!(lens.swatch().unwrap().shade(), Shade::Unknown(9));
    assert_eq!(lens.flagged(), HasUnknown::Unrecognized(9));
}

#[test]
fn absent_enums_default_and_optional_enums_have_presence() {
    let lens = Palette::parse(&[][..]).unwrap();
    assert_eq!(lens.primary(), Color::Unspecified);
    assert_eq!(lens.accent(), None);
    assert_eq!(lens.history().count(), 0);
    assert_eq!(lens.shade(), Shade::Light);
    assert!(lens.pick().is_none());

    let bytes = pb::Palette {
        accent: Some(0),
        ..Default::default()
    }
    .encode_to_vec();
    let lens = Palette::parse(bytes.as_slice()).unwrap();
    assert_eq!(lens.accent(), Some(Color::Unspecified));
}

#[test]
fn repeated_enums_merge_packed_and_unpacked() {
    // history (3): packed [1, 2], then unpacked 42, then packed [0].
    let bytes = [0x1a, 0x02, 0x01, 0x02, 0x18, 0x2a, 0x1a, 0x01, 0x00];
    let lens = Palette::parse(&bytes[..]).unwrap();
    assert_eq!(
        lens.history().collect::<Vec<_>>(),
        [
            Color::Red,
            Color::Green,
            Color::Unknown(42),
            Color::Unspecified
        ]
    );
}

#[test]
fn enum_with_wrong_wire_type_fails_parse() {
    let bytes = [0x0a, 0x01, 0x01]; // primary (1) sent length-delimited
    assert_eq!(
        Palette::parse(&bytes[..]).err(),
        Some(DecodeError::UnexpectedWireType {
            field: 1,
            wire_type: WireType::LengthDelimited
        })
    );
}

// ---------------------------------------------------------------------------------------
// Maps
// ---------------------------------------------------------------------------------------

fn value(id: u32, name: &str) -> pb::Value {
    pb::Value {
        id,
        name: name.to_owned(),
    }
}

fn populated_maps() -> pb::Maps {
    pb::Maps {
        labels: HashMap::from([
            ("env".to_owned(), "prod".to_owned()),
            (String::new(), "empty key".to_owned()),
            ("ü".to_owned(), String::new()),
        ]),
        by_id: HashMap::from([
            (1, value(1, "one")),
            (-5, value(0, "")),
            (i32::MAX, value(7, "max")),
        ]),
        blobs: HashMap::from([(0, vec![]), (u64::MAX, vec![1, 2, 3])]),
        scores: HashMap::from([(-1, 0.5), (i64::MIN, -2.25), (300, f64::MAX)]),
        flags: HashMap::from([(true, pb::Color::Green as i32), (false, 42)]),
        foreign: HashMap::from([(
            9,
            pb::Inner {
                ratio: 1.5,
                scale: 6,
                text: "inner".to_owned(),
                alt_text: String::new(),
            },
        )]),
        counters: HashMap::from([("hits".to_owned(), -3), ("misses".to_owned(), i64::MAX)]),
        between: 17,
        wide: HashMap::from([(-1, 1), (i64::MAX, u32::MAX)]),
    }
}

fn assert_maps(lens: &Maps<&[u8]>, expected: &pb::Maps) {
    let pb::Maps {
        labels,
        by_id,
        blobs,
        scores,
        flags,
        foreign,
        counters,
        between,
        wide,
    } = expected;
    let got: HashMap<String, String> = lens
        .labels()
        .map(|(k, v)| (k.unwrap().to_owned(), v.unwrap().to_owned()))
        .collect();
    assert_eq!(&got, labels, "labels");

    let got: HashMap<i32, (u32, String)> = lens
        .by_id()
        .map(|(k, v)| (k, (v.id(), v.name().unwrap().to_owned())))
        .collect();
    let want: HashMap<i32, (u32, String)> = by_id
        .iter()
        .map(|(k, v)| (*k, (v.id, v.name.clone())))
        .collect();
    assert_eq!(got, want, "by_id");

    let got: HashMap<u64, Vec<u8>> = lens.blobs().map(|(k, v)| (k, v.to_vec())).collect();
    assert_eq!(&got, blobs, "blobs");

    let got: HashMap<i64, f64> = lens.scores().collect();
    assert_eq!(&got, scores, "scores");

    let got: HashMap<bool, i32> = lens.flags().map(|(k, v)| (k, v.to_i32())).collect();
    assert_eq!(&got, flags, "flags");

    let got: HashMap<u32, (f64, u32, String, String)> = lens
        .foreign()
        .map(|(k, v)| {
            let fields = (
                v.ratio(),
                v.scale(),
                v.text().unwrap().to_owned(),
                v.alt_text().unwrap().to_owned(),
            );
            (k, fields)
        })
        .collect();
    let want: HashMap<u32, (f64, u32, String, String)> = foreign
        .iter()
        .map(|(k, v)| (*k, (v.ratio, v.scale, v.text.clone(), v.alt_text.clone())))
        .collect();
    assert_eq!(got, want, "foreign");

    let got: HashMap<String, i64> = lens
        .counters()
        .map(|(k, v)| (k.unwrap().to_owned(), v))
        .collect();
    assert_eq!(&got, counters, "counters");

    assert_eq!(lens.between(), *between, "between");

    let got: HashMap<i64, u32> = lens.wide().collect();
    assert_eq!(&got, wide, "wide");
}

#[test]
fn maps_round_trip_through_prost() {
    for expected in [populated_maps(), pb::Maps::default()] {
        let bytes = expected.encode_to_vec();
        let lens = Maps::parse(bytes.as_slice()).unwrap();
        assert_maps(&lens, &expected);
    }
}

#[test]
fn concatenated_encodings_interleave_entries() {
    // Two encodings back to back: every map's entries from the second follow `between`
    // from the first, and duplicate keys appear twice on the wire.
    let first = populated_maps();
    let mut second = pb::Maps::default();
    second.labels.insert("env".to_owned(), "staging".to_owned());
    second.between = 18;
    let mut bytes = first.encode_to_vec();
    bytes.extend(second.encode_to_vec());

    let lens = Maps::parse(bytes.as_slice()).unwrap();
    let env: Vec<&str> = lens
        .labels()
        .filter(|(k, _)| *k == Ok("env"))
        .map(|(_, v)| v.unwrap())
        .collect();
    assert_eq!(env, ["prod", "staging"], "every occurrence, in wire order");

    let mut merged = first.clone();
    merged.merge(second.encode_to_vec().as_slice()).unwrap();
    assert_maps(&lens, &merged);
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push(value as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn len_delim(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
    varint(u64::from(field << 3 | 2), out);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

#[test]
fn entries_missing_key_or_value_read_as_defaults() {
    let mut msg = Vec::new();
    len_delim(1, &[0x12, 0x01, b'v'], &mut msg); // labels: value only
    len_delim(1, &[0x0a, 0x01, b'k'], &mut msg); // labels: key only
    len_delim(1, &[], &mut msg); // labels: neither
    len_delim(2, &[0x08, 0x05], &mut msg); // by_id: key 5, no value message
    len_delim(5, &[0x08, 0x01], &mut msg); // flags: key true, no value

    let lens = Maps::parse(msg.as_slice()).unwrap();
    let labels: Vec<(&str, &str)> = lens
        .labels()
        .map(|(k, v)| (k.unwrap(), v.unwrap()))
        .collect();
    assert_eq!(labels, [("", "v"), ("k", ""), ("", "")]);

    let (key, value) = lens.by_id().next().unwrap();
    assert_eq!(key, 5);
    assert_eq!((value.id(), value.name().unwrap()), (0, ""));

    assert_eq!(
        lens.flags().collect::<Vec<_>>(),
        [(true, Color::Unspecified)]
    );
}

#[test]
fn entry_fields_may_repeat_reorder_and_carry_unknowns() {
    // value "a", unknown field 3, key "k", value "b": key "k" and the last value win.
    let entry = [
        0x12, 0x01, b'a', 0x18, 0x07, 0x0a, 0x01, b'k', 0x12, 0x01, b'b',
    ];
    let mut msg = Vec::new();
    len_delim(1, &entry, &mut msg);
    let lens = Maps::parse(msg.as_slice()).unwrap();
    let labels: Vec<_> = lens
        .labels()
        .map(|(k, v)| (k.unwrap(), v.unwrap()))
        .collect();
    assert_eq!(labels, [("k", "b")]);
}

#[test]
fn map_entries_are_validated() {
    // A by_id (2) value message whose own field is truncated.
    let mut entry = vec![0x08, 0x01];
    len_delim(2, &[0x08], &mut entry);
    let mut msg = Vec::new();
    len_delim(2, &entry, &mut msg);
    assert_eq!(
        Maps::parse(msg.as_slice()).err(),
        Some(DecodeError::UnexpectedEof)
    );

    // A labels (1) key sent as a varint instead of a string.
    let mut msg = Vec::new();
    len_delim(1, &[0x08, 0x01], &mut msg);
    assert_eq!(
        Maps::parse(msg.as_slice()).err(),
        Some(DecodeError::UnexpectedWireType {
            field: 1,
            wire_type: WireType::Varint
        })
    );

    // The map field itself sent as a varint.
    assert_eq!(
        Maps::parse(&[0x08, 0x01][..]).err(),
        Some(DecodeError::UnexpectedWireType {
            field: 1,
            wire_type: WireType::Varint
        })
    );
}
