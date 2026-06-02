# Flark v2 Execution Log

## 2026-05-02

- Established the v2 operating goal after completing the prior extraction and
  production-readiness goal from the engineering side. Remaining prior-goal
  blockers are owner decisions: license and canonical public URLs.
- Reviewed current standalone package docs and code shape, including
  `docs/production_readiness/execution_plan.md`,
  `docs/production_readiness/audit_2026-05-01.md`,
  `docs/architecture/flark/flark_editor_how_it_works.md`, and
  `docs/architecture/rfc/rfc_017_flark_controller_module_boundaries.md`.
- Confirmed the current v1 architecture is production-hardened but still
  centered on `FlarkController extends TextEditingController`, which should
  not be the v2 center of gravity.
- Researched markdown/spec sources: CommonMark, GitHub Flavored Markdown,
  Comrak, Dart `markdown`, and `flutter_markdown_plus`.
- Researched editor architecture sources: CodeMirror 6, ProseMirror, Lexical,
  Slate, and Milkdown.
- Researched Flutter text input boundaries: `EditableText`, `TextEditingDelta`,
  and `DeltaTextInputClient`.
- Researched Flutter ecosystem alternatives: `super_editor`,
  `appflowy_editor`, and `flutter_quill`.
- Added `docs/architecture/v2/research_matrix_2026-05-02.md`.
- Added `docs/architecture/v2/rewrite_plan_2026-05-02.md`.
- Added `docs/architecture/v2/execution_plan.md`.
- Added this execution log.
- Started Phase 1 by adding the first pure Dart v2 core slice under
  `lib/src/v2/core`.
- Added `FlarkTextBuffer`, `FlarkDocument`, `FlarkSelection`,
  `FlarkSourceRange`, `FlarkSourceOperation`,
  `FlarkTransaction`, and `FlarkEditorState`.
- Added headless v2 tests for text-buffer line indexing, immutable replacement,
  insert/delete/replace transactions, selection mapping, explicit transaction
  selections, atomic multi-operation application, inversion, overlapping
  operation rejection, and the no-Flutter-import boundary.
- Ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2/core --reporter compact`: passed.
- Ran `flutter analyze lib/src/v2 test/v2`: passed after fixing the
  `prefer_initializing_formals` lint.
- Continued Phase 1 by adding typed transaction metadata:
  `FlarkTransactionMetadata` and `FlarkTransactionIntent`.
- Added `FlarkHistoryStack` and `FlarkHistoryEntry` with immutable
  undo/redo batches, redo clearing on new edits, opt-out handling, and adjacent
  undo-group merging.
- Added history tests for undo, redo, grouped undo, redo clearing, and
  `addToHistory: false`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2/core --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added richer source-position mapping tests for multi-operation transactions,
  including selections between operations, selections inside later operations
  after prior deltas, and explicit upstream/downstream insertion-boundary
  mapping.
- Added `docs/architecture/v2/core_invariants_2026-05-02.md` to document the
  current document, selection, operation, transaction, history, and framework
  boundary contracts.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2/core --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `docs/architecture/v2/public_api_sketch_2026-05-02.md` as a draft RFC
  for the eventual headless core, Flutter, and testing public libraries.
- Decided that `FlarkHistoryStack` remains a companion/runtime-owned state
  object rather than a field on `FlarkEditorState`; a future runtime state
  should compose editor state, history, command state, and extension state.
- Started Phase 2 by adding the first command runtime slice:
  `FlarkCommand`, `FlarkCommandContext`,
  `FlarkCommandResult`, `FlarkCommandRegistry`, and command priority
  constants.
- Added command registry tests for unhandled dispatch, priority ordering,
  not-handled fallthrough, rejected-result stop behavior, and transaction
  return/application.
- Added `docs/architecture/v2/command_runtime_2026-05-02.md`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2/core --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `FlarkExtension`, `FlarkExtensionSet`, and duplicate extension
  id validation.
- Added `FlarkCoreEditingCommands.insertText`,
  `FlarkInsertTextPayload`, and `FlarkCoreEditingExtension`.
- Added extension tests for duplicate id rejection and registry composition
  through a source-edit command.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2/core --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added the first headless markdown command slice under `lib/src/v2/markdown`:
  inline style markers, `FlarkMarkdownInlineCommands.toggleInlineStyle`,
  `FlarkToggleInlineStylePayload`, and
  `FlarkMarkdownInlineEditingExtension`.
- Implemented source-level inline style wrapping/unwrapping for selected ranges
  and rejected collapsed selections until active-mark state is designed.
- Added markdown inline command tests for wrap, unwrap, and collapsed-selection
  rejection.
- Expanded the v2 import-boundary test to scan all of `lib/src/v2`, not only
  `lib/src/v2/core`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added inline command handling for selections that include their own wrapping
  markers and added inline-code marker coverage.
- Added `docs/architecture/v2/command_capability_conventions_2026-05-02.md`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Extended `FlarkTransactionMetadata` with optional
  `parseInvalidationRange` and `projectionInvalidationRange`.
- Attached initial invalidation ranges to core insert-text and markdown inline
  style command transactions.
- Added test assertions for command-produced invalidation metadata.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Tightened history recording so source-neutral transactions do not create undo
  entries, and added regression coverage.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Ran wider `flutter analyze lib test`: passed.
- Hardened inline toggling for partial source-marker overlap and escaped
  surrounding markers.
- Added tests for partial selected markers, partial surrounding markers, and
  escaped markers.
- Added `FlarkEditorRuntime`, an immutable headless runtime that composes
  `FlarkEditorState`, `FlarkHistoryStack`, and
  `FlarkCommandRegistry`.
- Added runtime tests for extension command dispatch, rejected-command
  immutability, and undo/redo.
- Added `docs/architecture/v2/runtime_2026-05-02.md`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `FlarkMarkdownBlockEditingExtension` with heading, blockquote,
  bullet list, thematic break, and fenced-code source commands.
- Added selected-line helpers for headless markdown block commands.
- Added block command tests for heading set/change/remove/rejection,
  quote/list toggle, thematic break insertion, and fenced-code insertion over
  collapsed and selected ranges.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added Phase 3 parser protocol skeleton: markdown profiles, parse requests,
  parser capabilities, schema version constant, parse results, block nodes,
  inline tokens, and diagnostics.
- Added parser payload tests for unknown-field preservation, unknown
  block/inline variants, future schema versions, and profile capabilities.
- Added `docs/architecture/v2/parser_protocol_2026-05-02.md`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added first projection core slice: `FlarkHiddenRange`,
  `FlarkCursorMask`, and `FlarkProjection`.
- Added source/display mapping, cursor normalization, and overlapping hidden
  range tests.
- Added `docs/architecture/v2/projection_core_2026-05-02.md`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `FlarkUtf8Utf16Mapper` with ASCII, BMP multibyte, and non-BMP
  surrogate-pair contract tests.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added first render-plan slice: `FlarkRenderPlan`,
  `FlarkRenderBlock`, and `FlarkRenderInlineRun`.
- Added render-plan construction from parse results plus projection, including
  source/display ranges and unknown node preservation.
- Added `docs/architecture/v2/render_plan_2026-05-02.md`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added curated CommonMark/GFM fixture loader tests for the v2 conformance
  harness.
- Re-ran `flutter test test/v2 --reporter compact`: passed with 58 tests.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added parser-provided hidden ranges to `FlarkMarkdownParseResult`,
  including typed hidden range kinds, raw type preservation, attribute maps,
  unknown-field preservation, and forward-compatible unknown kinds.
- Added `FlarkProjection.fromParseResult` so marker hiding and cursor masks
  can be derived from the parser contract instead of adapter-local state.
- Added projection tests for parser-derived hidden ranges and unknown hidden
  range kinds.
- Re-ran `dart format lib/src/v2 test/v2`.
- Re-ran `flutter test test/v2 --reporter compact`: passed with 60 tests.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Tightened render-plan construction so nested container blocks do not duplicate
  inline runs owned by child blocks.
- Updated `FlarkRenderPlan.fromParseResult` to derive projection from parse
  results by default while still accepting an explicit projection override.
- Added render-plan tests for parser-derived projection and deepest-block inline
  ownership.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran focused parse/projection/render tests: passed.
- Added a curated GFM table fixture and upgraded the v2 GFM fixture loader test
  into a coverage contract for autolinks, strikethrough, tables, and task list
  items.
- Ran `flutter test test/v2/markdown/flark_markdown_fixture_loader_test.dart
  --reporter compact`: passed.
- Added `FlarkProjection.projectText` and projection fixtures for escaped
  delimiters, inline strong markers, reference-link inline markers, and hidden
  reference definitions.
- Added `escapeMarker` as a parser/projection hidden range kind.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran focused projection and parser protocol tests: passed.
- Added typed render-plan descriptors for tables, task-list items, fenced code
  blocks, links, and images.
- Added render-plan tests for table column alignments, task checked state, code
  language, and link/image action destinations.
- Ran `flutter test test/v2/render_plan --reporter compact`: passed.
- Added core range helpers for containment, intersection, and union.
- Exposed transaction offset mapping for downstream projection rebasing.
- Added parser ambiguity zones to `FlarkMarkdownParseResult`, including
  typed kinds, preferred affinity, unknown-kind tolerance, and unknown-field
  preservation.
- Added projection ambiguity zones, ambiguity offset normalization, predictive
  projection rebasing through transactions, projection-sensitive edit
  detection, and predicted/authoritative reconciliation.
- Added tests for parser-provided ambiguity zones, prediction rebasing,
  sensitive edit detection, and reconciliation stability/deltas.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran focused core/projection/parser tests: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added parser-derived projection fixtures for table syntax elision, image/media
  accessible-label projection, and raw HTML tag hiding.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2/projection --reporter compact`: passed.
- Added the first Flutter adapter slice:
  `FlarkFlutterController`, a `ChangeNotifier` over headless runtime,
  projection, and render plan.
- Updated the v2 import-boundary test so headless packages remain
  Flutter-free while the adapter can live under `lib/src/v2/flutter`.
- Added controller tests for stale initial render plans, command dispatch,
  projection prediction, stale parse rejection, current parse adoption, and
  undo projection reset behavior.
- Fixed projection range rebasing affinity so insertions at hidden-range
  boundaries are not absorbed into hidden marker spans.
- Tightened projection-sensitive edit detection so collapsed insertions are
  sensitive only when they are strictly inside hidden/ambiguous spans.
