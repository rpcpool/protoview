use std::collections::{BTreeMap, HashMap};

use prost_types::field_descriptor_proto::{Label, Type as FieldType};
use prost_types::{DescriptorProto, FileDescriptorSet};

use crate::error::Error;
use crate::naming::module_path;

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
}

/// A resolved reference to a generated message type.
#[derive(Debug, Clone)]
pub struct TypeRef {
    /// Rust module path of the package the type lives in, from the include root.
    pub module: Vec<String>,
    /// The type's Rust name.
    pub rust_name: String,
}

/// What a field's payload is, once `type_name` (if any) has been resolved.
#[derive(Debug, Clone)]
pub enum FieldKind {
    Scalar(Scalar),
    /// A nested message. Codegen renders the path relative to the referencing package.
    Message(TypeRef),
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub number: u32,
    /// Whether the field is `repeated`. Map fields never reach the model: their
    /// synthesized entry message is a nested declaration, which is rejected.
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
    /// Rust module path of this message's package, from the include root. Also decides
    /// which generated file the message is written to.
    pub module: Vec<String>,
    /// Fields outside any real `oneof`, in declaration order.
    pub fields: Vec<Field>,
    /// Real `oneof`s, in declaration order. proto3 `optional` fields, which protoc wraps
    /// in a synthetic single-member oneof, are in `fields` instead.
    pub oneofs: Vec<Oneof>,
}

/// The result of walking a compiled [`FileDescriptorSet`]: every message the codegen
/// knows how to emit.
pub struct Model {
    pub messages: Vec<Message>,
}

impl Model {
    /// Walks every file in `set`, producing a [`Model`].
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedField`] for map fields, group fields, and enum fields — none of these are implemented yet —
    /// and for nested `message`/`enum` declarations, since only top-level messages are
    /// handled so far. [`Error::UnresolvedType`] if a message field's `type_name` does
    /// not name a message declared in `set`.
    pub fn build(set: &FileDescriptorSet) -> Result<Self, Error> {
        let locations = index_types(set);

        let mut messages = Vec::new();
        for file in &set.file {
            let module = module_path(file.package());
            for descriptor in &file.message_type {
                messages.push(build_message(descriptor, &module, &locations)?);
            }
        }
        Ok(Self { messages })
    }
}

fn index_types(set: &FileDescriptorSet) -> HashMap<String, TypeRef> {
    let mut locations = HashMap::new();
    for file in &set.file {
        let module = module_path(file.package());
        for descriptor in &file.message_type {
            let proto_name = match file.package() {
                "" => format!(".{}", descriptor.name()),
                package => format!(".{package}.{}", descriptor.name()),
            };
            locations.insert(
                proto_name,
                TypeRef {
                    rust_name: descriptor.name().to_string(),
                    module: module.clone(),
                },
            );
        }
    }
    locations
}

fn build_message(
    descriptor: &DescriptorProto,
    module: &[String],
    locations: &HashMap<String, TypeRef>,
) -> Result<Message, Error> {
    let message_name = descriptor.name().to_string();

    if !descriptor.nested_type.is_empty() || !descriptor.enum_type.is_empty() {
        return Err(Error::UnsupportedField {
            message: message_name,
            field: String::new(),
            reason: "nested message/enum declarations are not supported yet",
        });
    }

    let mut fields = Vec::with_capacity(descriptor.field.len());
    // Keyed by `oneof_index`, so oneofs come out in declaration order.
    let mut oneof_members: BTreeMap<i32, Vec<Field>> = BTreeMap::new();
    for field in &descriptor.field {
        let field_name = field.name().to_string();
        // proto3 `optional` is encoded as a single-member synthetic oneof, so its
        // `oneof_index` is set too; only non-optional members belong to a real oneof.
        let optional = field.proto3_optional();
        let oneof_index = field.oneof_index.filter(|_| !optional);

        let kind = match field.r#type() {
            FieldType::Double => FieldKind::Scalar(Scalar::Double),
            FieldType::Float => FieldKind::Scalar(Scalar::Float),
            FieldType::Int64 => FieldKind::Scalar(Scalar::Int64),
            FieldType::Uint64 => FieldKind::Scalar(Scalar::Uint64),
            FieldType::Int32 => FieldKind::Scalar(Scalar::Int32),
            FieldType::Fixed64 => FieldKind::Scalar(Scalar::Fixed64),
            FieldType::Fixed32 => FieldKind::Scalar(Scalar::Fixed32),
            FieldType::Bool => FieldKind::Scalar(Scalar::Bool),
            FieldType::String => FieldKind::Scalar(Scalar::String),
            FieldType::Bytes => FieldKind::Scalar(Scalar::Bytes),
            FieldType::Uint32 => FieldKind::Scalar(Scalar::Uint32),
            FieldType::Sfixed32 => FieldKind::Scalar(Scalar::Sfixed32),
            FieldType::Sfixed64 => FieldKind::Scalar(Scalar::Sfixed64),
            FieldType::Sint32 => FieldKind::Scalar(Scalar::Sint32),
            FieldType::Sint64 => FieldKind::Scalar(Scalar::Sint64),
            FieldType::Message => {
                let type_name = field.type_name().to_string();
                match locations.get(&type_name) {
                    Some(target) => FieldKind::Message(target.clone()),
                    None => {
                        return Err(Error::UnresolvedType {
                            message: message_name,
                            field: field_name,
                            type_name,
                        });
                    }
                }
            }
            FieldType::Enum => {
                return Err(Error::UnsupportedField {
                    message: message_name,
                    field: field_name,
                    reason: "enum fields are not supported yet",
                });
            }
            FieldType::Group => {
                return Err(Error::UnsupportedField {
                    message: message_name,
                    field: field_name,
                    reason: "groups are not supported",
                });
            }
        };

        let field = Field {
            name: field_name,
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
        rust_name: message_name,
        module: module.to_vec(),
        fields,
        oneofs,
    })
}
