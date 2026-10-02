//! Build-time protobuf code generator producing zero-copy lens types.
//!
//! See `docs/design.md` in the repository root for the full design. [`Config::compile`]
//! handles every proto3 field shape — scalars, enums, messages, `optional`, `repeated`,
//! `map`, and `oneof` — and nested type declarations. Groups are rejected with
//! [`Error::UnsupportedField`]; the `fixed_bytes` option is not implemented yet.

mod codegen;
mod error;
mod model;
mod naming;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub use error::Error;
use model::Model;

/// Builder for a `.proto` -> Rust codegen run, in the style of `prost-build`.
pub struct Config {
    includes: Vec<PathBuf>,
    out_dir: Option<PathBuf>,
}

impl Config {
    /// Creates a config with no include paths and the default output directory
    /// (`$OUT_DIR`, as set by cargo for build scripts).
    pub fn new() -> Self {
        Self {
            includes: Vec::new(),
            out_dir: None,
        }
    }

    /// Adds a directory `protox` will search for imported `.proto` files.
    pub fn include(mut self, dir: impl AsRef<Path>) -> Self {
        self.includes.push(dir.as_ref().to_path_buf());
        self
    }

    /// Overrides the output directory. Defaults to `$OUT_DIR` when unset, which is
    /// correct for a normal build script; set this explicitly to check generated code
    /// into the repository instead.
    pub fn out_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.out_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Compiles `files` (and everything they transitively import) into one Rust source
    /// file per proto package under the configured output directory, named as
    /// `prost-build` names them: `solana.storage.ConfirmedBlock` becomes
    /// `solana.storage.confirmed_block.rs`, and a file without a `package` becomes `_.rs`.
    ///
    /// Each file holds only its package's items. Cross-package references are
    /// `super::`-relative, so include each file inside modules mirroring its package:
    ///
    /// ```ignore
    /// pub mod geyser {
    ///     include!(concat!(env!("OUT_DIR"), "/geyser.rs"));
    /// }
    /// pub mod solana {
    ///     pub mod storage {
    ///         pub mod confirmed_block {
    ///             include!(concat!(env!("OUT_DIR"), "/solana.storage.confirmed_block.rs"));
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// [`Error::Protox`] if the input files fail to parse or link.
    /// [`Error::UnsupportedField`] if a message uses a construct codegen does not
    /// support yet (see the module docs). [`Error::UnresolvedType`] if a message
    /// field's type could not be found among the compiled files. [`Error::Io`] if
    /// writing the generated file fails.
    pub fn compile(self, files: &[impl AsRef<Path>]) -> Result<(), Error> {
        let descriptor_set = protox::compile(files, &self.includes)?;
        let model = Model::build(&descriptor_set)?;
        let files = codegen::render(&model);

        let out_dir = self
            .out_dir
            .or_else(|| env::var_os("OUT_DIR").map(PathBuf::from))
            .expect("Config::out_dir or $OUT_DIR must be set");
        for file in files {
            let out_path = out_dir.join(&file.file_name);
            fs::write(&out_path, file.source).map_err(|source| Error::Io {
                path: out_path,
                source,
            })?;
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
