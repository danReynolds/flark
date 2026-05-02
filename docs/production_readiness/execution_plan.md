# Sovereign Production Readiness Execution Plan

Status date: 2026-05-02
Current phase: Phase 3 - architecture hardening

## Objective

Turn Sovereign into a standalone, production-quality Flutter markdown editor
and preview package with a clear public API, reliable native packaging, strong
tests, and durable execution records.

## Success Criteria

- The package lives and builds from `/Users/dan/Coding/sovereign`.
- The repo has audit, plan, and execution log documents that stay current.
- The package has a small documented public API and hides implementation
  details behind `lib/src`.
- Native CommonMark/GFM parsing is packaged through a repeatable consumer-safe
  build/install flow on supported platforms.
- The verification story is explicit: fast local confidence gate, full release
  gate, benchmark gate, native packaging gates, and publish dry run.
- Documentation is package-neutral and covers installation, examples, API,
  theming, native setup, platform support, and release process.
- Release metadata is complete before public publishing or external adoption.

## Current State

Completed in Phase 0:

- Created `/Users/dan/Coding/sovereign` as a new git repo.
- Copied tracked package source, tests, scripts, native bridge, fixtures, and
  Sovereign architecture docs from Dune.
- Excluded generated build state (`.dart_tool`, `build`, `coverage`, Rust
  `target`).
- Removed `resolution: workspace` so dependency resolution can work outside the
  Dune workspace.
- Added root `.gitignore` and `analysis_options.yaml`.
- Updated README and scripts from old monorepo paths to root-relative paths.
- Staged native mobile outputs under `native/comrak_bridge/dist` instead of
  writing into a Dune app.
- Resolved the enforced benchmark lane blocker and confirmed the default
  release-readiness gate passes.

## Phase 0: Extraction and Baseline

Goal: make the package operate as a standalone workspace and record the starting
condition.

Tasks:

- [x] Create `/Users/dan/Coding/sovereign`.
- [x] Copy tracked Sovereign package files from Dune.
- [x] Copy relevant Sovereign architecture docs.
- [x] Initialize git repo.
- [x] Remove workspace-only pubspec setting.
- [x] Add gitignore and analysis options.
- [x] Update root-relative script/README paths.
- [x] Create audit, plan, and execution log docs.
- [x] Run `flutter pub get`.
- [x] Run `flutter analyze lib test`.
- [x] Build host native bridge.
- [x] Run fast confidence gate.
- [x] Run native editor CI gate.
- [x] Run full package test suite.
- [x] Record command results in execution log.
- [x] Resolve enforced benchmark lane failures.

## Phase 1: Package Shape and Public API

Goal: make the package look like a real external dependency, not a copied app
module.

Tasks:

- [x] Define stable public API inventory.
- [x] Create initial public API inventory.
- [x] Remove clearly unsupported internals from the top-level public barrel.
- [x] Add a top-level public API smoke test.
- [x] Move implementation files to `lib/src` in focused waves.
- [x] Move undo/edit-diff internals into `lib/src`.
- [x] Move presentation/render helper internals into `lib/src`.
- [x] Move command implementation internals into `lib/src`.
- [x] Move syntax parse scheduler into `lib/src`.
- [x] Move syntax engine factory into `lib/src`.
- [x] Move parser backend/adapter implementations into `lib/src`.
- [x] Move markdown logic/scanner internals into `lib/src`.
- [x] Move core service/rendering/pipeline internals into `lib/src`.
- [x] Move controller/editor private helper files into `lib/src`.
- [x] Keep `lib/sovereign_editor.dart` as the main public barrel.
- [x] Decide whether any secondary public libraries are warranted.
- [x] Rename Dune-specific public names to Sovereign vocabulary.
- [x] Add migration notes for breaking public API cleanup.
- [x] Add package-level API docs for all public classes and methods.
- [x] Add first primary consumer API docs wave.

## Phase 2: Native Packaging Architecture

Goal: make native parsing installable and verifiable by consumers.

Tasks:

- [x] Decide final package model: FFI package with native assets hooks unless
  a plugin API requirement appears.
