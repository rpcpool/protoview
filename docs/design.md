# protoview — design

Status: agreed design, implementation in progress.

A build-time protobuf code generator producing **zero-copy view types**: decoders that
are views over a byte buffer rather than owned structs. Alternative to `prost` for
read-heavy paths where full materialization is wasted work.

Motivating workload: Solana/Yellowstone streaming (`proto/geyser.proto`,
`proto/fumarole.proto`), where a consumer routes on `slot` and drops most of a
multi-megabyte `SubscribeUpdate`, but `prost` allocates and copies every field first.

## Scope

Decode only. **No encoder** in v1 — encoding is a separate design with its own core
problem (nested messages are length-delimited, so submessage lengths must be known
before their header is written, which needs random write access and therefore rules out
a generic `BufMut` sink). Deferred deliberately, not forgotten.

Service definitions are parsed and ignored; only messages and enums generate code.

Corpus-driven: `proto/` is the acceptance criterion. Constructs outside it hard-error
rather than silently miscompiling.

## Crates

| crate | role | dependencies |
| --- | --- | --- |
| `protoview-build` | build-dependency; descriptors → Rust source | `protox`, `prost-types` |
| `protoview` | runtime linked by generated code | **none**; `no_std` + `alloc` |
| `protoview-tests` | `publish = false`; build.rs generates `proto/`, `cargo test` compiles it | — |

The runtime having **zero dependencies** is a deliberate, defended position and a real
differentiator against `prost` (which pulls `bytes`). It is possible only because the
container bound is `AsRef<[u8]>` rather than anything `bytes`-shaped. Guard it in CI.

`protoview-tests` exists because generated code that does not compile is the most likely
failure mode, and the only way to catch it is to generate and compile the full corpus.

## Pipeline

```
.proto ──protox──> FileDescriptorSet ──codegen──> one .rs per package (OUT_DIR)
                   (+ SourceCodeInfo)             + one root file declaring the tree
```

`protox` is a pure-Rust proto compiler, so **no `protoc` binary is required at build
time** — clean machines, CI, minimal containers and cross-compilation all work. This is
a felt improvement over `prost-build` and costs nothing given the corpus-driven scope.
A pre-built `FileDescriptorSet` is accepted as an alternate entry point for anyone
needing `protoc`'s exact conformance.

`SourceCodeInfo` is requested from day one so errors carry `file:line:col`. Retrofitting
spans means threading position through every codegen layer afterwards; carrying them
from the start is nearly free. build.rs errors are read in a wall of cargo output, where
a span is the difference between a fixable error and a bug report.

## Generated code

### Container

```rust
pub struct SubscribeUpdate<B: AsRef<[u8]>> {
    buf: B,
    index: [u32; N],   // N = declared field count; index[i] = payload offset, 0 = absent
}
```

`AsRef<[u8]>` covers `Vec<u8>`, `Bytes`, `&[u8]`, `[u8; N]`. Nested getters cannot
manufacture a sub-`B`, so they borrow from the parent:

```rust
fn account(&self) -> Option<SubscribeUpdateAccount<&'_ [u8]>>
```

**Known trade:** nested views cannot detach from the parent buffer, so fanning a
block's transactions out to worker threads needs the root kept alive (e.g. behind an
`Arc`) or a copy. Recoverable later without breakage by adding a second impl block
gated on a sliceable trait — new methods, no change to the struct bound:

```rust
impl<B: AsRef<[u8]>> SubscribeUpdate<B> { fn account(&self) -> Option<Account<&'_ [u8]>> }
impl<B: ByteView>    SubscribeUpdate<B> { fn account_owned(&self) -> Option<Account<B>> }
```

Rejected: `bytes::Buf`. It is a sequential cursor — getters would need `&mut self`,
`chunk()` may return a partial slice for `Chain`/`VecDeque<Bytes>`, and there is no
rewind. All three are fatal for a type that re-reads from offset 0 on every access.

### Construction validates; getters do not fail

`Msg::parse(buf) -> Result<Msg<B>, DecodeError>` performs one structural walk of the
whole message tree — read tag, check wire type, check bounds, descend into nested
messages — and builds the root's index. Getters then return plain `Option<T>`/`T`, at
every depth. The walk reads tags, lengths and packed varints, never decodes values, so
it is a fraction of full decode, and it removes `Result` from every downstream call site
(except UTF-8 checks on strings).

**Known trade:** there is no free-until-touched path. `parse` pays for a structural walk
of every byte up front, and accessing a nested view pays for a second, non-validating
walk of that view's own fields to index it. Iterating 5000 transactions means 5000
extra walks — still zero allocation and zero byte copying, so well ahead of `prost`, but
not free.

A seek mode (no index, rescan per getter) was considered and dropped. Since a validating
construct already computes the offsets, retaining them is nearly free, and the two modes
would have collapsed into each other or diverged into incompatible APIs. Adding it later
is additive.

### Index layout

`[u32; N]` indexed by a **slot assigned per declared field** (one for singular fields,
two for repeated), not by field number. Codegen knows each field's slot statically, so a
getter reads `self.index[3]` directly — no lookup, no
tag stored. `0` is the absent sentinel, valid because a payload offset is always
preceded by at least one tag byte.

