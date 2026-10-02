use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::model::{Enum, Field, FieldKind, MapKind, Message, Model, Oneof, Scalar, TypeRef};
use crate::naming::{escape_ident, snake_case, upper_camel};

/// One generated source file: the code for every message and enum in a single proto
/// package.
pub struct PackageFile {
    /// File name, following `prost-build`: the package's Rust module segments joined by
    /// `.`, plus `.rs` — or `_.rs` for files without a `package`.
    pub file_name: String,
    pub source: String,
}

/// Renders a [`Model`] into one Rust source file per proto package, in package order.
///
/// Files carry no wrapper for the package itself: the includer nests each `include!` in
/// modules that mirror the package hierarchy, which is what the `super::`-relative paths
/// emitted for cross-package references assume. Within a file, nested types and oneof
/// enums go in a module named after their message, as `prost` lays them out.
pub fn render(model: &Model) -> Vec<PackageFile> {
    let mut packages: BTreeMap<Vec<String>, ModuleTree> = BTreeMap::new();
    for message in &model.messages {
        let tree = packages.entry(message.package.clone()).or_default();
        let depth = message.package.len();
        tree.node(&message.module[depth..])
            .items
            .push_str(&render_message(message));
        if !message.oneofs.is_empty() {
            tree.node(&message.nested_module[depth..])
                .items
                .push_str(&render_oneof_enums(message));
        }
    }
    for enumeration in &model.enums {
        let tree = packages.entry(enumeration.package.clone()).or_default();
        tree.node(&enumeration.module[enumeration.package.len()..])
            .items
            .push_str(&render_enum(enumeration));
    }

    packages
        .into_iter()
        .map(|(package, tree)| {
            let mut source = String::new();
            tree.write(&mut source, 0);
            PackageFile {
                file_name: file_name(&package),
                source,
            }
        })
        .collect()
}

/// The modules of one package file, relative to the package, with the code each holds.
#[derive(Default)]
struct ModuleTree {
    items: String,
    children: BTreeMap<String, ModuleTree>,
}

impl ModuleTree {
    /// Returns the node at `path` below this one, creating it if needed.
    fn node(&mut self, path: &[String]) -> &mut Self {
        match path.split_first() {
            None => self,
            Some((head, rest)) => self.children.entry(head.clone()).or_default().node(rest),
        }
    }

    /// Writes this node's items, then each child as an indented `pub mod`.
    fn write(&self, out: &mut String, depth: usize) {
        let pad = "    ".repeat(depth);
        for line in self.items.lines() {
            if !line.is_empty() {
                out.push_str(&pad);
                out.push_str(line);
            }
            out.push('\n');
        }
        for (name, child) in &self.children {
            let _ = writeln!(
                out,
                "{pad}/// Nested types and oneof enums of the message of the same name."
            );
            // A message named like its package (`maps.Maps`) nests `maps::maps`.
            let _ = writeln!(out, "{pad}#[allow(clippy::module_inception)]");
            let _ = writeln!(out, "{pad}pub mod {name} {{");
            child.write(out, depth + 1);
            let _ = writeln!(out, "{pad}}}");
        }
    }
}

/// The getter name for a field or oneof, as `prost` names the struct field:
/// `snake_case`, keyword-escaped (`blockFilters` -> `block_filters`, `type` -> `r#type`).
fn field_fn_name(name: &str) -> String {
    escape_ident(&snake_case(name))
}

fn file_name(module: &[String]) -> String {
    if module.is_empty() {
        "_.rs".to_string()
    } else {
        format!("{}.rs", module.join("."))
    }
}

/// Renders the path to `target` as seen from code in module `from`, the way `prost` does:
/// a bare name within the same module, otherwise `super::` up to the common ancestor and
/// back down, e.g. `super::solana::storage::confirmed_block::Tx`.
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

// ---------------------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------------------

