> **v5 in progress (2026-09).** The editor is being rebuilt on a synchronous core; see [RFC 030](docs/architecture/rfc/rfc_030_synchronous_core.md) and the [v5 build plan](docs/architecture/v5/build_plan.md). The v4 text below is historical; v4 lives under `legacy/v4/`.

# Flark

Flark is a high-performance, continuously rendered Markdown editor with a
headless Dart kernel and a Flutter adapter. The repository has two product
packages:

- [`flark`](packages/flark): headless Dart API over the Rust-owned
  source, incremental GFM parser, transactions, anchors, certification, and
  history.
- [`flark_flutter`](packages/flark_flutter): the Flutter custom editor and
  read-only rendering surfaces.

`flark` builds and bundles its Rust native asset automatically. Consumers
do not install a separate library or configure a runtime path.

Read the [Flark North Star](NORTH_STAR.md) before changing editor architecture,
projection authority, rendering, or live-edit test methodology.

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
