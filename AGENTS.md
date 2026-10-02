# AGENTS.md

Guidance for coding agents (and people) working in this repository. The full design
rationale lives in [docs/design.md](docs/design.md); read it before changing what the
generated code looks like.

## What this is

A build-time protobuf code generator that emits **zero-copy lens types** instead of
owned structs. A lens is a view over the encoded bytes: `parse` validates the whole
message tree once and records field offsets in a fixed-size index; getters then read
values straight out of the buffer. Naming and module layout match `prost`, so lenses can
sit next to prost-generated types for the same schema.

## Layout

| Path | What it is |
| --- | --- |
| `crates/proto-codec` | Runtime used by generated code. `no_std`, no dependencies. Varint/wire readers, the structural `Scanner`, repeated-field and map-entry iteration, `DecodeError`. |
| `crates/proto-codec-gen` | The generator. `protox` parses `.proto` files → `model.rs` resolves them → `codegen.rs` renders one Rust file per proto package. `naming.rs` holds the prost-compatible naming rules. |
| `crates/codec-tests` | Unpublished. Generates code from the fixtures and tests it end to end. The only way to catch generated code that does not compile. |
| `crates/geyser-index-bench` | CLI that subscribes to Yellowstone gRPC and times lens indexing of live `SubscribeUpdate`s against `prost`. Reads `GRPC_ENDPOINT` and `X_TOKEN` from the environment or `.env` (see `.env.example`). |
| `proto/yellowstone/` | **Verbatim** copies of `yellowstone-grpc-proto` 13.0.0's protos, shared by `codec-tests` and the bench. Do not edit them: tests compare against that crate's prost types. |
| `crates/codec-tests/proto/` | Hand-written fixtures (`nested`, `repeated`, `all_types`, `oneof`, `enums`, `maps`) plus `fumarole.proto`. The `geyser.proto` / `solana-storage.proto` here are an **older, unused** yellowstone version, shadowed by include order (see below). |
| `docs/design.md` | Design decisions and their reasons. Update it when behaviour changes. |

## Commands

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets          # must be warning-free, generated code included
cargo doc --no-deps --workspace --document-private-items   # must be warning-free
cargo run --release -p geyser-index-bench -- --subscribe txn,account --duration 60
```

Everything builds offline from the local cargo cache (`--offline` works). `yellowstone-grpc-proto`
vendors its own `protoc`; nothing else needs one. Debug-build bench timings are meaningless,
use `--release`.

## The lens contract

Changes to codegen or the runtime must preserve these:

- **`parse` validates everything reachable**: bounds of every field, wire types of known
  fields, packed-run contents, every nested message, oneof member and map entry, and nesting
  depth up to `MAX_DEPTH` (100). After a successful `parse`, no getter on that lens or any
  lens reached from it can fail.
- **The one exception is UTF-8**: strings are checked lazily, per getter call, returning
  `Result<&str, Utf8Error>`. This is deliberate (see design.md); do not "fix" it.
- **Indexing is lazy**: `parse` keeps only the root's offsets. A nested lens re-indexes its
  own bytes, unvalidated, via `from_validated` when accessed.
- **The index is a fixed `[u32; N]`**: one slot per singular field, two per repeated/map
  field (span start tag, span end), two per oneof (winning payload offset, member number).
  `0` means absent. No allocation anywhere in generated or runtime code.
- **Wire semantics**: last occurrence wins for singular fields and across oneof members;
  repeated elements may be interleaved with other fields; packed and unpacked numeric
  encodings may be mixed; unknown fields are skipped.
- **Unknown enum values are kept** as `Unknown(i32)` (or `Unrecognized` if the schema declares
  `UNKNOWN`). There is no `Unknown` on oneofs: a member the schema does not know is just an
  unknown field on the wire, so a decoder cannot attribute it.

## Generated code layout

- One file per proto package, named like `prost-build` (`solana.storage.confirmed_block.rs`,
  `_.rs` for no package), with no wrapper module for the package itself. Users `include!`
  each file inside modules mirroring the package path; cross-package references are
  `super::`-relative. See `crates/codec-tests/src/lib.rs` for the pattern.
- Nested types, oneof enums and hidden map-entry lenses go in a module named after their
  message (`subscribe_update::UpdateOneof`), as `prost` does.
- **Names must match prost**: messages, enums and variants are `UpperCamel` (acronym-aware),
  enum variants get prost's prefix stripping, getters are `snake_case` field names with
  keywords escaped. Prost-dictated names that trip clippy get a targeted `#[allow]` in the
  generated code rather than a different name.