fn render_message(message: &Message) -> String {
    let mut out = String::new();
    let inner = "    ";
    let name = &message.rust_name;
    let layout = IndexLayout::of(message);
    let slot_count = layout.slot_count;

    if message.map_entry {
        let _ = writeln!(
            out,
            "/// A map entry protoc synthesizes; read maps through their field's getter."
        );
        let _ = writeln!(out, "#[doc(hidden)]");
    }
    if !has_indexed_fields(message) {
        // No getters read the buffer or index of a field-less message.
        let _ = writeln!(out, "#[allow(dead_code)]");
    }
    let _ = writeln!(out, "pub struct {name}<B: AsRef<[u8]>> {{");
    let _ = writeln!(out, "{inner}buf: B,");
    let _ = writeln!(out, "{inner}index: [u32; {slot_count}],");
    let _ = writeln!(out, "}}");
    // Getter names are field names, which may read like conversions (`from_slot`).
    let _ = writeln!(out, "#[allow(clippy::wrong_self_convention)]");
    let _ = writeln!(out, "impl<B: AsRef<[u8]>> {name}<B> {{");

    render_parse(&mut out, inner, message, &layout);
    for (field, &slot) in message.fields.iter().zip(&layout.field_slots) {
        render_getter(&mut out, inner, &message.module, slot, field);
    }
    for (oneof, &slot) in message.oneofs.iter().zip(&layout.oneof_slots) {
        render_oneof_getter(&mut out, inner, message, oneof, slot);
    }

    let _ = writeln!(out, "}}");
    out
}

/// Whether `message` has anything to index: a field outside or inside a `oneof`.
fn has_indexed_fields(message: &Message) -> bool {
    !message.fields.is_empty() || !message.oneofs.is_empty()
}

