use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::model::{Field, FieldKind, Message, Model, Oneof, Scalar, TypeRef};
use crate::naming::{escape_ident, snake_case, upper_camel};

/// One generated source file: the code for every message in a single proto package.
pub struct PackageFile {
    /// File name, following `prost-build`: the package's Rust module segments joined by
    /// `.`, plus `.rs` — or `_.rs` for files without a `package`.
    pub file_name: String,
    pub source: String,
}

/// Renders a [`Model`] into one Rust source file per proto package, in package order.
///
/// Files carry no `mod` wrappers: the includer nests each `include!` in modules that
/// mirror the package hierarchy, which is what the `super::`-relative paths emitted for
/// cross-package references assume.
pub fn render(model: &Model) -> Vec<PackageFile> {
    let mut packages: BTreeMap<&[String], Vec<&Message>> = BTreeMap::new();
    for message in &model.messages {
        packages.entry(&message.module).or_default().push(message);
    }

    packages
        .into_iter()
        .map(|(module, messages)| {
            let mut source = String::new();
            for message in messages {
                render_message(&mut source, message);
            }
            PackageFile {
                file_name: file_name(module),
                source,
            }
        })
        .collect()
}

fn file_name(module: &[String]) -> String {
    if module.is_empty() {
        "_.rs".to_string()
    } else {
        format!("{}.rs", module.join("."))
    }
}

/// Renders the path to `target` as seen from code in package module `from`, the way
/// `prost` does: a bare name within the same package, otherwise `super::` up to the
/// common ancestor and back down, e.g. `super::solana::storage::confirmed_block::Tx`.
fn relative_path(from: &[String], target: &TypeRef) -> String {
    let common = from
        .iter()
        .zip(&target.module)
        .take_while(|(a, b)| a == b)
        .count();
    let mut path: Vec<&str> = vec!["super"; from.len() - common];
    path.extend(target.module[common..].iter().map(String::as_str));
    path.push(&target.rust_name);
    path.join("::")
}

fn render_message(out: &mut String, message: &Message) {
    let pad = "";
    let inner = "    ";
    let module = message.module.as_slice();
    let name = &message.rust_name;
    let layout = IndexLayout::of(message);
    let slot_count = layout.slot_count;

    if !has_indexed_fields(message) {
        // No getters read the buffer or index of a field-less message.
        let _ = writeln!(out, "{pad}#[allow(dead_code)]");
    }
    let _ = writeln!(out, "{pad}pub struct {name}<B: AsRef<[u8]>> {{");
    let _ = writeln!(out, "{inner}buf: B,");
    let _ = writeln!(out, "{inner}index: [u32; {slot_count}],");
    let _ = writeln!(out, "{pad}}}");
    let _ = writeln!(out, "{pad}impl<B: AsRef<[u8]>> {name}<B> {{");

    render_parse(out, inner, message, &layout);
    for (field, &slot) in message.fields.iter().zip(&layout.field_slots) {
        render_getter(out, inner, module, slot, field);
    }
    for (oneof, &slot) in message.oneofs.iter().zip(&layout.oneof_slots) {
        render_oneof_getter(out, inner, message, oneof, slot);
    }

    let _ = writeln!(out, "{pad}}}");

    if !message.oneofs.is_empty() {
        render_oneof_module(out, message);
    }
}

/// Whether `message` has anything to index: a field outside or inside a `oneof`.
fn has_indexed_fields(message: &Message) -> bool {
    !message.fields.is_empty() || !message.oneofs.is_empty()
}

/// Where each field and `oneof` lives in a message's index table.
///
/// - A singular field takes one slot holding its payload offset (last occurrence wins).
/// - A repeated field takes two, holding the tag offset of its first element and the
///   offset just past its last, so iteration re-walks only that span.
/// - A `oneof` takes two, holding the payload offset of the member seen last and that
///   member's field number, which is how last-wins applies across members.
struct IndexLayout {
    field_slots: Vec<usize>,
    oneof_slots: Vec<usize>,
    slot_count: usize,
}

impl IndexLayout {
    fn of(message: &Message) -> Self {
        let mut next = 0;
        let mut take = |width| {
            let slot = next;
            next += width;
            slot
        };
        let field_slots = message
            .fields
            .iter()
            .map(|field| take(if field.repeated { 2 } else { 1 }))
            .collect();
        let oneof_slots = message.oneofs.iter().map(|_| take(2)).collect();
        Self {
            field_slots,
            oneof_slots,
            slot_count: next,
        }
    }
}

