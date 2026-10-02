fn main() {
    // The yellowstone protos live at the workspace root, shared with geyser-index-bench.
    // Files outside the package are not tracked by default, so list every input.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=../../proto");

    protoview_build::Config::new()
        // First, so `import "geyser.proto"` in fumarole.proto resolves to yellowstone
        // 13.0.0 rather than the older copy in proto/.
        .include("../../proto/yellowstone")
        .include("proto")
        .fixed_bytes(".fixtures.fixed.Account.pubkey", 32)
        .fixed_bytes(".fixtures.fixed.Account.signature", 64)
        .fixed_bytes(".fixtures.fixed.Account.history", 32)
        .fixed_bytes("fixtures.fixed.Account.hash", 8) // leading `.` is optional
        .fixed_bytes(".fixtures.fixed.Account.Owner.key", 16)
        .fixed_bytes(".shop.orders.Order.customer_id", 16)
        .compile(&[
            "proto/nested.proto",
            "proto/repeated.proto",
            "proto/all_types.proto",
            "proto/oneof.proto",
            "proto/enums.proto",
            "proto/maps.proto",
            "proto/fixed_bytes.proto",
            "proto/shop/orders.proto",
            "../../proto/yellowstone/geyser.proto",
            "proto/fumarole.proto",
        ])
        .expect("codegen for the fixture corpus must succeed");
}
