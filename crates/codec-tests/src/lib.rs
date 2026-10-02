//! Compiles `proto/` with `proto-codec-gen` and exercises the result.
//!
//! Not published: generated code that fails to compile is the most likely failure
//! mode for a codegen crate, and the only way to catch it is to generate and compile
//! it, which is what `cargo test` here does.

// One generated file per proto package; the module tree mirrors the package hierarchy,
// which the generated `super::` paths between packages rely on.
pub mod fixtures {
    pub mod all_types {
        include!(concat!(env!("OUT_DIR"), "/fixtures.all_types.rs"));
    }
    pub mod oneof {
        include!(concat!(env!("OUT_DIR"), "/fixtures.oneof.rs"));
    }
    pub mod nested {
        include!(concat!(env!("OUT_DIR"), "/fixtures.nested.rs"));
    }
    pub mod repeated {
        include!(concat!(env!("OUT_DIR"), "/fixtures.repeated.rs"));
    }
}
pub mod geyser {
    include!(concat!(env!("OUT_DIR"), "/geyser.rs"));
}
pub mod google {
    pub mod protobuf {
        include!(concat!(env!("OUT_DIR"), "/google.protobuf.rs"));
    }
}
pub mod solana {
    pub mod storage {
        pub mod confirmed_block {
            include!(concat!(env!("OUT_DIR"), "/solana.storage.confirmed_block.rs"));
        }
    }
}

#[cfg(test)]
mod all_types_tests;
#[cfg(test)]
mod oneof_tests;
#[cfg(test)]
mod yellowstone_tests;

#[cfg(test)]
mod tests {
    use super::fixtures::nested::{Inner, Outer};

    fn tag(field: u32, wire_type: u8) -> u8 {
        ((field << 3) | u32::from(wire_type)) as u8
    }

    fn encode_len_delim(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
        out.push(tag(field, 2));
        out.push(bytes.len() as u8);
        out.extend_from_slice(bytes);
    }

    #[test]
    fn decodes_nested_scalars_and_defaults() {
        let mut inner = Vec::new();
        inner.push(tag(1, 1)); // double ratio = 1
        inner.extend_from_slice(&1.5f64.to_le_bytes());
        encode_len_delim(3, b"inner-text", &mut inner);
        // scale (field 2) and alt_text (field 4) left absent.

        let mut msg = Vec::new();
        msg.push(tag(1, 0)); // id
        msg.push(7);
        encode_len_delim(2, b"outer-name", &mut msg);
        encode_len_delim(3, &inner, &mut msg);
        // owner (field 4) left absent.
        encode_len_delim(5, b"outer-label", &mut msg);

        let outer = Outer::parse(msg.as_slice()).unwrap();
        assert_eq!(outer.id(), 7);
        assert_eq!(
            outer.name().unwrap(),
            "outer-name"
        );
        assert_eq!(outer.owner().unwrap(), "");
        assert_eq!(
            outer.label().unwrap(),
            "outer-label"
        );

        let inner: Inner<&[u8]> = outer.inner().unwrap();
        assert_eq!(inner.ratio(), 1.5);
        assert_eq!(inner.scale(), 0);
        assert_eq!(inner.text().unwrap(), "inner-text");
        assert_eq!(inner.alt_text().unwrap(), "");
    }

    #[test]
    fn missing_nested_message_is_none() {
        let msg = vec![tag(1, 0), 3];

        let outer = Outer::parse(msg.as_slice()).unwrap();
        assert_eq!(outer.id(), 3);
        assert!(outer.inner().is_none());
    }
}

#[cfg(test)]
mod repeated_tests {
    use super::fixtures::repeated::{Collection, Tree};
    use proto_codec::{DecodeError, MAX_DEPTH, WireType};

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

    fn len_delim(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
        key(field, 2, out);
        varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }

    fn uint(field: u32, value: u64, out: &mut Vec<u8>) {
        key(field, 0, out);
        varint(value, out);
    }

    fn item(id: u64, name: &str) -> Vec<u8> {
        let mut out = Vec::new();
        uint(1, id, &mut out);
        len_delim(2, name.as_bytes(), &mut out);
        out
    }

    #[test]
    fn interleaved_elements_come_back_in_wire_order() {
        let mut msg = Vec::new();
        len_delim(2, &item(1, "a"), &mut msg);
        len_delim(3, b"x", &mut msg);
        uint(1, 42, &mut msg);
        len_delim(2, &item(2, "b"), &mut msg);
        len_delim(99, b"unknown", &mut msg);
        len_delim(3, b"y", &mut msg);
        len_delim(11, b"note", &mut msg);
        len_delim(2, &item(3, "c"), &mut msg);

        let c = Collection::parse(msg.as_slice()).unwrap();
        assert_eq!(c.slot(), 42);
        assert_eq!(c.note().unwrap(), "note");

        let items: Vec<(u32, String)> = c
            .items()
            .map(|item| (item.id(), item.name().unwrap().to_owned()))
            .collect();
        assert_eq!(items, [(1, "a".into()), (2, "b".into()), (3, "c".into())]);

        let tags: Vec<&str> = c.tags().map(Result::unwrap).collect();
        assert_eq!(tags, ["x", "y"]);
    }

    #[test]
    fn absent_repeated_fields_are_empty() {
        let mut msg = Vec::new();
        uint(1, 7, &mut msg);

        let c = Collection::parse(msg.as_slice()).unwrap();
        assert_eq!(c.items().count(), 0);
        assert_eq!(c.tags().count(), 0);
        assert_eq!(c.blobs().count(), 0);
        assert_eq!(c.counts().count(), 0);
        assert_eq!(c.weights().count(), 0);
    }

