# yellowstone-grpc-protoview

Zero-copy [`protoview`](../protoview) views of the [Yellowstone gRPC](https://github.com/rpcpool/yellowstone-grpc)
geyser schema, plus a `tonic` codec that hands each incoming message back as raw bytes.

- **Views**: `geyser`, `solana::storage::confirmed_block` and `google::protobuf` hold
  view types generated from `yellowstone-grpc-proto` 13.0.0's protos (kept verbatim in
  [`proto/`](proto)). Names and layout match `prost`, so they sit next to
  `yellowstone-grpc-proto`'s types. `SubscribeUpdate::parse` validates a message once and
  copies nothing; every getter afterwards reads in place.
- **Client**: `client::GeyserClient` (feature `client`, on by default) connects with TLS and
  `x-token` handling and `subscribe`s to a `GeyserStream` of parsed updates.
- **Codec**: `codec::RawUpdateCodec` encodes `SubscribeRequest`s with `prost` and yields each
  incoming frame as `bytes::Bytes`, skipping tonic's own decode. Usable on its own with
  `--no-default-features`.

No `protoc` is needed to build.

```rust
use yellowstone_grpc_protoview::client::{ClientConfig, GeyserClient};
use yellowstone_grpc_protoview::geyser::subscribe_update::UpdateOneof;

let client = GeyserClient::connect(&ClientConfig {
    endpoint: "https://example.rpcpool.com".to_string(),
    x_token: Some("token".to_string()),
})
.await?;

// `request` is a `yellowstone_grpc_proto::geyser::SubscribeRequest`.
let mut subscription = client.subscribe(request).await?;
while let Some(update) = subscription.updates.next().await {
    // A `SubscribeUpdateView` (`SubscribeUpdate<Bytes>`): validated once, read in place.
    if let Some(UpdateOneof::Slot(slot)) = update?.update_oneof() {
        println!("slot {}", slot.slot());
    }
}
```

`GeyserStream` also implements `tokio_stream::Stream`. `subscribe_raw` yields the unparsed
`Bytes` of each update instead, and `subscription.requests` sends further requests (ping
replies, changed filters) on the same stream.

`GeyserStream` yields `SubscribeUpdate<Bytes>`, and `Bytes` implements `protoview::SharedBytes`, so
`update.update_oneof_owned()` gives a detached `Transaction`, `Account`, ... view that can move to
another thread without keeping the update alive. `update.into_inner()` returns the `Bytes`.

The `protoview` runtime is re-exported as `yellowstone_grpc_protoview::protoview`.

## `protoview-client`

A small binary that subscribes to accounts, transactions, entries, block meta and slot
statuses and prints one summary line per update. It sits behind the `cli` feature:

```sh
GRPC_ENDPOINT=https://example.rpcpool.com X_TOKEN=... \
  cargo run -p yellowstone-grpc-protoview --features cli --bin protoview-client
```

`--endpoint`, `--x-token` and `--env-file` (default `.env`) are also accepted. Stops on
Ctrl-C.
