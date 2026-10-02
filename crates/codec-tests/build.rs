fn main() {
    // The yellowstone fixtures live at the workspace root, shared with geyser-index-bench.
    // Files outside the package are not tracked by default, so list every input.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=../../proto");

    proto_codec_gen::Config::new()
        .include("proto")
        .include("../../proto")
        .compile(&[
            "proto/nested.proto",
            "proto/repeated.proto",
            "proto/all_types.proto",
            "proto/oneof.proto",
            "../../proto/yellowstone/geyser.proto",
        ])
        .expect("codegen for the fixture corpus must succeed");
}
