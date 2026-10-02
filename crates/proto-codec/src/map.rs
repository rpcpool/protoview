//! Reading the entries of `map` fields.
//!
//! On the wire a `map<K, V>` is a repeated message whose entries carry the key as field 1
//! and the value as field 2. Either may be absent, in which case it takes its type's
//! default, and either may repeat, in which case the last occurrence wins.

use crate::wire::Scanner;

/// Locates the key and value of one map entry.
///
/// # Arguments
///
/// * `entry` - The entry's bytes, i.e. one length-delimited record of the map field with
///   its length prefix removed.
///
/// # Returns
///
/// The payload offsets of the key (field 1) and the value (field 2) within `entry`, each
/// `0` when absent. Payload offsets are never `0` for a present field, since a tag always
/// precedes the payload. Unknown fields are skipped. The entry was validated by `parse`,
/// so a malformed entry cannot occur in practice; scanning stops at the first error.
pub fn entry_offsets(entry: &[u8]) -> (u32, u32) {
    let Ok(mut scanner) = Scanner::new(entry) else {
        return (0, 0);
    };
    let (mut key, mut value) = (0, 0);
    while let Ok(Some(field)) = scanner.next_field() {
        match field.number {
            1 => key = field.payload,
            2 => value = field.payload,
            _ => {}
        }
    }
    (key, value)
}