Populating it during the walk:

- **Singular fields: last occurrence wins** (proto3 semantics — overwrite the slot).
- **Repeated/map fields: record a span, two slots wide** — the tag offset of the first
  occurrence and the offset just past the last. Iteration re-walks only that span,
  yielding matching records in wire order and skipping anything interleaved (other
  fields, unknown fields): repeated fields may legally appear non-contiguously on the
  wire. Absence is keyed off the end slot, since a start offset of `0` is legal.
  Numeric fields merge packed runs and unpacked values, in any mix, into one stream.
- **Oneofs: two slots** — the payload offset of the member seen last and that member's
  field number. Every member writes both, so last-wins holds across members, not just
  within one. Absence is keyed off the offset slot.
- Unknown fields (not in the schema) are skipped and not indexed.

Validated recursively, indexed lazily. `parse` walks the whole message tree once —
every nested message and repeated element, wire types of known fields, packed-run
contents, and nesting depth up to `MAX_DEPTH` (100, as `prost`) — so nothing reachable
from a parsed view can fail. It does not keep the children's offsets: each nested view
re-indexes its own bytes, unvalidated, when accessed. Retaining a recursive index would
allocate and would pay for all 5000 transactions of a block even when only one is read.

No allocation: `N` is known at codegen time, so the table is inline.

### Field mapping

| proto | generated |
| --- | --- |
| implicit-presence scalar | `fn slot(&self) -> u64` — proto default substituted when absent |
| `optional` scalar | `fn parent(&self) -> Option<u64>` |
| singular message | `fn account(&self) -> Option<AccountInfo<&'_ [u8]>>` — no default exists |
| `string` | `fn name(&self) -> Result<&str, Utf8Error>` and `fn name_bytes(&self) -> &[u8]` |
| `bytes` | `fn data(&self) -> &[u8]` |
| `bytes` + `fixed_bytes` config | `fn pubkey(&self) -> [u8; 32]` — by value; length checked at parse. `Option<[u8; N]>` when `optional`, `Iterator<Item = [u8; N]>` when `repeated`, a `[u8; N]` variant in a oneof |
| `repeated T` | `fn items(&self) -> impl Iterator<Item = T>`; strings yield `Result<&str, Utf8Error>` (plus `items_bytes()`), as the singular getter does |
| `map<K, V>` | `fn m(&self) -> impl Iterator<Item = (K, V)>`, in wire order; a missing key or value reads as its default |
| `oneof update_oneof` in `SubscribeUpdate` | `fn update_oneof(&self) -> Option<subscribe_update::UpdateOneof<'_>>`; the enum has one variant per member, named and placed as `prost` does, and a lifetime only if some member borrows |
| proto3 `enum` | `enum` with an `Unknown(i32)` variant and `to_i32()` |

Rationale for the less obvious ones:

- **`Unknown` on enums.** Live in the corpus — `SlotStatus` grew from 3 variants to 7.
  Folding an unrecognized value into a default makes a server rollout emitting a new
  variant indistinguishable from an old one. Cost: an extra match arm, and enums are not
  `#[repr(i32)]` so `to_i32()` replaces an `as` cast.
- **No `Unknown` on oneofs.** An earlier draft promised one, motivated by `SubscribeUpdate`
  gaining `pong = 9` and `transaction_status = 10`. It cannot be built: a member the
  schema does not know is just an unknown field number on the wire, with nothing tying it
  to the oneof, so the decoder cannot tell it from any other unknown field. Such an
  update reads as `None` — as with `prost`. An enum is different: its unknown value
  arrives inside a field the schema does know.
- **UTF-8 is checked lazily, per getter call.** `parse` skips string payloads like
  `bytes` (length, bounds, jump), so strings are the one getter that can fail. Deliberate:
  eager validation would make `parse` read every string byte — `log_messages` dominates
  Solana payloads and is rarely read — and an infallible `&str` getter would need
  `from_utf8_unchecked` resting on `B::as_ref()` returning the same bytes each call.
  Unlike `prost`, a message with invalid UTF-8 in a string field still parses; the error
  surfaces only if that field is read. `_bytes()` twins give unchecked access.
- **No `get(&key)` on maps.** A map is `repeated` message on the wire; lookup is a
  linear scan however it is named. `.find(...)` puts the cost where the reader sees it.
  No `collect_map()` either, though an earlier draft had one: `.collect::<HashMap<_, _>>()`
  already gives protobuf's last-wins semantics on duplicate keys, and a generated helper
  would pull `std` into otherwise `core`-only code. String keys yield
  `Result<&str, Utf8Error>` like every string, so collecting them goes through `?`.
- **No `_opt` presence twins.** Would roughly double the generated surface across ~300
  corpus fields to serve a case that `optional` in the schema already covers.
- **Iterator-only `repeated`.** Random access would hide an O(n) scan; the access
  pattern is a pass anyway.

### Naming and modules

Identical to `prost`, stutter included:

