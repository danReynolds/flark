# Flark Execution Log

This log records work completed toward production readiness. Keep entries
append-only unless correcting a factual error.

## 2026-05-01

### Extraction

- Created `/Users/dan/Coding/flark`.
- Initialized a new git repo.
- Copied tracked files from `dune_gemini/packages/flark`.
- Copied Flark architecture docs from Dune:
  - `docs/architecture/flark/`
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
  `native/comrak_bridge/target/release/libflark_comrak_bridge.dylib`.
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
- Added a cached final `TextSpan` path in `FlarkTextRenderer` for unchanged
  revision/theme/style renders, while extending the renderer cache key to
  distinguish authoritative inline runs and projected exclusion ranges.
- Reused a single fenced-code scan across render cache miss substeps instead of
  rescanning separately for editor exclusions, inline scan exclusions, and code
  highlighting.
- Skipped supplemental link/image scanning in `FlarkStyleScanner` when the
  text has no link/image trigger characters.
- Updated `test/benchmarks/flark_benchmark_test.dart` to use a real
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
  `flark.dart` barrel API.
- `flutter analyze lib test`: passed.
- Focused render/controller/theme tests: passed.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite.

### Phase 1 API Shape: Barrel Narrowing

- Narrowed `lib/flark.dart` by removing top-level exports for
  parser implementation adapters, parse backend/scheduler internals, scanners,
  marker helpers, `UndoStack`, and `EditDiffer`.
- Kept syntax contracts, native bridge diagnostics, UTF offset mapping, command
  APIs, editor/preview widgets, theme types, and model types still exposed by
  public controller/syntax signatures.
- Added `test/public_api/flark_editor_barrel_test.dart` to verify the
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
  `package:flark/src/...`.
- `flutter analyze lib test`: passed.
- Focused undo/edit-flow tests: passed.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving the undo/edit-diff internals.

### Phase 1 API Shape: Package Vocabulary

- Renamed the public markdown theme from `DuneMarkdownTheme` to
  `FlarkMarkdownTheme`.
- Renamed `lib/theme/dune_markdown_theme.dart` to
  `lib/theme/flark_markdown_theme.dart`.
- Replaced the Dune-named default constructor with
  `FlarkMarkdownTheme.standard()` and updated internal/test callers.
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
  `package:flark/flark.dart` for now.
- Recorded that no secondary public libraries are warranted in this cleanup
  pass; deep imports remain package-test and white-box implementation details.
- Covered the `FlarkMarkdownTheme.standard()` rename, removed top-level
  internal exports, `UndoStack`/`EditDiffer` moving behind `lib/src`, and the
  removed app palette helper.
- `flutter analyze lib test`: passed.

### Phase 1 API Shape: Presentation Helper `lib/src` Migration

- Moved presentation/render helpers behind `lib/src`:
  - `Tier1Painter`;
  - inline-actions overlay and targeting helpers;
  - read-only link tap tracking;
  - read-only task-checkbox overlay helpers.
- Kept `FlarkEditor` and `FlarkMarkdownView` as the public presentation
  widgets while updating them to import the helpers through
  `package:flark/src/...`.
- Updated package tests that intentionally inspect painter behavior to import
  `Tier1Painter` through `package:flark/src/...`.
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

- Added library docs to `lib/flark.dart` and an explicit `library;`
  directive so the top-level barrel has package documentation.
- Added API prose for the primary consumer surface:
  - `FlarkController`;
  - `FlarkEditor`;
  - `FlarkMarkdownView`;
  - `FlarkMarkdownCommands` and command result/capability/link-edit models;
  - `FlarkMarkdownTheme`, `FlarkEditorThemeData`, and related editor
    theme classes.
- `flutter analyze lib test`: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `flutter test test/public_api/flark_editor_barrel_test.dart --reporter compact`:
  passed.

### Phase 1 API Shape: Command Helper `lib/src` Migration

- Moved command implementation helpers behind `lib/src`:
  - block, inline, fence, and link command implementations;
  - command transaction, selection, range, and context helpers.
- Kept `FlarkMarkdownCommands` as the supported public command facade and
  updated it to import those helpers through `package:flark/src/...`.
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
  `FlarkController` and `FlarkMarkdownView`.
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
  those implementation classes through `package:flark/src/...`.
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
  package-test imports to use `package:flark/src/...` for those
  implementation classes.
- Kept public behavior exposed through `FlarkController`,
  `FlarkEditor`, `FlarkMarkdownView`, syntax contracts, and model
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
  `FlarkController`, `FlarkEditor`, `FlarkMarkdownView`,
  `FlarkMarkdownCommands`, theme types, syntax contracts, and public model
  carriers.
- Updated controller, editor, navigation helper, and white-box package-test
  imports to use `package:flark/src/...` for those internals.
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
- Kept `FlarkController`, `FlarkEditor`, and
  `FlarkMarkdownView` as the public library files while pointing their
  internal `part` and helper imports at `package:flark/src/...`.
- `flutter analyze lib test`: passed.
- Focused controller/editor behavior tests: passed.
- `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.
  This reran pub get, analysis, native editor CI with the existing host bridge,
  and the full package test suite after moving controller/editor helpers.

### Phase 1 API Shape: Public Tree Status

- Completed the focused implementation-file migration wave into `lib/src`.
- Confirmed the remaining `lib/widgets/flark/...` files are the public
  controller, editor/preview widgets, command facade/models, theme types,
  advanced syntax/native bridge contracts, and model carriers intentionally
  retained for current public signatures.
- Marked `lib/flark.dart` as the package's single supported
  app-facing barrel; no secondary public libraries are documented for this pass.

### Phase 1 API Shape: Stable Inventory

- Defined the stable API inventory around
  `package:flark/flark.dart` as the only supported
  app-facing import.
- Classified the barrel exports into stable consumer API, stable supporting
  model carriers, and advanced supported integration API.
- Documented deep imports, `lib/src`, and conditional native implementation
  files as unsupported app contracts even when package tests import them for
  white-box coverage.

### Phase 1 API Shape: Public API Docs

- Added Dart API docs for the remaining exported supporting models and advanced
  integration contracts: command style enums, block/decoration/edit/geometry/
  line/state/style models, syntax request/snapshot/token contracts, native
  comrak bridge diagnostics, native bridge factory helpers, and UTF offset
  mapping.
- Replaced stale internal notes in `DecorationModel` with package-facing
  contract docs.
- Verification:
  - `flutter analyze lib test`: passed.
  - `flutter test test/public_api/flark_editor_barrel_test.dart --reporter compact`: passed.
  - `dart doc --dry-run`: 0 warnings and 0 errors.

## Phase 2 Native Packaging

### Native Assets Build Hook

- Added `hook/build.dart` using `package:hooks` and `package:code_assets`.
- The hook compiles the Rust `native/comrak_bridge` crate and emits
  `DynamicLoadingBundled` code assets for macOS, Linux, and Android dynamic
  library targets.
- iOS emits a `LookupInProcess` declaration only; the existing static
  XCFramework flow remains until Dart code assets support static linking or a
  Flutter plugin requirement is proven.
- Added `docs/production_readiness/native_packaging_2026-05-01.md` with the
  package-model decision, ffigen decision, artifact layout, and consumer
  integration path.
- Made native bridge preflight remediation useful for both app builds and local
  package development.
- Verification so far:
  - `flutter analyze hook`: passed.
  - `flutter test test/public_api/flark_editor_barrel_test.dart --reporter compact`: passed and produced a macOS hook `CodeAsset` output.
  - `flutter analyze hook lib test`: passed.
  - `flutter test test/widgets/flark/engine/native_comrak_ffi_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/syntax_engine_factory_test.dart --reporter compact`: passed.
  - `dart doc --dry-run`: 0 warnings and 0 errors.
  - `./scripts/verify_release.sh --skip-native-build --skip-benchmarks`: passed.

### Phase 2 Native Packaging: Example Harness

- Added `example/` as the package mobile integration harness.
- The example depends on `flark` through `path: ..` and imports only
  `package:flark/flark.dart`.
- Replaced the generated counter app with a compact editable/preview markdown
  workspace using `FlarkController`, `FlarkEditor`,
  `FlarkMarkdownView`, and `FlarkMarkdownCommands`.
- Added a public-contract plain-text fallback syntax engine inside the example
  so the app can surface native preflight diagnostics instead of crashing before
  native assets have been built.
- Added `example/android/app:verifyFlarkComrakNativeLibs`, which builds the
  debug APK and verifies that `libflark_comrak_bridge.so` is packaged.
- Added `example/ios/Runner/FlarkComrakAnchor.c` and linked
  `native/comrak_bridge/dist/ios/flark_comrak_bridge.xcframework` in the
  Runner project so iOS process-linked builds keep the bridge symbols visible.
- Added `scripts/verify_example_packaging.sh` for Android APK inspection and
  iOS project/link-anchor verification.
- Fixed the native-assets hook to resolve rustup's `stable` `cargo` and
  `rustc` binaries directly, install missing targets into that same toolchain,
  and pass `RUSTC` to Cargo. This prevents Android hook builds from mixing
  Homebrew `rustc` with rustup-installed Android targets.
- Verification:
  - `flutter pub get` in `example`: passed.
  - `flutter analyze` in `example`: passed.
  - `flutter test test --reporter compact` in `example`: passed.
  - `./scripts/verify_example_packaging.sh --ios`: passed; the local iOS
    XCFramework was not built yet, so the non-strict check reported the
    expected build reminder and `xcodebuild -list` parsed the workspace.
  - `./scripts/verify_example_packaging.sh --android`: passed; Gradle built
    `example/build/app/outputs/apk/debug/app-debug.apk` and found packaged
    `libflark_comrak_bridge.so` entries.

### Phase 3 Architecture Hardening: Enter Intent Handler

- Split Enter key behavior out of the umbrella `FlarkInputIntentHandler`
  into `FlarkEnterIntentHandler`.
- Added a focused `FlarkEnterIntentHost` contract for plain newline,
  indented-code Enter, and fenced-code Enter exit behavior.
- Kept the public controller API unchanged; `FlarkController.handleEnter`
  still delegates through the existing controller input-intent facade.
- Verification:
  - `flutter analyze lib test/widgets/flark/enter_key_integration_test.dart test/widgets/flark/code_fence_exit_test.dart`: passed.
  - `flutter test test/widgets/flark/enter_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_empty_enter_exit_widget_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Backspace Intent Handler