- Added `docs/architecture/v2/flutter_adapter_2026-05-02.md`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran focused Flutter/projection/import-boundary tests: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `FlarkTextDeltaAdapter` to map Flutter
  `TextEditingDeltaInsertion`, deletion, replacement, and non-text selection
  updates into typed source transactions.
- Added `FlarkFlutterController.applyTextEditingDelta`, including stale
  `oldText` rejection before source mutation.
- Added delta-adapter tests for insert/delete/replace/selection/stale/invalid
  delta cases and controller tests for applying current deltas.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2/flutter --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `FlarkEditableText`, the first raw-source Flutter editing surface
  over `FlarkFlutterController`.
- The widget keeps Flutter text state synchronized from headless runtime state
  and converts `EditableText.onChanged` updates into minimal source-range
  replacement transactions.
- Added widget tests for editing source text through the v2 controller and
  syncing external controller edits back into `EditableText`.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2/flutter --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `FlarkCommandInvocation`, `FlarkCommandIntent`,
  `FlarkCommandAction`, and `FlarkCommandActions` for Flutter
  `Intent`/`Action` integration over typed v2 commands.
- Added optional shortcut installation to `FlarkEditableText`.
- Added Flutter action tests for invoking markdown inline commands through
  Actions and through the editable widget's installed action scope.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2/flutter --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `FlarkReadOnlyPreview`, a read-only Flutter adapter that consumes
  controller projection/render-plan state instead of reparsing markdown in the
  widget layer.
- Added preview tests for stale projected text fallback, strong inline style
  rendering from the shared render plan, and controller-driven rebuilds.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2/flutter --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added the first read/edit render-plan parity fixture: `FlarkEditableText`
  and `FlarkReadOnlyPreview` share one controller/render-plan source, and
  source edits through the editable surface mark preview render state stale
  until parse adoption.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran `flutter test test/v2/flutter --reporter compact`: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Ran full `flutter test test/v2 --reporter compact`: passed with 95 tests.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added overlay-oriented render-plan query APIs for all blocks, all inline runs,
  link/image action runs, table/task/code descriptor blocks, and display-offset
  lookup.
- Added render-plan query tests covering nested blocks and action descriptors.
- Re-ran `dart format lib/src/v2/render_plan test/v2/render_plan`.
- Ran `flutter test test/v2/render_plan --reporter compact`: passed.
- Added `FlarkRenderOverlayPlan` and `FlarkRenderOverlayTarget` for
  stable overlay targets from link/image/task/table/code render descriptors.
- Added overlay-target tests covering action destinations, checked task state,
  table descriptors, and code-block language.
- Re-ran `dart format lib/src/v2/render_plan test/v2/render_plan`.
- Ran `flutter test test/v2/render_plan --reporter compact`: passed.
- Added projection selection helpers for source-to-display and
  display-to-source selection mapping through hidden ranges.
- Added selection projection tests for marker-hidden ranges and caret
  normalization.
- Re-ran `dart format lib/src/v2/projection test/v2/projection`.
- Ran `flutter test test/v2/projection --reporter compact`: passed.
- Added renderer-neutral text style tokens to render blocks and inline runs.
- Updated `FlarkReadOnlyPreview` to map render style tokens into Flutter
  `TextStyle` instead of switching directly on markdown parser kinds.
- Added render-plan tests for block/inline style tokens and reran preview style
  coverage.
- Re-ran `dart format lib/src/v2 test/v2`.
- Ran focused render-plan and preview tests: passed.
- Re-ran `flutter analyze lib/src/v2 test/v2`: passed.
- Added `docs/architecture/v2/public_library_names_2026-05-02.md`.
- Added experimental public v2 library export at `lib/flark_advanced.dart`
  while keeping v1 as the stable `flark.dart` entry point.
- Added public API smoke coverage that imports
  `package:flark/flark_advanced.dart`.
- Ran `flutter test test/v2/public_api --reporter compact`: passed.
- Ran `flutter analyze lib/flark_advanced.dart lib/src/v2 test/v2`:
  passed.
- Added a v1/v2 implementation switch to the example app.
- v2 mode uses `package:flark/flark_advanced.dart`, the v2
  controller, `FlarkEditableText`, and `FlarkReadOnlyPreview`.
- Added example widget coverage for switching into the v2 surfaces.
- `flutter test example/test --reporter compact` from the repo root failed
  because the example test imports `package:example/main.dart`; reran from
  `example/`, where the package root is correct.
- Ran `flutter test test --reporter compact` in `example/`: passed.
- Ran `flutter analyze` in `example/`: passed.
- Added the first v1/v2 command oracle harness under `test/v2/oracle`.
- Oracle coverage compares public v1 and public v2 APIs for selected strong
  text wrapping, heading level, blockquote toggling, and bullet-list toggling.
- Ran `flutter test test/v2/oracle --reporter compact`: passed.
- Ran `flutter analyze lib/flark_advanced.dart lib/src/v2 test/v2`:
  passed.
- Added performance budget coverage for controller parse-result adoption, so
  the Phase 7 budget lane now covers localized source transactions,
  projection mapping, parse adoption, and render-plan generation.
- Optimized projection source/display mapping with precomputed hidden-prefix
  lengths and display starts, preserving the projection fixture behavior while
  meeting the performance budget.
- Made the existing native Comrak bridge model pure Dart by removing the
  Flutter foundation dependency from `native_comrak_ffi.dart`.
- Added `FlarkNativeComrakParseBackend`, a v2 parser adapter over the
  existing native bridge ABI. The adapter maps native UTF-8 ranges into v2
  source ranges, preserves unknown native block/inline variants, carries native
  diagnostics, and exposes native exclusion ranges through result extensions.
- Added fake-bridge and real-host-library tests for the v2 native Comrak
  adapter.
- Added headless v2 table commands for table insertion, row insertion/deletion,
  column insertion/deletion, fenced-code fallthrough, and source-aligned table
  formatting.
- Added headless v2 link edit context resolution plus insert/apply link
  commands.
- Extended v2 block commands with quote-aware bullet-list toggling and
  task-list checkbox insertion/toggling.
- Extended the v1/v2 oracle harness with table insertion, link insertion, and
  task-list checkbox parity.
- Wired the example app's v2 mode to the public v2 link/table/task command
  extensions and toolbar buttons.
- Re-ran `flutter test test/v2 --reporter compact`: passed with 138 tests.
- Re-ran `flutter analyze lib/flark_advanced.dart lib/src/v2 test/v2`:
  passed.
- Ran `flutter analyze` in `example/`: passed.
- Ran `flutter test test --reporter compact` in `example/`: passed.
- Added `FlarkMarkdownCommandQueries.capabilitiesAtSelection` and
  `FlarkMarkdownCommandCapabilities` for active inline style, heading,
  quote, bullet-list, task-list, and table state.
- Added projection boundary affinity so display offsets at hidden marker
  boundaries can map upstream or downstream.
- Added `FlarkProjectedTextEditAdapter`, a headless projected-edit adapter
  that maps display-text diffs back into source transactions while preserving
  exact source-selection ambiguity when available.
- Added `FlarkFlutterController.applyProjectedTextEdit`,
  `applyProjectedSelection`, and `applySelection`.
- Added `FlarkProjectedEditableText`, a projection-backed Flutter editing
  surface over the same controller/runtime architecture.
- Fixed `FlarkEditableText` so caret movement updates controller selection
  even when text does not change; this prevents toolbar commands from using stale
  source selections.
- Added Flutter selection/accessibility behavior coverage for raw editing,
  projected editing, and read-only preview semantics.
- Added v2 native packaging contract coverage for the hook-owned native bridge
  asset, ABI symbols across Rust/header/iOS anchor, and hook dependencies.
- Added the v2 native packaging contract and native Comrak v2 adapter tests to
  `scripts/verify_native_editor_ci.sh` and `scripts/verify_package_confidence.sh`.
- Added `docs/architecture/v2/public_api_inventory_2026-05-02.md` and a
  barrel-shape public API test for `lib/flark_advanced.dart`.
- Ran `flutter test test/v2/flutter --reporter compact`: passed.
- Ran focused v2 projection/packaging/markdown-native/public-api tests: passed.
- Ran `flutter analyze lib/flark_advanced.dart lib/src/v2 test/v2`:
  passed.