fn render_parse(out: &mut String, pad: &str, message: &Message, layout: &IndexLayout) {
    let name = &message.rust_name;
    let slot_count = layout.slot_count;
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
    let index_mut = if !has_indexed_fields(message) {
        let _ = writeln!(out, "{pad}    let _ = depth;");
        ""
    } else {
        "mut "
    };
    let _ = writeln!(out, "{pad}    let {index_mut}index = [0u32; {slot_count}];");
    let _ = writeln!(out, "{pad}    let mut scanner = proto_codec::Scanner::new(buf)?;");
    if !has_indexed_fields(message) {
        // Nothing to index, but the walk still validates the message's structure.
        let _ = writeln!(out, "{pad}    while scanner.next_field()?.is_some() {{}}");
        let _ = writeln!(out, "{pad}    Ok(index)");
        let _ = writeln!(out, "{pad}}}");
        return;
    }
    if message.fields.iter().any(|field| field.repeated) {
        let _ = writeln!(out, "{pad}    loop {{");
        let _ = writeln!(out, "{pad}        let tag = scanner.position() as u32;");
        let _ = writeln!(out, "{pad}        let Some(field) = scanner.next_field()? else {{ break }};");
    } else {
        let _ = writeln!(out, "{pad}    while let Some(field) = scanner.next_field()? {{");
    }
    let _ = writeln!(out, "{pad}        match field.number {{");
    let arm = format!("{pad}                ");
    for (field, &slot) in message.fields.iter().zip(&layout.field_slots) {
        let _ = writeln!(out, "{pad}            {number} => {{", number = field.number);
        render_field_validation(out, &arm, &message.module, field);
        if field.repeated {
            let end = slot + 1;
            let _ = writeln!(out, "{arm}if index[{end}] == 0 {{ index[{slot}] = tag; }}");
            let _ = writeln!(out, "{arm}index[{end}] = scanner.position() as u32;");
        } else {
            let _ = writeln!(out, "{arm}index[{slot}] = field.payload;");
        }
        let _ = writeln!(out, "{pad}            }}");
    }
    for (oneof, &slot) in message.oneofs.iter().zip(&layout.oneof_slots) {
        let member_slot = slot + 1;
        for member in &oneof.members {
            let number = member.number;
            let _ = writeln!(out, "{pad}            {number} => {{");
            render_field_validation(out, &arm, &message.module, member);
            let _ = writeln!(out, "{arm}index[{slot}] = field.payload;");
            let _ = writeln!(out, "{arm}index[{member_slot}] = {number};");
            let _ = writeln!(out, "{pad}            }}");
        }
    }
    let _ = writeln!(out, "{pad}            _ => {{}}");
    let _ = writeln!(out, "{pad}        }}");
    let _ = writeln!(out, "{pad}    }}");
    let _ = writeln!(out, "{pad}    Ok(index)");
    let _ = writeln!(out, "{pad}}}");
}

