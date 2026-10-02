//! Views generated from `proto/yellowstone/`, one `include!` per proto package.
//!
//! The module tree mirrors the package hierarchy, which the generated `super::` paths
//! between packages rely on.

pub mod geyser {
    include!(concat!(env!("OUT_DIR"), "/geyser.rs"));
}

pub mod google {
    pub mod protobuf {
        include!(concat!(env!("OUT_DIR"), "/google.protobuf.rs"));
    }
}

pub mod solana {
    pub mod storage {
        pub mod confirmed_block {
            include!(concat!(
                env!("OUT_DIR"),
                "/solana.storage.confirmed_block.rs"
            ));
        }
    }
}