/// Where each field and `oneof` lives in a message's index table.
///
/// - A singular field takes one slot holding its payload offset (last occurrence wins).
/// - A repeated or map field takes two, holding the tag offset of its first element and
///   the offset just past its last, so iteration re-walks only that span.
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
    let _ = writeln!(
        out,
        "{pad}pub fn parse(buf: B) -> Result<Self, proto_codec::DecodeError> {{"
    );
    let _ = writeln!(
        out,
        "{pad}    let index = Self::walk(buf.as_ref(), Some(0))?;"
    );
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
    let _ = writeln!(
        out,
        "{pad}    let mut scanner = proto_codec::Scanner::new(buf)?;"
    );
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
        let _ = writeln!(
            out,
            "{pad}        let Some(field) = scanner.next_field()? else {{ break }};"
        );
    } else {
        let _ = writeln!(
            out,
            "{pad}    while let Some(field) = scanner.next_field()? {{"
        );
    }
    let _ = writeln!(out, "{pad}        match field.number {{");
    let arm = format!("{pad}                ");
    for (field, &slot) in message.fields.iter().zip(&layout.field_slots) {
        let _ = writeln!(
            out,
            "{pad}            {number} => {{",
            number = field.number
        );
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
/// wire type, the contents of a packed run, and, for a message or map entry, the nested
/// message itself.
fn render_field_validation(out: &mut String, pad: &str, module: &[String], field: &Field) {
    let expect = |wire_type: &str| {
        format!("proto_codec::wire::expect_wire_type(&field, proto_codec::WireType::{wire_type})?;")
    };
    let numeric = |scalar: Scalar| {
        let (_, width, _) = scalar_element(scalar);
        if field.repeated {
            format!(
                "proto_codec::repeated::validate_numeric(buf, &field, proto_codec::repeated::Width::{width})?;"
            )
        } else {
            // `Width` variants share their names with the matching `WireType`.
            expect(width)
        }
    };
    let nested = match &field.kind {
        FieldKind::Scalar(Scalar::String | Scalar::Bytes) => {
            let _ = writeln!(
                out,
                "{pad}if depth.is_some() {{ {} }}",
                expect("LengthDelimited")
            );
            return;
        }
        FieldKind::Scalar(scalar) => {
            let _ = writeln!(out, "{pad}if depth.is_some() {{ {} }}", numeric(*scalar));
            return;
        }
        FieldKind::Enum(_) => {
            let _ = writeln!(
                out,
                "{pad}if depth.is_some() {{ {} }}",
                numeric(Scalar::Int32)
            );
            return;
        }
        FieldKind::Message(target) => target,
        FieldKind::Map(map) => &map.entry,
    };
    let rust_path = relative_path(module, nested);
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

// ---------------------------------------------------------------------------------------
// Getters
// ---------------------------------------------------------------------------------------

fn render_getter(out: &mut String, pad: &str, module: &[String], slot: usize, field: &Field) {
    if field.repeated {
        render_repeated_getter(out, pad, module, slot, field);
        return;
    }
    let name = field_fn_name(&field.name);
    let offset = format!("self.index[{slot}]");

    // A proto3 `optional` field reports absence as `None`; an implicit-presence one
    // substitutes the proto default. A message has no default, so it is always `Option`.
    let wrap = |ty: &str| {
        if field.optional {
            format!("Option<{ty}>")
        } else {
            ty.to_string()
        }
    };

    let (ret, absent, present) = match &field.kind {
        FieldKind::Scalar(Scalar::String) => {
            let body = if field.optional {
                format!("self.{name}_bytes().map(core::str::from_utf8)")
            } else {
                format!("core::str::from_utf8(self.{name}_bytes())")
            };
            let _ = writeln!(
                out,
                "{pad}pub fn {name}(&self) -> {} {{",
                wrap("Result<&str, core::str::Utf8Error>")
            );
            let _ = writeln!(out, "{pad}    {body}");
            let _ = writeln!(out, "{pad}}}");
            render_bytes_getter(
                out,
                pad,
                &format!("{name}_bytes"),
                &wrap("&[u8]"),
                &offset,
                field.optional,
            );
            return;
        }
        FieldKind::Scalar(Scalar::Bytes) => {
            render_bytes_getter(out, pad, &name, &wrap("&[u8]"), &offset, field.optional);
            return;
        }
        FieldKind::Map(_) => unreachable!("map fields are always repeated"),
        kind @ FieldKind::Message(_) => (
            format!("Option<{}>", value_type(kind, module, "")),
            "None".to_string(),
            format!(
                "Some({})",
                value_expr(kind, module, "self.buf.as_ref()", "offset")
            ),
        ),
        kind if field.optional => (
            format!("Option<{}>", value_type(kind, module, "")),
            "None".to_string(),
            format!(
                "Some({})",
                value_expr(kind, module, "self.buf.as_ref()", "offset")
            ),
        ),
        kind => (
            value_type(kind, module, ""),
            value_default(kind, module),
            value_expr(kind, module, "self.buf.as_ref()", "offset"),
        ),
    };
    let _ = writeln!(out, "{pad}pub fn {name}(&self) -> {ret} {{");
    let _ = writeln!(out, "{pad}    let offset = {offset};");
    let _ = writeln!(out, "{pad}    if offset == 0 {{ return {absent}; }}");
    let _ = writeln!(out, "{pad}    let offset = offset as usize;");
    let _ = writeln!(out, "{pad}    {present}");
    let _ = writeln!(out, "{pad}}}");
}

/// Renders a getter returning the payload of a singular length-delimited field, as
/// `Option<&[u8]>` when `optional` and as `&[u8]` (empty when absent) otherwise.
fn render_bytes_getter(
    out: &mut String,
    pad: &str,
    fn_name: &str,
    ret: &str,
    offset: &str,
    optional: bool,
) {
    let (absent, read) = if optional {
        (
            "None",
            "proto_codec::wire::read_length_delimited(self.buf.as_ref(), offset as usize).ok()",
        )
    } else {
        (
            "&[]",
            "proto_codec::wire::read_length_delimited(self.buf.as_ref(), offset as usize).unwrap_or(&[])",
        )
    };
    let _ = writeln!(out, "{pad}pub fn {fn_name}(&self) -> {ret} {{");
    let _ = writeln!(out, "{pad}    let offset = {offset};");
    let _ = writeln!(out, "{pad}    if offset == 0 {{ return {absent}; }}");
    let _ = writeln!(out, "{pad}    {read}");
    let _ = writeln!(out, "{pad}}}");
}

/// Renders the iterator getter(s) for a repeated or map field. Elements are yielded in
/// wire order, including elements interleaved with other fields. `parse` has rejected
/// records of the wrong wire type, so the filters below only guard the unvalidated path.
fn render_repeated_getter(
    out: &mut String,
    pad: &str,
    module: &[String],
    slot: usize,
    field: &Field,
) {
    let name = field_fn_name(&field.name);
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
            let _ = writeln!(
                out,
                "{pad}    self.{name}_bytes().map(core::str::from_utf8)"
            );
            let _ = writeln!(out, "{pad}}}");
            let _ = writeln!(
                out,
                "{pad}pub fn {name}_bytes(&self) -> impl Iterator<Item = &[u8]> + '_ {{"
            );
            let _ = writeln!(out, "{pad}    {length_delimited}");
            let _ = writeln!(out, "{pad}}}");
        }
        FieldKind::Scalar(Scalar::Bytes) => {
            let _ = writeln!(
                out,
                "{pad}pub fn {name}(&self) -> impl Iterator<Item = &[u8]> + '_ {{"
            );
            let _ = writeln!(out, "{pad}    {length_delimited}");
            let _ = writeln!(out, "{pad}}}");
        }
        FieldKind::Scalar(scalar) => {
            let (rust_type, width, convert) = scalar_element(*scalar);
            let _ = writeln!(
                out,
                "{pad}pub fn {name}(&self) -> impl Iterator<Item = {rust_type}> + '_ {{"
            );
            let _ = writeln!(
                out,
                "{pad}    proto_codec::repeated::Scalars::new(self.buf.as_ref(), {records}, proto_codec::repeated::Width::{width})"
            );
            if let Some(convert) = convert {
                let _ = writeln!(out, "{pad}        .map({convert})");
            }
            let _ = writeln!(out, "{pad}}}");
        }
        FieldKind::Enum(target) => {
            let rust_path = relative_path(module, target);
            let _ = writeln!(
                out,
                "{pad}pub fn {name}(&self) -> impl Iterator<Item = {rust_path}> + '_ {{"
            );
            let _ = writeln!(
                out,
                "{pad}    proto_codec::repeated::Scalars::new(self.buf.as_ref(), {records}, proto_codec::repeated::Width::Varint)"
            );
            let _ = writeln!(
                out,
                "{pad}        .map(|v| {rust_path}::from_i32(v as i32))"
            );
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
        FieldKind::Map(map) => render_map_getter(out, pad, module, slot, field, map),
    }
}

/// Renders the getter for a `map<K, V>` field: an iterator over `(key, value)` pairs in
/// wire order. Duplicate keys are all yielded; collecting into a map keeps the last, which
/// is protobuf's rule. An entry missing its key or value yields that type's default.
fn render_map_getter(
    out: &mut String,
    pad: &str,
    module: &[String],
    slot: usize,
    field: &Field,
    map: &MapKind,
) {
    let name = field_fn_name(&field.name);
    let key_type = value_type(&map.key, module, "");
    let value_type = value_type(&map.value, module, "");
    let key = value_expr(&map.key, module, "key_buf", "key as usize");
    let value = value_expr(&map.value, module, "value_buf", "value as usize");
    let (start, end, number) = (slot, slot + 1, field.number);

    let _ = writeln!(
        out,
        "{pad}/// The entries of `{field_name}` in wire order; collect them into a map for last-wins\n\
         {pad}/// semantics on duplicate keys.",
        field_name = field.name
    );
    let _ = writeln!(
        out,
        "{pad}pub fn {name}(&self) -> impl Iterator<Item = ({key_type}, {value_type})> + '_ {{"
    );
    let _ = writeln!(out, "{pad}    let buf = self.buf.as_ref();");
    let _ = writeln!(
        out,
        "{pad}    proto_codec::repeated::Records::new(buf, self.index[{start}], self.index[{end}], {number})"
    );
    let _ = writeln!(
        out,
        "{pad}        .filter(|field| field.wire_type == proto_codec::WireType::LengthDelimited)"
    );
    let _ = writeln!(
        out,
        "{pad}        .filter_map(move |field| proto_codec::wire::read_length_delimited(buf, field.payload as usize).ok())"
    );
    let _ = writeln!(out, "{pad}        .map(|entry| {{");
    let _ = writeln!(
        out,
        "{pad}            // An absent key or value reads as its default: every reader falls back to\n\
         {pad}            // the default when handed an empty buffer."
    );
    let _ = writeln!(
        out,
        "{pad}            let (key, value) = proto_codec::map::entry_offsets(entry);"
    );
    let _ = writeln!(
        out,
        "{pad}            let key_buf: &[u8] = if key == 0 {{ &[] }} else {{ entry }};"
    );
    let _ = writeln!(
        out,
        "{pad}            let value_buf: &[u8] = if value == 0 {{ &[] }} else {{ entry }};"
    );
    let _ = writeln!(out, "{pad}            ({key}, {value})");
    let _ = writeln!(out, "{pad}        }})");
    let _ = writeln!(out, "{pad}}}");
}

// ---------------------------------------------------------------------------------------
// Oneofs
// ---------------------------------------------------------------------------------------

/// Rust names for one `oneof`, as `prost` derives them: the enum lives in the message's
/// nested module (`subscribe_update`) and is named after the `oneof` (`UpdateOneof`);
/// each variant is named after its member field.
struct OneofNames {
    module: String,
    enum_name: String,
    /// Whether any variant borrows from the buffer, so the enum needs a lifetime.
    borrows: bool,
}

impl OneofNames {
    fn of(message: &Message, oneof: &Oneof) -> Self {
        Self {
            module: message.nested_module.last().cloned().unwrap_or_default(),
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
    let fn_name = field_fn_name(&oneof.name);
    let enum_path = format!("{}::{}", names.module, names.enum_name);
    let member_slot = slot + 1;

    let _ = writeln!(
        out,
        "{pad}/// The member of `{oneof_name}` that was set last on the wire, or `None` if none was.",
        oneof_name = oneof.name
    );
    let _ = writeln!(
        out,
        "{pad}pub fn {fn_name}(&self) -> Option<{}> {{",
        names.type_from_parent()
    );
    let _ = writeln!(out, "{pad}    let offset = self.index[{slot}];");
    let _ = writeln!(out, "{pad}    if offset == 0 {{ return None; }}");
    let _ = writeln!(out, "{pad}    let offset = offset as usize;");
    let _ = writeln!(out, "{pad}    match self.index[{member_slot}] {{");
    for member in &oneof.members {
        let variant = upper_camel(&member.name);
        let value = value_expr(&member.kind, &message.module, "self.buf.as_ref()", "offset");
        let _ = writeln!(
            out,
            "{pad}        {} => Some({enum_path}::{variant}({value})),",
            member.number
        );
    }
    let _ = writeln!(out, "{pad}        _ => None,");
    let _ = writeln!(out, "{pad}    }}");
    let _ = writeln!(out, "{pad}}}");
}

/// Renders `message`'s oneof enums, to be placed in its nested module.
fn render_oneof_enums(message: &Message) -> String {
    let mut out = String::new();
    for oneof in &message.oneofs {
        let names = OneofNames::of(message, oneof);
        let lifetime = if names.borrows { "<'a>" } else { "" };
        let _ = writeln!(
            out,
            "/// The members of `{}.{}`.",
            message.rust_name, oneof.name
        );
        let _ = writeln!(
            out,
            "#[allow(clippy::enum_variant_names, clippy::large_enum_variant)]"
        );
        let _ = writeln!(out, "pub enum {}{lifetime} {{", names.enum_name);
        for member in &oneof.members {
            let variant = upper_camel(&member.name);
            let payload = value_type(&member.kind, &message.nested_module, "'a ");
            let _ = writeln!(out, "    {variant}({payload}),");
        }
        let _ = writeln!(out, "}}");
    }
    out
}

// ---------------------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------------------

/// Renders a proto enum as a Rust enum whose extra variant carries values the schema does
/// not declare, so a server sending a newer value is distinguishable from an old one.
fn render_enum(enumeration: &Enum) -> String {
    let mut out = String::new();
    let name = &enumeration.rust_name;
    let unknown = &enumeration.unknown_variant;

    let _ = writeln!(
        out,
        "/// The `{name}` enum. Values this schema does not declare are kept as `{unknown}`."
    );
    let _ = writeln!(out, "#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]");
    let _ = writeln!(out, "#[allow(clippy::enum_variant_names)]");
    let _ = writeln!(out, "pub enum {name} {{");
    for value in &enumeration.values {
        let _ = writeln!(out, "    /// `{} = {}`", value.proto_name, value.number);
        let _ = writeln!(out, "    {},", value.rust_name);
    }
    let _ = writeln!(out, "    /// A value this schema does not declare.");
    let _ = writeln!(out, "    {unknown}(i32),");
    let _ = writeln!(out, "}}");

    let _ = writeln!(out, "impl {name} {{");
    let _ = writeln!(
        out,
        "    /// Maps a wire value to its variant; undeclared values become `{unknown}`."
    );
    let _ = writeln!(out, "    pub const fn from_i32(value: i32) -> Self {{");
    let _ = writeln!(out, "        match value {{");
    for value in &enumeration.values {
        let _ = writeln!(
            out,
            "            {} => Self::{},",
            value.number, value.rust_name
        );
    }
    let _ = writeln!(out, "            other => Self::{unknown}(other),");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "    /// Returns the wire value.");
    let _ = writeln!(out, "    pub const fn to_i32(self) -> i32 {{");
    let _ = writeln!(out, "        match self {{");
    for value in &enumeration.values {
        let _ = writeln!(
            out,
            "            Self::{} => {},",
            value.rust_name, value.number
        );
    }
    let _ = writeln!(out, "            Self::{unknown}(value) => value,");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(
        out,
        "    /// Returns the value's name in the `.proto` file, or `None` for an undeclared value."
    );
    let _ = writeln!(
        out,
        "    pub const fn as_str_name(self) -> Option<&'static str> {{"
    );
    let _ = writeln!(out, "        match self {{");
    for value in &enumeration.values {
        let _ = writeln!(
            out,
            "            Self::{} => Some(\"{}\"),",
            value.rust_name, value.proto_name
        );
    }
    let _ = writeln!(out, "            Self::{unknown}(_) => None,");
    let _ = writeln!(out, "        }}");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    let _ = writeln!(out, "impl Default for {name} {{");
    let _ = writeln!(
        out,
        "    /// The variant for `0`, proto3's default for every enum field."
    );
    let _ = writeln!(out, "    fn default() -> Self {{");
    let _ = writeln!(out, "        Self::from_i32(0)");
    let _ = writeln!(out, "    }}");
    let _ = writeln!(out, "}}");
    out
}

// ---------------------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------------------

/// The Rust type a single value of `kind` is exposed as, seen from module `from`.
/// `lifetime` is spliced into borrowed types: `""` for an elided lifetime, `"'a "` for a
/// named one.
fn value_type(kind: &FieldKind, from: &[String], lifetime: &str) -> String {
    match kind {
        FieldKind::Scalar(Scalar::String) => {
            format!("Result<&{lifetime}str, core::str::Utf8Error>")
        }
        FieldKind::Scalar(Scalar::Bytes) => format!("&{lifetime}[u8]"),
        FieldKind::Scalar(scalar) => scalar_element(*scalar).0.to_string(),
        FieldKind::Enum(target) => relative_path(from, target),
        FieldKind::Message(target) => format!("{}<&{lifetime}[u8]>", relative_path(from, target)),
        FieldKind::Map(_) => unreachable!("a map is never a single value"),
    }
}

/// An expression reading a single value of `kind` whose payload starts at `offset` in
/// `buf`. Every reader falls back to the type's default if the read fails, which cannot
/// happen on validated input — except deliberately, by passing an empty `buf`.
fn value_expr(kind: &FieldKind, from: &[String], buf: &str, offset: &str) -> String {
    let length_delimited =
        format!("proto_codec::wire::read_length_delimited({buf}, {offset}).unwrap_or(&[])");
    match kind {
        FieldKind::Scalar(Scalar::String) => format!("core::str::from_utf8({length_delimited})"),
        FieldKind::Scalar(Scalar::Bytes) => length_delimited,
        FieldKind::Scalar(scalar) => scalar_read(*scalar, buf, offset),
        FieldKind::Enum(target) => format!(
            "{}::from_i32({})",
            relative_path(from, target),
            scalar_read(Scalar::Int32, buf, offset)
        ),
        FieldKind::Message(target) => {
            format!(
                "{}::from_validated({length_delimited})",
                relative_path(from, target)
            )
        }
        FieldKind::Map(_) => unreachable!("a map is never a single value"),
    }
}

/// The default of an implicit-presence field of `kind`, returned when it is absent.
fn value_default(kind: &FieldKind, from: &[String]) -> String {
    match kind {
        FieldKind::Scalar(Scalar::Bool) => "false".to_string(),
        FieldKind::Scalar(Scalar::Float | Scalar::Double) => "0.0".to_string(),
        FieldKind::Scalar(_) => "0".to_string(),
        FieldKind::Enum(target) => format!("{}::from_i32(0)", relative_path(from, target)),
        FieldKind::Message(_) | FieldKind::Map(_) => unreachable!("no implicit default"),
    }
}

/// Returns `(rust_type, width_variant, conversion)` for a numeric value. `conversion` is
/// the argument to a `.map(...)` over the raw `u64` bits — a function path where one
/// exists, otherwise a closure — or [`None`] when the raw bits are already the value.
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
        Scalar::Sint64 => (
            "i64",
            "Varint",
            Some("proto_codec::varint::zigzag_decode64"),
        ),
        Scalar::Fixed32 => ("u32", "Fixed32", Some("|v| v as u32")),
        Scalar::Sfixed32 => ("i32", "Fixed32", Some("|v| v as u32 as i32")),
        Scalar::Float => ("f32", "Fixed32", Some("|v| f32::from_bits(v as u32)")),
        Scalar::Fixed64 => ("u64", "Fixed64", None),
        Scalar::Sfixed64 => ("i64", "Fixed64", Some("|v| v as i64")),
        Scalar::Double => ("f64", "Fixed64", Some("f64::from_bits")),
        Scalar::String | Scalar::Bytes => unreachable!("length-delimited, not numeric"),
    }
}

