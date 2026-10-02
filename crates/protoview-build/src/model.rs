use std::collections::{BTreeMap, BTreeSet, HashMap};

use prost_types::field_descriptor_proto::{Label, Type as FieldType};
use prost_types::{DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorSet};

use crate::error::Error;
use crate::naming::{escape_ident, module_path, snake_case, upper_camel};

/// A scalar (non-message) field kind, grouped by how it is read off the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    Int32,
    Int64,
    Uint32,
    Uint64,
    Sint32,
    Sint64,
    Fixed32,
    Fixed64,
    Sfixed32,
    Sfixed64,
    Float,
    Double,
    Bool,
    String,
    Bytes,
    /// `bytes` configured with `fixed_bytes`: exactly this many bytes, checked at parse
    /// and exposed as `[u8; N]`.
    FixedBytes(u32),
}

/// A resolved reference to a generated message or enum type.
#[derive(Debug, Clone)]
pub struct TypeRef {
    /// Rust module path the type lives in, from the include root: its package's module,
    /// plus one module per enclosing message for nested types.
    pub module: Vec<String>,
    /// The type's Rust name.
    pub rust_name: String,
}

/// The key and value of a `map` field, resolved from its synthesized entry message.
#[derive(Debug, Clone)]
pub struct MapKind {
    /// The entry message protoc synthesizes (`XxxEntry`), used to validate entries.
    pub entry: TypeRef,
    /// The key's kind: always an integral, `bool` or `string` scalar.
    pub key: FieldKind,
    /// The value's kind: anything but another map.
    pub value: FieldKind,
}

/// What a field's payload is, once `type_name` (if any) has been resolved.
#[derive(Debug, Clone)]
pub enum FieldKind {
    Scalar(Scalar),
    /// An enum, encoded as an `int32` varint.
    Enum(TypeRef),
    /// A nested message.
    Message(TypeRef),
    /// A `map<K, V>`; such fields are always `repeated` on the wire.
    Map(Box<MapKind>),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub number: u32,
    /// Whether the field is `repeated`, which includes every map field.
    pub repeated: bool,
    /// Whether the field is proto3 `optional`, i.e. has explicit presence.
    pub optional: bool,
    pub kind: FieldKind,
}

/// A real (non-synthetic) `oneof`: at most one of its members is set, and the last one
/// on the wire wins.
#[derive(Debug, Clone)]
pub struct Oneof {
    /// The `oneof`'s proto name, e.g. `update_oneof`.
    pub name: String,
    /// Its members, in declaration order. Never `repeated` or `optional`.
    pub members: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub rust_name: String,
    /// Rust module path of the package, from the include root. Decides which generated
    /// file the message is written to.
    pub package: Vec<String>,
    /// Rust module path the message itself lives in: `package`, plus one module per
    /// enclosing message.
    pub module: Vec<String>,
    /// Module holding this message's nested types and oneof enums, as `prost` lays them
    /// out: `module` plus the message's `snake_case` name.
    pub nested_module: Vec<String>,
    /// Whether this is a map entry synthesized by protoc. Entries get a view, used to
    /// validate them, but are hidden from documentation as `prost` hides them entirely.
    pub map_entry: bool,
    /// Fields outside any real `oneof`, in declaration order.
    pub fields: Vec<Field>,
    /// Real `oneof`s, in declaration order. proto3 `optional` fields, which protoc wraps
    /// in a synthetic single-member oneof, are in `fields` instead.
    pub oneofs: Vec<Oneof>,
}

/// One variant of an [`Enum`].
#[derive(Debug, Clone)]
pub struct EnumValue {
    /// The proto value name, e.g. `SLOT_PROCESSED`.
    pub proto_name: String,
    /// The Rust variant name, e.g. `SlotProcessed`.
    pub rust_name: String,
    pub number: i32,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub rust_name: String,
    /// See [`Message::package`].
    pub package: Vec<String>,
    /// See [`Message::module`].
    pub module: Vec<String>,
    /// Values in declaration order, aliases (a repeated number) removed.
    pub values: Vec<EnumValue>,
    /// Name of the variant carrying values this schema does not know: `Unknown`, unless a
    /// declared value already takes that name.
    pub unknown_variant: String,
}