- Ran `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- Ported the v1 markdown support matrix into
  `docs/architecture/v2/markdown_support_matrix_2026-05-02.md`, explicitly
  separating supported v2 core behavior from partial widget/live-editing parity.
- Expanded v1/v2 oracle coverage for emphasis, inline code, horizontal-rule
  insertion, and fenced-code insertion.
- Fixed two oracle-discovered command parity gaps: v2 horizontal-rule insertion
  now preserves the blank line used by v1 after non-empty paragraphs, and v2
  empty fence insertion no longer adds an extra trailing newline.
- Ran focused v2 block-command and oracle tests: passed.
- Added v2 ordered-list command support, ordered-list active capability state,
  public API smoke coverage, and example toolbar wiring.
- Ran focused v2 block-command/capability/public-api tests: passed.
- Ran `flutter analyze lib/flark_advanced.dart lib/src/v2 test/v2`:
  passed.
- Ran `flutter analyze` in `example/`: passed.
- Added `FlarkMarkdownInputCommands.handleEnter` and
  `FlarkMarkdownInputCommands.handleBackspace` coverage for empty heading
  exit, blockquote/list continuation, structural list/task Backspace, indented
  code Enter/Backspace, and basic fenced-code indentation.
- Wired both `FlarkEditableText` and `FlarkProjectedEditableText` so
  platform Enter/Backspace changes dispatch through the same typed headless
  markdown input policy before falling back to generic source/display diffs.
- Added render-plan-backed `FlarkRenderPlanOverlayControls`, a
  design-system-neutral Flutter adapter over link, image, task, table, and code
  overlay targets.
- Added `FlarkParseScheduler`, which listens to a v2 Flutter controller,
  debounces parser requests, applies fresh parse results, and reparses the
  latest revision after stale in-flight results complete.
- Wired the example app's v2 mode to the native parse scheduler when the native
  bridge is available and added v2 render-plan overlay controls to the preview
  pane.
- Narrowed `lib/flark_advanced.dart` from wildcard module exports to
  explicit `show` exports for the experimental public surface.
- Ran focused input, projected/raw widget, overlay-control, parse-scheduler,
  and public API tests: passed.
- Ran `flutter test test/v2 --reporter compact`: passed with 183 tests.
- Ran `flutter analyze lib/flark_advanced.dart lib/src/v2 test/v2`:
  passed.
- Ran `flutter analyze` in `example/`: passed.
- Ran `flutter test test --reporter compact` in `example/`: passed.
- Ran `dart doc --dry-run`: passed with 0 warnings and 0 errors.
- Ran `./scripts/verify_package_confidence.sh`: passed.
- Extended the native Comrak JSON payload contract so inline tokens can carry
  payload metadata.
- Updated the Rust native bridge to emit link destinations/titles/labels, image
  source/title/alt metadata, list-item checked state, table row blocks, and
  table cell ranges.
- Updated `NativeComrakPayloadCodec`, `NativeComrakInlineToken`, and the v2
  native parse adapter to preserve this metadata into v2 parse attributes.
- Rebuilt host, iOS, and Android Comrak bridge artifacts with
  `./scripts/build_comrak_all.sh`.
- Ran focused native payload and v2 native adapter tests: passed.
- Ran `flutter analyze hook lib test`: passed.
- Re-ran `flutter test test/v2 --reporter compact`: passed with 185 tests.
- Added native raw HTML block/inline emission, v2 raw-HTML hidden ranges, and
  reference-definition hidden-range derivation for native parse results.
- Kept v1 native backend compatibility by ignoring known v2-only supplemental
  block detail instead of treating it as a parse error.
- Rebuilt host, iOS, and Android Comrak bridge artifacts again with
  `./scripts/build_comrak_all.sh`.
- Ran focused native payload, upstream parity, and v2 native adapter tests:
  passed.
- Re-ran `./scripts/verify_package_confidence.sh`: passed.

Next action:

- Promoted the v2 API through the stable `flark.dart` barrel while
  keeping v1 symbols available as compatibility APIs.
- Added `FlarkMarkdownEditingExtensions.standard()` as the default v2
  markdown editing extension set.
- Added promoted high-level v2 widgets: `FlarkMarkdownEditor` and
  `FlarkMarkdownPreview`.
- Updated the example app so v2 mode imports only the stable package barrel and
  uses the standard v2 markdown extension set.
- Added stable-barrel and widget tests covering the promoted v2 editor,
  preview, controller, parser wiring, and native backend surface.
- Re-ran `flutter test test/v2 --reporter compact`: passed with 188 tests.
- Ran focused stable/v2 public API and promoted surface tests: passed.
- Ran `flutter analyze lib/flark.dart lib/flark_advanced.dart
  lib/src/v2 test/v2 test/public_api/flark_editor_barrel_test.dart`:
  passed.
- Ran `flutter analyze` in `example/`: passed.
- Added `docs/architecture/v2/migration_guide_2026-05-02.md` and updated the
  README plus v2 public API inventory for stable-barrel adoption.
- Fixed two v1 native-backend compatibility regressions exposed by the release
  gate after richer native v2 payloads landed: reference-definition marker
  prefixes are restored as v1 marker ranges, and native link/image/autolink
  inline tokens are narrowed to the visible v1 style range instead of the full
  markdown source range.
- Hardened the reference-definition rendering test so it asserts hidden/visible
  behavior instead of exact leaf-span segmentation.
- Hardened the benchmark lane so renderer cache benchmarks isolate the
  synchronous renderer/cache path from background parser timing.
- Ran `flutter test test --reporter compact`: passed with 774 tests.
- Ran `./scripts/verify_benchmark_lane.sh`: passed.
- Ran `./scripts/verify_release.sh`: passed end to end.

Next action:

- External publishing remains blocked only on owner decisions recorded in the
  release checklist: license, canonical repository/issue/documentation URLs,
  screenshots, and removal/update of `publish_to: none`.

Phase 9 quality hardening:

- Added a readable v2 visual golden at
  `test/v2/flutter/flark_v2_visual_golden_test.dart` with the generated
  baseline in `test/v2/flutter/goldens/flark_v2_surfaces.png`. The golden
  covers the promoted source editor, projected editor, preview, and
  render-plan overlay controls side by side.
- Added the visual golden to `scripts/verify_package_confidence.sh`.
- Added no-throw native backend probing via
  `FlarkNativeComrakParseBackend.preflight()` and `.tryLoad()`.
- Updated `FlarkMarkdownEditor` and `FlarkMarkdownPreview` so they
  automatically use native Comrak when available, degrade to source-only
  behavior when unavailable, and expose `autoLoadNativeParseBackend` for
  deterministic/custom-backend contexts.
- Added `scripts/verify_web_adapter_ci.sh`, wired it into
  `scripts/verify_release.sh`, and added
  `test/v2/flutter/flark_markdown_web_smoke_test.dart` for Chrome
  source-only fallback coverage.
- Added `test/v2/markdown/flark_v2_native_upstream_contract_test.dart`,
  which runs native Comrak output through v2 projection and render-plan
  generation across upstream CommonMark/GFM fixtures.
- Fixed invalid reference-definition-looking paragraph handling in the v2
  native adapter: reference-definition hidden ranges are now dropped when they
  overlap native parser-owned block ranges.
- Added render-plan extension hooks through `FlarkRenderPlanExtension` and
  `FlarkRenderPlanContext`; `FlarkFlutterController.applyParseResult`
  now applies render-plan extensions from the runtime extension set.
- Added `FlarkReadOnlyPreview.blockBuilder` and
  `FlarkPreviewBlockWidgetBuilder` for custom block rendering.
- Dogfooded shared-controller v2 parsing in the example by disabling duplicate
  widget auto-loading where the example owns a `FlarkParseScheduler`, and
  added custom read-only code-block rendering through `blockBuilder`.
- Updated README, migration guide, public API inventory, execution plan, and
  release checklist for the new visual/backend/web/extensibility contracts.
- Verification:
  - `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_markdown_surface_test.dart test/v2/markdown/flark_native_comrak_parse_backend_test.dart --reporter compact`: passed.
  - `./scripts/verify_web_adapter_ci.sh`: passed.
  - `flutter test test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/markdown/flark_v2_native_upstream_contract_test.dart --reporter compact`: passed; v2 upstream contract reported 644/644 core and 662/662 GFM compared cases passing after registered skips.
  - Focused render-plan, controller, preview, and public API tests: passed.
  - `flutter analyze` for focused changed v2 API files and tests: passed.
  - `flutter analyze` in `example/`: passed.
  - `flutter test test --reporter compact` in `example/`: passed.

Next action:

- Run the full v2 test lane, package analysis/docs, native CI, full package
  test suite, web gate, benchmark lane, and release gate after this hardening
  pass.

Phase 9 release gate result:

- Ran `./scripts/verify_release.sh`: passed end to end after the Phase 9
  visual/backend/web/extensibility hardening pass.
- The release gate covered dependency resolution, package analysis, docs dry
  run, example analysis/tests, Chrome web smoke coverage, host native Comrak
  build, native editor CI, full package tests, and benchmark budgets.
- Native CI reported 644/644 core and 662/662 GFM compared upstream fixture
  cases passing for both the v1 native contract and the v2 projection/render
  plan contract after registered skips.
- Full package tests passed with 784 tests.
- Benchmark budgets passed through `./scripts/verify_benchmark_lane.sh`.
- The only remaining release blockers are non-engineering owner decisions:
  license, canonical URLs, pub publishing intent, and external screenshots.

Phase 10 comprehensive visual regression hardening:

- Raised the v2 visual baseline from one broad smoke PNG to a curated six-PNG
  suite covering overview surfaces, inline styling/wrapping, code fences,
  blockquotes, tasks/tables/overlays, and compact mixed markdown.
- Refactored `test/v2/flutter/flark_v2_visual_golden_test.dart` around a
  deterministic fixture parser and per-scenario viewport sizing, so narrow
  wrapping cases are actually captured at narrow widths.
- Added default v2 read-only preview rendering for code blocks, blockquotes,
  task checkboxes, and simple table grids so goldens exercise meaningful visual
  regions rather than plain text only.
- Added widget assertions for default code-block, blockquote, task checkbox,
  and table rendering in `test/v2/flutter/flark_read_only_preview_test.dart`.
- Added `test/v2/flutter/goldens/README.md` to document which visual scenarios
  are covered by PNGs and which semantics should stay in code assertions.
- Verification:
  - `flutter test --update-goldens test/v2/flutter/flark_v2_visual_golden_test.dart --reporter compact`: passed and regenerated six PNG baselines.
  - `flutter test test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_v2_visual_golden_test.dart --reporter compact`: passed.
  - `flutter analyze lib/src/v2/flutter/flark_read_only_preview.dart test/v2/flutter/flark_read_only_preview_test.dart test/v2/flutter/flark_v2_visual_golden_test.dart`: passed.
  - `./scripts/verify_release.sh`: passed end to end with 793 package tests, native upstream contracts, web smoke coverage, docs dry run, example tests, and benchmark budgets.

Phase 11 community-library surface and packaging hardening:

- Split the public API into intent-based barrels:
  - `lib/flark.dart` is now the promoted v2 app API.
  - `lib/flark_core.dart` exposes the headless Dart runtime,
    parser DTOs, projection, and render-plan APIs without Flutter widgets.
  - `lib/flark_advanced.dart` remains the complete advanced v2 surface.
  - `lib/flark_editor_legacy.dart` owns the v1 compatibility API.
- Updated v1 tests and the example app to import legacy symbols explicitly from
  `flark_editor_legacy.dart`.
- Added typed `FlarkFlutterController.events` so extensions can react to
  projection prediction, parser adoption, selection-only changes, undo, redo,
  and generic runtime changes without diffing `ChangeNotifier` state.
- Avoided repeated operation sorting in `FlarkTransaction.mapSelection`.
- Removed the default `google_fonts` dependency. Package defaults now use a
  generic monospace family; consumers can still override typography through
  widget styles and markdown/editor themes.
- Kept `highlight` in the default dependency set for now because it remains tied
  to the legacy v1 highlighting pipeline. Removing it cleanly requires either a
  dedicated legacy/highlight package split or a broader v1 compatibility
  retirement.
- Added `docs/architecture/v2/web_parser_strategy_2026-05-03.md` to define the
  first-party web target as a source-position-preserving Comrak WASM backend,
  and to reject a weak default HTML-AST fallback for projected editing.
- Rewrote the README opening around quickstart, v2 differentiators, visual
  goldens, parser platform model, shared editor/preview usage, and barrel
  choices.
- Expanded `CHANGELOG.md` with the v2 architecture, public surface split,
  controller events, visual testing, dependency cleanup, and conformance gates.
- Verification:
  - Focused public API, v1/v2 oracle, controller event, transaction, promoted
    surface, web smoke, read-only preview, and visual golden tests passed.
  - `flutter analyze` at the package root passed.
  - `flutter analyze` in `example/` passed.
  - `flutter test test --reporter compact` in `example/` passed.
  - `dart doc --dry-run` initially exposed ambiguous re-export warnings between
    `flark_core.dart` and `flark_advanced.dart`; added
    canonical dartdoc directives to the full v2 barrel and reran with 0
    warnings and 0 errors.
  - `./scripts/verify_release.sh` passed end to end after the doc fix, covering
    dependency resolution, package analysis, docs dry run, example
    analysis/tests, web smoke, host Comrak build, native editor CI, full package
    tests, upstream CommonMark/GFM v1/v2 contracts, visual goldens, and enforced
    benchmark budgets.

Phase 12 v2-only cleanup and interactive validation:

- Removed the transition-only v1 implementation tree:
  - deleted `lib/widgets`, `lib/src/widgets`, `lib/theme`, and
    `lib/src/helpers`;
  - deleted `lib/flark_editor_legacy.dart`;
  - deleted legacy widget/engine tests, the temporary v1/v2 oracle suite, and
    the old v1 benchmark lane.
- Moved the native Comrak Dart bridge from the old widget namespace to
  `lib/src/v2/native`, with a `native.dart` barrel for the full v2 API.
- Updated `hook/build.dart` so the code asset name follows the v2 native bridge
  path: `src/v2/native/native_comrak_ffi.dart`.
- Updated public barrels so `flark.dart` and
  `flark_advanced.dart` are v2-only and no longer reference legacy
  symbols.
- Removed the default `highlight` dependency along with the old renderer.
- Rebuilt `example/lib/main.dart` as a v2-only integration harness with:
  source/projected mode switching, editor and preview panes, toolbar commands,
  native preflight status, undo/redo, and scenario buttons for mixed inline,
  block, code-fence, task, quote, and table cases.
- Updated verification scripts to use v2-only tests and v2 performance budgets.
- Verification in progress:
  - `dart format lib test example/lib example/test hook`: passed.
  - `flutter pub get`: passed and removed `highlight` from the lockfile.
  - `flutter pub get` in `example/`: passed.
  - `flutter analyze hook lib test`: passed.
  - `flutter analyze` in `example/`: passed.

Phase 12 interactive example validation:

- Ran the rebuilt example on the booted iOS simulator
  `C880BCBA-DC57-4AB9-87DA-50A44357BC40`.
- Confirmed the example loaded the packaged native Comrak backend and displayed
  the `Comrak` status chip.
- Manual visual inspection exposed a real editor contrast defect: passing a
  partial `TextStyle` to the v2 editables discarded the ambient text color and
  rendered the editor text nearly white on the pale example surface.
- Fixed the issue in `FlarkEditableText` and
  `FlarkProjectedEditableText` by merging partial widget styles with
  `DefaultTextStyle.of(context).style`.
- Added focused regression coverage for partial-style merging in source and
  projected editable widgets.
- Added `example/integration_test/markdown_flow_test.dart` so the example is
  now driven as a real app flow on a simulator. The flow covers native parser
  availability, projected/source mode switching, user text replacement,
  inline strong/emphasis/code, links, blockquotes, code fences, tasks, tables,
  scenario loading, and parser adoption events.
- The first integration run exposed a projection prediction defect during
  full-document scenario replacement from projected mode. Hidden ranges fully
  replaced by a transaction could map to reversed ranges before authoritative
  parsing arrived.
- Fixed full-document projection prediction by collapsing replaced hidden
  ranges out of the predicted projection and added a projection regression
  test.
- Verification:
  - `flutter test test/v2/flutter/flark_editable_text_test.dart test/v2/flutter/flark_projected_editable_text_test.dart --reporter compact`: passed.
  - `flutter test test/v2/projection/flark_projection_test.dart --reporter compact`: passed.
  - `flutter analyze` in `example/`: passed.
  - `flutter test test --exclude-tags benchmark --reporter compact` in
    `example/`: passed.
  - `flutter test integration_test/markdown_flow_test.dart -d C880BCBA-DC57-4AB9-87DA-50A44357BC40 --reporter compact`: passed.
  - `./scripts/verify_release.sh`: passed end to end after the interactive
    validation fixes. The gate covered package and example dependency
    resolution, package analysis, docs dry run with 0 warnings/errors, example
    analysis/tests, web smoke coverage, host Comrak build, native editor CI,
    v2 upstream CommonMark/GFM contracts at 644/644 and 662/662 compared
    cases, the full package test suite, and enforced benchmark budgets.

Phase 13 macOS desktop dogfood app:

- Generated the macOS platform target under `example/macos` with
  `flutter create --platforms macos .`.
- Reworked `example/lib/main.dart` from a mobile-shaped toolbar into a desktop
  workbench:
  - left document-scenario sidebar;
  - compact command bar for source/projected mode, undo/redo, and Markdown
    commands;
  - split editor/preview panes;
  - native parser and event status surfaces;
  - compact fallback layout for mobile/integration test devices.
- Updated the macOS shell to launch as `Flark Markdown` with a 1280x840
  initial window and 1040x720 minimum size.
- Manual macOS run initially exposed that the native-assets build bundled
  `flark_comrak_bridge.framework`, but the FFI loader only checked the
  executable directory and local package target paths.
- Fixed the macOS native loader to also probe
  `Contents/Frameworks/flark_comrak_bridge.framework/Versions/A/flark_comrak_bridge`
  and the framework symlink path.
- After hot restart, the running macOS app reported `Comrak active`, adopted
  parser output, displayed projected editor text, and rendered the preview
  with headings, blockquotes, task checkboxes, code blocks, and table cells.
- Tightened example tests after the desktop UI added duplicate toolbar/sidebar
  icons and an extra top-bar revision pill.
- Verification:
  - `flutter analyze` in `example/`: passed.
  - `flutter test test --reporter compact` in `example/`: passed.
  - `flutter test test/v2/native/flark_native_comrak_bridge_test.dart --reporter compact`: passed.
  - `flutter run -d macos`: built and launched
    `build/macos/Build/Products/Debug/Flark Markdown.app`.
  - `flutter test integration_test/markdown_flow_test.dart -d macos --reporter compact`: passed.

Phase 13 scratch document follow-up:

- Added a `Scratch` document scenario to the example app so manual dogfooding
  is not limited to prefilled documents.
- Selecting `Scratch` clears the markdown source, switches the editor to source
  mode, and requests editor focus so the user can immediately type freeform
  Markdown.
- Added `expands` support to the source, projected, and high-level Markdown
  editor widgets, then enabled it in the example so the whole editor pane is a
  document-sized interaction target rather than only the rendered text bounds.
- Added an example-side pointer focus bridge around the editor pane so tapping
  empty document space requests editor focus without taking over text selection
  inside `EditableText`.
- Extended the shared example integration flow to verify that Scratch starts
  empty, enters source mode, accepts typed Markdown, and still projects the
  typed content into the preview.
- Added focused widget coverage for expanded source/projected editables and
  the example Scratch flow, including blank source state, full-pane sizing,
  focus after tapping the document surface, and typed Markdown.
- Verification:
  - `flutter analyze`: passed.
  - `flutter test test/v2/flutter/flark_editable_text_test.dart test/v2/flutter/flark_projected_editable_text_test.dart test/v2/flutter/flark_markdown_surface_test.dart --reporter compact`: passed.
  - `flutter analyze` in `example/`: passed.
  - `flutter test test --reporter compact` in `example/`: passed.
  - `flutter test integration_test/markdown_flow_test.dart -d macos --reporter compact`: passed.
  - `flutter build macos --debug` in `example/`: passed and rebuilt the normal
    standalone `Flark Markdown.app` after the integration-test host run.

Phase 13 rendered live mode follow-up:

- Promoted parsing ownership in the example app from per-widget parser
  schedulers to one screen-level `FlarkParseScheduler` attached to the
  shared `FlarkFlutterController`.
- Switched the preview pane from standalone `FlarkMarkdownPreview` to
  `FlarkReadOnlyPreview(controller: ...)` so editor, preview, and rendered
  mode all consume the same live projection/render-plan state.
- Added a third workbench mode, `Rendered`, beside `Source` and `Projected`.
  `Projected` remains text editing with hidden Markdown markers; `Rendered`
  now shows the actual render-plan surface that paints code fences, blockquotes,
  task checkboxes, and tables.
- After dogfooding, clarified the mode model: `Rendered` is intentionally
  read-only preview, not rendered-in-place WYSIWYG editing. Added `Live Edit`
  as the editable source-plus-rendered-output mode and made Scratch open there
  by default.
- Adjusted the responsive workbench layout so `Live Edit` and `Projected`
  present an editable surface beside the rendered preview, `Source` presents a
  focused raw editor, and `Rendered` presents a single full-width read-only
  render surface instead of duplicating preview panes.
- Extended the example widget and macOS integration coverage so the rendered
  mode is present, replaces the editable text surface, and renders code fence
  and blockquote visual blocks from typed Scratch Markdown.
- Verification:
  - `flutter analyze`: passed.
  - `flutter analyze` in `example/`: passed.
  - `flutter test test --reporter compact` in `example/`: passed.
  - `flutter test integration_test/markdown_flow_test.dart -d macos --reporter compact`: passed.

Phase 14 live rendered editing surface:

- Re-checked the editing architecture against official Flutter APIs and
  editor-decoration systems:
  - Flutter `TextEditingController.buildTextSpan` is the intended hook for
    changing editable text appearance while preserving composing underlines.
  - Flutter `EditableText` remains the right low-level widget for selection,
    platform text input, caret visibility, and scroll-controller integration.
  - CodeMirror and ProseMirror both reinforce decorations as view data layered
    over canonical editor state, with mapped/persistent decoration sets for
    efficient updates.
- Refactored `FlarkProjectedEditableText` around one shared projected
  editing host. Projected and live-rendered editing now share source/display
  edit mapping, projected selection mapping, Enter/Backspace Markdown input
  policies, focus ownership, shortcuts/actions, and editable scroll wiring.
- Added `FlarkLiveRenderedEditableText`, which edits projected text but
  builds styled editable `TextSpan` segments from the current render plan:
  headings, strong, emphasis, inline code, strikethrough, links, code-block
  text, and blockquote text are styled without reparsing Markdown in Flutter.
- Added a render-plan-backed block-decoration painter behind live editable
  text for code-fence backgrounds/borders and blockquote rail/background
  chrome. The painter uses the same projected display offsets as the text
  surface and shares the `EditableText` scroll controller.
- Cached live render segmentation per display text/render-plan identity so
  focus, cursor, and parent-layout rebuilds do not recompute styled span
  ranges unnecessarily.
- Promoted `FlarkMarkdownEditingMode.liveRendered` through the high-level
  editor and public barrels. The example app's `Live Edit` mode and `Scratch`
  scenario now use rendered-in-place editing beside the shared live preview.
- Fixed controller render-plan continuity:
  - selection-only transactions now keep the authoritative render plan instead
    of marking it stale;
  - document edits predict render-plan source/display ranges through the same
    transaction/projection mapping used by projected editing, so inline styles
    and block chrome do not flash off during the parse debounce window.
- Added focused coverage:
  - live rendered editable widget tests for projected source editing, inline
    strong/emphasis/code styling, predictive style retention after edits, block
    chrome presence, and high-level editor mode routing;
  - controller tests for selection-only render-plan retention and predictive
    render-plan mapping;
  - public API tests for the promoted live rendered widget;
  - `flark_v2_live_rendered_editing.png`, a dedicated visual golden for
    rendered-in-place editing.
- Updated README and golden inventory docs to describe live rendered editing.
- Verification:
  - `flutter analyze`: passed.
  - `flutter test test/v2/flutter/flark_flutter_controller_test.dart test/v2/flutter/flark_projected_editable_text_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_markdown_surface_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart --update-goldens --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart test/public_api/flark_editor_barrel_test.dart --reporter compact`: passed.
  - `flutter analyze` in `example/`: passed.
  - `flutter test test --reporter compact` in `example/`: passed.
  - `flutter test integration_test/markdown_flow_test.dart -d macos --reporter compact` in `example/`: passed.

Phase 16 quality audit and release boundary:

- Assessed the current V2 package against the explicit goal of being a
  best-in-class live Markdown editor and read-only previewer.
- Added `docs/architecture/v2/quality_audit_2026-05-05.md` with a
  prompt-to-artifact checklist mapping read-only preview, render-as-you-type
  editing, source-first architecture, consumer API quality, visual quality,
  performance, and release readiness to concrete repo artifacts and command
  evidence.
- Updated the V2 execution plan current phase and added Phase 16 audit tasks.
- Updated the markdown support matrix so task-list, fenced-code, table, and
  blockquote live-editing status reflects the Phase 15 block-widget work
  instead of older partial status.
- Refreshed older V2 architecture notes whose "next work" sections still
  described already-completed projection, render-plan, and Flutter-adapter
  parity work.
- Verification:
  - `flutter analyze hook lib test`: passed.
  - `flutter test test --exclude-tags benchmark --reporter compact`: passed.
  - `./scripts/verify_benchmark_lane.sh`: passed.
  - `./scripts/verify_release.sh`: passed end to end, including dependency
    resolution, package/example analysis, dartdoc dry run with 0 warnings and
    0 errors, example tests, web smoke coverage, host native Comrak build,
    native editor CI, full package tests, visual goldens, and benchmark
    budgets.

Post-audit prioritized hardening, slice 1: Dart fallback parser backend:

- Added `FlarkDartMarkdownParseBackend`, a pure Dart
  `FlarkMarkdownParseBackend` implementation for web and unsupported native
  environments where FFI Comrak cannot load.
- The fallback is source-position preserving and emits parser DTOs, hidden
  ranges, projection-compatible marker elision, render-plan blocks, link/image
  action descriptors, task metadata, code-fence metadata, and table alignment
  metadata for common Markdown structures.
- Promoted the fallback through the public barrels and wired
  `FlarkMarkdownEditor` / `FlarkMarkdownPreview` to use it after native
  backend probing fails.
- Kept the web parser strategy honest: native Comrak remains authoritative, the
  Dart backend is a useful fallback rather than a full conformance claim, and
  Comrak WASM remains the full-conformance web target.
- Verification:
  - `flutter analyze lib/src/v2/markdown/parse/flark_dart_markdown_parse_backend.dart lib/src/v2/flutter/flark_markdown_editor.dart lib/src/v2/flutter/flark_markdown_preview.dart lib/flark.dart lib/flark_advanced.dart test/v2/markdown/flark_dart_markdown_parse_backend_test.dart test/v2/flutter/flark_markdown_web_smoke_test.dart`: passed.
  - `flutter test test/v2/markdown/flark_dart_markdown_parse_backend_test.dart test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test test --exclude-tags benchmark --reporter compact`: passed.
  - `flutter build macos --debug` in `example/`: passed.
  - `flutter test test --exclude-tags benchmark --reporter compact`: passed.

Phase 15 editable block widgets:

- Treated true block-widget editing as part of the live rendered editor goal,
  not an example-only follow-up.
- Re-checked the architecture against the same external editor constraints:
  Flutter `EditableText` should still own platform text input for editable text
  regions, while block widgets should be view decorations driven by canonical
  editor state; CodeMirror's block decorations and ProseMirror's node views
  both reinforce that widgets must map back to document transactions rather
  than mutate a parallel rendered model.
- Reworked `FlarkLiveRenderedEditableText` so parsed documents use a
  render-plan-backed block editor:
  - ordinary blocks, headings, and blockquotes are block-local editable text
    surfaces that still apply projected text edits back to canonical Markdown;
  - parser-unavailable documents still fall back to the previous single
    projected editable host, so source-only/web fallback remains usable.
- Added interactive task-list block widgets. Tapping the checkbox replaces the
  source marker check character (`[ ]`/`[x]`) through a normal
  `FlarkTransaction`.
- Added editable fenced-code block widgets. The widget shows language metadata
  and an editable code body while source edits preserve the opener, language
  info string, and closing fence through projected edit mapping.
- Added editable GFM table block widgets. The table renderer parses source
  table rows/cells into precise source ranges, skips the separator row for
  editing, aligns cells from the render-plan descriptor, and writes cell edits
  back with source-range transactions.
- Preserved inline render-plan styling inside block-local editors, including
  headings, strong/emphasis/inline-code/link styles, and composing underline
  behavior.
- Fixed a layout bug in the first block-editor pass where a `LayoutBuilder`
  closure captured the reassigned editor variable and recursively returned
  itself. The block editor now keeps the column content separate from the
  scroll/constraint wrapper.
- Fixed table-widget robustness after the macOS integration flow exposed
  irregular source rows from real parser output. Editable table rows are now
  normalized to a rectangular grid with missing cells represented by collapsed
  source ranges.
- Updated example tests to read text across block-local editable surfaces
  instead of assuming live edit mode always has a single `EditableText`.
- Updated focused live-rendered tests to assert the new behavior:
  - code fence widgets edit only the code body and preserve fences;
  - task checkbox widgets toggle canonical Markdown;
  - table-cell widgets edit the Markdown table cell source range.
  - irregular table rows are padded visually and missing cells can be edited
    into real Markdown cells.
- Refreshed `flark_v2_live_rendered_editing.png`; the baseline now shows
  task checkboxes, a language-labelled code block widget, and a rendered table
  grid inside the editable live surface.
- Verification:
  - `flutter analyze`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_projected_editable_text_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_markdown_surface_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart test/public_api/flark_editor_barrel_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart --update-goldens --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart --reporter compact`: passed.
  - `flutter analyze` in `example/`: passed.
  - `flutter test test --reporter compact` in `example/`: passed.
  - `flutter test integration_test/markdown_flow_test.dart -d macos --reporter compact` in `example/`: passed.

