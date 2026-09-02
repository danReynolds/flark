# Flark Core native runtime

This workspace is the Rust implementation bundled by `flark`'s Dart
build hook. `flark-abi` is the only exported native asset; it wraps the v4
document runtime, incremental parser, anchors, history, and edit-intent API.

Consumers do not build or stage this workspace themselves. Dart and Flutter
builds invoke `packages/flark/hook/build.dart`, which compiles the target
artifact and registers it as `package:flark/src/native/bindings.dart`.

The workspace contains four product crates with one-way ownership boundaries:

- `flark-engine`: persistent source, recursive syntax, live reference state,
  and bounded parser work primitives; it does not own Markdown recognition or
  candidate/snapshot transport
- `flark-parser`: exact incremental GFM recognition and atomic, source-stamped
  inline captures over the engine primitives
- `flark-runtime`: serialized document, edit-intent, and rendered-view authority
- `flark-abi`: bounded C ABI exposed to Dart

The live parser-to-engine seam is intentionally private and narrow. It exposes
bounded source-range cursors, scratch admission, recursive-Green storage, reference
journals, and inline-capture validation—only the services exercised by the
editor runtime.

Repository verification is driven by `scripts/verify_v4.sh`. Historical v2/v3
bridge sources are intentionally outside this package under `legacy/`.
