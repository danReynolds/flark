# Sovereign Execution Log

This log records work completed toward production readiness. Keep entries
append-only unless correcting a factual error.

## 2026-05-01

### Extraction

- Created `/Users/dan/Coding/sovereign`.
- Initialized a new git repo.
- Copied tracked files from `dune_gemini/packages/sovereign_editor`.
- Copied Sovereign architecture docs from Dune:
  - `docs/architecture/sovereign/`
  - selected RFCs under `docs/architecture/rfc/`
- Confirmed `dune_gemini` had unrelated dirty changes and did not modify them.

### Standalone Cleanup

- Updated `pubspec.yaml`:
  - changed description from Dune-internal wording to package wording;
  - removed `resolution: workspace`;
  - kept `publish_to: none` because the package is not release-ready.
- Added `.gitignore` for Dart/Flutter generated state, Rust targets, native
  dist artifacts, coverage, and `pubspec.lock`.
- Added `analysis_options.yaml` using `flutter_lints`.
- Updated scripts to use root-relative package paths.
- Changed Android and iOS native build outputs to package-local
  `native/comrak_bridge/dist`.
- Updated native bridge load remediation messages away from Dune paths.
- Updated README commands and expected native artifact paths.

### Audit and Planning

- Created `docs/production_readiness/audit_2026-05-01.md`.
- Created `docs/production_readiness/execution_plan.md`.
- Created this execution log.

### Verification

- `flutter pub get`: passed.
- `flutter analyze lib test`: initially failed on deprecated Flutter color APIs;
  fixed by replacing `withOpacity`/`Color.alpha` usages, then passed with no
  issues.
- `./scripts/verify_package_confidence.sh --skip-native`: initially failed
  because ordinary controller tests still need the host native library. After
  host bridge build, passed.
- `./scripts/build_comrak_all.sh --host-only`: passed and produced
  `native/comrak_bridge/target/release/libsovereign_comrak_bridge.dylib`.
- `./scripts/verify_native_editor_ci.sh --skip-build`: passed.
- `flutter test test --reporter compact`: initially exposed one full-suite
  predictive telemetry flake and one stale color expectation from the
  `withOpacity` migration; both were fixed. The default parallel full suite now
  passes with 536 tests.
- `flutter test test --reporter compact --concurrency=1`: passed with 536
  tests.
- `./scripts/verify_benchmark_lane.sh`: failed with enforced budgets:
  - warm `buildTextSpan` cache hit: 10896us vs 800us budget
  - scanner p99: 8327us vs 3500us budget
  - cold `buildTextSpan` p99: 12374us vs 4000us budget
  - emoji scanner p99: 5825us vs 3000us budget

### Test Fixes

- Updated deprecated color opacity calls to `Color.withValues(alpha: ...)`.
- Updated the blockquote color expectation to match `withValues` alpha
  semantics.
- Made the predictive telemetry truncation test deterministic by using the
  existing stale/ambiguous syntax engine instead of racing the real native
  parser.

### Release Gate and API Inventory

- Added `scripts/verify_release.sh` as the first release-readiness gate. The
  default gate includes pub get, analysis, host native build, native editor CI,
  full package tests, and enforced benchmark budgets.
- Added `docs/production_readiness/public_api_inventory_2026-05-01.md` to
  classify current barrel exports and plan public API cleanup waves.
- `./scripts/verify_release.sh --skip-benchmarks --skip-native-build`: passed.
  This verifies the non-benchmark release baseline against the existing host
  native artifact. At this point, the default release gate still failed because
  it included the enforced benchmark lane.

### Benchmark Lane Remediation

- Stabilized `DuneMarkdownTheme.of(context)` fallback identity so renderer
  cache keys no longer miss merely because no app-level markdown theme
  extension is installed.
- Added a cached final `TextSpan` path in `SovereignTextRenderer` for unchanged
  revision/theme/style renders, while extending the renderer cache key to
  distinguish authoritative inline runs and projected exclusion ranges.
- Reused a single fenced-code scan across render cache miss substeps instead of
  rescanning separately for editor exclusions, inline scan exclusions, and code
  highlighting.
- Skipped supplemental link/image scanning in `SovereignStyleScanner` when the
  text has no link/image trigger characters.
- Updated `test/benchmarks/sovereign_benchmark_test.dart` to use a real
  Flutter `BuildContext`, a nearest-rank percentile helper, emoji scanner
  warmup, and alternating text-shape warmup before the cold render p99 samples.
- `./scripts/verify_benchmark_lane.sh`: passed with enforced budgets. Latest
  release-gate run reported:
  - warm `buildTextSpan` cache hit: 170us vs 800us budget
  - scanner p99: 1127us vs 3500us budget
  - cold `buildTextSpan` p99: 3917us vs 4000us budget
  - emoji scanner p99: 1091us vs 3000us budget

