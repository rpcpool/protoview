//! `Config::fixed_bytes` misconfigurations are build errors naming the path, never
//! silently ignored.

use std::fs;
use std::path::PathBuf;

use protoview_build::{Config, Error};

const PROTO: &str = r#"
syntax = "proto3";
package demo;
message Thing {
    bytes key = 1;
    string name = 2;
    map<string, bytes> blobs = 3;
    message Inner { bytes id = 1; }
}
"#;

/// A scratch directory holding `demo.proto`, unique per test.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("protoview-build-{test}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("demo.proto"), PROTO).unwrap();
    dir
}

fn compile(test: &str, configure: impl FnOnce(Config) -> Config) -> Result<(), Error> {
    let dir = scratch(test);
    let result =
        configure(Config::new().include(&dir).out_dir(&dir)).compile(&[dir.join("demo.proto")]);
    let _ = fs::remove_dir_all(&dir);
    result
}

fn rejected(result: Result<(), Error>) -> (String, &'static str) {
    match result {
        Err(Error::InvalidFixedBytes { path, reason }) => (path, reason),
        other => panic!("expected InvalidFixedBytes, got {other:?}"),
    }
}

#[test]
fn valid_paths_compile() {
    compile("valid", |c| {
        c.fixed_bytes(".demo.Thing.key", 32)
            .fixed_bytes("demo.Thing.Inner.id", 16)
    })
    .unwrap();
}

#[test]
fn unmatched_path_is_an_error() {
    let (path, reason) = rejected(compile("unmatched", |c| {
        c.fixed_bytes(".demo.Thing.kye", 32)
    }));
    assert_eq!(path, ".demo.Thing.kye");
    assert!(reason.contains("no field"), "{reason}");
}

#[test]
fn non_bytes_field_is_an_error() {
    let (path, reason) = rejected(compile("non-bytes", |c| {
        c.fixed_bytes(".demo.Thing.name", 4)
    }));
    assert_eq!(path, ".demo.Thing.name");
    assert!(reason.contains("bytes"), "{reason}");
}

#[test]
fn maps_are_an_error() {
    let (_, reason) = rejected(compile("map-field", |c| {
        c.fixed_bytes(".demo.Thing.blobs", 4)
    }));
    assert!(reason.contains("map"), "{reason}");

    let (_, reason) = rejected(compile("map-value", |c| {
        c.fixed_bytes(".demo.Thing.BlobsEntry.value", 4)
    }));
    assert!(reason.contains("map"), "{reason}");
}

#[test]
fn zero_length_is_an_error() {
    let (path, reason) = rejected(compile("zero", |c| c.fixed_bytes(".demo.Thing.key", 0)));
    assert_eq!(path, ".demo.Thing.key");
    assert!(reason.contains("at least 1"), "{reason}");
}
