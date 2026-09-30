fn main() {
    proto_codec_gen::Config::new()
        .include("proto")
        .compile(&[
            "proto/nested.proto",
            "proto/repeated.proto",
            "proto/all_types.proto",
        ])
        .expect("codegen for the fixture corpus must succeed");
}
