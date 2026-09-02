# Development and verification

Flark v4 is split into `packages/flark` (Dart plus Rust runtime) and
`packages/flark_flutter` (Flutter). The repository root is a non-publishable
qualification workspace.

## Prerequisites

Use a supported Flutter/Dart SDK and a stable Rust toolchain. A cold
cross-platform build may need network access while `rustup` installs the
target triple. See [Parser and platforms](parser_and_platforms.md).

## Gates

The fast gate builds the native ABI and executes all active Rust, Dart Core,
Flutter, and qualification tests with an explicit native library path:

```sh
bash scripts/verify_v4.sh
```

The feature-gated streamed-open surface has a separate run:

```sh
FLARK_V4_FEATURES=opening-session bash scripts/verify_v4.sh
```

Release and exact-archive gates are canonical and independently runnable:

```sh
bash scripts/verify_v4_release.sh
bash scripts/verify_v4_publish_archives.sh
```

`--skip-stress`, `--skip-archive-runtime`, and `--skip-runtime` produce
iteration evidence only. They are not full release receipts.

## Platform evidence

Build smokes prove that the native-assets cross-compile path works; they do
not prove device interaction or packaged-app launch behavior. The archive
gate separately executes extracted-package Core JIT/AOT and Flutter widget
runtime smokes, but its desktop application consumer remains build-only:

```sh
bash scripts/verify_platform_smoke.sh --platform macos
bash scripts/verify_platform_smoke.sh --platform ios
bash scripts/verify_platform_smoke.sh --platform android
bash scripts/verify_platform_smoke.sh --platform android --device <device-id>
```

The last form adds the Android integration smoke. There is no v4 Web smoke or
Pages deployment.

## Dogfood and profiling

```sh
bash scripts/run_v4_dogfood.sh
bash scripts/profile_v4_macos.sh
bash scripts/profile_v4_sweep.sh /tmp/flark-v4-sweep.jsonl
```

Profile receipts require a foregrounded live display. Keep local checks,
committed-SHA CI, package archive consumption, and physical-device receipts as
separate evidence levels.
