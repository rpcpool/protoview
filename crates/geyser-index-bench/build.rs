fn main() {
    // The yellowstone protos live at the workspace root, shared with protoview-tests. Files
    // outside the package are not tracked by default, so list every input.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../proto");

    protoview_build::Config::new()
        .include("../../proto/yellowstone")
        .compile(&["../../proto/yellowstone/geyser.proto"])
        .expect("codegen for the yellowstone protos must succeed");
}