Post-audit prioritized hardening, slices 2-4:

- Added Tab and Shift-Tab handling to live-rendered code-block widgets. The
  widget now applies code-body indentation/outdent operations through normal
  `FlarkTransaction`s and preserves the surrounding fence opener/info and
  closer.
- Added widget coverage for code-block Tab indentation and Shift-Tab outdent in
  `FlarkLiveRenderedEditableText`.
- Extended `FlarkDartMarkdownParseBackend` so full reference links,
  collapsed reference links, shortcut reference links, and reference-style
  images resolve through collected reference definitions. The fallback now hides
  reference definition lines and emits the same render-plan action metadata
  shape as inline links/images.
- Added fallback parser coverage proving reference-style link/image resolution,
  hidden reference definitions, projected text, and render-plan link/image
  action descriptors.
- Added a default read-only preview image card for image action runs. The card
  displays the accessible label, destination, and title from the render plan
  without introducing an image loading or network policy.
- Added preview widget coverage for default image cards.
- Fixed a web-only projection validation bug exposed by the Dart fallback
  parser. `_sortedHiddenRanges` no longer uses `1 << 62` as a sentinel maximum
  because Dart-to-JS bit shifts are 32-bit; it now validates range shape
  directly before text-length-specific validation.
- Updated the support matrix, render-plan note, quality audit, and execution
  plan so completed post-audit hardening is no longer listed as an open gap.
