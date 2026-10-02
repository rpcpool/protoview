fn main() {
    // Files outside OUT_DIR are not tracked once any `rerun-if-changed` is emitted, so list
    // every input.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=proto");

    protoview_build::Config::new()
        .include("proto")
        .compile(&["proto/geyser.proto"])
        .expect("codegen for the yellowstone protos must succeed");
}