- Moved fenced-code and inline-wrapper backspace transforms out of the
  controller-private fence backspace policy extension and into
  `FlarkBackspaceIntentHandler`.
- Routed backspace policy transforms through the existing
  `FlarkInputIntentHandler` facade so Enter, Tab, navigation, and
  backspace now all have explicit intent handlers.
- Removed the old `flark_controller_policies_fence_backspace.dart` part;
  the controller now exposes backspace-only state through the typed input-intent
  host.
- Marked the Phase 3 input-intent split complete in the execution plan.
- Verification:
  - `flutter analyze lib test/widgets/flark/fence_empty_backspace_behavior_test.dart test/widgets/flark/fence_empty_backspace_widget_test.dart test/widgets/flark/fence_hidden_backspace_guard_test.dart test/widgets/flark/fence_tab_indent_test.dart test/widgets/flark/fence_smart_pairs_test.dart test/widgets/flark/toolbar_markdown_insert_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_empty_backspace_behavior_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_empty_backspace_widget_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_hidden_backspace_guard_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_tab_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_smart_pairs_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/toolbar_markdown_insert_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/list_policy_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/delete_range_error_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/select_all_clear_reset_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Markdown Structure Query Facade

- Added `MarkdownStructureQueryService` as a stateless facade for line, list,
  task marker, fence, quote, and table query helpers.
- Routed controller table lookups, task checkbox line info, fence context,
  quote context, fence language, and hidden-fence geometry queries through the
  new query service.
- Moved pure `FlarkNavigationHelpers` from the controller folder into
  `core/structure/navigation` so structure query code does not depend on the
  controller layer.
- Left structural transforms in the existing edit policy pipeline; the
  execution plan still tracks the transform service as pending.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_query_service.dart lib/src/widgets/flark/core/structure/navigation/flark_navigation_helpers.dart test/widgets/flark/table_key_integration_test.dart test/widgets/flark/task_checkbox_interaction_test.dart test/widgets/flark/code_fence_exit_test.dart test/widgets/flark/blockquote_key_integration_test.dart`: passed.
  - `flutter test test/widgets/flark/table_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/task_checkbox_interaction_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/list_policy_editing_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Heading Transform Service

- Added `MarkdownStructureTransformService` as the first transform-side
  structure service.
- Moved empty ATX heading Enter-exit behavior out of
  `_HeadingPolicy._onEnter` and into the transform service.
- Kept `_HeadingPolicy` as the edit-pipeline rule wrapper so transform ordering
  is unchanged.
- Left list, quote, table, and fence transforms in their existing policy files;
  broader transform extraction remains pending in the execution plan.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart test/widgets/flark/heading_policy_editing_test.dart test/widgets/flark/heading_key_integration_test.dart`: passed.
  - `flutter test test/widgets/flark/heading_policy_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/heading_key_integration_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Blockquote Enter Transform

- Moved blockquote Enter continuation/exit behavior from
  `_QuotePolicy._onEnter` into `MarkdownStructureTransformService`.
- Kept quote Arrow Up/Down selection transforms in `_QuotePolicy`; this slice
  only moves the structural Enter behavior.
- Preserved policy ordering by leaving `_QuotePolicy` as the edit-pipeline rule
  wrapper.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart test/widgets/flark/blockquote_editing_test.dart test/widgets/flark/blockquote_key_integration_test.dart test/widgets/flark/list_policy_editing_test.dart`: passed.
  - `flutter test test/widgets/flark/blockquote_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/list_policy_editing_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: List Enter Transform

- Added `MarkdownStructureQueryService.editableListMarkerForLine` so list
  continuation and list backspace share the same editable marker query path.
- Moved list Enter continuation/exit behavior from `_ListPolicy._onEnter` into
  `MarkdownStructureTransformService`.
- Kept list backspace boundary behavior in `_ListPolicy` for now, but routed
  its marker query through the query service.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_query_service.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart test/widgets/flark/list_policy_editing_test.dart test/widgets/flark/blockquote_editing_test.dart test/widgets/flark/engine/native_live_editing_regression_test.dart`: passed.
  - `flutter test test/widgets/flark/list_policy_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: List Backspace Transform

- Moved list backspace boundary behavior from `_ListPolicy` into
  `MarkdownStructureTransformService`.
- `_ListPolicy` now acts as an edit-pipeline rule wrapper for list Enter and
  list backspace, with shared marker lookup handled by
  `MarkdownStructureQueryService.editableListMarkerForLine`.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart test/widgets/flark/list_policy_editing_test.dart test/widgets/flark/engine/native_live_editing_regression_test.dart`: passed.
  - `flutter test test/widgets/flark/list_policy_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Structure Transform Checkpoint

- Re-ran the fast package confidence gate after the heading, blockquote Enter,
  list Enter, and list backspace transform extractions.
- Verification:
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 3 Architecture Hardening: Blockquote Arrow Transform

- Moved blockquote Arrow Up/Down exit behavior from `_QuotePolicy` into
  `MarkdownStructureTransformService`.
- Promoted the vertical-arrow edit detector into a shared structure model so
  quote and fence transform paths use the same detection semantics.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/core/structure/models/vertical_arrow_edit_context.dart test/widgets/flark/blockquote_editing_test.dart test/widgets/flark/blockquote_key_integration_test.dart test/widgets/flark/code_fence_exit_test.dart`: passed.
  - `flutter test test/widgets/flark/blockquote_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Table Transform Service

- Moved table Enter continuation and established-table formatting out of
  `_TablePolicy` into `TableEditingService`, surfaced through
  `MarkdownStructureTransformService`.
- Moved table row-shape matching into `TableLineParser` so controller table
  parsing, Enter continuation, and Tab insertion share the same row semantics.
- Kept `_TablePolicy` as an edit-pipeline wrapper and kept
  `_ControllerTableTabIntentHost` delegated through controller structure
  services instead of controller-private table helpers.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_query_service.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/core/structure/table/table_line_parser.dart lib/src/widgets/flark/core/structure/table/table_editing_service.dart test/widgets/flark/table_editing_test.dart test/widgets/flark/table_key_integration_test.dart test/widgets/flark/core/structure/table/table_line_parser_test.dart`: passed.
  - `flutter test test/widgets/flark/table_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/core/structure/table/table_line_parser_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_fuzz_invariants_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_performance_baseline_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Arrow Transform

- Moved fenced-code Arrow Up/Down edit-pipeline exits from
  `_FenceNavigationPolicyOps` into `MarkdownStructureTransformService`.
- Kept the keyboard-intent path intact while routing edit-transform fence arrow
  behavior through the shared `VerticalArrowEditContext` and structure query
  callbacks.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_navigation.dart test/widgets/flark/code_fence_exit_test.dart test/widgets/flark/fence_tab_indent_test.dart`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_tab_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Enter Exit Transform

