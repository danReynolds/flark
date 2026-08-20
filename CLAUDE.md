# Flark repository guide

Flark is a native Flutter Markdown editor and read-only renderer backed by a
Rust-owned source and incremental GFM engine. The repository root is a
non-publishable qualification harness. Active product code lives in two
packages:

- `packages/flark_core`: headless Dart API, native build hook, ABI, runtime,
  parser, and engine; it must not import Flutter.
- `packages/flark`: Flutter editor and read-only surface, with the macOS/Android
  dogfood app at `packages/flark/example`.

Superseded v2/v3 code lives under `legacy/` and is historical evidence, not an
active dependency. The current v4 product is native-only; do not claim a Web or
Pages surface.

## Commands

- Active gate: `bash scripts/verify_v4.sh`.
- Opening-session feature lane:
  `FLARK_V4_FEATURES=opening-session bash scripts/verify_v4.sh`.
- Release reconciliation: `bash scripts/verify_v4_release.sh`.
- Archive consumers: `bash scripts/verify_v4_publish_archives.sh`.
- Package-local iteration: `cd packages/flark_core && dart analyze && dart test`
  or `cd packages/flark && flutter analyze && flutter test --concurrency=1`.
  Native-backed tests skip without an explicit `FLARK_V4_LIBRARY_PATH`; use the
  active gate above for evidence that they executed.
- Dogfood app: `cd packages/flark/example && flutter run -d macos`.
- Physical Android: `bash scripts/v4_android.sh verify <device-id>`.

## Architecture notes

- Rust owns canonical source, revision order, Markdown semantics, and
  certification. Dart and Flutter retain only bounded views and caches.
- Supported imports are `package:flark_core/flark_core.dart` for headless users
  and `package:flark/flark.dart` for Flutter users. Production consumers do not
  deep-import `lib/src`.
- `packages/flark_core/hook/build.dart` compiles and bundles `flark-abi` from
  `packages/flark_core/native/comrak_bridge` for supported native targets.
- `FlarkEditorController`, `FlarkEditor`, and `FlarkMarkdownView` are the active
  Flutter product surface.
- Feature-gated opening sessions require the `opening-session` Cargo feature;
  default builds must remain byte-for-byte on the default feature set.

## Conventions & quality bar

- Keep source, UTF-8 byte, and UTF-16 coordinate spaces explicit at every
  boundary; host `String` input must be well-formed before mutation.
- Parser changes need the pinned CommonMark/GFM fixture ledgers plus direct Rust
  differentials. Flutter tests do not substitute for grammar evidence.
- Performance claims require checked-in provenance and the declared detector
  tiers. Local green tests, profile traces, physical-device receipts, CI, merge,
  and publication are separate proof levels.

## Pointers

- `docs/architecture/rfc/` contains controlling architecture decisions.
- `docs/architecture/v4/build_plan.md` is the execution/evidence contract.
- `benchmark/v4/` contains checked-in qualification artifacts; narrative prose
  alone is not a receipt.