- Verification:
  - `flutter analyze lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed.
  - `flutter analyze lib/src/v2/markdown/parse/flark_dart_markdown_parse_backend.dart test/v2/markdown/flark_dart_markdown_parse_backend_test.dart`: passed.
  - `flutter test test/v2/markdown/flark_dart_markdown_parse_backend_test.dart --reporter compact`: passed.
  - `flutter test test/v2/markdown/flark_dart_markdown_parse_backend_test.dart --platform chrome --reporter compact`: passed.
  - `flutter analyze lib/src/v2/flutter/flark_read_only_preview.dart test/v2/flutter/flark_read_only_preview_test.dart`: passed.
  - `flutter test test/v2/flutter/flark_read_only_preview_test.dart --reporter compact`: passed.
  - `./scripts/verify_web_adapter_ci.sh`: passed.
  - `flutter analyze hook lib test`: passed.
  - `flutter test test --exclude-tags benchmark --reporter compact`: passed.
  - `./scripts/verify_benchmark_lane.sh`: passed.
  - `./scripts/verify_release.sh`: passed end to end, including dartdoc dry
    run, example analysis/tests, Chrome web smoke coverage, host native Comrak
    build, native editor CI, full package tests, visual goldens, and benchmark
    budgets.

Comrak WASM web backend:

- Confirmed the existing Rust Comrak bridge is viable for
  `wasm32-unknown-unknown` and added browser-owned input allocation/free ABI
  exports so web code can copy Markdown bytes into WASM memory safely.
- Fixed response payload ownership in the bridge ABI by moving payload bytes
  through boxed slices before `flark_comrak_response_free` reconstructs
  them. The previous Vec pointer/length-only ownership shape could trap under
  WASM when capacity differed from length.
- Added `scripts/build_comrak_wasm.sh` and wired `scripts/build_comrak_all.sh`
  with `--wasm-only` / `--skip-wasm`. The build stages
  `lib/assets/wasm/flark_comrak_bridge.wasm`.
- Added a browser implementation of the v2 native bridge factory. It resolves
  the packaged WASM asset through Flutter web asset URLs, uses Dart JS interop
  to `fetch` and instantiate WebAssembly, calls the exported C ABI functions,
  and decodes the same JSON payload schema as the native FFI path.
- Added Chrome coverage proving `FlarkNativeComrakParseBackend.tryLoad()`
  reaches Comrak WASM on web, parses GFM tables/strong text, and feeds promoted
  editor rendering with a Comrak-produced render plan.
- Updated packaging contracts, README, native bridge docs, and this web parser
  strategy to describe Comrak WASM as shipped rather than planned.
- Verification:
  - `./scripts/build_comrak_wasm.sh`: passed.
  - `flutter analyze lib/src/v2/native/native_comrak_bridge_factory_web.dart test/v2/flutter/flark_markdown_web_smoke_test.dart test/v2/packaging/flark_v2_native_packaging_contract_test.dart`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test test/v2/packaging/flark_v2_native_packaging_contract_test.dart --reporter compact`: passed.
  - `flutter test test/v2/native/flark_native_comrak_bridge_test.dart --reporter compact`: passed.
  - `flutter analyze hook lib test`: passed.
  - `./scripts/verify_web_adapter_ci.sh`: passed.
  - `./scripts/build_comrak_all.sh --wasm-only`: passed.
  - `rustup run stable cargo test --manifest-path native/comrak_bridge/Cargo.toml`: passed.
  - `flutter test test/v2/markdown/flark_v2_native_upstream_contract_test.dart --reporter compact`: passed.