/// The result of walking a compiled [`FileDescriptorSet`]: every message and enum the
/// codegen emits.
pub struct Model {
    pub messages: Vec<Message>,
    pub enums: Vec<Enum>,
}

/// What a fully-qualified proto type name resolves to.
enum TypeInfo {
    Message {
        target: TypeRef,
        /// For a synthesized map entry, its key and value fields.
        map_entry: Option<Box<(FieldDescriptorProto, FieldDescriptorProto)>>,
    },
    Enum(TypeRef),
}

impl Model {
    /// Walks every file in `set`, producing a [`Model`].
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedField`] for group fields. [`Error::UnresolvedType`] if a
    /// field's `type_name` does not name a message or enum declared in `set`.
    /// [`Error::InvalidFixedBytes`] if a `fixed_bytes` path names no field, a field that
    /// is not `bytes`, or a map field or entry.
    pub fn build(
        set: &FileDescriptorSet,
        fixed_bytes: &BTreeMap<String, u32>,
    ) -> Result<Self, Error> {
        let mut types = HashMap::new();
        for file in &set.file {
            let package = module_path(file.package());
            let prefix = match file.package() {
                "" => String::new(),
                package => format!(".{package}"),
            };
            for descriptor in &file.message_type {
                index_message(descriptor, &prefix, &package, &mut types);
            }
            for descriptor in &file.enum_type {
                index_enum(descriptor, &prefix, &package, &mut types);
            }
        }

        let mut model = Self {
            messages: Vec::new(),
            enums: Vec::new(),
        };
        let mut context = BuildContext {
            types: &types,
            fixed_bytes,
            matched: BTreeSet::new(),
        };
        for file in &set.file {
            let package = module_path(file.package());
            let prefix = match file.package() {
                "" => String::new(),
                package => format!(".{package}"),
            };
            for descriptor in &file.message_type {
                model.add_message(descriptor, &prefix, &package, &package, &mut context)?;
            }
            for descriptor in &file.enum_type {
                model.enums.push(build_enum(descriptor, &package, &package));
            }
        }
        if let Some(path) = fixed_bytes
            .keys()
            .find(|path| !context.matched.contains(*path))
        {
            return Err(Error::InvalidFixedBytes {
                path: path.clone(),
                reason: "no field has this fully-qualified name",
            });
        }
        Ok(model)
    }

    /// Adds `descriptor`, and every message and enum nested in it, to the model.
    fn add_message(
        &mut self,
        descriptor: &DescriptorProto,
        prefix: &str,
        package: &[String],
        module: &[String],
        context: &mut BuildContext<'_>,
    ) -> Result<(), Error> {
        let proto_name = format!("{prefix}.{}", descriptor.name());
        let message = build_message(descriptor, &proto_name, package, module, context)?;
        for nested in &descriptor.nested_type {
            self.add_message(
                nested,
                &proto_name,
                package,
                &message.nested_module,
                context,
            )?;
        }
        for nested in &descriptor.enum_type {
            self.enums
                .push(build_enum(nested, package, &message.nested_module));
        }
        self.messages.push(message);
        Ok(())
    }
}

/// The module nested types of a message named `rust_name` live in, under `module`.
fn nested_module(module: &[String], rust_name: &str) -> Vec<String> {
    let mut nested = module.to_vec();
    nested.push(escape_ident(&snake_case(rust_name)));
    nested
}

/// Records `descriptor` and everything nested in it under their fully-qualified proto
/// names, e.g. `.geyser.SubscribeRequest.AccountsEntry`.
fn index_message(
    descriptor: &DescriptorProto,
    prefix: &str,
    module: &[String],
    types: &mut HashMap<String, TypeInfo>,
) {
    let proto_name = format!("{prefix}.{}", descriptor.name());
    let rust_name = upper_camel(descriptor.name());
    let nested = nested_module(module, &rust_name);

    for child in &descriptor.nested_type {
        index_message(child, &proto_name, &nested, types);
    }
    for child in &descriptor.enum_type {
        index_enum(child, &proto_name, &nested, types);
    }

    let is_map_entry = descriptor
        .options
        .as_ref()
        .is_some_and(|options| options.map_entry());
    let map_entry = if is_map_entry {
        let field = |number| {
            descriptor
                .field
                .iter()
                .find(|f| f.number() == number)
                .cloned()
        };
        field(1).zip(field(2)).map(Box::new)
    } else {
        None
    };
    types.insert(
        proto_name,
        TypeInfo::Message {
            target: TypeRef {
                module: module.to_vec(),
                rust_name,
            },
            map_entry,
        },
    );
}

/// Records an enum under its fully-qualified proto name.
fn index_enum(
    descriptor: &EnumDescriptorProto,
    prefix: &str,
    module: &[String],
    types: &mut HashMap<String, TypeInfo>,
) {
    types.insert(
        format!("{prefix}.{}", descriptor.name()),
        TypeInfo::Enum(TypeRef {
            module: module.to_vec(),
            rust_name: upper_camel(descriptor.name()),
        }),
    );
}

fn build_enum(descriptor: &EnumDescriptorProto, package: &[String], module: &[String]) -> Enum {
    let rust_name = upper_camel(descriptor.name());
    let mut seen = std::collections::HashSet::new();
    let values: Vec<EnumValue> = descriptor
        .value
        .iter()
        // `allow_alias` lets several names share a number; like `prost`, keep the first.
        .filter(|value| seen.insert(value.number()))
        .map(|value| EnumValue {
            proto_name: value.name().to_string(),
            rust_name: strip_enum_prefix(&rust_name, &upper_camel(value.name())),
            number: value.number(),
        })
        .collect();
    let unknown_variant = if values.iter().any(|value| value.rust_name == "Unknown") {
        "Unrecognized".to_string()
    } else {
        "Unknown".to_string()
    };
    Enum {
        rust_name,
        package: package.to_vec(),
        module: module.to_vec(),
        values,
        unknown_variant,
    }
}

/// Strips the enum's name from the front of a variant name, as `prost` does by default:
/// only when the remainder starts a new word, so `CommitmentLevel` + `CommitmentLevelHigh`
/// gives `High`, while `SlotStatus` + `SlotProcessed` keeps `SlotProcessed`.
fn strip_enum_prefix(enum_name: &str, variant: &str) -> String {
    match variant.strip_prefix(enum_name) {
        Some(rest) if rest.starts_with(char::is_uppercase) => rest.to_string(),
        _ => variant.to_string(),
    }
}

/// State shared across one [`Model::build`].
struct BuildContext<'a> {
    types: &'a HashMap<String, TypeInfo>,
    /// `fixed_bytes` configuration: fully-qualified field path to length.
    fixed_bytes: &'a BTreeMap<String, u32>,
    /// The configured paths that matched a field, to report the ones that did not.
    matched: BTreeSet<String>,
}

fn build_message(
    descriptor: &DescriptorProto,
    proto_name: &str,
    package: &[String],
    module: &[String],
    context: &mut BuildContext<'_>,
) -> Result<Message, Error> {
    let rust_name = upper_camel(descriptor.name());
    let message_name = descriptor.name();
    let map_entry = descriptor
        .options
        .as_ref()
        .is_some_and(|options| options.map_entry());

    let mut fields = Vec::with_capacity(descriptor.field.len());
    // Keyed by `oneof_index`, so oneofs come out in declaration order.
    let mut oneof_members: BTreeMap<i32, Vec<Field>> = BTreeMap::new();
    for field in &descriptor.field {
        // proto3 `optional` is encoded as a single-member synthetic oneof, so its
        // `oneof_index` is set too; only non-optional members belong to a real oneof.
        let optional = field.proto3_optional();
        let oneof_index = field.oneof_index.filter(|_| !optional);

        let mut kind = field_kind(field, message_name, context.types)?;
        let path = format!("{proto_name}.{}", field.name());
        if let Some(&len) = context.fixed_bytes.get(&path) {
            kind = fixed_bytes_kind(&path, len, kind, map_entry)?;
            context.matched.insert(path);
        }
        let field = Field {
            name: field.name().to_string(),
            number: field.number() as u32,
            repeated: field.label() == Label::Repeated,
            optional,
            kind,
        };
        match oneof_index {
            Some(index) => oneof_members.entry(index).or_default().push(field),
            None => fields.push(field),
        }
    }

    let oneofs = oneof_members
        .into_iter()
        .map(|(index, members)| Oneof {
            name: descriptor
                .oneof_decl
                .get(index as usize)
                .map(|decl| decl.name().to_string())
                .unwrap_or_default(),
            members,
        })
        .collect();

    Ok(Message {
        nested_module: nested_module(module, &rust_name),
        rust_name,
        package: package.to_vec(),
        module: module.to_vec(),
        map_entry,
        fields,
        oneofs,
    })
}

/// Applies a `fixed_bytes` configuration to the field at `path`.
///
/// # Errors
///
/// [`Error::InvalidFixedBytes`] unless the field is a plain, optional, repeated or oneof
/// `bytes` field outside a map.
fn fixed_bytes_kind(
    path: &str,
    len: u32,
    kind: FieldKind,
    in_map_entry: bool,
) -> Result<FieldKind, Error> {
    let invalid = |reason| Error::InvalidFixedBytes {
        path: path.to_string(),
        reason,
    };
    if in_map_entry {
        return Err(invalid("map keys and values cannot be fixed_bytes"));
    }
    match kind {
        FieldKind::Scalar(Scalar::Bytes) => Ok(FieldKind::Scalar(Scalar::FixedBytes(len))),
        FieldKind::Map(_) => Err(invalid("map fields cannot be fixed_bytes")),
        _ => Err(invalid("only `bytes` fields can be fixed_bytes")),
    }
}

/// Resolves what a field carries.
///
/// # Errors
///
/// [`Error::UnsupportedField`] for groups, [`Error::UnresolvedType`] for a message or enum
/// type not found in the compiled set.
fn field_kind(
    field: &FieldDescriptorProto,
    message_name: &str,
    types: &HashMap<String, TypeInfo>,
) -> Result<FieldKind, Error> {
    let scalar = match field.r#type() {
        FieldType::Double => Scalar::Double,
        FieldType::Float => Scalar::Float,
        FieldType::Int64 => Scalar::Int64,
        FieldType::Uint64 => Scalar::Uint64,
        FieldType::Int32 => Scalar::Int32,
        FieldType::Fixed64 => Scalar::Fixed64,
        FieldType::Fixed32 => Scalar::Fixed32,
        FieldType::Bool => Scalar::Bool,
        FieldType::String => Scalar::String,
        FieldType::Bytes => Scalar::Bytes,
        FieldType::Uint32 => Scalar::Uint32,
        FieldType::Sfixed32 => Scalar::Sfixed32,
        FieldType::Sfixed64 => Scalar::Sfixed64,
        FieldType::Sint32 => Scalar::Sint32,
        FieldType::Sint64 => Scalar::Sint64,
        FieldType::Group => {
            return Err(Error::UnsupportedField {
                message: message_name.to_string(),
                field: field.name().to_string(),
                reason: "groups are not supported",
            });
        }
        FieldType::Message | FieldType::Enum => {
            let unresolved = || Error::UnresolvedType {
                message: message_name.to_string(),
                field: field.name().to_string(),
                type_name: field.type_name().to_string(),
            };
            return match types.get(field.type_name()).ok_or_else(unresolved)? {
                TypeInfo::Enum(target) => Ok(FieldKind::Enum(target.clone())),
                TypeInfo::Message {
                    target,
                    map_entry: Some(entry),
                } if field.label() == Label::Repeated => Ok(FieldKind::Map(Box::new(MapKind {
                    entry: target.clone(),
                    key: field_kind(&entry.0, message_name, types)?,
                    value: field_kind(&entry.1, message_name, types)?,
                }))),
                TypeInfo::Message { target, .. } => Ok(FieldKind::Message(target.clone())),
            };
        }
    };
    Ok(FieldKind::Scalar(scalar))
}