| proto | Rust |
| --- | --- |
| `message SubscribeUpdateAccountInfo` | `SubscribeUpdateAccountInfo` |
| `enum BROKER_STATE` | `BrokerState` |
| `OFFLINE` | `BrokerState::Offline` |
| `SLOT_PROCESSED` in `SlotStatus` | `SlotStatus::SlotProcessed` |
| field `block_time` | `fn block_time()` |
| field `type` | `fn r#type()` |
| package `solana.storage.ConfirmedBlock` | mod `solana::storage::confirmed_block`, file `solana.storage.confirmed_block.rs` |

`SlotStatus::SlotProcessed` stutters because `prost` strips a variant prefix only when it
matches the enum's full shouty name (`SLOT_STATUS`). Kept anyway: "identical to prost" is
a rule users can verify, and these schemas will coexist with prost-generated types in the
same binaries.

Cross-package references use `prost`'s relative form (`super::solana::storage::
confirmed_block::Transaction`), which requires the include tree to mirror the package
hierarchy. `import public` affects proto name resolution only; no Rust re-exports are
emitted.

`google.protobuf.Timestamp` is generated as an ordinary message — it is structurally
`{ int64 seconds = 1; int32 nanos = 2; }`. No datetime dependency, no special case; users
write their own `From`.

`extern_path` was considered and rejected: the types it would map to are prost-style
owned structs, shape-incompatible with views, so the interop story does not work.

## build.rs API

```rust
fn main() -> Result<(), protoview_build::Error> {
    protoview_build::Config::new()
        .include("proto")
        .fixed_bytes(".geyser.SubscribeUpdateAccountInfo.pubkey", 32)
        .fixed_bytes(".geyser.SubscribeUpdateAccountInfo.owner", 32)
        .fixed_bytes(".fumarole.BlockchainEvent.block_uid", 16)
        .compile(&["proto/fumarole.proto", "proto/broker-rpc.proto"])
}
```

- Builder style; `prost`-style fully-qualified field paths.
- **Transitive imports are generated automatically.** `geyser.proto` and
  `solana-storage.proto` are not listed — they arrive through `fumarole.proto`'s imports.
  Requiring explicit listing means a schema author adding an import silently breaks a
  downstream build with a "cannot find type" error inside generated code.
- Output to `OUT_DIR` by default, one file per package named as `prost-build` names it
  (module segments joined by `.`; `_.rs` for no package). Files carry no `mod` wrappers;
  the user nests one `include!` per package in modules mirroring the package hierarchy,
  as with `prost`. A generated root file declaring the whole tree is not provided yet.
- A configurable output directory is supported, for checked-in generated code — for a
  library whose value proposition is the *shape* of its output, reading it in a PR diff
  matters.

### `fixed_bytes` and infallible getters

`fixed_bytes` is length-checked during the validating walk, which already reads every
field's length. A mismatch is rejected at `parse`, so `fn pubkey(&self) -> [u8; 32]`
stays infallible. Never pad or truncate: a silently zero-padded pubkey is a wrong-account
bug that surfaces days later in someone else's system.

That rule extends to absence. proto3 encodes an empty `bytes` value by omitting the field,
so an absent plain field *is* a zero-length value, and `parse` rejects it with
`FixedBytesLenMismatch { actual: 0 }` rather than returning `[0; N]`. A field that may
legitimately be unset should be `optional` in the schema, which gives `Option<[u8; N]>`.

Configuration mistakes are build errors, not no-ops: a path that matches no field, a
field that is not `bytes`, a map field or a map entry's key or value, and a zero length
all fail `compile` with `Error::InvalidFixedBytes` naming the path. Map values are left
out for now: an entry missing its value reads as the default, which for a fixed length
would again mean padding.

A general `map_type(path, "my::Type")` via `TryFrom<&[u8]>` was dropped for exactly this
reason — an arbitrary conversion cannot be validated at parse without running it twice or
storing the result (at which point it no longer just reads the bytes in place). `[u8; N]` covers the
motivating cases (pubkeys, signatures, blockhashes, block UIDs) and
`Pubkey::from(msg.pubkey())` is free. Addable later as `Result`-returning getters.

## Unsupported constructs

**Accepted silently:** services (ignored), `reserved`, custom options, recursive message
types (views are constructed lazily, so no infinite type), `google.protobuf.Any`
(structurally an ordinary message; no dynamic decoding, but nothing breaks).

**Supported:** nested type declarations (`message Foo { message Bar {} }`), placed in a
`foo` module as `prost` does; protoc's synthesized map entries are generated there too,
`#[doc(hidden)]`, to validate entries. Absent from
the corpus but common in the wild; erroring would make the library feel broken to the
first person pointing it at their own schema, and the cost is only module nesting.

**Hard error, with a span:** proto2 syntax, groups, extensions, anything unrecognized.

## Build order

1. `protoview` runtime: varint reader, wire types, structural walker, error types.
2. Codegen for scalars and nested messages on one hand-picked message — end to end
   before breadth.
3. `repeated`, `map`, `oneof`, enums.
4. `fixed_bytes` config and field-path matching.
5. Point `protoview-tests` at the full corpus. That is the acceptance gate.