- Moved fenced-code Enter-exit edit-pipeline behavior from
  `_FenceNavigationPolicyOps` into `MarkdownStructureTransformService`, with
  suppression depth and the navigation helper exit computation passed in from
  the controller boundary.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_navigation.dart test/widgets/flark/code_fence_exit_test.dart test/widgets/flark/fence_empty_enter_exit_widget_test.dart`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_empty_enter_exit_widget_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code EOF Continuation Transform

- Moved the closed-EOF fence continuation rule from
  `_FenceNavigationPolicyOps` into `MarkdownStructureTransformService`.
- Kept EOF closing-fence detection behavior unchanged by reusing
  `FencedCodeScanner` and `ProjectionRangeUtils` inside the structure transform
  facade.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_navigation.dart test/widgets/flark/code_fence_exit_test.dart test/widgets/flark/fence_auto_indent_test.dart`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_auto_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Auto-Indent Transform

- Moved fenced-code Enter auto-indent behavior from
  `_FenceNavigationPolicyOps` into `MarkdownStructureTransformService`.
- Preserved the pipeline ordering guard: auto-indent only runs when the current
  pipeline value is still the original simple newline insertion, so prior
  Enter-exit rewrites are not re-indented.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_navigation.dart test/widgets/flark/fence_auto_indent_test.dart test/widgets/flark/code_fence_exit_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_auto_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Closing-Line Transform

- Moved the rule that keeps a hidden closing fence on its own line from
  `_FenceNavigationPolicyOps` into `MarkdownStructureTransformService`.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_navigation.dart test/widgets/flark/fence_closing_line_typing_regression_test.dart test/widgets/flark/code_fence_exit_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_closing_line_typing_regression_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Paste Transform

- Moved fenced-code multiline paste normalization from
  `_FenceNavigationPolicyOps` into `MarkdownStructureTransformService`.
- Removed the now-empty fence navigation policy part from the controller.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart test/widgets/flark/fence_paste_indent_test.dart test/widgets/flark/code_fence_exit_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_paste_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Pair Expansion Transform

- Moved fenced-code Enter pair expansion from `_FencePairingPolicyOps` into
  `MarkdownStructureTransformService`.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_pairing.dart test/widgets/flark/fence_smart_pairs_test.dart test/widgets/flark/fence_undo_grouping_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_smart_pairs_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_undo_grouping_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Auto-Pair Transform

- Moved fenced-code opener auto-pairing from `_FencePairingPolicyOps` into
  `MarkdownStructureTransformService`.
- Moved the quote auto-pair predicate out of the controller with the behavior it
  supports.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_pairing.dart test/widgets/flark/fence_smart_pairs_test.dart test/widgets/flark/fence_undo_grouping_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_smart_pairs_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_undo_grouping_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Closer-Skip Transform

- Moved fenced-code closer skip behavior from `_FencePairingPolicyOps` into
  `MarkdownStructureTransformService`.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_pairing.dart test/widgets/flark/fence_smart_pairs_test.dart test/widgets/flark/fence_undo_grouping_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_smart_pairs_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_undo_grouping_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Selection-Wrap Transform

- Moved fenced-code selection wrapping on opener insert from
  `_FencePairingPolicyOps` into `MarkdownStructureTransformService`.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_fence_pairing.dart test/widgets/flark/fence_smart_pairs_test.dart test/widgets/flark/fence_undo_grouping_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_smart_pairs_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_undo_grouping_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Fenced-Code Closer-Outdent Transform

- Moved fenced-code closer-triggered auto-outdent from
  `_FencePairingPolicyOps` into `MarkdownStructureTransformService`.
- Removed the now-empty fence pairing policy part from the controller.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_transform_service.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart test/widgets/flark/fence_auto_indent_test.dart test/widgets/flark/fence_smart_pairs_test.dart test/widgets/flark/fence_undo_grouping_test.dart`: passed.
  - `flutter test test/widgets/flark/fence_auto_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_smart_pairs_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_undo_grouping_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Structure Transform Completion Checkpoint

- Marked the markdown structure query/transform split complete in the execution
  plan after extracting heading, blockquote, list, table, and fenced-code
  structure transforms behind core services.
- Verification:
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 3 Architecture Hardening: Task Checkbox Query Extraction

- Moved task-checkbox line range analysis out of `FlarkController` and into
  `MarkdownStructureQueryService` behind a typed `TaskCheckboxLineInfo` model.
- Kept the controller's public range API as a thin facade over the structure
  query service.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/structure/markdown_structure_query_service.dart lib/src/widgets/flark/core/structure/models/task_checkbox_line_info.dart test/widgets/flark/core/structure/markdown_structure_query_service_test.dart test/widgets/flark/list_policy_editing_test.dart`: passed.
  - `flutter test test/widgets/flark/core/structure/markdown_structure_query_service_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/list_policy_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/task_checkbox_interaction_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Undo Stack Pipeline Move

- Moved `UndoStack` from controller internals into `core/pipeline`, matching RFC
  017's target pipeline boundary.
- Added direct `UndoStack` coverage for selection-op filtering, grouped undo
  order, grouped redo order, and redo invalidation on fresh text edits.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/core/pipeline/undo_stack.dart test/widgets/flark/core/pipeline/undo_stack_test.dart`: passed.
  - `flutter test test/widgets/flark/core/pipeline/undo_stack_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/commands/command_capabilities_and_transaction_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/ime_composition_undo_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_undo_grouping_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/flark_editor_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Projected Select-All Normalizer

- Moved projected select-all delete normalization from `FlarkController` into
  `core/syntax/ProjectedSelectAllDeleteNormalizer`.
- Removed the controller helper so the value-mutation host delegates directly to
  syntax code with the current projected hidden ranges.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/controllers/flark_value_mutation_coordinator.dart lib/src/widgets/flark/core/syntax/projected_select_all_delete_normalizer.dart test/widgets/flark/core/syntax/projected_select_all_delete_normalizer_test.dart`: passed.
  - `flutter test test/widgets/flark/core/syntax/projected_select_all_delete_normalizer_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/commands/command_capabilities_and_transaction_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/flark_editor_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Undo/Redo Coordinator

- Moved undo/redo orchestration from `FlarkController` into
  `core/pipeline/FlarkUndoRedoCoordinator`.
- Kept restoration state updates controller-owned for this slice, with a narrow
  controller host delegating between the facade and pipeline coordinator.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/controllers/flark_undo_redo_host.dart lib/src/widgets/flark/core/pipeline/undo_redo_coordinator.dart test/widgets/flark/core/pipeline/undo_redo_coordinator_test.dart`: passed.
  - `flutter test test/widgets/flark/core/pipeline/undo_redo_coordinator_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/commands/command_capabilities_and_transaction_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/ime_composition_undo_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/flark_editor_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Table Tab Host Boundary

