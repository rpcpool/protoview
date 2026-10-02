# protoview

Zero-copy protobuf reading for Rust, generated at build time.

`protoview-build` reads your `.proto` files in a build script and generates **view
types**: read-only windows onto encoded protobuf bytes. Parsing a view validates the whole
message once, records where each field lives in a small fixed-size index, and copies
nothing. Every getter after that reads straight from your buffer and cannot fail.

```rust
let order = Order::parse(bytes)?;            // one validating pass, no allocation
let id: u64 = order.id();                    // read in place
for item in order.items() { /* ... */ }      // nested messages are views too
```

Compared with `prost`, which decodes into owned structs (a `String` per string, a `Vec`
per repeated field), a view costs one structural walk and a few hundred bytes of stack.
That suits hot paths that read a handful of fields from large or numerous messages. Names
and module layout match `prost`, so view types can sit next to prost-generated types for
the same schema.

## Crates

| Crate | Role |
| --- | --- |
| [`protoview-build`](crates/protoview-build) | Build-time generator. Use it from `build.rs`. |
| [`protoview`](crates/protoview) | Runtime the generated code calls into. `no_std`, no dependencies. |

The other crates in the workspace are tests and a benchmark tool; see [Development](#development).

## Quick start

### 1. Dependencies

No `protoc` is needed: `.proto` files are parsed by [`protox`](https://crates.io/crates/protox),
which also bundles the `google/protobuf/*.proto` well-known types.

### 2. A schema

```proto
// proto/shop/common.proto
syntax = "proto3";
package shop.common;

message Money {
  string currency = 1;
  int64 units = 2;
  int32 nanos = 3;
}

message Address {
  string line1 = 1;
  string city = 2;
  string country = 3;
}
```

```proto
// proto/shop/orders.proto
syntax = "proto3";
package shop.orders;

import "shop/common.proto";

enum Status {
  STATUS_UNSPECIFIED = 0;
  STATUS_PENDING = 1;
  STATUS_SHIPPED = 2;
}

message LineItem {
  string sku = 1;
  uint32 quantity = 2;
  shop.common.Money price = 3;
}

message Order {
  uint64 id = 1;
  bytes customer_id = 2;  // always a 16-byte UUID
  Status status = 3;
  repeated LineItem items = 4;
  map<string, string> tags = 5;
  optional string note = 6;
  oneof delivery {
    shop.common.Address address = 7;
    string pickup_point = 8;
  }
}
```

### 3. `build.rs`

```rust
fn main() {
    println!("cargo:rerun-if-changed=proto");

    protoview_build::Config::new()
        .include("proto")
        // `customer_id` is always 16 bytes: expose it as `[u8; 16]`.
        .fixed_bytes(".shop.orders.Order.customer_id", 16)
        .compile(&["proto/shop/orders.proto"])
        .expect("protobuf codegen failed");
}
```

Only `orders.proto` is listed: everything it imports (here `shop/common.proto`) is
generated too.

### 4. Include the generated code

One file is generated per proto package, named like `prost-build` names them
(`shop.common.rs`, `shop.orders.rs`). Include each one in a module tree that mirrors the
package names; references between packages are `super::`-relative and rely on it:

```rust
pub mod shop {
    pub mod common {
        include!(concat!(env!("OUT_DIR"), "/shop.common.rs"));
    }
    pub mod orders {
        include!(concat!(env!("OUT_DIR"), "/shop.orders.rs"));
    }
}
```

### 5. Read messages

```rust
use std::collections::HashMap;

use protoview::DecodeError;

use crate::shop::orders::order::Delivery;
use crate::shop::orders::{Order, Status};

fn print_order(bytes: &[u8]) -> Result<(), DecodeError> {
    // One pass over the bytes validates the whole message tree. Nothing is copied or
    // allocated; `order` is a view over `bytes`.
    let order = Order::parse(bytes)?;

    // Scalars come back by value, with the proto3 default when absent.
    let id: u64 = order.id();
    // A `fixed_bytes` field is a `[u8; N]`, length-checked by `parse`.
    let customer: [u8; 16] = order.customer_id();
    println!("order {id} for customer {customer:02x?}");

    // Enums keep values this schema does not know.
    match order.status() {
        Status::Unspecified => println!("status: not set"),
        Status::Pending => println!("status: pending"),
        Status::Shipped => println!("status: shipped"),
        Status::Unknown(value) => println!("status: {value} (newer than this schema)"),
    }

    // Repeated fields are iterators, in wire order.
    for item in order.items() {
        // Strings are UTF-8-checked when read, so they return a `Result`.
        let sku = item.sku().unwrap_or("<invalid utf-8>");
        // A nested message is an `Option`: there is no default to fall back to.
        let price = item
            .price()
            .map(|money| format!("{}.{:09} {}", money.units(), money.nanos(), money.currency().unwrap_or("?")))
            .unwrap_or_else(|| "no price".to_string());
        println!("  {} x {sku} at {price}", item.quantity());
    }

    // Maps iterate as (key, value) pairs; collect them for last-wins lookups.
    let tags: HashMap<&str, &str> = order
        .tags()
        .filter_map(|(key, value)| Some((key.ok()?, value.ok()?)))
        .collect();
    println!("  tags: {tags:?}");

    // `optional` fields have explicit presence.
    if let Some(Ok(note)) = order.note() {
        println!("  note: {note}");
    }

    // A oneof is an enum of its members, or `None` if none was set.
    match order.delivery() {
        Some(Delivery::Address(address)) => {
            println!("  ship to {}, {}", address.city().unwrap_or("?"), address.country().unwrap_or("?"))
        }
        Some(Delivery::PickupPoint(point)) => println!("  pick up at {}", point.unwrap_or("?")),
        None => println!("  no delivery chosen"),
    }
    Ok(())
}
```

This example is compiled and run as a test
([`crates/protoview-tests/src/readme_tests.rs`](crates/protoview-tests/src/readme_tests.rs)), so it
tracks the real API.

## Buffers

A view is generic over its buffer: `Order<B: AsRef<[u8]>>`. `parse` takes the buffer by
value, so the view can borrow or own it:

```rust
let borrowed = Order::parse(bytes.as_slice())?;   // Order<&[u8]>
let owned = Order::parse(bytes)?;                 // Order<Vec<u8>>, movable, 'static
let shared = Order::parse(frame)?;                // Order<bytes::Bytes>, zero-copy from the network
```

Nested views returned by getters always borrow from their parent (`LineItem<&[u8]>`).

## What gets generated

For a message `Order`, the generator emits `pub struct Order<B: AsRef<[u8]>>` with:

- `pub fn parse(buf: B) -> Result<Self, protoview::DecodeError>`
- one getter per field, named after the field (`snake_case`, keywords escaped as `r#type`):

| Proto field | Getter returns |
| --- | --- |
| `uint64 id` (any numeric or `bool`) | `u64`, the proto3 default (`0`, `false`) when absent |
| `string sku` | `Result<&str, Utf8Error>`, plus `sku_bytes() -> &[u8]` |
| `bytes data` | `&[u8]`, empty when absent |
| `bytes id` + `fixed_bytes(…, N)` | `[u8; N]` |
| `Money price` | `Option<Money<&[u8]>>` |
| `Status status` | `Status`, with `Status::Unknown(i32)` for undeclared values |
| `optional T x` | `Option<T>` (strings: `Option<Result<&str, Utf8Error>>`) |
| `repeated T xs` | `impl Iterator<Item = T>`, in wire order |
| `map<K, V> m` | `impl Iterator<Item = (K, V)>`, in wire order |
| `oneof delivery { … }` | `Option<order::Delivery<'_>>`, an enum with one variant per member |

Proto enums become Rust enums deriving `Debug, Clone, Copy, PartialEq, Eq, Hash`, with
`from_i32`, `to_i32`, `as_str_name`, and a `Default` of the variant for `0`. Variant names follow `prost`, including its
prefix stripping: `STATUS_PENDING` in `Status` becomes `Status::Pending`. Every enum has an
extra variant (`Unknown(i32)`, or `Unrecognized(i32)` if the schema declares an `UNKNOWN`
value) so a value added to the schema later is not mistaken for a known one.

Nested messages and enums, oneof enums, and the entry types protoc synthesizes for maps
live in a module named after their message (`order::Delivery`), as with `prost`.

## What `parse` checks

`parse` walks every byte once and rejects anything that would make a getter fail later:

- every field's bounds, and the wire type of every field the schema knows;
- every nested message, repeated element, map entry and oneof member, recursively, up to
  `protoview::MAX_DEPTH` (100) levels;
- that packed repeated fields split into whole values;
- the length of every `fixed_bytes` field, including a plain field being absent (proto3
  encodes an empty `bytes` by omitting it, and zero bytes is not `N` bytes; declare the
  field `optional` if absence is legitimate).

Unknown fields are skipped, so older code reads newer messages. Field semantics follow the
protobuf spec: the last occurrence of a singular field wins, including across the members
of a oneof; repeated elements may be interleaved with other fields; packed and unpacked
encodings may be mixed.

The one thing `parse` does not check is **UTF-8**: string getters check it on each call
and return a `Result`. This keeps `parse` from reading every string byte when most
strings are never looked at. `*_bytes()` getters give unchecked access.

Failures are reported as [`protoview::DecodeError`](crates/protoview/src/error.rs):
`UnexpectedEof`, `VarintOverflow`, `LengthOverflow`, `InvalidWireType`,
`InvalidFieldNumber`, `UnexpectedWireType`, `MalformedPackedField`,
`RecursionLimitExceeded`, `FixedBytesLenMismatch`, `MessageTooLarge`.

## Configuration

`protoview_build::Config` is a builder:

| Method | Purpose |
| --- | --- |
| `include(dir)` | Adds a directory to search for imports. Earlier directories win. |
| `fixed_bytes(path, len)` | Exposes the `bytes` field at the fully-qualified `path` (`.pkg.Message.field`) as `[u8; len]`. Works on plain, `optional`, `repeated` and oneof-member fields. |
| `out_dir(dir)` | Writes the generated files somewhere other than `$OUT_DIR`, e.g. to check them in. |
| `compile(files)` | Generates code for `files` and everything they import. |

Configuration mistakes are build errors, not silent no-ops: a `fixed_bytes` path that
matches no field, names a non-`bytes` field or a map, or has a zero length fails with
`Error::InvalidFixedBytes`.

## Cost model

- **`parse`**: one pass over the message. Opaque payloads (`bytes`, `string`) are skipped
  by their length prefix; nested messages and packed varints are walked to validate them.
  No heap allocation.
- **The index**: a `[u32; N]` stored inline in the view, with `N` fixed per message type
  by the schema (one slot per singular field, two per repeated, map or oneof). A repeated
  field with 10,000 elements takes the same two slots as one with a single element.
- **Getters**: scalars are a direct read at a known offset. Opening a nested message
  indexes that message's own fields (a second, non-validating walk of its bytes), so a
  getter returning a view is cheaper than `parse` but not free.

## Limitations

- **Decode only.** There is no encoder; build messages with `prost` or another library.
- **proto3.** Groups are rejected; proto2 extensions and default values are not handled.
- **Views borrow their parent.** A nested view borrows the view it came from, not the
  underlying buffer, so walking down a tree in a loop that drops each parent does not
  borrow-check; recurse instead.
- View structs and oneof enums derive no traits (`Debug`, `Clone`).
- `fixed_bytes` does not apply to map values.

## Development

| Path | Contents |
| --- | --- |
| `crates/protoview-tests` | Generates code from the fixtures in `crates/protoview-tests/proto/` and tests it, mostly against `prost` as a reference encoder. |
| [`crates/yellowstone-grpc-protoview`](crates/yellowstone-grpc-protoview) | Ready-made views for the Yellowstone gRPC schema plus a raw-bytes tonic codec, for reuse. |
| `crates/geyser-index-bench` | A CLI that benchmarks view indexing against `prost` on a live Yellowstone gRPC stream. |
| `docs/design.md` | Design decisions and the reasons behind them. |
| `AGENTS.md` | Conventions and gotchas for contributors, human or not. |

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

Copyright 2026 Louis-Vincent.
