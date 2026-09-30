use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::model::{Field, FieldKind, Message, Model, Scalar};
use crate::naming::escape_ident;

/// Renders a [`Model`] into a single Rust source string, with `pub mod` nesting
/// matching each message's proto package.
pub fn render(model: &Model) -> String {
    let mut root = ModuleNode::default();
    for message in &model.messages {
        root.insert(&message.module, message);
    }

    let mut out = String::new();
    root.render(&mut out, 0);
    out
}

#[derive(Default)]
struct ModuleNode<'a> {
    children: BTreeMap<String, ModuleNode<'a>>,
    messages: Vec<&'a Message>,
}

impl<'a> ModuleNode<'a> {
    fn insert(&mut self, path: &[String], message: &'a Message) {
        match path.split_first() {
            None => self.messages.push(message),
            Some((head, rest)) => self.children.entry(head.clone()).or_default().insert(rest, message),
        }
    }

    fn render(&self, out: &mut String, indent: usize) {
        for message in &self.messages {
            render_message(out, indent, message);
        }
        for (name, child) in &self.children {
            let pad = "    ".repeat(indent);
            let _ = writeln!(out, "{pad}pub mod {name} {{");
            child.render(out, indent + 1);
            let _ = writeln!(out, "{pad}}}");
        }
    }
}

fn render_message(out: &mut String, indent: usize, message: &Message) {
    let pad = "    ".repeat(indent);
    let inner = "    ".repeat(indent + 1);
    let name = &message.rust_name;
    let (slots, slot_count) = index_slots(message);

    let _ = writeln!(out, "{pad}pub struct {name}<B: AsRef<[u8]>> {{");
    let _ = writeln!(out, "{inner}buf: B,");
    let _ = writeln!(out, "{inner}index: [u32; {slot_count}],");
    let _ = writeln!(out, "{pad}}}");
    let _ = writeln!(out, "{pad}impl<B: AsRef<[u8]>> {name}<B> {{");

    render_parse(out, &inner, message, &slots, slot_count);
    for (field, &slot) in message.fields.iter().zip(&slots) {
        render_getter(out, &inner, slot, field);
    }

    let _ = writeln!(out, "{pad}}}");
}

/// Assigns each field its first slot in the index table. A singular field takes one slot
/// holding its payload offset (last occurrence wins); a repeated field takes two, holding
/// the tag offset of its first element and the offset just past its last, so iteration
/// re-walks only that span. Returns the per-field slots and the table length.
fn index_slots(message: &Message) -> (Vec<usize>, usize) {
    let mut next = 0;
    let slots = message
        .fields
        .iter()
        .map(|field| {
            let slot = next;
            next += if field.repeated { 2 } else { 1 };
            slot
        })
        .collect();
    (slots, next)
}