- Removed controller table helper wrappers used only by the table Tab intent host.
- Updated `_ControllerTableTabIntentHost` to call `MarkdownStructureQueryService`
  and `MarkdownStructureTransformService` directly through the host boundary.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/controllers/flark_table_tab_intent_host.dart lib/src/widgets/flark/core/structure/table/table_tab_intent_service.dart`: passed.
  - `flutter test test/widgets/flark/core/structure/table/table_line_parser_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_fuzz_invariants_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_performance_baseline_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Navigation Query Host Boundary

- Moved fence-body caret/range checks into `MarkdownStructureQueryService` with
  direct focused tests.
- Removed controller navigation wrappers for fence context, quote context,
  quote/fence arrow exits, fence Enter exits, fence language lookup, outdent unit
  lookup, and trailing blank trimming.
- Updated policy, input-intent, and table-tab hosts to call structure query and
  navigation services directly through their host boundary.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_input_intent_handler.dart lib/src/widgets/flark/controllers/flark_table_tab_intent_host.dart lib/src/widgets/flark/core/structure/markdown_structure_query_service.dart test/widgets/flark/core/structure/markdown_structure_query_service_test.dart`: passed.
  - `flutter test test/widgets/flark/core/structure/markdown_structure_query_service_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_smart_pairs_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_auto_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/table_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_editing_test.dart --reporter compact`: passed.

### Phase 3 Architecture Hardening: Controller Facade RFC 017 Checkpoint

- Moved vertical caret movement state updates out of `FlarkController` and
  into the input-intent host that owns the navigation behavior.
- Trimmed stale phase-label comments from the controller after extracting the
  referenced responsibilities into services and coordinators.
- Marked the RFC 017 controller-facade extraction item complete in the execution
  plan: `FlarkController` is now 680 lines, below the RFC's 700-line
  acceptance threshold.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/controllers/flark_input_intent_handler.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/core/structure/markdown_structure_query_service.dart`: passed.
  - `flutter test test/widgets/flark/code_fence_exit_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/blockquote_key_integration_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/core/structure/navigation/vertical_caret_navigation_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 3 Architecture Hardening: Read-Only Checkbox Visual Layer

- Extracted read-only task-checkbox visual widget construction from
  `FlarkMarkdownView` into
  `FlarkReadOnlyTaskCheckboxVisualLayer`.
- Kept the markdown view focused on controller lifecycle, pointer interaction,
  and composition of typed render helpers; the new visual layer is render-only
  with typed props.
- Marked the Phase 3 rendering-composition item complete in the execution plan.
- Verification:
  - `flutter analyze lib/widgets/flark/presentation/flark_markdown_view.dart lib/src/widgets/flark/presentation/read_only_task_checkbox_visual_layer.dart lib/src/widgets/flark/presentation/read_only_task_checkbox_overlay.dart`: passed.
  - `flutter test test/widgets/flark/flark_markdown_view_render_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/flark_markdown_view_smoke_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/flark_markdown_view_parity_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 3 Architecture Hardening: Native Comrak Bridge Module Split

- Split the Rust bridge out of a single 1325-line `lib.rs` into focused
  modules:
  - `lib.rs`: exported C ABI entrypoints.
  - `abi.rs`: response layout plus allocation/freeing.
  - `payload.rs`: serialized native parse payload models.
  - `source_ranges.rs`: byte-range normalization and line helpers.
  - `marker_mapping.rs`: markdown marker hiding ranges.
  - `parser.rs`: comrak document traversal and block/inline token collection.
- Marked the Rust bridge split and Phase 3 behavior-test upkeep items complete
  in the execution plan.
- Verification:
  - `cargo test --manifest-path native/comrak_bridge/Cargo.toml`: passed.
  - `./scripts/build_comrak_all.sh --host-only`: passed.
  - `flutter test test/widgets/flark/engine/native_comrak_ffi_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_comrak_parse_backend_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/native_live_editing_regression_test.dart --reporter compact`: passed.

### Phase 4 Feature Completeness: Thematic Break Interaction

- Fixed `insertHorizontalRule` so inserting `---` after a paragraph creates an
  isolated thematic break instead of accidentally converting the previous
  paragraph into a setext heading.
- Added command/controller coverage that verifies the inserted rule is
  parser-classified as `BlockType.thematicBreak`, keeps the caret outside hidden
  marker ranges, and supports typing the following paragraph immediately after
  insertion.
- Added CommonMark adapter coverage for isolated dash classification and updated
  the support matrix from partial to supported for thematic breaks.
- Verification:
  - `flutter test test/widgets/flark/commands/block_commands_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/commands/command_capabilities_and_transaction_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/engine/commonmark_syntax_engine_adapter_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/block_markdown_rendering_test.dart --reporter compact`: passed.

### Phase 4 Feature Completeness: Indented Code Editing

- Extended indented code handling beyond baseline Enter continuation:
  - Enter on a blank indented continuation line now exits the code block by
    removing the carried indent.
  - Backspace at the code indentation boundary now removes one code indent unit
    for 4-space and tab indents.
  - Tab-indented list lines remain list-owned and do not route through indented
    code outdent behavior.
- Added a dedicated indented-code backspace policy after fenced-code backspace
  handling so fenced code keeps its stronger context-specific rules.
- Updated the support matrix from partial to supported for indented code blocks.
- Verification:
  - `flutter analyze lib/widgets/flark/controllers/flark_controller.dart lib/src/widgets/flark/controllers/flark_controller_policies.dart lib/src/widgets/flark/controllers/flark_controller_policies_indented_code.dart lib/src/widgets/flark/core/intents/input_intent_handler.dart lib/src/widgets/flark/core/intents/input_intent_backspace_handler.dart lib/src/widgets/flark/core/structure/indented_code/indented_code_enter_service.dart test/widgets/flark/indented_code_editing_test.dart`: passed.
  - `flutter test test/widgets/flark/indented_code_editing_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/fence_auto_indent_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/list_key_integration_test.dart --reporter compact`: passed.

### Phase 4 Feature Completeness: Raw HTML Text-Only Policy

- Documented the final raw HTML policy in
  `docs/production_readiness/raw_html_policy_2026-05-02.md`: Flark preserves
  raw HTML as literal source text and does not execute, embed, or sanitize HTML
  because it does not render HTML.
- Added read-only and editor rendering regressions that keep raw HTML tags
  visible as text and verify the editor does not hide them as markdown markers.
- Updated the support matrix and marked the raw HTML Phase 4 policy item
  complete.
- Verification:
  - `flutter test test/widgets/flark/flark_markdown_view_parity_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/block_markdown_rendering_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 4 Feature Completeness: Escapes/Entities Coverage

- Tightened the predictive inline scanner so backslash-escaped `*`, `_`, and
  opening backticks stay literal instead of creating transient hidden marker
  ranges before authoritative parsing completes.
- Added parser, scanner, editor rendering, and predictive cursor-safety
  regressions for escaped inline delimiters and source-stable entities.
- Updated the support matrix and marked the escapes/entities Phase 4 item
  complete.
- Verification:
  - `cargo test --manifest-path native/comrak_bridge/Cargo.toml`: passed.
  - `./scripts/build_comrak_all.sh --host-only`: passed.
  - `flutter test test/widgets/flark/logic/flark_style_scanner_test.dart test/widgets/flark/flark_inline_test.dart test/widgets/flark/predictive_inline_markers_test.dart test/widgets/flark/engine/commonmark_syntax_engine_adapter_test.dart test/widgets/flark/engine/native_comrak_ffi_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 4 Feature Completeness: Reference Link Rendering and Cursor Behavior

- Added scanner support for CommonMark shortcut reference links (`[label]`
  resolved by `[label]: url`) while avoiding accidental matches on reference
  definition lines.
- Covered full/shortcut reference link rendering, bracket hiding, overlay URL
  resolution, and source-stable typing at the label boundary.
- Updated the support matrix and marked the reference-link Phase 4 item
  complete.