    #[test]
    fn first_field_in_buffer_can_be_repeated() {
        // The span's start offset is 0 here, so absence must be keyed off the end slot.
        let mut msg = Vec::new();
        uint(5, 9, &mut msg);

        let c = Collection::parse(msg.as_slice()).unwrap();
        assert_eq!(c.counts().collect::<Vec<_>>(), [9]);
    }

    #[test]
    fn packed_and_unpacked_runs_merge() {
        let mut packed = Vec::new();
        varint(1, &mut packed);
        varint(300, &mut packed);

        let mut msg = Vec::new();
        len_delim(5, &packed, &mut msg);
        uint(1, 1, &mut msg);
        uint(5, 2, &mut msg);
        len_delim(5, &[0x03], &mut msg);

        let c = Collection::parse(msg.as_slice()).unwrap();
        assert_eq!(c.counts().collect::<Vec<_>>(), [1, 300, 2, 3]);
    }

    #[test]
    fn numeric_element_types_decode() {
        let mut deltas = Vec::new();
        for zigzag in [1u64, 2, 3] {
            varint(zigzag, &mut deltas); // -1, 1, -2
        }
        let mut stamps = Vec::new();
        stamps.extend_from_slice(&10u64.to_le_bytes());
        stamps.extend_from_slice(&u64::MAX.to_le_bytes());

        let mut msg = Vec::new();
        len_delim(6, &deltas, &mut msg);
        len_delim(7, &[1, 0, 1], &mut msg);
        len_delim(8, &stamps, &mut msg);
        key(9, 5, &mut msg); // sfixed32, unpacked
        msg.extend_from_slice(&(-5i32).to_le_bytes());
        key(10, 5, &mut msg); // float, unpacked
        msg.extend_from_slice(&1.5f32.to_le_bytes());
        len_delim(10, &2.25f32.to_le_bytes(), &mut msg); // float, packed

        let c = Collection::parse(msg.as_slice()).unwrap();
        assert_eq!(c.deltas().collect::<Vec<_>>(), [-1, 1, -2]);
        assert_eq!(c.flags().collect::<Vec<_>>(), [true, false, true]);
        assert_eq!(c.stamps().collect::<Vec<_>>(), [10, u64::MAX]);
        assert_eq!(c.offsets().collect::<Vec<_>>(), [-5]);
        assert_eq!(c.weights().collect::<Vec<_>>(), [1.5, 2.25]);
    }

    #[test]
    fn bytes_and_invalid_utf8_are_yielded_per_element() {
        let mut msg = Vec::new();
        len_delim(4, &[0xde, 0xad], &mut msg);
        len_delim(4, &[], &mut msg);
        len_delim(3, b"ok", &mut msg);
        len_delim(3, &[0xff], &mut msg);

        let c = Collection::parse(msg.as_slice()).unwrap();
        assert_eq!(c.blobs().collect::<Vec<_>>(), [&[0xde, 0xad][..], &[]]);
        let tags: Vec<_> = c.tags().collect();
        assert_eq!(tags[0], Ok("ok"));
        assert!(tags[1].is_err());
        assert_eq!(c.tags_bytes().nth(1), Some(&[0xff][..]));
    }

    #[test]
    fn malformed_nested_element_fails_parse() {
        let mut msg = Vec::new();
        len_delim(2, &item(1, "a"), &mut msg);
        len_delim(2, &[0x0a, 0x05, b'x'], &mut msg); // declares 5 bytes, has 1
        len_delim(2, &item(3, "c"), &mut msg);

        assert_eq!(
            Collection::parse(msg.as_slice()).err(),
            Some(DecodeError::LengthOverflow)
        );
    }

    #[test]
    fn wrong_wire_type_on_known_field_fails_parse() {
        let mut msg = Vec::new();
        uint(2, 1, &mut msg); // `items` is a message field, sent as a varint

        assert_eq!(
            Collection::parse(msg.as_slice()).err(),
            Some(DecodeError::UnexpectedWireType {
                field: 2,
                wire_type: WireType::Varint
            })
        );
    }

    #[test]
    fn packed_run_ending_mid_value_fails_parse() {
        let mut msg = Vec::new();
        len_delim(5, &[0x01, 0x80], &mut msg); // second varint never terminates
        assert_eq!(
            Collection::parse(msg.as_slice()).err(),
            Some(DecodeError::MalformedPackedField { field: 5 })
        );

        let mut msg = Vec::new();
        len_delim(8, &[0; 12], &mut msg); // fixed64 run of 1.5 values
        assert_eq!(
            Collection::parse(msg.as_slice()).err(),
            Some(DecodeError::MalformedPackedField { field: 8 })
        );
    }

    fn nested_tree(levels: u32) -> Vec<u8> {
        let mut tree = Vec::new();
        for _ in 0..levels {
            let mut parent = Vec::new();
            len_delim(1, &tree, &mut parent);
            tree = parent;
        }
        tree
    }

    #[test]
    fn nesting_is_validated_up_to_the_depth_limit() {
        // `levels` wrappers put the innermost child at depth `levels`.
        let at_limit = nested_tree(MAX_DEPTH);
        fn depth<B: AsRef<[u8]>>(tree: &Tree<B>) -> u32 {
            tree.children().map(|child| 1 + depth(&child)).max().unwrap_or(0)
        }
        let tree = Tree::parse(at_limit.as_slice()).unwrap();
        assert_eq!(depth(&tree), MAX_DEPTH);

        let past_limit = nested_tree(MAX_DEPTH + 1);
        assert_eq!(
            Tree::parse(past_limit.as_slice()).err(),
            Some(DecodeError::RecursionLimitExceeded)
        );
    }
}