fn render_parse(out: &mut String, pad: &str, message: &Message, slots: &[usize], slot_count: usize) {
    let name = &message.rust_name;
    let _ = writeln!(
        out,
        "{pad}/// Validates `buf` as a `{name}`, including every nested message, and indexes its\n\
         {pad}/// own fields. Nested messages are validated here but indexed lazily, when accessed,\n\
         {pad}/// so no getter on the result, or on any lens reached from it, can fail.\n\
         {pad}///\n\
         {pad}/// # Errors\n\
         {pad}///\n\
         {pad}/// Any [`proto_codec::DecodeError`] found anywhere in the message tree."
    );
    let _ = writeln!(out, "{pad}pub fn parse(buf: B) -> Result<Self, proto_codec::DecodeError> {{");
    let _ = writeln!(out, "{pad}    let index = Self::walk(buf.as_ref(), Some(0))?;");
    let _ = writeln!(out, "{pad}    Ok(Self {{ buf, index }})");
    let _ = writeln!(out, "{pad}}}");

    let _ = writeln!(
        out,
        "{pad}/// Validates `buf` as a `{name}` nested at `depth`, without keeping the index.\n\
         {pad}#[allow(dead_code)]\n\
         {pad}pub(crate) fn validate(buf: &[u8], depth: u32) -> Result<(), proto_codec::DecodeError> {{\n\
         {pad}    Self::walk(buf, Some(depth)).map(|_| ())\n\
         {pad}}}"
    );
    let _ = writeln!(
        out,
        "{pad}/// Indexes `buf`, which an enclosing `parse` has already validated as a `{name}`.\n\
         {pad}/// Skips validation; the fallback to an empty index is unreachable for such input.\n\
         {pad}#[allow(dead_code)]\n\
         {pad}pub(crate) fn from_validated(buf: B) -> Self {{\n\
         {pad}    let index = Self::walk(buf.as_ref(), None).unwrap_or([0; {slot_count}]);\n\
         {pad}    Self {{ buf, index }}\n\
         {pad}}}"
    );

    let _ = writeln!(
        out,
        "{pad}/// Walks `buf`, indexing its fields and, when `depth` is set, validating them and\n\
         {pad}/// every nested message at that depth.\n\
         {pad}#[allow(clippy::single_match)]\n\
         {pad}fn walk(buf: &[u8], depth: Option<u32>) -> Result<[u32; {slot_count}], proto_codec::DecodeError> {{"
    );
    if message.fields.is_empty() {
        let _ = writeln!(out, "{pad}    let _ = depth;");
    }
    let _ = writeln!(out, "{pad}    let mut index = [0u32; {slot_count}];");
    let _ = writeln!(out, "{pad}    let mut scanner = proto_codec::Scanner::new(buf)?;");
    if message.fields.iter().any(|field| field.repeated) {
        let _ = writeln!(out, "{pad}    loop {{");
        let _ = writeln!(out, "{pad}        let tag = scanner.position() as u32;");
        let _ = writeln!(out, "{pad}        let Some(field) = scanner.next_field()? else {{ break }};");
    } else {
        let _ = writeln!(out, "{pad}    while let Some(field) = scanner.next_field()? {{");
    }
    let _ = writeln!(out, "{pad}        match field.number {{");
    for (field, &slot) in message.fields.iter().zip(slots) {
        let arm = format!("{pad}                ");
        let _ = writeln!(out, "{pad}            {number} => {{", number = field.number);
        render_field_validation(out, &arm, field);
        if field.repeated {
            let end = slot + 1;
            let _ = writeln!(out, "{arm}if index[{end}] == 0 {{ index[{slot}] = tag; }}");
            let _ = writeln!(out, "{arm}index[{end}] = scanner.position() as u32;");
        } else {
            let _ = writeln!(out, "{arm}index[{slot}] = field.payload;");
        }
        let _ = writeln!(out, "{pad}            }}");
    }
    let _ = writeln!(out, "{pad}            _ => {{}}");
    let _ = writeln!(out, "{pad}        }}");
    let _ = writeln!(out, "{pad}    }}");
    let _ = writeln!(out, "{pad}    Ok(index)");
    let _ = writeln!(out, "{pad}}}");
}

/// Renders the checks one occurrence of `field` must pass when the walk is validating: its
/// wire type, the contents of a packed run, and, for a message, the nested message itself.
fn render_field_validation(out: &mut String, pad: &str, field: &Field) {
    let expect = |wire_type: &str| {
        format!("proto_codec::wire::expect_wire_type(&field, proto_codec::WireType::{wire_type})?;")
    };
    match &field.kind {
        FieldKind::Scalar(Scalar::String | Scalar::Bytes) => {
            let _ = writeln!(out, "{pad}if depth.is_some() {{ {} }}", expect("LengthDelimited"));
        }
        FieldKind::Scalar(scalar) => {
            let (_, width, _) = scalar_element(*scalar);
            let check = if field.repeated {
                format!(
                    "proto_codec::repeated::validate_numeric(buf, &field, proto_codec::repeated::Width::{width})?;"
                )
            } else {
                // `Width` variants share their names with the matching `WireType`.
                expect(width)
            };
            let _ = writeln!(out, "{pad}if depth.is_some() {{ {check} }}");
        }
        FieldKind::Message(rust_path) => {
            let _ = writeln!(out, "{pad}if let Some(depth) = depth {{");
            let _ = writeln!(out, "{pad}    {}", expect("LengthDelimited"));
            let _ = writeln!(
                out,
                "{pad}    let bytes = proto_codec::wire::read_length_delimited(buf, field.payload as usize)?;"
            );
            let _ = writeln!(
                out,
                "{pad}    {rust_path}::<&[u8]>::validate(bytes, proto_codec::wire::descend(depth)?)?;"
            );
            let _ = writeln!(out, "{pad}}}");
        }
    }
}

