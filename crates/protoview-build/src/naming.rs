//! Identifier conventions shared by the model and codegen stages, matching `prost`.

/// Converts a `PascalCase` or `SHOUTY_CASE` proto package segment into a `snake_case`
/// Rust module name.
pub fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut prev_lower_or_digit = false;
    for ch in name.chars() {
        if ch == '_' {
            out.push('_');
            prev_lower_or_digit = false;
            continue;
        }
        if ch.is_uppercase() {
            if prev_lower_or_digit {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
            prev_lower_or_digit = false;
        } else {
            out.push(ch);
            prev_lower_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

/// Converts a proto name into `UpperCamelCase`, as `prost` does (via `heck`) for message,
/// enum, variant and oneof names: `update_oneof` -> `UpdateOneof`, `SLOT_STATUS` ->
/// `SlotStatus`, `VATDebit` -> `VatDebit`, `dataV2` -> `DataV2`.
///
/// Words break at `_`, at a lower-case letter or digit followed by an upper-case one, and
/// before the last capital of an acronym followed by a lower-case letter (`HTTPServer` ->
/// `Http` + `Server`).
pub fn upper_camel(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len());
    let mut word_start = true;
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' {
            word_start = true;
            continue;
        }
        if i > 0 && ch.is_uppercase() {
            let prev = chars[i - 1];
            let next_is_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if prev.is_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_uppercase() && next_is_lower)
            {
                word_start = true;
            }
        }
        if word_start {
            out.extend(ch.to_uppercase());
        } else {
            out.extend(ch.to_lowercase());
        }
        word_start = false;
    }
    out
}

/// Converts a `package.segment` path into Rust module segments.
pub fn module_path(package: &str) -> Vec<String> {
    package
        .split('.')
        .filter(|s| !s.is_empty())
        .map(snake_case)
        .collect()
}

/// Escapes a Rust reserved word by prefixing it with `r#`, as `prost` does for field and
/// method names.
pub fn escape_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
    ];
    if RESERVED.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{snake_case, upper_camel};

    #[test]
    fn upper_camel_matches_prost() {
        assert_eq!(upper_camel("update_oneof"), "UpdateOneof");
        assert_eq!(upper_camel("transaction_status"), "TransactionStatus");
        assert_eq!(upper_camel("data"), "Data");
        assert_eq!(upper_camel("dataV2"), "DataV2");
        assert_eq!(upper_camel("SLOT_STATUS"), "SlotStatus");
        assert_eq!(upper_camel("field1_x"), "Field1X");
        assert_eq!(upper_camel("VATDebit"), "VatDebit");
        assert_eq!(upper_camel("HTTPServer"), "HttpServer");
        assert_eq!(
            upper_camel("SubscribeUpdateAccountInfo"),
            "SubscribeUpdateAccountInfo"
        );
        assert_eq!(
            upper_camel("SLOT_FIRST_SHRED_RECEIVED"),
            "SlotFirstShredReceived"
        );
        assert_eq!(upper_camel("UiTokenAmount"), "UiTokenAmount");
    }

    #[test]
    fn snake_case_matches_prost() {
        assert_eq!(snake_case("SubscribeUpdate"), "subscribe_update");
        assert_eq!(snake_case("ConfirmedBlock"), "confirmed_block");
    }
}
