use std::collections::HashMap;

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

/// What a field's payload is, once `type_name` (if any) has been resolved.
#[derive(Debug, Clone)]
pub enum FieldKind {
    Scalar(Scalar),
    /// A singular nested message, naming its absolute Rust path from the crate root.
    Message(String),
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

#[derive(Debug, Clone)]
pub struct Message {
    pub rust_name: String,
    /// Rust module path this message's code lives in, from the crate root.
    pub module: Vec<String>,
    pub fields: Vec<Field>,
}

/// The result of walking a compiled [`FileDescriptorSet`]: every message the codegen
/// knows how to emit.
pub struct Model {
    pub messages: Vec<Message>,
}

/// Where a resolved proto type lives, for rendering field types as absolute Rust paths.
struct TypeLocation {
    rust_name: String,
    module: Vec<String>,
}

impl Model {
    /// Walks every file in `set`, producing a [`Model`].
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedField`] for `oneof` members,
    /// map fields, group fields, and enum fields — none of these are implemented yet —
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

fn index_types(set: &FileDescriptorSet) -> HashMap<String, TypeLocation> {
    let mut locations = HashMap::new();
    for file in &set.file {
        let module = module_path(file.package());
        for descriptor in &file.message_type {
            let proto_name = format!(".{}.{}", file.package(), descriptor.name());
            locations.insert(
                proto_name,
                TypeLocation {
                    rust_name: descriptor.name().to_string(),
                    module: module.clone(),
                },
            );
        }
    }
    locations
}

/// Resolves a fully-qualified proto message name to an absolute Rust path from the
/// crate root, e.g. `.pkg.Foo` -> `crate::pkg::Foo`.
fn rust_path(type_name: &str, locations: &HashMap<String, TypeLocation>) -> Option<String> {
    let location = locations.get(type_name)?;
    let mut path = vec!["crate".to_string()];
    path.extend(location.module.iter().cloned());
    path.push(location.rust_name.clone());
    Some(path.join("::"))
}

fn build_message(
    descriptor: &DescriptorProto,
    module: &[String],
    locations: &HashMap<String, TypeLocation>,
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
    for field in &descriptor.field {
        let field_name = field.name().to_string();
        // proto3 `optional` is encoded as a single-member synthetic oneof, so its
        // `oneof_index` is set; only real oneofs are rejected.
        let optional = field.proto3_optional();
        if field.oneof_index.is_some() && !optional {
            return Err(Error::UnsupportedField {
                message: message_name,
                field: field_name,
                reason: "oneof members are not supported yet",
            });
        }

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
                match rust_path(&type_name, locations) {
                    Some(path) => FieldKind::Message(path),
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

        fields.push(Field {
            name: field_name,
            number: field.number() as u32,
            repeated: field.label() == Label::Repeated,
            optional,
            kind,
        });
    }

    Ok(Message {
        rust_name: message_name,
        module: module.to_vec(),
        fields,
    })
}