- [x] Add `hook/build.dart` or equivalent native assets build flow.
- [x] Decide whether Dart FFI bindings should be generated with `ffigen`.
- [x] Define artifact layout for macOS, Linux, iOS, Android, and unsupported
  platforms.
- [x] Add a small example app/harness for Android/iOS packaging verification.
- [x] Make bridge preflight errors package-neutral and actionable.
- [x] Document the consumer integration path.

## Phase 3: Architecture Hardening

Goal: reduce large-file risk and make behavior modules easier to reason about.

Tasks:

- [ ] Finish controller facade extraction from RFC 017.
- [x] Split input intent routing into explicit handlers.
- [ ] Split markdown structure query and transform services; initial query
  facade plus heading, blockquote Enter/arrow exits, list Enter, and list
  backspace, table Enter/formatting, and fenced-code arrow/Enter-exit/EOF
  continuation/auto-indent/closing-line transforms extracted, broader
  transform service still pending.
- [ ] Keep rendering composition pure and widget-facing code thin.
- [ ] Split Rust bridge into smaller parse/ABI/mapping modules.
- [ ] Keep behavior-level tests green after each wave.

## Phase 4: Feature Completeness and UX Polish

Goal: close known markdown support gaps according to the support matrix.

Tasks:

- [ ] Tables: true structured rendering and richer row/column operations.
- [ ] Images/media: stronger preview and interaction model.
- [ ] Reference links: full rendering/cursor behavior.
- [ ] Thematic breaks: parser-backed classification and interaction tests.
- [ ] Indented code: continuation/exit/backspace parity where useful.
- [ ] Escapes/entities: focused rendering and cursor-safety coverage.
- [ ] Raw HTML: final text-only/sanitization policy.

## Phase 5: Documentation, Examples, and Release Gates

Goal: make the package maintainable and releasable.

Tasks:

- [ ] Add `LICENSE` after owner decision.
- [ ] Add `CHANGELOG.md`.
- [x] Add `example/` with an editable and read-only demo.
- [ ] Add screenshots if publishing externally.
- [ ] Add repository/issue tracker/documentation metadata.
- [ ] Add `scripts/verify_release.sh`.
- [x] Add initial `scripts/verify_release.sh`.
- [ ] Add docs generation to release gate.
- [ ] Add `dart pub publish --dry-run` to the release checklist while keeping
  `publish_to: none` until release intent is explicit.

## Verification Gates

Fast local gate:

```bash
./scripts/verify_package_confidence.sh --skip-native
```

Native host gate:

```bash
./scripts/build_comrak_all.sh --host-only
flutter test test/widgets/sovereign/engine/native_comrak_parse_backend_test.dart
```

Benchmark gate:

```bash
./scripts/verify_benchmark_lane.sh
```

Current status: passing as of 2026-05-01. See
`docs/production_readiness/execution_log.md`.

Future release gate:

```bash
./scripts/verify_release.sh
```

Current status: passing as of 2026-05-01. This gate currently covers pub get,
package analysis, example analysis/tests, host native bridge build, native
editor CI, full package tests, and the enforced benchmark lane.

Example mobile packaging gate:

```bash
./scripts/verify_example_packaging.sh --android
./scripts/verify_example_packaging.sh --ios
```

Current status: passing as of 2026-05-01. The Android check builds the example
debug APK and verifies that `libsovereign_comrak_bridge.so` is packaged. The
iOS check verifies the XCFramework/link-anchor project wiring and parses the
workspace with `xcodebuild -list`.

## Research Notes

The plan follows current Dart and Flutter package guidance:

- Dart package layout expects package roots to keep `lib`, `test`, `example`,
  `tool`, `README.md`, `CHANGELOG.md`, and `LICENSE` in conventional places,
  and not commit generated `.dart_tool`, API docs, or package lockfiles for
  reusable packages.
- Flutter's FFI package guidance points to a native package layout with Dart
  code under `lib`, native source, and a build hook.
- Pub publishing guidance recommends using `dart pub publish --dry-run` to
  inspect package contents and package-layout warnings before publishing.

References:

- https://dart.dev/tools/pub/package-layout
- https://docs.flutter.dev/packages-and-plugins/developing-packages
- https://dart.dev/tools/pub/pubspec
- https://dart.dev/tools/pub/publishing