## Conventions

- **Errors**: typed `thiserror` enums per module with a local `Result` alias. No `anyhow`
  except, at most, a binary's top level; the bench uses an `AppError` aggregator instead.
  Error variants that add context use the inline `{ path, #[source] source }` shape.
- **Other enums**: a variant carrying several fields gets its own named struct, wrapped in
  a tuple variant, rather than inline `{ a, b }` fields.
- **Doc comments** on every function, public or private: a summary, then `# Arguments`,
  `# Returns`, and `# Errors` / `# Panics` / `# Safety` where they apply. Link every
  mentioned item with intra-doc links, never link a private item from a public doc, and
  confirm with `cargo doc` (above).
- **Hot paths** (anything per message or per field): no `Box<dyn Future>`, no locks, no
  allocation. The bench's receive loop counts as a hot path too.
- **Runtime crate** stays `no_std` and dependency-free. Generated code uses `core::` paths.
- Match the surrounding code's comment density and idiom; generated-code strings in
  `codegen.rs` are written to produce readable, rustfmt-like output.

## Testing

- Add generator features with a **fixture in `crates/codec-tests/proto/`** and tests in
  `crates/codec-tests/src/`. Unit tests for pure helpers (naming, paths, histogram) live
  next to the code.
- **Use `prost` as the reference encoder**: hand-derive `prost::Message` mirrors of the
  fixture (no `protoc` needed), encode with prost, parse with the lens, compare every field.
  The yellowstone tests do the same with the real `yellowstone-grpc-proto` types.
- **Destructure the expected struct exhaustively** in comparisons (no `..`), so a field that
  is not asserted fails to compile.
- **Hand-encode bytes for layouts prost never emits**: interleaved repeated elements, split
  packed runs, duplicate map keys, entries missing key or value, last-wins across oneof
  members, malformed nested messages, wrong wire types.
- After adding a test that passes first time, **mutate the fixture** (e.g. renumber a field)
  and confirm the test fails, then restore it. For `proto/yellowstone/`, restore from the
  cargo registry copy so the files stay verbatim.

## Gotchas

- **Include order matters** in `crates/codec-tests/build.rs`: `../../proto/yellowstone` comes
  before `proto`, so `fumarole.proto`'s `import "geyser.proto"` resolves to 13.0.0, not the
  stale copy in `crates/codec-tests/proto/`.
- **Build scripts reading `../../proto`** must emit `cargo:rerun-if-changed` for it (and for
  everything else they read, since emitting any disables cargo's default tracking).
- `OUT_DIR` can hold stale files from older layouts (e.g. `proto_codec_gen.rs`); they are
  harmless and not included.
- Lens getters return lenses borrowing `&self`, not the underlying buffer, so walking down
  a tree in a loop that drops parents does not borrow-check; recurse instead.
- Lenses and oneof enums derive nothing (`Debug`, `Clone`); proto enums derive the usual set.

## Not implemented yet

- The `fixed_bytes` build option (`[u8; N]` getters for fixed-length `bytes`), described in
  design.md.
- proto2 features: groups are rejected; extensions and proto2 defaults are not handled.
- A generated root file declaring the whole module tree (one `include!` instead of one per
  package).
