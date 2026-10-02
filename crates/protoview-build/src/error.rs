use std::fmt;
use std::path::PathBuf;

/// A failure encountered while compiling `.proto` files into Rust source.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// `protox` failed to parse or link the input files.
    Protox(protox::Error),
    /// A message field used a construct this crate's codegen does not yet support.
    ///
    /// Only groups (proto2) reach this today; every proto3 field shape is supported.
    UnsupportedField {
        /// Fully-qualified name of the message the field is declared on.
        message: String,
        /// The field's name.
        field: String,
        /// Why codegen refused it.
        reason: &'static str,
    },
    /// A field's `type_name` did not resolve to a message or enum this crate generated
    /// from the input file set.
    UnresolvedType {
        message: String,
        field: String,
        type_name: String,
    },
    /// A `fixed_bytes` configuration does not apply to a field it can describe.
    InvalidFixedBytes {
        /// The configured field path, e.g. `.geyser.SubscribeUpdateAccountInfo.pubkey`.
        path: String,
        /// Why it was refused.
        reason: &'static str,
    },
    /// Writing generated source to disk failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protox(err) => write!(f, "protox failed: {err}"),
            Self::UnsupportedField {
                message,
                field,
                reason,
            } => write!(f, "{message}.{field}: {reason}"),
            Self::UnresolvedType {
                message,
                field,
                type_name,
            } => write!(
                f,
                "{message}.{field}: type {type_name} was not found among the compiled files"
            ),
            Self::InvalidFixedBytes { path, reason } => write!(f, "fixed_bytes {path}: {reason}"),
            Self::Io { path, source } => write!(f, "writing {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

impl From<protox::Error> for Error {
    fn from(err: protox::Error) -> Self {
        Self::Protox(err)
    }
}