Comrak-only parser requirement and Markdown coverage matrix:

- Removed stale fallback-parser assumptions from README, migration, public API,
  quality audit, support matrix, execution plan, and web parser strategy docs.
  The promoted widgets now require the packaged Comrak backend by default:
  FFI on native targets and WASM on web.
- Added `docs/architecture/v2/markdown_test_matrix_2026-05-08.md` as the
  coverage contract. It maps each Markdown feature to parser,
  projection/render-plan, command, keyboard, preview, live-edit, web, and
  example lanes.
- Added `test/v2/markdown/flark_markdown_feature_matrix_test.dart`, an
  executable matrix that runs real Comrak output through projection and
  render-plan construction for ATX/setext headings, quotes, lists, tasks,
  code blocks, inline styles, escaped delimiters, links/autolinks,
  strikethrough, images, reference links, thematic breaks, tables, and raw
  HTML.
- The matrix exposed native bridge gaps and adjacent parser/projection risks:
  - GFM strikethrough did not emit an inline token or hide `~~` markers.
  - Escaped delimiter backslashes were not projected away.
  - The old marker scanner could pair delimiter-looking characters across
    link destinations and nested image links, creating overlapping hidden
    ranges.
- Fixed the bridge by emitting strikethrough tokens, collecting `~~` marker
  ranges, emitting escaped-delimiter marker ranges, and replacing the
  delimiter-pair scanner with Comrak-token-driven marker collection.
- Fixed the Dart Comrak adapter so native marker ranges are excluded from
  already-hidden inline link/image ranges, and link-label parsing handles
  nested image/link labels instead of stopping at the first `]`.
- Recorded HTML entity substitution as an explicit release-boundary item
  because the current projection model supports source deletions/hidden ranges,
  not replacement text.
- Verification:
  - `dart format test/v2/markdown/flark_markdown_feature_matrix_test.dart test/v2/native/flark_native_comrak_bridge_test.dart lib/src/v2/markdown/parse/flark_native_comrak_parse_backend.dart`: passed.
  - `cargo fmt --manifest-path native/comrak_bridge/Cargo.toml`: passed.
  - `./scripts/build_comrak_all.sh --strict`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2/markdown/flark_markdown_feature_matrix_test.dart test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/projection/flark_projection_test.dart test/v2/render_plan/flark_render_plan_test.dart test/v2/markdown/flark_markdown_input_commands_test.dart test/v2/flutter/flark_markdown_input_policy_contract_test.dart --reporter compact`: passed.
  - `flutter test test/v2/native/flark_native_comrak_bridge_test.dart test/v2/markdown/flark_v2_native_upstream_contract_test.dart test/v2/markdown/flark_markdown_feature_matrix_test.dart --reporter compact`: passed; upstream core and GFM lanes both reached 100% for projection/render-plan construction.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `cargo test --manifest-path native/comrak_bridge/Cargo.toml`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.

Replacement-capable projection and HTML entities:

- Closed the HTML entity release boundary by adding replacement ranges to the
  v2 parser contract and bumping the Markdown parse schema to version 3.
- Extended the native Comrak bridge payload with `replacementRanges`; the Rust
  parser now emits decoded `htmlEntity` spans for named and numeric entities
  while excluding literal code and raw HTML ranges.
- Broadened escaped-marker collection to CommonMark escaped ASCII punctuation
  so escaped entities such as `\&amp;` display as literal entity text rather
  than decoding or showing the backslash.
- Updated the Dart Comrak adapter to map native replacements into
  `FlarkMarkdownReplacementRange` values and filter replacements that
  overlap hidden link/image/reference/raw-HTML regions.
- Reworked projection from hidden-range-only elision into a unified projection
  span model. Hidden ranges still display as empty text; replacement ranges
  display decoded text and preserve source/display offset mapping, cursor
  masking, prediction, reconciliation, render-plan ranges, and projected edit
  transactions.
- Added coverage for replacement payload decoding, native bridge entity
  extraction, native adapter filtering, projection offset mapping, edit-adapter
  source replacement, render-plan ranges, feature matrix rows, and live
  rendered editing.
- Updated the support/test matrices, quality audit, and execution plan so HTML
  entities are covered core behavior rather than a known gap.
- Verification:
  - `./scripts/build_comrak_all.sh --strict`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2/markdown/flark_markdown_parse_protocol_test.dart test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/native/flark_native_comrak_bridge_test.dart test/v2/projection/flark_projection_test.dart test/v2/projection/flark_projected_text_edit_adapter_test.dart test/v2/render_plan/flark_render_plan_test.dart test/v2/markdown/flark_markdown_feature_matrix_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter test test/public_api/flark_editor_barrel_test.dart --reporter compact`: passed.
  - `cargo test --manifest-path native/comrak_bridge/Cargo.toml`: passed.

Predictive live-edit invariants and structured source ranges:

- Moved predictive render-plan mapping into the platform-neutral render-plan
  model so `FlarkRenderPlan`, blocks, and inline runs preserve descriptors
  and action metadata while mapping ranges through pending transactions.
- Simplified the Flutter controller prediction path to delegate render-plan
  prediction to the render model instead of manually recreating block and inline
  runs in the controller.
- Added render-plan invariant coverage for semantic descriptor preservation
  across headings, blockquotes, unordered lists, ordered lists, task lists,
  fenced code blocks, tables, and link action runs.
- Added live-rendered surface stability coverage for blockquote rails, list
  markers, task checkboxes, code blocks, and tables. The matrix asserts the
  same rendered surface remains mounted during the predictive state and after
  Comrak parse adoption.
- Fixed the native Comrak adapter's fenced-code projection boundary by hiding
  the newline immediately before the closing fence with the closing fence
  marker, while excluding that composite range from generic marker hiding so
  projection spans do not overlap.
- Generalized live block editing so structured block widgets can provide an
  explicit source edit range. Fenced code now edits the parsed code body range
  directly instead of applying a projected-text replacement that can consume the
  closing fence or following block.
- Updated the Markdown test matrix and execution plan so transitional
  predictive states are an explicit coverage lane rather than an implied manual
  testing concern.
- Verification:
  - `flutter test test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/render_plan/flark_render_plan_test.dart test/v2/flutter/flark_flutter_controller_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with a `Last-Modified` time from the rebuilt bundle.

Live block focus ownership after structural edits:

- Diagnosed list continuation as a focus/caret ownership bug rather than a
  markdown transaction bug: Enter correctly created the second list item and
  moved the controller source selection there, but the old block-local
  `EditableText` retained focus.
- Added `_LiveRenderedBlockFocusCoordinator` under
  `_FlarkLiveRenderedBlockEditor` so rendered block widgets use stable,
  reconciled focus nodes and focus follows the block whose source line owns the
  current controller selection after rebuilds.
- Trimmed trailing separator newlines from block-local display ranges so a
  continued list item does not leave the prior item with an editable newline
  that competes with the new block's caret.
- Routed block-local Select All and non-collapsed Delete/Backspace intents back
  through the canonical runtime selection/transaction path, preventing hidden
  list markers from being stranded when a live-rendered list item is cleared.
- Added a regression test proving Enter from the first unordered list item
  creates `* item\n* `, leaves the controller selection at the new item, and
  moves focus/local caret to the second editable block.
- Added a regression test proving Select All plus Delete clears the hidden list
  marker as well as visible item text in live-rendered block widgets.
- Tightened the Markdown test matrix so live-edit and transitional coverage now
  explicitly requires focus/caret ownership and document-level selection/delete
  checks for structural multi-block edits, not just rendered-surface stability.
- Verification:
  - `flutter analyze`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_markdown_input_policy_contract_test.dart --reporter compact`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter build web` in `example/`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with a `Last-Modified` time from the rebuilt bundle.

Parser-omitted selection hosts after structural exits:

- Diagnosed blank final unordered-list exit as a live-rendered block ownership
  bug, not a markdown input transaction bug. The source command already changed
  `* item\n* ` to `* item\n\n` with the selection on the blank line, but Comrak
  correctly omits a trailing blank paragraph, leaving the separate block-widget
  editor with no rendered block to own the collapsed selection.
- Added a synthetic live-rendered selection host for parser-omitted collapsed
  selection positions. The host is only created when no existing rendered block
  owns the source selection, so Comrak remains authoritative for semantic
  structure while the editor still has a concrete caret target in source gaps
  and trailing blank lines.
- Shared source-selection ownership logic between block construction and focus
  reconciliation so synthetic hosts, normal blocks, focus handoff, and
  block-local selection checks use the same boundary rules.
- Added live-rendered regression coverage proving Enter from an empty final
  unordered-list item exits to a blank paragraph, leaves the prior list marker
  rendered once, and moves focus/local caret to the blank selection host.
- Extended the example Scratch Chrome test to cover list creation, continuation,
  and empty-item exit on the web example path.
- Verification:
  - `dart format lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart example/test/widget_test.dart`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name 'moves focus out of an empty final unordered list item after Enter' --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_markdown_input_policy_contract_test.dart --reporter compact`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --plain-name 'scratch renders unordered list marker immediately' --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with a `Last-Modified` time from the rebuilt bundle.

