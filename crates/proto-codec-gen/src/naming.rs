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
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box",
        "do", "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield",
        "try",
    ];
    if RESERVED.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}