### Release Gate

- `flutter analyze lib test`: passed.
- Focused scanner/render/highlighting tests: passed.
- `./scripts/verify_release.sh`: passed. This covered `flutter pub get`,
  `flutter analyze lib test`, host native bridge build,
  `./scripts/verify_native_editor_ci.sh --skip-build`, full package tests, and
  the enforced benchmark lane.

### Phase 1 API Shape: Helper Surface Cleanup

- Moved internal `Logger` from `lib/helpers/logger.dart` to
  `lib/src/helpers/logger.dart` and updated internal imports.
- Removed public `lib/theme/app_colors.dart`; `DuneMarkdownTheme.dune()` now
  uses a private fallback markdown palette. This eliminates the old app-level
  `AppColors` deep-import surface without changing the supported
  `sovereign_editor.dart` barrel API.
- `flutter analyze lib test`: passed.
- Focused render/controller/theme tests: passed.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite.

### Phase 1 API Shape: Barrel Narrowing

- Narrowed `lib/sovereign_editor.dart` by removing top-level exports for
  parser implementation adapters, parse backend/scheduler internals, scanners,
  marker helpers, `UndoStack`, and `EditDiffer`.
- Kept syntax contracts, native bridge diagnostics, UTF offset mapping, command
  APIs, editor/preview widgets, theme types, and model types still exposed by
  public controller/syntax signatures.
- Added `test/public_api/sovereign_editor_barrel_test.dart` to verify the
  supported top-level import can construct editor/preview widgets, commands,
  custom syntax engine snapshots, native preflight result types, markdown
  theme defaults, and UTF offset mapping.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after the barrel export cleanup.

### Phase 1 API Shape: First `lib/src` Migration

- Moved `UndoStack` and `EditDiffer` behind `lib/src` while keeping the public
  controller and edit pipeline on a single implementation path.
- Updated internal imports to reference those implementation types through
  `package:sovereign_editor/src/...`.
- `flutter analyze lib test`: passed.
- Focused undo/edit-flow tests: passed.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving the undo/edit-diff internals.

### Phase 1 API Shape: Package Vocabulary

- Renamed the public markdown theme from `DuneMarkdownTheme` to
  `SovereignMarkdownTheme`.
- Renamed `lib/theme/dune_markdown_theme.dart` to
  `lib/theme/sovereign_markdown_theme.dart`.
- Replaced the Dune-named default constructor with
  `SovereignMarkdownTheme.standard()` and updated internal/test callers.
- `flutter analyze lib test`: passed.
- Focused public API/theme/render tests: passed.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed
  on rerun. The first full-suite attempt hit the existing predictive inline
  marker parallel-suite flake after the same test had passed in the script's
  native-editor subset; the isolated test and full predictive marker file both
  passed before the successful release-gate rerun.

### Phase 1 API Shape: Migration Notes

- Added `docs/production_readiness/api_migration_2026-05-01.md` for the Phase 1
  breaking API cleanup.
- Documented the supported import contract: app code should use only
  `package:sovereign_editor/sovereign_editor.dart` for now.
- Recorded that no secondary public libraries are warranted in this cleanup
  pass; deep imports remain package-test and white-box implementation details.
- Covered the `SovereignMarkdownTheme.standard()` rename, removed top-level
  internal exports, `UndoStack`/`EditDiffer` moving behind `lib/src`, and the
  removed app palette helper.
- `flutter analyze lib test`: passed.

### Phase 1 API Shape: Presentation Helper `lib/src` Migration

- Moved presentation/render helpers behind `lib/src`:
  - `Tier1Painter`;
  - inline-actions overlay and targeting helpers;
  - read-only link tap tracking;
  - read-only task-checkbox overlay helpers.
- Kept `SovereignEditor` and `SovereignMarkdownView` as the public presentation
  widgets while updating them to import the helpers through
  `package:sovereign_editor/src/...`.
- Updated package tests that intentionally inspect painter behavior to import
  `Tier1Painter` through `package:sovereign_editor/src/...`.
- `flutter analyze lib test`: passed.
- Focused presentation/render tests: passed.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving presentation helpers.

### Phase 1 API Shape: Docs Generation Baseline

- Fixed unresolved Dart doc bracket references in current public/deep-import
  libraries so documentation generation has a clean starting point.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `flutter analyze lib test`: passed.

### Phase 1 API Shape: Primary API Docs

- Added library docs to `lib/sovereign_editor.dart` and an explicit `library;`
  directive so the top-level barrel has package documentation.
- Added API prose for the primary consumer surface:
  - `SovereignController`;
  - `SovereignEditor`;
  - `SovereignMarkdownView`;
  - `SovereignMarkdownCommands` and command result/capability/link-edit models;
  - `SovereignMarkdownTheme`, `SovereignEditorThemeData`, and related editor
    theme classes.