Playground hardening after nested list and soft-break exploration:

- Exercised the example Scratch/playground path against additional structural
  editing cases beyond the previously fixed quote and unordered-list flows:
  ordered lists, task lists, Shift+Enter, and source-mode round trips.
- Fixed quoted empty-list Enter semantics. Empty unordered, ordered, task, and
  indented list items inside a blockquote now remove the list marker while
  keeping quote mode, instead of preserving a phantom empty list item.
- Added live-rendered regression coverage for ordered-list and task-list empty
  final item exits so parser-omitted selection hosts are proven across all list
  variants, not only unordered bullets.
- Added Shift+Enter as an explicit markdown input-policy path. It now inserts a
  raw soft line break through the canonical transaction pipeline and does not
  trigger structural quote/list continuation.
- Added Chrome playground coverage for ordered/task list continuation and exit,
  plus Shift+Enter soft line breaks in live list editing, with source-mode
  assertions proving the canonical Markdown output.
- Verification:
  - `dart format lib/src/v2/markdown/commands/flark_markdown_input_commands.dart lib/src/v2/flutter/flark_markdown_input_policy.dart test/v2/markdown/flark_markdown_input_commands_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_markdown_input_policy_contract_test.dart example/test/widget_test.dart`: passed.
  - `flutter test test/v2/markdown/flark_markdown_input_commands_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_markdown_input_policy_contract_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name 'moves focus out of an empty final ordered list item after Enter' --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name 'moves focus out of an empty final task item after Enter' --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --plain-name 'scratch exits ordered and task lists into blank live paragraphs' --reporter compact` in `example/`: passed.
  - `flutter test --platform chrome test/widget_test.dart --plain-name 'scratch keeps Shift+Enter as a soft line break in lists' --reporter compact` in `example/`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with a `Last-Modified` time from the rebuilt bundle.

Multi-line blockquote rail hardening:

- Diagnosed the separate-rail blockquote bug as a parser-shape mismatch. The
  native adapter was replacing one Comrak blockquote with one synthetic
  blockquote per quoted source line, so preview and live editing had no way to
  know those lines were one semantic quote region.
- Removed synthetic per-line blockquote decomposition from the native adapter.
  Native multi-line blockquotes now remain one semantic block, including
  unmarked continuation lines that CommonMark treats as part of the quote.
- Preserved trailing display newlines inside live blockquote editors. This
  keeps an empty quoted continuation line visible inside the joined rail and
  lets Enter exit the quote instead of continuing it again.
- Added parser, render-plan matrix, read-only preview, live-edit widget, and
  example Scratch regressions for one continuous multi-line quote rail and the
  Enter-to-exit flow from the empty quoted line.
- Confirmed current code-fence language support is metadata-only in v2:
  commands can insert a specified info string, Comrak exposes it as render-plan
  language metadata, overlay controls surface the value, and the live code
  block displays a badge. V2 does not currently include an interactive language
  selector or automatic language detection/highlighting.
- Verification:
  - `flutter test test/v2/markdown/flark_native_comrak_parse_backend_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_read_only_preview_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed.
  - `flutter test test/v2/markdown/flark_markdown_feature_matrix_test.dart --reporter compact`: passed.
  - `flutter analyze`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with a `Last-Modified` time from the rebuilt bundle.

Parser-omitted source-line hosts for live blank spacing:

- Diagnosed the blank-line spacing bug as the remaining half of the
  parser-omitted source problem. The prior synthetic selection host gave the
  caret a place to land after list exit, but the live block editor still only
  rendered parsed blocks plus that one active host. Extra blank source lines
  therefore remained visible in preview/source text but disappeared from the
  block-widget live editor.
- Replaced the one-off selection-host construction with source-gap-aware block
  construction. In block-widget mode, parser-omitted source lines now become
  synthetic editable source-line hosts, while the previous collapsed selection
  host remains a fallback only when no real or synthetic line owns the current
  selection.
- Routed synthetic source hosts through the markdown input policy and direct
  source-range replacement path. This lets Enter add repeated blank lines,
  Backspace/Delete use canonical source commands, and ordinary typing remain
  visible until Comrak adopts a new authoritative parse.
- Preserved source selection after direct source-range edits so source-bounded
  live editors no longer force the caret to the end of the replacement text.
- Added package widget regressions proving multiple parser-omitted blank lines
  render as separate editable rows, repeated Enter from a blank source host adds
  more visible rows, and typing in a blank source line remains visible before
  and after parser adoption.
- Added example Chrome coverage proving Scratch live-list editing keeps extra
  blank lines visible and source mode still round-trips the canonical Markdown.
- Updated the live-rendered editing golden to intentionally capture visible
  blank source rows between parsed blocks.
- Verification:
  - `dart format lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart example/test/widget_test.dart`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_transition_matrix_test.dart --plain-name 'Flark live-rendered transition matrix fenced code preserves typing through activation' --reporter compact`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with a `Last-Modified` time from the rebuilt bundle.

Rich rendered interaction architecture:

- Added a source-first `FlarkMarkdownInteractions` layer that wraps editor
  and preview surfaces with shared interaction configuration for rendered UI
  affordances. The first targets are code-fence language selection, link menus,
  and task checkbox toggles.
- Added canonical command support for rich UI controls instead of widget-local
  source edits:
  - `markdown.setFenceLanguage` updates the opening fenced-code info string
    while preserving indentation and backtick/tilde marker shape.
  - `markdown.setTaskListChecked` toggles the checkbox marker for an explicit
    task-list source line.
  - `markdown.removeLink` removes markdown link syntax while preserving the
    visible label.
- Made Flutter convenience controllers default to
  `FlarkMarkdownEditingExtensions.standard()` so rendered UI controls work
  out of the box, while the headless runtime still keeps explicit extension
  registration for command-surface tests.
- Added live code-fence language selector chrome backed by the canonical source
  command. The selector is configurable through
  `FlarkMarkdownInteractionConfig.codeLanguages`.
- Routed live task checkbox widgets through the same command path and made the
  interaction config authoritative when checkbox toggles are disabled.
- Added inline link menu support for rendered preview runs with Open, Edit,
  Copy, and Remove actions. Open/Edit are callback-driven; Remove dispatches
  the canonical link command.
- Promoted the interaction config and new payloads through both public barrels
  and updated visual goldens to capture the intentional language-label and link
  rendering changes.
- Verification:
  - `flutter test test/v2/markdown/flark_markdown_block_commands_test.dart test/v2/markdown/flark_markdown_link_commands_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_read_only_preview_test.dart test/public_api/flark_editor_barrel_test.dart test/v2/public_api/flark_editor_v2_public_api_test.dart --reporter compact`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with `Last-Modified: Sat, 09 May 2026 17:00:41 GMT`.

Code-fence language placement and syntax highlighting:

- Confirmed the prior language selector only edited the fenced-code info string
  and label chrome. It did not yet apply syntax highlighting, so selecting a
  language could look like a no-op unless the user inspected the source.
- Added a shared Flutter syntax-highlighting adapter backed by the `highlight`
  package. It registers a bounded set of common languages on demand, normalizes
  aliases such as `js`, `ts`, `py`, `rs`, and `sh`, and falls back to plain code
  text when a language is unset or unsupported.
- Routed both live editable code fences and read-only preview code fences
  through the same highlighter so the selected language changes the actual code
  text styling in both surfaces.
- Made live code fences derive their language from the current source opener
  first, then the adopted render plan. That keeps highlighting and the visible
  selector label in sync immediately after a language selection, before the
  next Comrak adoption.
- Moved the live code-fence language selector and label chrome to the right
  side of the fence.
- Added widget tests proving the selector is positioned on the right, language
  selection updates the visible label, live code blocks color keyword spans from
  the fence language, and read-only preview code blocks apply the same syntax
  styling.
- Updated visual goldens to capture the intentional right-aligned language
  control and highlighted code text.
