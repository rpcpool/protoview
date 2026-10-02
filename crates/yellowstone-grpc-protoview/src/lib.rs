//! Zero-copy [`protoview`] views of the Yellowstone gRPC schema, and a [`tonic`] codec that
//! hands incoming messages back as raw bytes so they can be viewed instead of decoded.
//!
//! - [`geyser`], [`solana`] and [`google`] hold the generated view types, one module per proto
//!   package with the same names and layout `prost` uses. The schema is
//!   `yellowstone-grpc-proto` 13.0.0's, verbatim.
//! - [`codec::RawUpdateCodec`] encodes `SubscribeRequest`s with `prost` and yields each
//!   incoming frame as [`Bytes`](bytes::Bytes). Pass it to
//!   [`tonic::client::Grpc::streaming`] for `/geyser.Geyser/Subscribe` to use it directly,
//!   then [`parse`](geyser::SubscribeUpdate::parse) each frame.
//!
//! - [`client::GeyserClient`] (feature `client`, on by default) connects to an endpoint and
//!   opens the `Subscribe` stream with that codec, handling TLS and the `x-token` header.
//!
//! [`protoview`] is re-exported so callers can name [`protoview::DecodeError`] without a
//! direct dependency.

#[cfg(feature = "client")]
pub mod client;
pub mod codec;

pub use protoview;

// One generated file per proto package; the module tree mirrors the package hierarchy,
// which the generated `super::` paths between packages rely on.

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
