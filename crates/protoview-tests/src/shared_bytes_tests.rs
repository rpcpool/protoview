//! The `*_owned` getters: with a [`protoview::SharedBytes`] buffer, nested views and
//! `oneof` members own a slice of the parent's buffer instead of borrowing from it.

use bytes::Bytes;

use crate::fixtures::nested::Outer;
use crate::fixtures::oneof::Choice;
use crate::fixtures::oneof::choice::ValueOwned;
use crate::fixtures::repeated::Collection;

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

fn uint(field: u32, value: u64, out: &mut Vec<u8>) {
    varint(u64::from(field << 3), out);
    varint(value, out);
}

fn item(id: u64, name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    uint(1, id, &mut out);
    len_delim(2, name.as_bytes(), &mut out);
    out
}

/// Whether `part` points into `whole`'s allocation, so no copy was made.
fn within(whole: &Bytes, part: &[u8]) -> bool {
    let start = whole.as_ptr() as usize;
    let at = part.as_ptr() as usize;
    at >= start && at + part.len() <= start + whole.len()
}

#[test]
fn repeated_messages_are_owned_slices_that_outlive_the_parent() {
    let mut msg = Vec::new();
    len_delim(2, &item(1, "a"), &mut msg);
    uint(1, 42, &mut msg);
    len_delim(2, &item(2, "b"), &mut msg);
    let bytes = Bytes::from(msg);

    let items: Vec<_> = Collection::parse(bytes.clone())
        .unwrap()
        .items_owned()
        .collect();

    // The parent view is gone; the elements are `'static` and can cross threads.
    let seen = std::thread::spawn(move || {
        items
            .iter()
            .map(|item| (item.id(), item.name().unwrap().to_owned()))
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    assert_eq!(seen, [(1, "a".to_string()), (2, "b".to_string())]);
}

#[test]
fn owned_views_share_the_parents_allocation() {
    let mut msg = Vec::new();
    len_delim(2, &item(7, "x"), &mut msg);
    let bytes = Bytes::from(msg);

    let collection = Collection::parse(bytes.clone()).unwrap();
    let owned = collection.items_owned().next().unwrap().into_inner();
    assert_eq!(owned.as_ref(), item(7, "x"));
    assert!(within(&bytes, &owned));
}

#[test]
fn absent_repeated_and_singular_fields_are_empty() {
    let mut msg = Vec::new();
    uint(1, 3, &mut msg);
    let bytes = Bytes::from(msg);

    assert_eq!(
        Collection::parse(bytes.clone())
            .unwrap()
            .items_owned()
            .count(),
        0
    );
    assert!(Outer::parse(bytes).unwrap().inner_owned().is_none());
}

#[test]
fn singular_message_is_owned() {
    let mut inner = Vec::new();
    len_delim(3, b"inner-text", &mut inner);
    let mut msg = Vec::new();
    uint(1, 7, &mut msg);
    len_delim(3, &inner, &mut msg);
    let bytes = Bytes::from(msg);

    let inner = Outer::parse(bytes.clone()).unwrap().inner_owned().unwrap();
    assert_eq!(inner.text().unwrap(), "inner-text");
    assert!(within(&bytes, inner.into_inner().as_ref()));
}

#[test]
fn borrowed_slices_work_too() {
    let mut msg = Vec::new();
    len_delim(2, &item(5, "q"), &mut msg);

    let collection = Collection::parse(msg.as_slice()).unwrap();
    let owned = collection.items_owned().next().unwrap();
    assert_eq!(owned.id(), 5);
    assert_eq!(owned.name().unwrap(), "q");
}

#[test]
fn oneof_members_that_borrow_become_owned() {
    // `text = 6`, `blob = 7`, `item = 8`, `number = 2`.
    let cases: [(Vec<u8>, &str); 4] = [
        (
            {
                let mut m = Vec::new();
                len_delim(6, b"hello", &mut m);
                m
            },
            "text",
        ),
        (
            {
                let mut m = Vec::new();
                len_delim(7, &[0xde, 0xad], &mut m);
                m
            },
            "blob",
        ),
        (
            {
                let mut m = Vec::new();
                let mut it = Vec::new();
                uint(1, 9, &mut it);
                len_delim(8, &it, &mut m);
                m
            },
            "item",
        ),
        (
            {
                let mut m = Vec::new();
                uint(2, 11, &mut m);
                m
            },
            "number",
        ),
    ];

    for (msg, expected) in cases {
        let bytes = Bytes::from(msg);
        let choice = Choice::parse(bytes.clone()).unwrap();
        match choice.value_owned().expect("a member is set") {
            ValueOwned::Text(text) => {
                assert_eq!(expected, "text");
                assert_eq!(text.as_ref(), b"hello");
                assert!(within(&bytes, &text));
            }
            ValueOwned::Blob(blob) => {
                assert_eq!(expected, "blob");
                assert_eq!(blob.as_ref(), [0xde, 0xad]);
                assert!(within(&bytes, &blob));
            }
            ValueOwned::Item(item) => {
                assert_eq!(expected, "item");
                assert_eq!(item.id(), 9);
            }
            ValueOwned::Number(number) => {
                assert_eq!(expected, "number");
                assert_eq!(number, 11);
            }
            _ => panic!("unexpected member for {expected}"),
        }
    }
}

#[test]
fn absent_oneof_is_none() {
    let mut msg = Vec::new();
    uint(1, 3, &mut msg);
    assert!(
        Choice::parse(Bytes::from(msg))
            .unwrap()
            .value_owned()
            .is_none()
    );
}