fn render_getter(out: &mut String, pad: &str, slot: usize, field: &Field) {
    if field.repeated {
        render_repeated_getter(out, pad, slot, field);
        return;
    }
    let name = escape_ident(&field.name);
    let offset = format!("self.index[{slot}]");

    // A proto3 `optional` field reports absence as `None`; an implicit-presence one
    // substitutes the proto default.
    let wrap = |ty: &str| if field.optional { format!("Option<{ty}>") } else { ty.to_string() };
    let present = |expr: &str| if field.optional { format!("Some({expr})") } else { expr.to_string() };

    match &field.kind {
        FieldKind::Scalar(Scalar::String) => {
            let (str_type, bytes_type) = (
                wrap("Result<&str, core::str::Utf8Error>"),
                wrap("&[u8]"),
            );
            let body = if field.optional {
                format!("self.{name}_bytes().map(core::str::from_utf8)")
            } else {
                format!("core::str::from_utf8(self.{name}_bytes())")
            };
            let _ = writeln!(out, "{pad}pub fn {name}(&self) -> {str_type} {{");
            let _ = writeln!(out, "{pad}    {body}");
            let _ = writeln!(out, "{pad}}}");
            render_bytes_getter(out, pad, &format!("{name}_bytes"), &bytes_type, &offset, field.optional);
        }
        FieldKind::Scalar(Scalar::Bytes) => {
            render_bytes_getter(out, pad, &name, &wrap("&[u8]"), &offset, field.optional);
        }
        FieldKind::Scalar(scalar) => {
            let (rust_type, default, read_expr) = scalar_read(*scalar);
            let absent = if field.optional { "None" } else { default };
            let _ = writeln!(out, "{pad}pub fn {name}(&self) -> {} {{", wrap(rust_type));
            let _ = writeln!(out, "{pad}    let offset = {offset};");
            let _ = writeln!(out, "{pad}    if offset == 0 {{ return {absent}; }}");
            let _ = writeln!(out, "{pad}    let offset = offset as usize;");
            let _ = writeln!(out, "{pad}    {}", present(&read_expr));
            let _ = writeln!(out, "{pad}}}");
        }
        FieldKind::Message(rust_path) => {
            let _ = writeln!(
                out,
                "{pad}pub fn {name}(&self) -> Option<{rust_path}<&[u8]>> {{"
            );
            let _ = writeln!(out, "{pad}    let offset = {offset};");
            let _ = writeln!(out, "{pad}    if offset == 0 {{ return None; }}");
            let _ = writeln!(
                out,
                "{pad}    let bytes = proto_codec::wire::read_length_delimited(self.buf.as_ref(), offset as usize).ok()?;"
            );
            let _ = writeln!(out, "{pad}    Some({rust_path}::from_validated(bytes))");
            let _ = writeln!(out, "{pad}}}");
        }
    }
}

/// Renders a getter returning the payload of a singular length-delimited field, as
/// `Option<&[u8]>` when `optional` and as `&[u8]` (empty when absent) otherwise.
fn render_bytes_getter(out: &mut String, pad: &str, fn_name: &str, ret: &str, offset: &str, optional: bool) {
    let (absent, read) = if optional {
        ("None", "proto_codec::wire::read_length_delimited(self.buf.as_ref(), offset as usize).ok()")
    } else {
        ("&[]", "proto_codec::wire::read_length_delimited(self.buf.as_ref(), offset as usize).unwrap_or(&[])")
    };
    let _ = writeln!(out, "{pad}pub fn {fn_name}(&self) -> {ret} {{");
    let _ = writeln!(out, "{pad}    let offset = {offset};");
    let _ = writeln!(out, "{pad}    if offset == 0 {{ return {absent}; }}");
    let _ = writeln!(out, "{pad}    {read}");
    let _ = writeln!(out, "{pad}}}");
}

