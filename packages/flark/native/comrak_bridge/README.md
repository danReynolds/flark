# Flark Core native runtime

This workspace is the Rust implementation bundled by `flark`'s Dart
build hook. `flark-abi` is the only exported native asset; it wraps the v4
document runtime, incremental parser, anchors, history, and edit-intent API.

Consumers do not build or stage this workspace themselves. Dart and Flutter
builds invoke `packages/flark/hook/build.dart`, which compiles the target
artifact and registers it as `package:flark/src/native/bindings.dart`.

The workspace contains four product crates:

- `flark-parser`: incremental GFM parsing and source projection
- `flark-engine`: persistent document and projection machinery
- `flark-runtime`: serialized document and edit-intent authority
- `flark-abi`: bounded C ABI exposed to Dart

Repository verification is driven by `scripts/verify_v4.sh`. Historical v2/v3
bridge sources are intentionally outside this package under `legacy/`.
