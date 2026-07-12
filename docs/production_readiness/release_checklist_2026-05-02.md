# Flark Release Checklist

Status date: 2026-06-08

This checklist records the release decision boundary for the first public
`flark` package release.

## Required Before Publishing

- [x] Add `LICENSE`: MIT License, following local Dan-owned package precedent.
- [x] Add canonical pub metadata:
  - `repository`: <https://github.com/danReynolds/flark>
  - `issue_tracker`: <https://github.com/danReynolds/flark/issues>
  - `documentation`: <https://danreynolds.github.io/flark/>
  - `homepage`: <https://danreynolds.github.io/flark/>
- [x] Add a pub.dev screenshot: `screenshots/flark_surfaces.png`.
- [x] Remove `publish_to: none` for public publishing intent.
- [x] Add CI coverage for the full release gate: `.github/workflows/ci.yml`.
- [x] Add per-platform launch smokes (macOS, iOS, Android, Linux) to
  `.github/workflows/ci.yml`, driven by `scripts/verify_platform_smoke.sh` and
  the `example/integration_test/flark_smoke_test.dart` suite. Each job compiles
  the native bridge for its platform through `hook/build.dart` (proving the
  cross-compile path) and, on desktop hosts, runs the on-device smoke.

## Release Dry Run

Run:

```bash
dart pub publish --dry-run
```

Latest result on 2026-06-08: validation passed with only the expected
dirty-worktree warning while release-prep files were uncommitted. The archive
shape was corrected to use `doc/`, exclude internal `docs/` and peer benchmark
fixtures, include the release screenshot, and keep the compressed archive at
about 1 MB. Rerun from a clean commit before publishing.

## Current Release Gate

Run:

```bash
./scripts/verify_release.sh
```

The gate covers dependency resolution, package and example analysis, docs
generation dry run, example tests, promoted-widget web smoke coverage, host
native build, native editor CI, full package tests, the curated v2 visual
golden suite, and benchmark budgets.

Latest result on 2026-06-08: passed end to end after accepting the
single-heading-scale live-rendered golden baseline and resolving dartdoc
canonicalization warnings. The gate covered package/example dependency
resolution, package and example analysis, `dart doc --dry-run` with 0 warnings
and 0 errors, example tests, web smoke coverage, host native Comrak build,
native editor CI, full package tests including visual goldens, and enforced
benchmark budgets. The web smoke includes the packaged Comrak WASM backend.

## Optional Device Dogfood Gate

Two suites exercise the example app end to end:

- `example/test/markdown_flow_test.dart` — the exhaustive widget-level editing
  matrix (host `flutter test`, and on Chrome via
  `scripts/verify_web_adapter_ci.sh`).
- `example/integration_test/flark_smoke_test.dart` — the lean cross-platform
  launch smoke that proves the shipped parser loads and the
  edit→parse→project→render pipeline works on a given device/desktop.

Run the launch smoke on an iOS simulator when example behavior changes:

```bash
cd example
flutter test integration_test/flark_smoke_test.dart -d <ios-simulator-id>
# or: ./scripts/verify_platform_smoke.sh --platform ios --device <ios-simulator-id>
```

The current simulator validation was run against
`C880BCBA-DC57-4AB9-87DA-50A44357BC40`. It exercises the v2 example app through
common Markdown cases, source/live-rendered mode switching, forms, and native
parser adoption.

Run it on macOS when desktop behavior or native loading changes:

```bash
cd example
flutter test integration_test/flark_smoke_test.dart -d macos
# or: ./scripts/verify_platform_smoke.sh --platform macos
```

The macOS dogfood app can be launched manually with:

```bash
cd example
flutter run -d macos
```
