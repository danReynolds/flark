# sovereign — home of the `flark` package

Flark is a Flutter Markdown editor + read-only previewer with native CommonMark/GFM
parsing via Comrak: Rust FFI on macOS/iOS/Android/Linux, prebundled WASM on web;
Windows unsupported. Single package (root pubspec `flark`, not a workspace) plus
`example/` (own pubspec), the dogfood app and demo site. Consumed by
`../dune_minimal` for markdown rendering.

## Commands

- Setup: `flutter pub get`. Native targets need Rust on PATH (`hook/build.dart`
  compiles the bridge during `flutter build`); web needs no extra tooling.
- Fast gate (analyze + high-signal tests): `./scripts/verify_package_confidence.sh`
  (`--skip-native` if native artifacts aren't built; `--full-suite`, `--benchmarks`).
- Full tests: `flutter test test` · Analyze: `flutter analyze hook lib test`
- Goldens: `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart`;
  add `--update-goldens` to regenerate baselines.
- Release gate: `./scripts/verify_release.sh` · App: `cd example && flutter run -d macos`
- Native bridge artifacts: `./scripts/build_comrak_all.sh --host-only`

## Architecture notes

- The Markdown source string is the document truth — no private rich-text model;
  editor, preview, commands, projection, and render plans all derive from it.
- Code lives under `lib/src/v2/` (no v1 exists); public API only via barrels
  `lib/flark.dart` (apps), `flark_core.dart` (headless), `flark_advanced.dart`.
- Headless core (`src/v2/{core,markdown,projection,render_plan}`) must not import
  Flutter — enforced by `test/v2/core/v2_core_import_boundary_test.dart`.
- Parser boundary: `src/v2/markdown/parse/` defines the backend protocol;
  `src/v2/native/` selects FFI vs WASM vs stub via conditional imports.
- Editor (`FlarkMarkdownEditor`, source or live-rendered) and previewer
  (`FlarkMarkdown`) are separate `src/v2/flutter/` widgets synced by one shared
  `FlarkFlutterController`.

## Conventions & quality bar

- Goldens (`test/v2/flutter/goldens/`) are a first-class gate covering
  paint/spacing/wrapping only — semantics belong in code assertions. A tolerant
  comparator (0.5%, `test/flutter_test_config.dart`) absorbs pixel noise; don't
  loosen it, and only regenerate for intentional visual changes (inspect the PNGs).
- Parser changes need conformance coverage: `test/fixtures/commonmark/` (upstream
  CommonMark/GFM cases + deviation registers) driven by `test/v2/markdown/`
  conformance, native-parity (`flark_v2_native_upstream_contract_test.dart`), and
  fuzz suites.

## Pointers

- `doc/` = shipped package docs; `docs/` = internal notes/RFCs (pubignored).
- DOSSIER.md — Phase 2 (planned; not in the repo yet).