- Verification:
  - `dart format lib/src/v2/flutter/flark_code_syntax_highlighting.dart lib/src/v2/flutter/flark_projected_editable_text.dart lib/src/v2/flutter/flark_read_only_preview.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_read_only_preview_test.dart`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart test/v2/flutter/flark_read_only_preview_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_v2_visual_golden_test.dart --update-goldens --reporter compact`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with `Last-Modified: Sat, 09 May 2026 17:28:28 GMT`.

Code-fence overlay layout and inline-code caret hardening:

- Reworked the live code-fence language selector so the menu is an anchored
  overlay instead of a child in the code-fence layout. Opening the selector no
  longer increases the fence height, and the button stays inline with the top
  of the rendered code region.
- Kept the selector backed by the same canonical `markdown.setFenceLanguage`
  command, with the overlay closing before dispatch so the widget can rebuild
  without trying to update a stale overlay entry.
- Routed live fenced-code blocks through the markdown input policy as well as
  direct source-range editing. This closes the widget-layer gap where Tab and
  Shift+Tab indentation worked, but Enter at an indented code line did not use
  the same fenced-code indentation continuation behavior as the command layer.
- Added widget tests for non-layout overlay behavior, selected-line code
  indentation, and Enter preserving fenced-code indentation in the live block
  widget.
- Set `paintCursorAboveText` on editable v2 text surfaces so styled inline
  backgrounds, especially inline code, cannot visually cover the caret.
- Added a live-editor widget test that taps inside visible inline code and
  proves the local editable selection lands inside `code` while the source
  selection lands between the hidden backticks.
- Updated the live-rendered golden to capture the intentional shorter
  code-fence layout after the selector was removed from normal layout flow.
- Verification:
  - `dart format lib/src/v2/flutter/flark_projected_editable_text.dart lib/src/v2/flutter/flark_editable_text.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --reporter compact`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name 'keeps inline code text selectable in live editing' --reporter compact`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_live_rendered_editable_text_test.dart --plain-name 'code fence language selector edits the opening fence info' --reporter compact`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed; Chromium printed a shutdown warning after the passing test run.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with `Last-Modified: Sat, 09 May 2026 18:00:42 GMT`.

Code-fence Enter and empty-body hardening:

- Fixed the live fenced-code body range resolver so Enter before the closing
  fence scans to the current closing fence in the source, not just the stale
  parsed block end from the previous adoption cycle.
- Preserved all-newline fenced-code bodies in the live block widget. A blank
  line inserted into an empty fence now remains visible/editable as the code
  body instead of being trimmed away as if it were only the trailing newline
  before the closing marker.
- Hardened native Comrak projection ranges for empty closed fences. The
  adapter now keeps the opening line break and closing marker ranges adjacent
  instead of overlapping, and the existing code-block range normalizer now
  handles both closed-fence clipping and unclosed-fence EOF extension in one
  parser path.
- Added package widget regressions that assert Enter expands both non-empty and
  empty live code fences at the editable code field level, not only in the
  underlying markdown source.
- Added browser-facing example regressions for the scratch pad so the Chrome
  widget suite covers Enter in a populated code fence and in an empty code
  fence.
- Direct Computer Use manual verification was unavailable in this environment
  because macOS returned Apple event error `-1743`; browser coverage was run
  through Flutter's Chrome widget test harness instead.
- Verification:
  - `dart format lib/src/v2/markdown/parse/flark_native_comrak_parse_backend.dart lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/markdown/flark_native_comrak_parse_backend_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart example/test/widget_test.dart`: passed.
  - `flutter test test/v2/markdown/flark_native_comrak_parse_backend_test.dart --name '(does not overlap hidden ranges for an empty closed code fence|keeps fenced code delimiters out of editable code content)' --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --name 'keeps (code indentation|an empty code line)' --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --name 'scratch (keeps blank code lines visible after Enter|expands an empty code fence after Enter)' --reporter compact` in `example/`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with `Last-Modified: Sat, 09 May 2026 18:23:49 GMT`.

Code-fence visual height follow-up:

- Confirmed the previous code-fence Enter regression covered source text and
  editable-controller text, but did not assert the rendered code editor grew
  when the new line was an empty trailing visual line.
- Made live block editables derive their `minLines` from the current local
  text line count. This makes trailing empty lines reserve a real visual row in
  code fences and other live block widgets instead of relying on TextPainter's
  treatment of a terminal newline.
- Strengthened the package and example Chrome regressions so Enter in both a
  non-empty code fence and an empty code fence must increase the editable
  rectangle height.
- Direct desktop Computer Use remained blocked by macOS Apple event error
  `-1743`. As a closer substitute, launched headless Chrome against the actual
  rebuilt `http://127.0.0.1:6200/` bundle through the Chrome DevTools Protocol:
  the active live code textarea measured `20px` high before Enter and `40px`
  high after Enter, with its value changing from `foo` to `foo\n`.
- Verification:
  - `dart format lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart example/test/widget_test.dart`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --name 'keeps (code indentation|an empty code line)' --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --name 'scratch (keeps blank code lines visible after Enter|expands an empty code fence after Enter)' --reporter compact` in `example/`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter analyze`: passed.
  - `flutter test test/v2 --reporter compact`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with `Last-Modified: Sat, 09 May 2026 21:28:09 GMT`.
  - `./scripts/verify_package_confidence.sh`: passed.

V1 code-fence behavior port and input hardening:

- Added a headless fenced-code input policy under the v2 markdown source layer.
  Enter inside fences now preserves indentation, adds one sensible indentation
  unit after `{`, `[`, and `(`, adds colon indentation only for Python/YAML/shell
  fences, and exits closed or unclosed fences from trailing blank body lines.
- Routed the v2 markdown input command through that shared policy instead of a
  narrow in-command fence check. The command file now also uses explicit
  character scanners for heading/list/quote/indented-code boundaries instead
  of deprecated `RegExp` helpers.
- Routed platform text insertions through the same policy for closing braces,
  brackets, and parens typed on indentation-only fenced-code lines. Source and
  live block editors therefore share the same outdent behavior.
- Added IME composition undo grouping in the source editable adapter. Intermediate
  composition updates are recorded under one undo group while normal adjacent
  typing remains a separate undo entry.
- Added deterministic mixed-edit fuzz coverage over insertion, Enter,
  Backspace, selection changes, undo/redo, inline toggles, and block toggles so
  the markdown runtime must preserve document/selection invariants across
  combinations rather than isolated examples.
- Verification:
  - `dart format lib/src/v2/flutter/flark_editable_text.dart lib/src/v2/flutter/flark_markdown_input_policy.dart lib/src/v2/markdown/commands/flark_markdown_input_commands.dart lib/src/v2/markdown/source/flark_markdown_fenced_code_policy.dart test/v2/markdown/flark_markdown_input_commands_test.dart test/v2/flutter/flark_editable_text_test.dart test/v2/markdown/flark_markdown_fuzz_invariants_test.dart`: passed.
  - `flutter test test/v2/markdown/flark_markdown_input_commands_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_editable_text_test.dart --reporter compact`: passed.
  - `flutter test test/v2/markdown/flark_markdown_fuzz_invariants_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --name "code fence|code indentation|empty code line|moves out of live code fences|indents live code" --reporter compact`: passed.
  - `dart analyze lib/src/v2/flutter/flark_editable_text.dart lib/src/v2/flutter/flark_markdown_input_policy.dart lib/src/v2/markdown/commands/flark_markdown_input_commands.dart lib/src/v2/markdown/source/flark_markdown_fenced_code_policy.dart test/v2/markdown/flark_markdown_input_commands_test.dart test/v2/flutter/flark_editable_text_test.dart test/v2/markdown/flark_markdown_fuzz_invariants_test.dart`: passed with no issues after removing the new and local deprecated `RegExp` usage.
  - `flutter test test/v2/markdown test/v2/flutter --reporter compact`: passed with 307 tests.
  - `flutter analyze`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.

Best-in-class fenced-code boundary and scanner extraction:

- Documented the intended fenced-code scope in
  `docs/architecture/v2/fenced_code_support_plan_2026-05-10.md`: Markdown
  editor fidelity, not IDE fidelity. The supported contract is marker hiding,
  language chrome, highlighting, Enter indentation, blank-line exit, closer
  outdent, multiline paste indentation, Tab/Shift+Tab, boundary arrows, and
  ordinary selectable body text.
- Added `FlarkMarkdownFencedCodeScanner` as the shared source-level scanner
  for opening fence metadata, body ranges, closing fence detection, and line
  helpers. This removes the live widget's duplicate regex-based fence scanner.
- Extended `FlarkMarkdownFencedCodePolicy` so fenced-code source behavior
  lives in one headless policy: Enter, blank-line exit, closer outdent,
  multiline paste indentation, Tab indentation, and Shift+Tab outdent.
- Routed platform insertion handling through the shared fenced-code policy for
  multiline paste, using paste transaction metadata while keeping closer
  outdent as input metadata.
- Updated the live code block widget to consume scanner body ranges/languages
  and source-policy indent/outdent operations instead of owning code semantics.
- Removed deprecated `RegExp` usage from the touched live widget paths by
  replacing simple quote/list detection with explicit character scanners.
- Added scanner, headless policy, source-widget, and live-rendered widget
  coverage for the new architecture, including web-visible multiline code paste
  behavior.
- Verification:
  - `dart format lib/src/v2/markdown/source/flark_markdown_fenced_code_scanner.dart lib/src/v2/markdown/source/flark_markdown_fenced_code_policy.dart lib/src/v2/flutter/flark_markdown_input_policy.dart lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/markdown/flark_markdown_fenced_code_scanner_test.dart test/v2/markdown/flark_markdown_input_commands_test.dart test/v2/flutter/flark_editable_text_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed.
  - `flutter test test/v2/markdown/flark_markdown_fenced_code_scanner_test.dart test/v2/markdown/flark_markdown_input_commands_test.dart test/v2/flutter/flark_editable_text_test.dart --reporter compact`: passed.
  - `flutter test test/v2/flutter/flark_live_rendered_editable_text_test.dart --name "code fence|code indentation|empty code line|moves out of live code fences|keeps vertical arrows native inside live code fences|indents live code|outdents live code|multiline paste" --reporter compact`: passed.
  - `dart analyze lib/src/v2/markdown/source/flark_markdown_fenced_code_scanner.dart lib/src/v2/markdown/source/flark_markdown_fenced_code_policy.dart lib/src/v2/flutter/flark_markdown_input_policy.dart lib/src/v2/flutter/flark_projected_editable_text.dart test/v2/markdown/flark_markdown_fenced_code_scanner_test.dart test/v2/markdown/flark_markdown_input_commands_test.dart test/v2/flutter/flark_editable_text_test.dart test/v2/flutter/flark_live_rendered_editable_text_test.dart`: passed with no issues.
  - `flutter test test/v2/markdown test/v2/flutter --reporter compact`: passed with 317 tests.
  - `flutter analyze`: passed.
  - `./scripts/verify_package_confidence.sh`: passed.
  - `flutter test --platform chrome test/v2/flutter/flark_markdown_web_smoke_test.dart --reporter compact`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with `Last-Modified: Sun, 10 May 2026 23:35:56 GMT`.

Example web playground UX pass:

- Reworked `example/lib/main.dart` from a thin demo shell into a higher-quality
  playground surface: neutral workbench background, compact top toolbar,
  responsive wrapping controls, framed editor/preview panes, pane headers,
  status/caret footer, and a narrow-viewport stacked layout.
- Replaced inert command buttons with real toolbar actions for heading, bold,
  italic, quote, bullet list, ordered list, task list, Dart code fence, and
  table insertion. Toolbar actions use public v2 commands and return rendered
  mode back to live editing after a successful mutation.
- Kept the existing web scratch-pad workflows and test handles intact while
  adding browser regressions for usable narrow layouts and functional toolbar
  commands.
- Direct desktop Computer Use remained blocked by macOS Apple event error
  `-1743`; browser verification used the Chrome Flutter widget suite and the
  rebuilt bundle served at `http://127.0.0.1:6200/`.
- Verification:
  - `dart format example/lib/main.dart example/test/widget_test.dart`: passed.
  - `flutter analyze` in `example/`: passed.
  - `flutter test --platform chrome test/widget_test.dart --reporter compact` in `example/`: passed with 19 tests.
  - `flutter analyze`: passed.
  - `flutter build web` in `example/`: passed.
  - `curl -s -I http://127.0.0.1:6200/main.dart.js`: returned `200 OK` with `Last-Modified: Mon, 11 May 2026 00:02:46 GMT`.