/// Renders the iterator getter(s) for a repeated field. Elements are yielded in wire
/// order, including elements interleaved with other fields. `parse` has rejected records
/// of the wrong wire type, so the filters below only guard the unvalidated path.
fn render_repeated_getter(out: &mut String, pad: &str, slot: usize, field: &Field) {
    let name = escape_ident(&field.name);
    let records = format!(
        "proto_codec::repeated::Records::new(self.buf.as_ref(), self.index[{slot}], self.index[{end}], {number})",
        end = slot + 1,
        number = field.number
    );
    let length_delimited = format!(
        "{records}\n\
         {pad}        .filter(|field| field.wire_type == proto_codec::WireType::LengthDelimited)\n\
         {pad}        .filter_map(|field| proto_codec::wire::read_length_delimited(self.buf.as_ref(), field.payload as usize).ok())"
    );

    match &field.kind {
        FieldKind::Scalar(Scalar::String) => {
            let _ = writeln!(
                out,
                "{pad}pub fn {name}(&self) -> impl Iterator<Item = Result<&str, core::str::Utf8Error>> + '_ {{"
            );
            let _ = writeln!(out, "{pad}    self.{name}_bytes().map(core::str::from_utf8)");
            let _ = writeln!(out, "{pad}}}");
            let _ = writeln!(out, "{pad}pub fn {name}_bytes(&self) -> impl Iterator<Item = &[u8]> + '_ {{");
            let _ = writeln!(out, "{pad}    {length_delimited}");
            let _ = writeln!(out, "{pad}}}");
        }
        FieldKind::Scalar(Scalar::Bytes) => {
            let _ = writeln!(out, "{pad}pub fn {name}(&self) -> impl Iterator<Item = &[u8]> + '_ {{");
            let _ = writeln!(out, "{pad}    {length_delimited}");
            let _ = writeln!(out, "{pad}}}");
        }
        FieldKind::Scalar(scalar) => {
            let (rust_type, width, convert) = scalar_element(*scalar);
            let _ = writeln!(out, "{pad}pub fn {name}(&self) -> impl Iterator<Item = {rust_type}> + '_ {{");
            let _ = writeln!(
                out,
                "{pad}    proto_codec::repeated::Scalars::new(self.buf.as_ref(), {records}, proto_codec::repeated::Width::{width})"
            );
            if let Some(convert) = convert {
                let _ = writeln!(out, "{pad}        .map({convert})");
            }
            let _ = writeln!(out, "{pad}}}");
        }
        FieldKind::Message(rust_path) => {
            let _ = writeln!(
                out,
                "{pad}pub fn {name}(&self) -> impl Iterator<Item = {rust_path}<&[u8]>> + '_ {{"
            );
            let _ = writeln!(out, "{pad}    {length_delimited}");
            let _ = writeln!(out, "{pad}        .map({rust_path}::from_validated)");
            let _ = writeln!(out, "{pad}}}");
        }
    }
}