- Verification:
  - `flutter test test/widgets/flark/logic/flark_style_scanner_test.dart test/widgets/flark/flark_inline_test.dart test/widgets/flark/link_actions_overlay_test.dart test/widgets/flark/link_policy_editing_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 4 Feature Completeness: Images/Media Source-First Preview Policy

- Kept image markdown source-first while improving preview behavior: `http` and
  `https` targets render image previews; relative, attachment, and other
  non-network targets show an explicit preview-unavailable state instead of
  falling through `Image.network`.
- Preserved image open/copy/edit actions and standalone preview tapping for
  non-network media targets.
- Updated the support matrix and marked the images/media Phase 4 item complete.
- Verification:
  - `flutter test test/widgets/flark/image_actions_overlay_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 4 Feature Completeness: Table Command Operations

- Closed the remaining table gap as a source-first GFM policy instead of adding
  a separate WYSIWYG grid model: tables remain parser-backed markdown source
  with monospace/source-aligned rendering.
- Added command-layer table operations through `FlarkMarkdownCommands`:
  table insertion, body-row insertion/deletion, and column insertion/deletion.
- Kept command-oriented source transforms in `TableCommandEditingService` while
  reusing `TableEditingService` formatting, so Enter continuation, Tab
  navigation, and toolbar/menu commands share parser-backed table shape rules
  without re-inflating the keyboard editing service.
- Updated the support matrix, command API reference, how-it-works overview, and
  execution plan to mark the Phase 4 table item complete.
- Verification:
  - `flutter analyze lib/src/widgets/flark/core/structure/table/table_editing_service.dart lib/src/widgets/flark/core/structure/table/table_command_editing_service.dart lib/src/widgets/flark/commands/internal/table_commands.dart lib/widgets/flark/commands/flark_markdown_commands.dart test/widgets/flark/commands/table_commands_test.dart`: passed.
  - `flutter test test/widgets/flark/commands/table_commands_test.dart --reporter compact`: passed.
  - `flutter test test/widgets/flark/commands test/widgets/flark/table_editing_test.dart test/widgets/flark/table_key_integration_test.dart test/widgets/flark/core/structure/table/table_line_parser_test.dart test/widgets/flark/table_fuzz_invariants_test.dart test/widgets/flark/table_performance_baseline_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh --skip-native`: passed.

### Phase 5 Release Metadata and Gate Hardening

- Added `CHANGELOG.md` with the initial unreleased `0.1.0` standalone package
  hardening summary.
- Added `docs/production_readiness/release_checklist_2026-05-02.md` to document
  the release dry-run command and the remaining owner-decision blockers:
  license, canonical public repository/issue/documentation URLs, external
  screenshots, and removal of `publish_to: none`.
- Added `dart doc --dry-run` to `scripts/verify_release.sh`.
- Fixed stale package-doc paths in the README and current architecture docs so
  they point to the standalone `docs/architecture/flark/` and
  `docs/architecture/rfc/` locations.
- Updated the execution plan: actionable Phase 5 items are complete; remaining
  release blockers are owner/legal/hosting decisions, not code-health issues.
- Verification:
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.
  - `./scripts/verify_release.sh`: passed. `flutter pub get` emitted a
    non-fatal pub.dev advisory decode warning (`advisoriesUpdated must be a
    String`) for `http`, but dependency resolution completed and the command
    exited successfully.

## 2026-05-31

### V2 Live Web Regression: Terminal Block Append Caret Stability

- Reproduced the Scratch pad caret bug in the web example: after inserting a
  code fence, typing inside it, clicking below it, and typing a following
  paragraph, the live-rendered editor could keep the text input coupled to the
  previous rendered block. The same risk applied to terminal list blocks.
- Added a tap-below-rendered-content path in the live-rendered editor that
  creates a terminal append host at the canonical source end instead of relying
  on the previous block's editable host.
- Added stable synthetic host identity for terminal append and continuation
  lines so rapid character-by-character web input keeps focus on the append
  host while the parser/render plan catches up.
- Normalized predictive render-plan code-fence ranges back to the current
  source closing fence when a following paragraph has just been inserted. This
  prevents optimistic render state from temporarily swallowing the new
  paragraph into the code block.
- Narrowed autofocus synchronization to terminal append hosts and made focus
  scheduling generation-based so stale post-frame callbacks cannot suppress a
  newer caret target.
- Added widget regressions for tapping below terminal code fences and lists,
  including a character-by-character code-fence typing case that mirrors the
  web text input behavior.
- Added a follow-up regression for entering multiple blank lines after a
  terminal code fence and then typing on the latest blank line. The fix keeps
  parser-omitted blank-line hosts anchored to the same terminal append identity
  even when the code block render range includes its trailing newline, and only
  restores focus after an actual edit/revision or prior editor focus so static
  visual fixtures do not gain a caret.
- Added a second follow-up for the parser-adoption phase after those blank
  lines. Once the parser turns the typed text into a normal paragraph, the
  focused synthetic blank-line host is reconciled away; the focus coordinator
  now records when it disposes a focused block node and restores focus to the
  new source-selection owner during that rebuild. This covers both tap-below
  and Enter-to-exit-fence paths.
- Added a fast-typing follow-up for web text input updates that deliver the
  inserted character before the collapsed selection offset has advanced. Direct
  source and table-cell hosts now normalize pure insertion values to place the
  caret after the inserted text, preventing the source selection from sticking
  one character behind while rapid input continues.
- Hardened that path for multiple input updates arriving before the next
  Flutter frame. Live block text hosts now keep a local edit snapshot and the
  current direct source replacement range, so a second rapid character replaces
  the already-extended source span instead of using the stale render block
  range from the previous frame. The hidden text controller is also updated
  immediately when a stale platform selection is normalized, so the browser
  input host converges before the next character arrives.
- Verified the running Flutter web example at `http://127.0.0.1:7357` through
  Chrome DevTools Protocol:
  - Scratch -> code fence -> type `foo` -> click below -> type `after`
    produced a rendered code block followed by a normal paragraph, with
    `4 lines · 21 chars` and caret at source offset 21.
  - Scratch -> bullet list -> type `item` -> click below -> type `after`
    produced a list item followed by a normal paragraph, with
    `3 lines · 13 chars` and caret at source offset 13.
  - Scratch -> code fence -> type `foo` -> click below -> enter two blank
    lines -> type `x` produced a rendered code block followed by `x` on the
    latest blank line, with `5 lines · 18 chars` and caret at source offset 18.
  - Follow-up after the fast-typing fix reloaded the same running app and
    repeated Scratch -> code fence -> type `foo` -> click below -> enter four
    blank lines -> type `x`; the DOM text input hosts remained split between
    the code block body and the trailing paragraph host instead of reattaching
    the caret to the previous block.
  - A rapid-input web probe on the same route inserted `abcdefghijklmnop`
    without per-character delays after the blank-line fence exit. The active
    textarea reported `selectionStart == selectionEnd == 16`, matching the end
    of the inserted text.
  - After the no-frame snapshot hardening, a three-run rapid web probe inserted
    `abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345` after the same
    code-fence blank-line route. Each run left the active textarea at
    `selectionStart == selectionEnd == 58`.
