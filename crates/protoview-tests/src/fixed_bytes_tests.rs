//! `fixed_bytes` fields: `[u8; N]` getters, and `parse` rejecting any other length —
//! including absence of a plain field — instead of padding or truncating.

use protoview::DecodeError;

use crate::fixtures::fixed::Account;
use crate::fixtures::fixed::account::Id;

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

/// An `Account` with every fixed field at its configured length; `extra` appends more.
fn account(extra: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut msg = Vec::new();
    len_delim(1, &[1; 32], &mut msg);
    extra(&mut msg);
    msg
}

fn mismatch(field: u32, expected: u32, actual: u32) -> Option<DecodeError> {
    Some(DecodeError::FixedBytesLenMismatch {
        field,
        expected,
        actual,
    })
}

#[test]
fn getters_return_arrays_by_value() {
    let msg = account(|m| {
        len_delim(2, &[2; 64], m);
        len_delim(3, &[3; 32], m);
        len_delim(6, b"free-form", m);
        len_delim(3, &[4; 32], m);
        len_delim(4, &[5; 8], m);
        let mut owner = Vec::new();
        len_delim(1, &[6; 16], &mut owner);
        len_delim(7, &owner, m);
    });
    let account = Account::parse(msg.as_slice()).unwrap();

    let pubkey: [u8; 32] = account.pubkey();
    assert_eq!(pubkey, [1; 32]);
    let signature: Option<[u8; 64]> = account.signature();
    assert_eq!(signature, Some([2; 64]));
    let history: Vec<[u8; 32]> = account.history().collect();
    assert_eq!(history, [[3; 32], [4; 32]]);
    assert!(matches!(
        account.id(),
        Some(Id::Hash([5, 5, 5, 5, 5, 5, 5, 5]))
    ));
    let free: &[u8] = account.free();
    assert_eq!(free, b"free-form");
    let key: [u8; 16] = account.owner().unwrap().key();
    assert_eq!(key, [6; 16]);
}

#[test]
fn optional_repeated_and_oneof_may_be_absent() {
    let msg = account(|_| {});
    let account = Account::parse(msg.as_slice()).unwrap();
    assert_eq!(account.signature(), None);
    assert_eq!(account.history().count(), 0);
    assert!(account.id().is_none());
    assert!(account.owner().is_none());
}

#[test]
fn wrong_lengths_fail_parse() {
    let mut short = Vec::new();
    len_delim(1, &[1; 31], &mut short);
    assert_eq!(Account::parse(short.as_slice()).err(), mismatch(1, 32, 31));

    let long = account(|m| len_delim(2, &[2; 65], m));
    assert_eq!(Account::parse(long.as_slice()).err(), mismatch(2, 64, 65));

    // One bad element in a repeated field rejects the message.
    let history = account(|m| {
        len_delim(3, &[3; 32], m);
        len_delim(3, &[3; 33], m);
    });
    assert_eq!(
        Account::parse(history.as_slice()).err(),
        mismatch(3, 32, 33)
    );

    let oneof = account(|m| len_delim(4, &[5; 7], m));
    assert_eq!(Account::parse(oneof.as_slice()).err(), mismatch(4, 8, 7));

    // Inside a nested message, validated recursively like everything else.
    let mut owner = Vec::new();
    len_delim(1, &[6; 15], &mut owner);
    let nested = account(|m| len_delim(7, &owner, m));
    assert_eq!(Account::parse(nested.as_slice()).err(), mismatch(1, 16, 15));
}

#[test]
fn absent_or_empty_plain_field_fails_parse() {
    // proto3 encodes an empty `bytes` by omitting it; zero bytes is not 32 bytes.
    assert_eq!(Account::parse(&[][..]).err(), mismatch(1, 32, 0));

    let mut empty = Vec::new();
    len_delim(1, &[], &mut empty);
    assert_eq!(Account::parse(empty.as_slice()).err(), mismatch(1, 32, 0));

    // A nested message missing its fixed field is rejected too.
    let nested = account(|m| len_delim(7, &[], m));
    assert_eq!(Account::parse(nested.as_slice()).err(), mismatch(1, 16, 0));
}

#[test]
fn last_occurrence_is_the_one_checked_and_returned() {
    // Every occurrence is length-checked, and the last one wins, as for any field.
    let msg = account(|m| len_delim(1, &[9; 32], m));
    assert_eq!(Account::parse(msg.as_slice()).unwrap().pubkey(), [9; 32]);

    let bad_first = {
        let mut m = Vec::new();
        len_delim(1, &[1; 3], &mut m);
        len_delim(1, &[1; 32], &mut m);
        m
    };
    assert_eq!(
        Account::parse(bad_first.as_slice()).err(),
        mismatch(1, 32, 3)
    );
}