/// An expression reading a numeric value at `offset` in `buf`, falling back to the
/// type's default if the read fails.
fn scalar_read(scalar: Scalar, buf: &str, offset: &str) -> String {
    match scalar {
        Scalar::Bool => format!(
            "proto_codec::varint::read_varint32({buf}, {offset}).map(|(v, _)| v != 0).unwrap_or(false)"
        ),
        Scalar::Int32 => format!(
            "proto_codec::varint::read_varint32({buf}, {offset}).map(|(v, _)| v as i32).unwrap_or(0)"
        ),
        Scalar::Uint32 => {
            format!(
                "proto_codec::varint::read_varint32({buf}, {offset}).map(|(v, _)| v).unwrap_or(0)"
            )
        }
        Scalar::Sint32 => format!(
            "proto_codec::varint::read_varint32({buf}, {offset}).map(|(v, _)| proto_codec::varint::zigzag_decode32(v)).unwrap_or(0)"
        ),
        Scalar::Int64 => format!(
            "proto_codec::varint::read_varint({buf}, {offset}).map(|(v, _)| v as i64).unwrap_or(0)"
        ),
        Scalar::Uint64 => {
            format!(
                "proto_codec::varint::read_varint({buf}, {offset}).map(|(v, _)| v).unwrap_or(0)"
            )
        }
        Scalar::Sint64 => format!(
            "proto_codec::varint::read_varint({buf}, {offset}).map(|(v, _)| proto_codec::varint::zigzag_decode64(v)).unwrap_or(0)"
        ),
        Scalar::Fixed32 => format!("proto_codec::wire::read_fixed32({buf}, {offset}).unwrap_or(0)"),
        Scalar::Sfixed32 => {
            format!(
                "proto_codec::wire::read_fixed32({buf}, {offset}).map(|v| v as i32).unwrap_or(0)"
            )
        }
        Scalar::Float => format!(
            "proto_codec::wire::read_fixed32({buf}, {offset}).map(f32::from_bits).unwrap_or(0.0)"
        ),
        Scalar::Fixed64 => format!("proto_codec::wire::read_fixed64({buf}, {offset}).unwrap_or(0)"),
        Scalar::Sfixed64 => {
            format!(
                "proto_codec::wire::read_fixed64({buf}, {offset}).map(|v| v as i64).unwrap_or(0)"
            )
        }
        Scalar::Double => format!(
            "proto_codec::wire::read_fixed64({buf}, {offset}).map(f64::from_bits).unwrap_or(0.0)"
        ),
        Scalar::String | Scalar::Bytes => unreachable!("length-delimited, not numeric"),
    }
}

#[cfg(test)]
mod tests {
    use super::{file_name, relative_path};
    use crate::model::TypeRef;

    fn module(path: &str) -> Vec<String> {
        path.split('.')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn target(path: &str, name: &str) -> TypeRef {
        TypeRef {
            module: module(path),
            rust_name: name.to_string(),
        }
    }

    #[test]
    fn same_package_is_a_bare_name() {
        assert_eq!(
            relative_path(&module("geyser"), &target("geyser", "Ping")),
            "Ping"
        );
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
        assert_eq!(
            relative_path(&module("a"), &target("", "Root")),
            "super::Root"
        );
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