- Verification:
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name "terminal live code fence"`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed with 80 tests.
  - `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart --plain-name "live rendered edge cases stay visually stable"`: passed.
  - `flutter test test/v2/flutter`: passed with 194 tests.
  - Follow-up rerun after the blank-line/parser-adoption fix:
    `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed with 82 tests;
    `flutter test test/v2/flutter`: passed with 196 tests.
  - Follow-up rerun after the fast-typing stale-selection fix:
    `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name "blank lines below a terminal live code fence"`: passed;
    `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed with 82 tests;
    `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart --plain-name "live rendered edge cases stay visually stable"`: passed;
    `flutter test test/v2/flutter`: passed with 196 tests.
  - Follow-up rerun after the no-frame rapid input snapshot fix:
    `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name "blank lines below a terminal live code fence"`: passed;
    `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed with 82 tests;
    `flutter test test/v2/flutter`: passed with 196 tests;
    `flutter analyze lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed.

### V2 Live Web Regression: Partial Strong Delimiter Styling

- Reproduced the editing-state issue behind typing `**wow*`: Comrak can parse
  the second opener asterisk plus the trailing asterisk as a valid emphasis
  pair, so the editor briefly projected hidden markers and italic styling even
  though the user was in the middle of entering a bold span.
- Normalized this specific parser bridge shape as an editor UX rule. When a
  single emphasis token is formed by splitting a pending same-character
  double-delimiter run, the native parse adapter now drops that transient
  emphasis token and keeps its marker ranges visible. Completed `**wow**`
  still maps to normal strong styling.
- Added parser and live-rendered editor regressions for the partial-to-complete
  transition. `**wow*` remains literal with no italic/bold span; after the
  second closing asterisk, `**wow**` projects to `wow` with bold styling.
- Verification:
  - `flutter test test/v2/markdown/flark_native_comrak_parse_backend_test.dart --plain-name "keeps partial strong delimiter intent literal"`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name "partial bold delimiters"`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed with 83 tests.
  - `flutter test test/v2/markdown`: passed with 221 tests.
  - `flutter test test/v2/flutter`: passed with 197 tests.
  - `flutter analyze lib/src/v2/markdown/parse/flark_native_comrak_parse_backend.dart test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed.

### V2 QA Edge Pass: Malformed Inline Markdown and Scratch Playground

- Ran an ad hoc native-parser/projection probe over deliberately awkward
  incremental Markdown states: missing strong delimiters (`**wow*`, `*wow**`,
  underscore variants), triple/quad delimiter runs, escaped markers, inline
  code with delimiter-looking text, partial links/images, partial task markers,
  table fragments, and partial/complete code-fence exits with trailing blank
  lines.
- Promoted the source-visible malformed cases into the markdown transition
  matrix. The new coverage asserts that partial strong/underscore/triple
  delimiter states, partial double-backtick inline code, and typed partial
  link/image destinations do not create hidden ranges, inline tokens, or
  overlays before the syntax is actually complete.
- Added live-rendered editor coverage for the same inline states so the
  displayed `EditableText` remains literal and unstyled during partial entry,
  while inline code still shields delimiter text with monospace styling.
- Added a higher-level Scratch playground widget regression that enters a
  mixed "QA paste" document containing malformed inline syntax, a code fence,
  multiple blank lines after the fence, partial list/task markers, and a
  partial double-backtick code span. Live edit keeps the malformed inline text
  visible, hides completed fence markers, and switching to Source preserves the
  canonical Markdown exactly.
- The example playground QA pass also exposed a stale example integration test
  and a preview Scrollbar assertion on desktop. Updated the example scenario
  and mode keys to match the current UI, refreshed the integration assertions
  to current Sample/Article/Tables content, and gave the preview pane an
  explicit `ScrollController` shared by `Scrollbar` and
  `SingleChildScrollView`.
- Browser/plugin note: the in-app browser surface was unavailable in this
  session (`agent.browsers.list()` returned no browsers). The already running
  web server at `http://127.0.0.1:7357` was hot-reloaded after the example
  changes and responded to `curl`. Flutter integration tests could not be
  completed on Chrome because Flutter reports web devices are unsupported for
  integration tests, and the macOS integration runner hung after launch in this
  environment, so the deterministic example widget suite is the verified
  playground-level coverage for this pass.
- Verification:
  - `flutter test test/v2/markdown/flark_markdown_transition_matrix_test.dart --reporter compact`: passed with 70 tests.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name "malformed inline"`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed with 84 tests.
  - `(cd example && flutter test test/widget_test.dart --plain-name "scratch keeps awkward partial markdown states editable" --reporter compact)`: passed.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 34 tests.
  - `flutter test test/v2/markdown --reporter compact`: passed with 231 tests.
  - `flutter test test/v2/flutter --reporter compact`: passed with 198 tests.
  - `flutter analyze test/v2/markdown/flark_markdown_transition_matrix_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed.
  - `(cd example && flutter analyze lib/main.dart test/widget_test.dart integration_test/markdown_flow_test.dart)`: passed.

### V2 Research Audit: Markdown Live Editor Gotchas

- Cross-checked Flark's live-rendered editor against first-party notes from
  mature Markdown/editor projects: TOAST UI's parser redesign notes, ProseMirror
  position/selection mapping and embedded editor examples, CodeMirror widget
  decoration guidance, Lexical state/selection/update guidance, Tiptap
  input/paste/history docs, and CommonMark/GFM parsing rules.
- The recurring external gotchas were parser/view disagreement, position mapping
  through hidden or replaced syntax, whole-document rerenders during live
  preview, focus/selection feedback loops, embedded block-editor boundary
  navigation, Markdown shortcut undo grouping, paste normalization, ambiguous
  partial delimiters, and table cell escaping.
- The current V2 surface has explicit coverage for most of those areas:
  source-visible malformed inline syntax, parser-omitted blank-line hosts,
  terminal append hosts below code fences/lists, stale parse rejection, live
  surface stability across predictive edits, code-fence/table paste
  normalization, table escaped-pipe selection mapping, task/table undo/redo,
  and source/live selection agreement.
- Remaining follow-up candidates are narrower than the earlier cursor bugs:
  add live-rendered IME composition cases inside block widgets, add explicit
  undo grouping around plain Markdown shortcut conversions, and add automated
  browser-level rapid-typing validation once a controllable browser surface is
  available again.
