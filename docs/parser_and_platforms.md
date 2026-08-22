# Parser and platforms

Flark v4 has one grammar and source authority: the Rust workspace bundled in
`flark_core`. Dart and Flutter communicate with it through the fixed v4 C ABI.
There is no Dart fallback parser and no active Web backend.

## Active product targets

The v4 product is built for:

| OS | Architectures |
| --- | --- |
| macOS | arm64, x64 |
| Android | arm, arm64, x64 |
| iOS device | arm64 |
| iOS simulator | arm64, x64 |

Linux and Web are intentionally not active v4 product targets. A native hook
or archive probe compiling on another host does not establish product support.

## Build prerequisites

Install a stable Rust toolchain and make either `rustup` or `cargo` available
on `PATH`. The native-assets hook builds with the committed `Cargo.lock` into
its isolated output directory and bundles the resulting dynamic library.
Consumers do not copy artifacts or configure a library path.

When `rustup` is available, the hook installs a missing cross-compilation
target on first use. That cold operation needs network access. Subsequent
builds use the installed target and can remain offline.

## Verification

```sh
bash scripts/verify_platform_smoke.sh --platform macos
bash scripts/verify_platform_smoke.sh --platform ios
bash scripts/verify_platform_smoke.sh --platform android
```

These are build receipts. Add `--device <id>` to the Android command for the
Android integration smoke. iOS build success is not an iOS device receipt.

`FLARK_V4_LIBRARY_PATH` is reserved for repository tests, profiles, and custom
embedders that intentionally supply a library. `FLARK_V4_CARGO_FEATURES` is an
experimental hook override; `opening-session` enables the streamed-open ABI.
