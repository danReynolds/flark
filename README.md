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
bash scripts/verify_v4.sh
FLARK_V4_FEATURES=opening-session bash scripts/verify_v4.sh
bash scripts/verify_v4_release.sh
bash scripts/verify_v4_publish_archives.sh
bash scripts/v4_android.sh verify <device-id>
bash scripts/v4_android.sh profile <device-id>
```

The repository root is a non-publishable qualification workspace. Superseded
v2/v3 sources live only under [`legacy/`](legacy) as historical evidence; active
package resolution, builds, and gates must not depend on them. Flark v4 is
currently native-only, so there is no active Web/Pages deployment.