/// Renders the checks one occurrence of `field` must pass when the walk is validating: its
/// wire type, the contents of a packed run, and, for a message, the nested message itself.
fn render_field_validation(out: &mut String, pad: &str, module: &[String], field: &Field) {
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
        FieldKind::Message(target) => {
            let rust_path = relative_path(module, target);
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

fn render_getter(out: &mut String, pad: &str, module: &[String], slot: usize, field: &Field) {
    if field.repeated {
        render_repeated_getter(out, pad, module, slot, field);
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
        FieldKind::Message(target) => {
            let rust_path = relative_path(module, target);
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

/// Rust names for one `oneof`, as `prost` derives them: the enum lives in a module named
/// after the message (`subscribe_update`) and is named after the `oneof`
/// (`UpdateOneof`); each variant is named after its member field.
struct OneofNames {
    module: String,
    enum_name: String,
    /// Whether any variant borrows from the buffer, so the enum needs a lifetime.
    borrows: bool,
}

impl OneofNames {
    fn of(message: &Message, oneof: &Oneof) -> Self {
        Self {
            module: escape_ident(&snake_case(&message.rust_name)),
            enum_name: upper_camel(&oneof.name),
            borrows: oneof.members.iter().any(|member| {
                matches!(
                    member.kind,
                    FieldKind::Message(_) | FieldKind::Scalar(Scalar::String | Scalar::Bytes)
                )
            }),
        }
    }

    /// The enum's type as written from the message's own module, e.g.
    /// `subscribe_update::UpdateOneof<'_>`.
    fn type_from_parent(&self) -> String {
        let lifetime = if self.borrows { "<'_>" } else { "" };
        format!("{}::{}{lifetime}", self.module, self.enum_name)
    }
}

/// Renders the getter for a `oneof`: the member seen last on the wire, or [`None`].
fn render_oneof_getter(out: &mut String, pad: &str, message: &Message, oneof: &Oneof, slot: usize) {
    let names = OneofNames::of(message, oneof);
    let fn_name = escape_ident(&oneof.name);
    let enum_path = format!("{}::{}", names.module, names.enum_name);
    let member_slot = slot + 1;

    let _ = writeln!(
        out,
        "{pad}/// The member of `{oneof_name}` that was set last on the wire, or `None` if none was.",
        oneof_name = oneof.name
    );
    let _ = writeln!(out, "{pad}pub fn {fn_name}(&self) -> Option<{}> {{", names.type_from_parent());
    let _ = writeln!(out, "{pad}    let offset = self.index[{slot}];");
    let _ = writeln!(out, "{pad}    if offset == 0 {{ return None; }}");
    let _ = writeln!(out, "{pad}    let offset = offset as usize;");
    let _ = writeln!(out, "{pad}    match self.index[{member_slot}] {{");
    for member in &oneof.members {
        let variant = upper_camel(&member.name);
        let number = member.number;
        let read_ld = "proto_codec::wire::read_length_delimited(self.buf.as_ref(), offset)";
        let value = match &member.kind {
            FieldKind::Scalar(Scalar::String) => {
                format!("core::str::from_utf8({read_ld}.unwrap_or(&[]))")
            }
            FieldKind::Scalar(Scalar::Bytes) => format!("{read_ld}.unwrap_or(&[])"),
            FieldKind::Scalar(scalar) => scalar_read(*scalar).2,
            FieldKind::Message(target) => format!(
                "{}::from_validated({read_ld}.ok()?)",
                relative_path(&message.module, target)
            ),
        };
        let _ = writeln!(out, "{pad}        {number} => Some({enum_path}::{variant}({value})),");
    }
    let _ = writeln!(out, "{pad}        _ => None,");
    let _ = writeln!(out, "{pad}    }}");
    let _ = writeln!(out, "{pad}}}");
}

/// Renders the module holding `message`'s oneof enums.
fn render_oneof_module(out: &mut String, message: &Message) {
    let Some(first) = message.oneofs.first() else {
        return;
    };
    let module_name = OneofNames::of(message, first).module;
    let mut module = message.module.clone();
    module.push(module_name.clone());

    let _ = writeln!(out, "/// Oneof enums of [`{}`].", message.rust_name);
    let _ = writeln!(out, "pub mod {module_name} {{");
    for oneof in &message.oneofs {
        let names = OneofNames::of(message, oneof);
        let lifetime = if names.borrows { "<'a>" } else { "" };
        let _ = writeln!(out, "    /// The members of `{}`.", oneof.name);
        let _ = writeln!(out, "    pub enum {}{lifetime} {{", names.enum_name);
        for member in &oneof.members {
            let variant = upper_camel(&member.name);
            let payload = match &member.kind {
                FieldKind::Scalar(Scalar::String) => {
                    "Result<&'a str, core::str::Utf8Error>".to_string()
                }
                FieldKind::Scalar(Scalar::Bytes) => "&'a [u8]".to_string(),
                FieldKind::Scalar(scalar) => scalar_read(*scalar).0.to_string(),
                FieldKind::Message(target) => {
                    format!("{}<&'a [u8]>", relative_path(&module, target))
                }
            };
            let _ = writeln!(out, "        {variant}({payload}),");
        }
        let _ = writeln!(out, "    }}");
    }
    let _ = writeln!(out, "}}");
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
fn render_repeated_getter(out: &mut String, pad: &str, module: &[String], slot: usize, field: &Field) {
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
        FieldKind::Message(target) => {
            let rust_path = relative_path(module, target);
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

#[cfg(test)]
mod tests {
    use super::{file_name, relative_path};
    use crate::model::TypeRef;

    fn module(path: &str) -> Vec<String> {
        path.split('.').filter(|s| !s.is_empty()).map(str::to_string).collect()
    }

    fn target(path: &str, name: &str) -> TypeRef {
        TypeRef {
            module: module(path),
            rust_name: name.to_string(),
        }
    }

    #[test]
    fn same_package_is_a_bare_name() {
        assert_eq!(relative_path(&module("geyser"), &target("geyser", "Ping")), "Ping");
        assert_eq!(relative_path(&[], &target("", "Root")), "Root");
    }

    #[test]
    fn cross_package_climbs_to_the_common_ancestor() {
        assert_eq!(
            relative_path(
                &module("geyser"),
                &target("solana.storage.confirmed_block", "Transaction")
            ),
            "super::solana::storage::confirmed_block::Transaction"
        );
        assert_eq!(
            relative_path(&module("fixtures.a"), &target("fixtures.b", "X")),
            "super::b::X"
        );
        assert_eq!(
            relative_path(&module("a.b.c"), &target("a", "X")),
            "super::super::X"
        );
        assert_eq!(relative_path(&module("a"), &target("a.b", "X")), "b::X");
        assert_eq!(relative_path(&module("a"), &target("", "Root")), "super::Root");
        assert_eq!(relative_path(&[], &target("a.b", "X")), "a::b::X");
    }

    #[test]
    fn file_names_match_prost_build() {
        assert_eq!(file_name(&module("geyser")), "geyser.rs");
        assert_eq!(
            file_name(&module("solana.storage.confirmed_block")),
            "solana.storage.confirmed_block.rs"
        );
        assert_eq!(file_name(&[]), "_.rs");
    }
}