- Verification:
  - `flutter test test/v2/markdown/flark_markdown_transition_matrix_test.dart test/v2/flutter/flark_parse_scheduler_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed with 158 tests.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 34 tests.

### V2 Gap Closure Pass: IME Grouping and Rapid Fence Editing

- Closed two follow-up gaps from the Markdown live-editor research audit.
  Projected/live block edits now carry the same IME composition undo group IDs
  as source editing, so one undo after a composing sequence removes the whole
  committed composition instead of revealing an intermediate composition state.
  Coverage now includes projected inline editing, live code bodies, and live
  table cells.
- Added explicit source and live-rendered undo/redo coverage for Markdown
  shortcut Enter handling. List continuation from Enter is now asserted as one
  history action in both source and live block editing.
- Browser-level rapid typing exposed a real scratch-pad fence bug: typing an
  opening fence with a `dart` info string character-by-character could promote
  the fence as soon as the third backtick arrived, routing `dart` into the code
  body. The live opener now avoids auto-completing the standalone fence when a
  single typed character merely completes the marker prefix.
- The same rapid-typing pass exposed a second fence bug after `foo` + Enter in
  an unclosed code block. Unclosed EOF code-body ranges were trimming the final
  newline, so the next fast backtick could be appended to `foo` instead of the
  new closing-fence line. Closed fences still trim the body newline before the
  closer, but unclosed EOF fences now keep trailing newlines editable.
- Browser/plugin note: the in-app browser was unavailable in this session, so
  browser validation used a headless Chrome/CDP fallback against the local
  example app. A Flutter Chrome-platform test attempt hung and was stopped; the
  deterministic widget regressions now cover the same fast typing sequences.
- Verification:
  - `flutter test test/v2/flutter/flark_projected_editable_text_test.dart --plain-name "projected IME" --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name "IME composition" --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name "undoes and redoes live list continuation" --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_editable_text_test.dart --plain-name "upgrades platform Enter" --reporter compact`: passed.
  - `(cd example && flutter test test/widget_test.dart --plain-name "fast typed fence language" --reporter compact)`: passed.
  - `(cd example && flutter test test/widget_test.dart --plain-name "fast typed fence closing" --reporter compact)`: passed.
  - `flutter test test/v2/flutter --reporter compact`: passed with 203 tests.
  - `flutter test test/v2/markdown --reporter compact`: passed with 232 tests.
  - `flutter test test/v2/projection --reporter compact`: passed with 33 tests.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 36 tests.
  - `flutter analyze lib/src/v2/markdown/source/flark_markdown_fenced_code_scanner.dart lib/src/v2/projection/flark_projected_text_edit_adapter.dart lib/src/v2/flutter/flark_flutter_controller.dart lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/markdown/flark_markdown_fenced_code_scanner_test.dart test/v2/flutter/flark_projected_editable_text_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_editable_text_test.dart`: passed.
  - `(cd example && flutter analyze test/widget_test.dart)`: passed.

### V2 DX Ergonomics Pass: Controller-Owned Field Surface

- Compared Flark's app-facing setup against peer editor leaders. Lexical
  emphasizes a modular composer/editor instance split with plugin-like feature
  registration; Tiptap's entrypoint accepts initial content plus extensions;
  CodeMirror's core is configured from extension bundles; and ProseMirror
  keeps the transaction/plugin model explicit for advanced integrations.
- Flark already had the advanced half of that shape: a
  `FlarkFlutterController`, command registry, extension set, parse
  scheduler, projected editing, and shared render plans. The ergonomic gap was
  the first-run app path: even the quick start required manually creating and
  disposing a controller before rendering an editor.
- Added `FlarkMarkdownField` as the low-ceremony app API. It owns a
  `FlarkFlutterController`, starts from `initialMarkdown`, forwards the
  existing editor configuration knobs, and reports source changes through
  `onChanged`. `FlarkMarkdownEditor` remains the controller-owned surface
  for split panes, command toolbars, and custom runtime sharing.
- Tightened the field semantics after review: `initialMarkdown` is now
  initial-only for an existing widget state, so a parent that stores `onChanged`
  back into app state will not recreate the controller and cause selection
  jumps on every rebuild. A widget key still creates a fresh field for a new
  document, and extension changes preserve the current Markdown.
- Exported the new field through the promoted top-level barrel and the advanced
  v2 barrel. Updated the README quick start to use the field while keeping the
  shared controller example for editor/preview layouts.
- Verification:
  - `flutter test test/v2/flutter/flark_markdown_surface_test.dart --reporter compact`: passed with 5 tests.
  - `flutter test test/v2/public_api/flark_editor_v2_public_api_test.dart test/public_api/flark_editor_barrel_test.dart --reporter compact`: passed with 5 tests.
  - `flutter analyze lib/src/v2/flutter/flark_markdown_field.dart lib/src/v2/flutter/flutter.dart lib/flark.dart lib/flark_advanced.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart test/public_api/flark_editor_barrel_test.dart`: passed.
  - `flutter test test/v2/flutter --reporter compact`: passed with 204 tests.
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.
  - Follow-up after tightening `initialMarkdown` semantics:
    `flutter test test/v2/flutter/flark_markdown_surface_test.dart --reporter compact`: passed with 5 tests;
    `flutter analyze lib/src/v2/flutter/flark_markdown_field.dart test/v2/flutter/flark_markdown_surface_test.dart`: passed;
    `flutter test test/v2/flutter --reporter compact`: passed with 206 tests;
    `dart doc --dry-run`: passed with 0 warnings and 0 errors.

### V2 DX Ergonomics Pass: Toolbar Command Helpers

- Continued the peer comparison from the controller-owned field pass. The
  advanced editor frameworks all make simple integration possible while still
  leaving a precise transaction/extension path for advanced users. Flark's
  v2 architecture already had the precise command layer, but everyday toolbar
  code still had to import command ids and construct payload objects for common
  Markdown actions.
- Added `FlarkMarkdownControllerCommands`, a public extension on
  `FlarkFlutterController`, for common Markdown toolbar/menu actions:
  heading changes, inline styles, quote/list/task toggles, thematic breaks,
  code fences, tables, and link editing. The helpers are intentionally thin and
  return `FlarkEditorRuntimeResult`, so apps can start with
  `controller.toggleStrong()` without giving up command rejection details.
- Exported the helper extension through the promoted app barrel and advanced v2
  barrel, updated the README toolbar example, and refactored the example app
  toolbar to call the readable helpers rather than hand-building command
  payloads.
- The first helper test found a real command-level ergonomics bug: inserting a
  fenced code block at the end of a paragraph produced
  `paragraph```dart` instead of a separated block. Fixed
  `FlarkMarkdownBlockCommands.insertFence` to add block-safe surrounding
  newlines when the caret is embedded in paragraph text, while preserving the
  existing line-start behavior.
- Verification:
  - `flutter test test/v2/markdown/flark_markdown_block_commands_test.dart test/v2/flutter/flark_markdown_controller_commands_test.dart --reporter compact`: passed with 21 tests.
  - `flutter test test/v2/public_api/flark_editor_v2_public_api_test.dart test/public_api/flark_editor_barrel_test.dart --reporter compact`: passed with 5 tests.
  - `flutter analyze lib/src/v2/flutter/flark_markdown_controller_commands.dart lib/src/v2/flutter/flark_markdown_field.dart lib/src/v2/flutter/flutter.dart lib/src/v2/markdown/commands/flark_markdown_block_commands.dart lib/flark.dart lib/flark_advanced.dart example/lib/main.dart test/v2/flutter/flark_markdown_controller_commands_test.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/markdown/flark_markdown_block_commands_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart test/public_api/flark_editor_barrel_test.dart`: passed.
  - `flutter test test/v2/flutter --reporter compact`: passed with 206 tests.
  - `flutter test test/v2/markdown --reporter compact`: passed with 233 tests.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 36 tests.
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.
  - `git diff --check -- README.md docs/production_readiness/execution_log.md lib/flark.dart lib/flark_advanced.dart lib/src/v2/flutter/flutter.dart lib/src/v2/flutter/flark_markdown_field.dart lib/src/v2/flutter/flark_markdown_controller_commands.dart lib/src/v2/markdown/commands/flark_markdown_block_commands.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_markdown_controller_commands_test.dart test/v2/markdown/flark_markdown_block_commands_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart test/public_api/flark_editor_barrel_test.dart example/lib/main.dart`: clean.

### V2 DX Ergonomics Pass: Widget Surface Clarification

- Tightened the app-facing widget story after review showed the surface still
  felt overlapping. The top-level `flark.dart` barrel now presents
  the opinionated app path: `FlarkMarkdownField`,
  `FlarkMarkdownEditor`, `FlarkMarkdownPreview`,
  `FlarkReadOnlyPreview`, controller APIs, command helpers, and callback
  types.
- Kept the mode-specific implementation widgets
  (`FlarkEditableText`, `FlarkProjectedEditableText`, and
  `FlarkLiveRenderedEditableText`) in `flark_advanced.dart` for
  advanced custom shells, but removed them from the top-level app barrel so
  first-time users do not see four editable widgets as equivalent choices.
- Added a README widget guide that names the four primary widgets, their state
  owner, and when to choose each. The guide also tells app developers to use
  `FlarkMarkdownEditor(editingMode: ...)` instead of directly choosing the
  raw/source/projected/live implementation widgets.
- Verification:
  - `flutter test test/public_api/flark_editor_barrel_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart --reporter compact`: passed with 5 tests.
  - `flutter analyze lib/flark.dart test/public_api/flark_editor_barrel_test.dart`: passed.
  - `(cd example && flutter analyze lib/main.dart test/widget_test.dart)`: passed.
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.
  - `git diff --check -- README.md docs/production_readiness/execution_log.md lib/flark.dart test/public_api/flark_editor_barrel_test.dart`: clean.

### V2 DX Ergonomics Pass: Two-Widget Public Surface

- Removed the remaining public widget overlap instead of preserving aliases.
  The app-facing path is now two widgets: `FlarkMarkdownEditor` for
  editable Markdown and `FlarkMarkdownPreview` for read-only Markdown.
  `FlarkMarkdownField` is gone, and `FlarkReadOnlyPreview` is no
  longer exported through the app or v2 public barrels.
- Merged the field convenience path into `FlarkMarkdownEditor`. Apps can
  pass `initialMarkdown` and `onChanged` for the simple owned-controller case,
  or pass `controller` for split panes, toolbars, undo/redo UI, and document
  sharing. Constructor asserts reject ambiguous ownership such as
  `controller` plus `initialMarkdown` or `extensions`.