/// Returns `(rust_type, width_variant, conversion)` for an element of a repeated numeric
/// field. `conversion` is the argument to a `.map(...)` over the raw `u64` bits — a
/// function path where one exists, otherwise a closure — or [`None`] when the raw bits
/// are already the element value.
fn scalar_element(scalar: Scalar) -> (&'static str, &'static str, Option<&'static str>) {
    match scalar {
        Scalar::Bool => ("bool", "Varint", Some("|v| v != 0")),
        Scalar::Int32 => ("i32", "Varint", Some("|v| v as i32")),
        Scalar::Uint32 => ("u32", "Varint", Some("|v| v as u32")),
        Scalar::Sint32 => (
            "i32",
            "Varint",
            Some("|v| proto_codec::varint::zigzag_decode32(v as u32)"),
        ),
        Scalar::Int64 => ("i64", "Varint", Some("|v| v as i64")),
        Scalar::Uint64 => ("u64", "Varint", None),
        Scalar::Sint64 => ("i64", "Varint", Some("proto_codec::varint::zigzag_decode64")),
        Scalar::Fixed32 => ("u32", "Fixed32", Some("|v| v as u32")),
        Scalar::Sfixed32 => ("i32", "Fixed32", Some("|v| v as u32 as i32")),
        Scalar::Float => ("f32", "Fixed32", Some("|v| f32::from_bits(v as u32)")),
        Scalar::Fixed64 => ("u64", "Fixed64", None),
        Scalar::Sfixed64 => ("i64", "Fixed64", Some("|v| v as i64")),
        Scalar::Double => ("f64", "Fixed64", Some("f64::from_bits")),
        Scalar::String | Scalar::Bytes => unreachable!("handled separately in render_repeated_getter"),
    }
}

/// Returns `(rust_type, default_value_expr, body_that_returns_the_value)` for a scalar
/// field, given a local `let offset: usize` binding the getter body can read from.
fn scalar_read(scalar: Scalar) -> (&'static str, &'static str, String) {
    let buf = "self.buf.as_ref()";
    match scalar {
        Scalar::Bool => (
            "bool",
            "false",
            format!(
                "proto_codec::varint::read_varint32({buf}, offset).map(|(v, _)| v != 0).unwrap_or(false)"
            ),
        ),
        Scalar::Int32 => (
            "i32",
            "0",
            format!(
                "proto_codec::varint::read_varint32({buf}, offset).map(|(v, _)| v as i32).unwrap_or(0)"
            ),
        ),
        Scalar::Uint32 => (
            "u32",
            "0",
            format!("proto_codec::varint::read_varint32({buf}, offset).map(|(v, _)| v).unwrap_or(0)"),
        ),
        Scalar::Sint32 => (
            "i32",
            "0",
            format!(
                "proto_codec::varint::read_varint32({buf}, offset).map(|(v, _)| proto_codec::varint::zigzag_decode32(v)).unwrap_or(0)"
            ),
        ),
        Scalar::Int64 => (
            "i64",
            "0",
            format!(
                "proto_codec::varint::read_varint({buf}, offset).map(|(v, _)| v as i64).unwrap_or(0)"
            ),
        ),
        Scalar::Uint64 => (
            "u64",
            "0",
            format!("proto_codec::varint::read_varint({buf}, offset).map(|(v, _)| v).unwrap_or(0)"),
        ),
        Scalar::Sint64 => (
            "i64",
            "0",
            format!(
                "proto_codec::varint::read_varint({buf}, offset).map(|(v, _)| proto_codec::varint::zigzag_decode64(v)).unwrap_or(0)"
            ),
        ),
        Scalar::Fixed32 => (
            "u32",
            "0",
            format!("proto_codec::wire::read_fixed32({buf}, offset).unwrap_or(0)"),
        ),
        Scalar::Sfixed32 => (
            "i32",
            "0",
            format!("proto_codec::wire::read_fixed32({buf}, offset).map(|v| v as i32).unwrap_or(0)"),
        ),
        Scalar::Float => (
            "f32",
            "0.0",
            format!(
                "proto_codec::wire::read_fixed32({buf}, offset).map(f32::from_bits).unwrap_or(0.0)"
            ),
        ),
        Scalar::Fixed64 => (
            "u64",
            "0",
            format!("proto_codec::wire::read_fixed64({buf}, offset).unwrap_or(0)"),
        ),
        Scalar::Sfixed64 => (
            "i64",
            "0",
            format!("proto_codec::wire::read_fixed64({buf}, offset).map(|v| v as i64).unwrap_or(0)"),
        ),
        Scalar::Double => (
            "f64",
            "0.0",
            format!(
                "proto_codec::wire::read_fixed64({buf}, offset).map(f64::from_bits).unwrap_or(0.0)"
            ),
        ),
        Scalar::String | Scalar::Bytes => unreachable!("handled separately in render_getter"),
    }
}
