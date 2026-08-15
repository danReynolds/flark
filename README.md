# Flark

Flark is a high-performance, continuously rendered Markdown editor for
Flutter. The repository has two product packages:

- [`flark_core`](packages/flark_core): headless Dart API over the Rust-owned
  source, incremental GFM parser, transactions, anchors, certification, and
  history.
- [`flark`](packages/flark): the Flutter custom editor and read-only rendering
  surfaces.

`flark_core` builds and bundles its Rust native asset automatically. Consumers
do not install a separate library or configure a runtime path.

## Development gates

```sh
./scripts/verify_v4.sh
./scripts/v4_android.sh verify <device-id>
./scripts/v4_android.sh profile <device-id>
```

The repository root is a non-publishable qualification workspace. The old
root implementation and `packages/flark_flutter` remain only as inert v2/v3
historical evidence and are excluded from active gates.