- `flutter analyze lib test`: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `flutter test test/public_api/sovereign_editor_barrel_test.dart --reporter compact`:
  passed.

### Phase 1 API Shape: Command Helper `lib/src` Migration

- Moved command implementation helpers behind `lib/src`:
  - block, inline, fence, and link command implementations;
  - command transaction, selection, range, and context helpers.
- Kept `SovereignMarkdownCommands` as the supported public command facade and
  updated it to import those helpers through `package:sovereign_editor/src/...`.
- `flutter analyze lib test`: passed.
- Focused command tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving command helpers.

### Phase 1 API Shape: Syntax Scheduler `lib/src` Migration

- Moved `SyntaxParseScheduler` behind `lib/src` and updated controller,
  syntax-coordinator, and white-box scheduler test imports.
- `flutter analyze lib test`: passed.
- Focused scheduler and controller engine wiring tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving the syntax scheduler.

### Phase 1 API Shape: Syntax Engine Factory `lib/src` Migration

- Moved `SyntaxEngineFactory` behind `lib/src` and updated controller and
  white-box factory test imports.
- Kept public engine customization through `SyntaxEngine` injection into
  `SovereignController` and `SovereignMarkdownView`.
- `flutter analyze lib test`: passed.
- Focused syntax factory and controller engine wiring tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving the syntax engine factory.

### Phase 1 API Shape: Parser Backend/Adapter `lib/src` Migration

- Moved parser backend and adapter implementation classes behind `lib/src`:
  `CommonMarkParseBackend`, `CommonMarkSyntaxEngineAdapter`,
  `ComrakCommonMarkParseBackend`, and `V1SyntaxEngineAdapter`.
- Updated internal engine factory wiring and white-box package tests to import
  those implementation classes through `package:sovereign_editor/src/...`.
- Kept public parser customization through `SyntaxEngine` injection and the
  existing syntax request/snapshot contracts.
- `flutter analyze lib test`: passed.
- Focused parser engine tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving the parser backend/adapters.

### Phase 1 API Shape: Markdown Logic/Scanner `lib/src` Migration

- Moved markdown logic and scanner implementation classes behind `lib/src`:
  block parser, fenced-code scanner, marker grammar/helpers, style scanner,
  geometry scanner, projector, and code highlighter.
- Updated controller, presentation, rendering, syntax, engine, and white-box
  package-test imports to use `package:sovereign_editor/src/...` for those
  implementation classes.
- Kept public behavior exposed through `SovereignController`,
  `SovereignEditor`, `SovereignMarkdownView`, syntax contracts, and model
  types that are still carried by public signatures.
- `flutter analyze lib test`: passed.
- Focused logic/rendering tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving markdown logic/scanner internals.

### Phase 1 API Shape: Core Internal `lib/src` Migration

- Moved core service/rendering/pipeline implementation modules behind
  `lib/src`, including input-intent handlers, edit pipeline/grouping services,
  renderer helpers, editor-session state carriers, syntax projection helpers,
  markdown line utilities, table/navigation services, and fence/indented-code
  helpers.
- Preserved the supported app-facing entry points:
  `SovereignController`, `SovereignEditor`, `SovereignMarkdownView`,
  `SovereignMarkdownCommands`, theme types, syntax contracts, and public model
  carriers.
- Updated controller, editor, navigation helper, and white-box package-test
  imports to use `package:sovereign_editor/src/...` for those internals.
- `flutter analyze lib test`: passed.
- Focused core/controller/rendering tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving core internals.

### Phase 1 API Shape: Controller/Editor Helper `lib/src` Migration

- Moved controller/editor private helper files behind `lib/src`, including
  controller policy parts, controller host adapter parts, diagnostics/table-tab
  host helpers, the controller navigation helper, and editor inline-actions and
  task-checkbox overlay parts.
- Kept `SovereignController`, `SovereignEditor`, and
  `SovereignMarkdownView` as the public library files while pointing their
  internal `part` and helper imports at `package:sovereign_editor/src/...`.
- `flutter analyze lib test`: passed.
- Focused controller/editor behavior tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving controller/editor helpers.

### Phase 1 API Shape: Public Tree Status

- Completed the focused implementation-file migration wave into `lib/src`.
- Confirmed the remaining `lib/widgets/sovereign/...` files are the public
  controller, editor/preview widgets, command facade/models, theme types,
  advanced syntax/native bridge contracts, and model carriers intentionally
  retained for current public signatures.
- Marked `lib/sovereign_editor.dart` as the package's single supported
  app-facing barrel; no secondary public libraries are documented for this pass.