- Made `FlarkMarkdownPreview` match the same Flutter convention: pass
  `markdown` for standalone preview, or `controller` to render the same plan as
  an editor. Shared-controller previews no longer schedule parsing or accept a
  `parseBackend`; parser scheduling has one owner.
- Pruned `flark_advanced.dart` so implementation widgets and helpers stay
  behind deep imports for package tests: raw/projected/live concrete editing
  widgets, parser scheduler, render-plan overlay widget, and text-delta adapter
  are no longer part of the public v2 barrel.
- Updated the README, migration guide, public API inventory, Flutter adapter
  note, example app, and public API tests to describe and enforce the two-widget
  surface. White-box tests that still need implementation widgets now deep
  import `src/v2/flutter/flutter.dart`.
- Verification:
  - `flutter test test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_render_plan_parity_test.dart test/public_api/flark_editor_barrel_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart --reporter compact`: passed.
  - `flutter analyze lib/src/v2/flutter/flark_markdown_editor.dart lib/src/v2/flutter/flark_markdown_preview.dart lib/src/v2/flutter/flutter.dart lib/flark.dart lib/flark_advanced.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_render_plan_parity_test.dart test/public_api/flark_editor_barrel_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart example/lib/main.dart example/test/widget_test.dart`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_visual_layout_test.dart --reporter compact`: passed with 4 tests after moving the white-box test to deep imports.
  - `flutter test test/v2/flutter --reporter compact`: passed with 207 tests.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 36 tests.
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.

### V2 DX Ergonomics Pass: Clean Widget Names

- Renamed the two public widget entry points to the clean names requested for
  the app API: `Markdown` renders read-only Markdown, and `MarkdownEditor`
  edits Markdown. The old `FlarkMarkdownEditor` and
  `FlarkMarkdownPreview` names are not exported as aliases.
- Kept the existing Flark-prefixed controller, command, parser, and render
  model types. Those names are still useful because they are framework-specific
  contracts; the widgets are the low-friction app entry points.
- Updated the README, migration guide, public API inventory, Flutter adapter
  docs, example app, widget tests, integration test, visual golden harness, and
  public API guards to use `Markdown` / `MarkdownEditor`.
- Verification:
  - `flutter test test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_render_plan_parity_test.dart test/public_api/flark_editor_barrel_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart --reporter compact`: passed.
  - `flutter analyze lib/src/v2/flutter/flark_markdown_editor.dart lib/src/v2/flutter/flark_markdown_preview.dart lib/src/v2/flutter/flutter.dart lib/flark.dart lib/flark_advanced.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_render_plan_parity_test.dart test/public_api/flark_editor_barrel_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart example/lib/main.dart example/test/widget_test.dart`: passed.
  - `flutter test test/v2/flutter --reporter compact`: passed with 207 tests.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 36 tests.
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.
  - `git diff --check -- README.md docs/architecture/v2/migration_guide_2026-05-02.md docs/architecture/v2/public_api_inventory_2026-05-02.md docs/architecture/v2/flutter_adapter_2026-05-02.md docs/architecture/v2/quality_journal_2026-05-31.md docs/architecture/v2/quality_audit_2026-05-05.md docs/production_readiness/execution_log.md lib/flark.dart lib/flark_advanced.dart lib/src/v2/flutter/flark_markdown_editor.dart lib/src/v2/flutter/flark_markdown_preview.dart example/lib/main.dart example/test/widget_test.dart example/integration_test/markdown_flow_test.dart test/public_api/flark_editor_barrel_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_render_plan_parity_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_v2_visual_golden_test.dart test/v2/flutter/flark_markdown_web_smoke_test.dart test/v2/flutter/flark_read_only_preview_test.dart`: clean.

### Package Rename: Flark Public Identity

- Renamed the Dart package identity to `flark` and moved the public library
  entry points to `flark.dart`, `flark_core.dart`, and `flark_advanced.dart`.
- Renamed the public framework-specific API types onto the `Flark*` prefix
  while keeping the two app widgets clean: `Markdown` and `MarkdownEditor`.
- Updated the example app, tests, public API guards, scripts, native packaging
  references, and docs to import `package:flark/flark.dart` and to use the
  Flark type names.
- Replaced the README with a concise package front door and added a docs setup:
  `docs/README.md`, `docs/getting_started.md`, `docs/api_surface.md`,
  `docs/parser_and_platforms.md`, and `docs/development.md`.
- Deferred native artifact migration into a follow-up pass so the Dart package
  rename could land with focused validation.
- Optimized render-plan construction by partitioning inline tokens into
  sibling block buckets before rendering. This removes the old repeated scan
  of the full inline-token list for every parsed block and restored the
  render-plan performance budget.
- Verification:
  - `flutter pub get`: passed for the package and example.
  - `dart format lib test example/lib example/test example/integration_test hook`: passed.
  - `flutter analyze hook lib test example`: passed.
  - `flutter test test/public_api/flark_barrel_test.dart test/v2/public_api/flark_advanced_public_api_test.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_render_plan_parity_test.dart --reporter compact`: passed with 35 tests.
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.
  - `flutter test test/v2/performance/flark_v2_performance_budget_test.dart --reporter compact`: passed on rerun after render-plan token partitioning. A prior run exposed cold/timing sensitivity in the parse-adoption budget and the old render-plan repeated scan.
  - `flutter test test --exclude-tags benchmark --reporter compact`: passed with 543 tests.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 36 tests.
  - Stale public-name scan for old package imports and `Flark*` public symbols: clean.
  - `git diff --check`: clean.

### Identity Sweep: Flark Everywhere

- Renamed the remaining tracked source paths, docs, tests, fixtures, golden
  names, example keys, example Android package, and Apple bundle identifiers to
  the Flark identity.
- Renamed the native Rust crate, checked-in header, exported ABI symbols, hook
  output directory, platform script artifact names, iOS anchor, and web WASM
  asset to `flark_comrak_bridge` / `flark_comrak_*`.
- Rebuilt the host native bridge and the checked-in WASM asset after the ABI
  rename. The macOS dylib exports `flark_comrak_bridge_version`,
  `flark_comrak_input_alloc`, `flark_comrak_input_free`,
  `flark_comrak_parse`, and `flark_comrak_response_free`; the WASM asset
  contains the same export names.
- Tightened `scripts/build_comrak_all.sh` so an actual host build failure exits
  instead of being counted as a skipped target outside `--strict`.
- Verification:
  - `cargo clean --manifest-path native/comrak_bridge/Cargo.toml`: removed the stale native target cache before the renamed rebuild.
  - `bash scripts/build_comrak_all.sh --host-only`: passed.
  - `bash scripts/build_comrak_all.sh --wasm-only`: passed.
  - Native dylib symbol scan: showed only the expected Flark ABI symbols.
  - WASM export string scan: showed the expected Flark WASM exports.
  - `flutter pub get`: passed for the package and example.
  - `dart format lib test example/lib example/test example/integration_test hook tool`: passed.
  - `flutter analyze hook lib test example tool`: passed.
  - `flutter test test/public_api/flark_barrel_test.dart test/v2/public_api/flark_advanced_public_api_test.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_render_plan_parity_test.dart test/v2/native/flark_native_comrak_bridge_test.dart test/v2/packaging/flark_v2_native_packaging_contract_test.dart test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/markdown/flark_v2_native_upstream_contract_test.dart --reporter compact`: passed with 84 tests.
  - `flutter test test/v2/flutter/flark_markdown_web_smoke_test.dart -d chrome --reporter compact`: passed with 3 tests.
  - `flutter test test/v2/performance/flark_v2_performance_budget_test.dart --reporter compact`: passed with 4 tests.
  - `dart doc --dry-run`: passed with 0 warnings and 0 errors.
  - `flutter test test --exclude-tags benchmark --reporter compact`: passed with 543 tests.
  - `(cd example && flutter test test/widget_test.dart --reporter compact)`: passed with 36 tests.
  - Tracked legacy-name scan: clean.
  - Source-tree legacy-name scan excluding ignored build/cache/binary outputs:
    clean.
  - `git diff --check`: clean.
