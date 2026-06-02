# Flark v2 Execution Plan

Status date: 2026-05-08
Current phase: Phase 17 - markdown coverage matrix and Comrak-only hardening

## Objective

Build Flark v2 as a best-in-class Dart/Flutter markdown editing and
previewing library with a headless source-first markdown engine, Flutter
adapters, shared edit/read render planning, spec-backed conformance, and a
small durable public API.

## Success Criteria

- Core editor runtime is pure Dart and imports no Flutter libraries.
- Markdown source text is the canonical durable document format.
- CommonMark and GFM behavior is validated by fixture/conformance tests.
- All edits flow through typed transactions and source operations.
- Commands, keyboard input, paste, toolbar actions, and table/list/fence
  policies share the same transaction pathway.
- Projection is a first-class, tested headless module.
- Editable and read-only surfaces consume the same render plan.
- Native parser bridge payloads are versioned, contract-tested, and
  forward-compatible.
- Promoted widgets require one default parser family: Comrak through FFI on
  native targets and WASM on web.
- Flutter widgets are adapters over core state, not the source of truth.
- The package ships one canonical v2 implementation path with no v1
  compatibility layer.
- Release docs, migration docs, examples, and verification gates cover v2.

## Phase 0: Research and Architecture

Goal: lock the v2 direction before code starts.

Tasks:

- [x] Research markdown specifications and parser contracts.
- [x] Research editor architecture patterns in CodeMirror, ProseMirror,
  Lexical, Slate, and Milkdown.
- [x] Research Flutter text input boundaries and delta model.
- [x] Research Flutter ecosystem alternatives.
- [x] Create v2 research matrix.
- [x] Create v2 rewrite plan.
- [x] Create v2 execution plan and log.
- [x] Create public v2 API sketch as an RFC.
- [x] Decide exact initial v2 source directory: `lib/src/v2`.
- [x] Create public v2 library names RFC.

## Phase 1: Headless Core Skeleton

Goal: establish the pure Dart core and transaction model.

Tasks:

- [x] Add `lib/src/v2/core/document`.
- [x] Add `FlarkDocument`.
- [x] Add `FlarkTextBuffer` with UTF-16 offsets and line indexing.
- [x] Add `FlarkSelection`.
- [x] Add `FlarkSourceOperation`.
- [x] Add `FlarkTransaction`.
- [x] Add `FlarkEditorState`.
- [x] Implement transaction apply/map/invert basics.
- [x] Add core tests for insert/delete/replace.
- [x] Add core tests for selection mapping.
- [x] Add core tests for transaction inversion basics.
- [x] Verify no Flutter imports in v2 core.
- [x] Add history stack model.
- [x] Add transaction metadata model beyond initial `userEvent` and
  `addToHistory`.
- [x] Add undo/redo grouping for adjacent transactions with the same undo group.
- [x] Add richer source-position mapping tests for multi-operation edge cases.
- [x] Document core transaction/history invariants.
- [x] Decide history ownership: keep `FlarkHistoryStack` companion-owned
  until a future engine/runtime state composes it.
- [x] Add headless runtime object to compose editor state, history, extensions,
  and command dispatch.

## Phase 2: Command and Extension Runtime

Goal: make behavior composable without leaking framework state.

Tasks:

- [x] Add typed command model.
- [x] Add command result model: handled, not handled, no-op, rejected.
- [x] Add priority-based command registry.
- [x] Add command runtime design note.
- [x] Add extension registration model.
- [x] Add canonical source-edit command ids.
- [x] Port first inline style command tests to headless v2.
- [x] Add command capability/rejection conventions.
- [x] Add inline style edge coverage for selected markers and inline code spans.
- [x] Define initial transaction metadata for user events, undo grouping, and
  history opt-out.
- [x] Extend transaction metadata for parse/projection invalidation.
- [x] Port heading/list/quote/fence/thematic-break command tests to v2.

## Phase 3: Markdown Parse Protocol v2

Goal: make parsing authoritative, spec-backed, and independent of Flutter.

Tasks:

- [x] Define parser backend interface in pure Dart.
- [x] Define native payload schema v2.
- [x] Add schema version and capabilities fields.
- [x] Add unknown-field tolerance tests.
- [x] Add parser protocol design note.
- [x] Add CommonMark fixture importer.
- [x] Add GFM fixture coverage for tables, task list items, strikethrough, and
  autolinks.
- [x] Add UTF-8/UTF-16 mapping contract tests.
- [x] Add parser-provided hidden range contract for projection.
- [x] Keep the native bridge available until the v2 adapter is verified.
- [x] Add a v2 native Comrak parse backend adapter over the existing native
  ABI, with fake-bridge and real-host-library contract tests.

## Phase 4: Projection Core

Goal: move hidden markers, cursor safety, and source/display mapping into a
headless model.

Tasks:

- [x] Add hidden range model.
- [x] Add cursor mask model.
- [x] Add source/display mapping model.
- [x] Build projection from parser-provided hidden ranges.
- [x] Add projection core design note.
- [x] Add ambiguity-zone model.
- [x] Add predictive projection API.
- [x] Add authoritative reconciliation API.
- [x] Port escaped delimiter fixtures.
- [x] Port reference link fixtures.
- [x] Port table projection fixtures.
- [x] Port image/media source-first fixtures.
- [x] Port raw HTML literal policy fixtures.

## Phase 5: Render Plan

Goal: create the shared edit/read rendering contract.

Tasks:

- [x] Add typed block render plan.
- [x] Add typed inline render plan.
- [x] Derive default projection from parse results.
- [x] Ensure nested container blocks do not duplicate child inline runs.
- [x] Add render plan design note.
- [x] Add table render descriptors.
- [x] Add task-list render descriptors.
- [x] Add fenced-code render descriptors.
- [x] Add link/image action descriptors.
- [x] Add overlay-oriented render-plan query APIs.
- [x] Add render-plan overlay target model.
- [x] Add theme-token model.
- [x] Add render-plan parity tests for edit and read surfaces.

## Phase 6: Flutter Adapter

Goal: wire core state into Flutter without making Flutter the core.

Tasks:

- [x] Add `FlarkFlutterController`.
- [x] Add Flutter adapter design note.
- [x] Add adapter from Flutter input changes to core transactions.
- [x] Add `EditableText` surface around core state.
- [x] Add shortcut/action integration.
- [x] Add selection and cursor overlay integration.
- [x] Add link/image/task/table overlays from render-plan descriptors.
- [x] Add read-only preview adapter using the same render plan.
- [x] Add parser scheduler that keeps controller parse/render state current.
- [x] Add example app v2 toggle for side-by-side v1/v2 validation.

## Phase 7: Feature Parity and Quality Gates

Goal: reach current v1 behavior parity and raise the quality bar.

Tasks:

- [x] Port current markdown support matrix to v2.
- [x] Add temporary v1/v2 oracle comparison tests during migration.
- [x] Add performance budgets for core transactions, parse adoption,
  projection, and render-plan generation.
- [x] Port link, task-list, and table command parity to typed v2 commands.
- [x] Wire v2 command parity through the example app.
- [x] Add command capability query APIs and active mark state.
- [x] Add projected editing surface over source/display mapping.
- [x] Add accessibility and selection behavior tests for Flutter adapter.
- [x] Add native bridge v2 packaging checks.
- [x] Add docs generation and public API inventory for v2.
- [x] Expand rich live-editing policies for headings, lists, blockquotes,
  fenced code, and indented code in the v2 input layer.
- [x] Add render-plan-backed overlay controls for links, images, tasks, tables,
  and code blocks.
- [x] Narrow the experimental v2 barrel to explicit public `show` exports.

## Phase 8: Release Path

Goal: make v2 the default implementation and prepare public adoption.

Tasks:

- [x] Promote v2 behind public widgets.
- [x] Remove or deprecate v1 internals.
- [x] Write migration guide.
- [x] Update README and example app.
- [ ] Add package screenshots when publishing externally.
- [ ] Add `LICENSE` after owner decision.
- [ ] Add repository/issue tracker/documentation metadata after canonical URLs
  are chosen.
- [x] Run full release gate.
- [ ] Run publish dry run after owner decisions for license, metadata, and
  `publish_to`.

## Phase 9: Ecosystem-Quality Hardening

Goal: move from a strong foundation to release-quality confidence across
visual regressions, backend/platform strategy, conformance breadth,
extensibility, and example-app dogfooding.

Tasks:

- [x] Add a readable visual golden covering promoted source editing,
  projected editing, preview rendering, and overlay controls.
- [x] Include the visual golden in the fast package confidence gate.
- [x] Add no-throw native backend probing and automatic native backend loading
  for promoted v2 widgets.
- [x] Add Chrome smoke coverage for promoted v2 widgets.
- [x] Add a v2 native upstream contract that runs Comrak output through
  projection and render-plan generation across CommonMark/GFM fixtures.
- [x] Fix invalid reference-definition-looking paragraphs so v2 projection only
  hides parser-recognized reference definitions.
- [x] Add render-plan extension hooks for semantic render customization.
- [x] Add preview block-builder support for custom read-only rendering.
- [x] Dogfood shared-controller parsing and custom block rendering in the
  example app.

## Phase 10: Comprehensive Visual Regression Hardening

Goal: make visual testing robust enough to catch markdown/editor paint,
wrapping, spacing, affordance, and overlay regressions across happy paths and
edge cases without moving parser or command semantics out of code assertions.

Tasks:

- [x] Convert the single v2 smoke golden into a curated multi-PNG suite.
- [x] Add focused inline styling and wrapping coverage.
- [x] Add focused code-fence visual coverage.
- [x] Add focused blockquote visual coverage.
- [x] Add focused task, table, and overlay visual coverage.
- [x] Add compact viewport mixed-markdown coverage.
- [x] Document the golden scenario inventory and update workflow.
- [x] Rerun focused visual tests, package analysis, and the full release gate.

## Phase 11: Community-Library Surface and Packaging Hardening

Goal: address the remaining library-shape issues that affect public adoption:
small default API, explicit transition boundary, lighter default dependencies,
typed controller updates, clear web parser strategy, and adopter-facing docs.

Tasks:

- [x] Split v1 compatibility into `sovereign_editor_legacy.dart` during the
  migration window.
- [x] Keep `flark.dart` v2-first with explicit promoted exports.
- [x] Add `flark_core.dart` for headless Dart integrations.
- [x] Keep `flark_advanced.dart` as the full advanced v2 integration
  barrel.
- [x] Add typed `FlarkFlutterController.events` for surgical extension
  reactions.
- [x] Avoid repeated transaction operation sorting during selection mapping.
- [x] Remove `google_fonts` from default package dependencies.
- [x] Document the first-party web parser target and parser contract.
- [x] Rewrite the README opening around quickstart, differentiators, visual
  artifacts, and public barrel choices.
- [x] Expand the changelog with the v2 architecture and visual-testing story.
- [x] Rerun focused tests, analyzers, and the full release gate.

## Phase 12: V2-Only Cleanup and Interactive Example Validation

Goal: remove transition-only v1 code and verify the package through the
example app, not only unit tests.

Tasks:

- [x] Delete the old v1 implementation tree under `lib/widgets`,
  `lib/src/widgets`, and `lib/theme`.
- [x] Delete the `sovereign_editor_legacy.dart` compatibility barrel.
- [x] Delete v1 widget/engine tests and the temporary v1/v2 oracle suite.
- [x] Move the shared native Comrak Dart bridge into `lib/src/v2/native`.
- [x] Remove the default `highlight` dependency with the old renderer.
- [x] Update public barrels so the supported app import is v2-only.
- [x] Rebuild the example app as a v2-only integration harness.
- [x] Run package analysis, docs, focused tests, full release gate, and example
  tests.
- [x] Run the example app and interact with common Markdown cases: headings,
  inline styling, code fences, blockquotes, tasks, tables, links, source mode,
  projected mode, undo, redo, and preview updates.

## Phase 13: macOS Desktop Dogfood App

Goal: make the example usable as a polished macOS workbench for manually
testing live editing and preview behavior.

Tasks:

- [x] Generate and wire the `example/macos` desktop target.
- [x] Redesign `example/lib/main.dart` for a desktop split-pane workflow while
  preserving compact/mobile behavior and stable integration-test keys.
- [x] Set a useful macOS app name, initial window size, and minimum window
  size.
- [x] Fix macOS native Comrak bundle loading from
  `Contents/Frameworks/sovereign_comrak_bridge.framework`.
- [x] Verify startup projection, source/projected switching, source edits,
  preview updates, and common Markdown cases through the macOS integration
  flow.

## Phase 14: Live Rendered Editing Surface

Goal: deliver the WYSIWYG-feeling v2 editing mode without abandoning source
Markdown as the canonical document model.

Tasks:

- [x] Re-check Flutter `EditableText`/`TextEditingController.buildTextSpan`
  boundaries and editor-decoration patterns from CodeMirror and ProseMirror.
- [x] Refactor projected editing through one shared host so projected and live
  rendered editing share source/display edit mapping, selection mapping, input
  policies, focus handling, and scroll wiring.
- [x] Add `FlarkLiveRenderedEditableText`, backed by projected text plus
  render-plan-derived styled `TextSpan` segments.
- [x] Paint render-plan block chrome for editable code fences and blockquotes
  behind the live editable text without reparsing Markdown in Flutter.
- [x] Add `FlarkMarkdownEditingMode.liveRendered` and expose it through
  the promoted app barrel and full v2 barrel.
- [x] Preserve render-plan continuity through selection-only changes and carry
  predictive render-plan ranges across source edits until authoritative parsing
  catches up.
- [x] Wire the macOS example `Live Edit` mode and `Scratch` flow to the
  rendered-in-place editor.
- [x] Add focused widget/controller coverage and a dedicated live-rendered
  visual golden.
- [x] Run package analysis, focused tests, visual goldens, full package tests,
  example tests, macOS integration flow, and rebuild the normal macOS app.

## Phase 15: Editable Block Widgets

Goal: move live rendered editing beyond styled text plus painted chrome into
real render-plan-backed block widgets that remain source-first and editable.

Tasks:

- [x] Re-check block-widget/editor-decoration architecture against Flutter,
  CodeMirror, and ProseMirror constraints.
- [x] Recompose `FlarkMarkdownEditingMode.liveRendered` around a parsed
  block editor when a render plan exists.
- [x] Render task-list items as interactive checkbox rows that toggle the
  canonical Markdown marker.
- [x] Render fenced code blocks as editable code widgets that preserve the
  source fence opener, language, and closer while editing only the projected
  code body.
- [x] Render GFM tables as editable table grids whose cells write source-range
  transactions back into the Markdown table.
- [x] Preserve projected inline styling and source/display selection mapping
  inside ordinary text blocks, headings, quotes, and code widgets.
- [x] Add focused widget coverage for code block edits, task toggles, and
  table-cell edits.
- [x] Refresh and verify the live-rendered visual golden so the PNG now covers
  true block widgets.
- [x] Run analysis, focused surrounding editor tests, example tests, macOS
  integration, visual goldens, and the full package test suite.

## Phase 16: Quality Audit and Release Boundary

Goal: assess the current V2 package against the best-in-class live Markdown
editor/previewer objective and leave the remaining work as explicit release
boundaries rather than implicit uncertainty.

Tasks:

- [x] Restate the editor/previewer goal as concrete quality gates.
- [x] Map each quality gate to current repo artifacts and command evidence.
- [x] Verify the promoted read-only preview and live rendered editor paths
  through public API tests, widget tests, visual goldens, example tests, and
  benchmark budgets.
- [x] Update the markdown support matrix so task, code-fence, and table
  live-block editing reflect Phase 15 rather than older partial status.
- [x] Run package analysis, full non-benchmark package tests, and the
  benchmark lane on the current worktree.
- [x] Run the full release gate on the final audited worktree.
- [ ] Resolve owner-controlled publication blockers: license, canonical URLs,
  screenshots, and `publish_to`.

## Phase 17: Markdown Coverage Matrix and Comrak-Only Hardening

Goal: make Markdown coverage auditable across parser, projection, render plan,
preview, live editing, keyboard policy, browser, and example surfaces while
keeping the parser architecture Comrak-only by default.

Tasks:

- [x] Remove fallback-parser assumptions from support, migration, public API,
  README, and web parser strategy docs.
- [x] Add an executable Markdown feature matrix that runs real Comrak output
  through projection and render-plan generation.
- [x] Add a prose test matrix that maps Markdown features to parser,
  projection/render-plan, command, keyboard, widget, web, and example lanes.
- [x] Fix native strikethrough token emission and `~~` marker projection gaps
  exposed by the executable matrix.
- [x] Add escaped-delimiter projection coverage and record HTML entity
  substitution as an explicit replacement-projection release boundary.
- [x] Rebuild host, mobile, and browser Comrak artifacts after the bridge
  changes.
- [x] Run final focused and broad validation gates on the matrix worktree.

## Phase 18: Replacement-Capable Projection

Goal: close the HTML entity release boundary by promoting projection from
hidden-range-only marker elision to a span model that can replace source ranges
with decoded display text while preserving canonical source transactions.

Tasks:

- [x] Add parser, native payload, and adapter contracts for replacement ranges.
- [x] Decode Comrak HTML entities into `htmlEntity` replacement ranges outside
  literal code and raw-HTML regions.
- [x] Teach projection, cursor masking, display/source mapping, prediction, and
  reconciliation about replacement spans.
- [x] Route projected/live display edits over replacement spans back to the
  source entity range.
- [x] Extend the executable Markdown feature matrix and widget tests so HTML
  entities are covered across native parsing, projection, render plan, and live
  editing.
- [x] Rebuild host, mobile, and browser Comrak artifacts after the schema
  change.

## Phase 19: Predictive Live-Edit Invariants

Goal: make live-rendered editing robust in the transient state between a user
edit and parser adoption, so structured markdown chrome does not flicker,
remount, or corrupt canonical source delimiters.

Tasks:

- [x] Move predictive render-plan mapping into the render-plan model so block
  and inline descriptors are preserved by the same API that maps ranges.
- [x] Add render-plan invariant coverage for descriptor preservation across
  heading, quote, list, task, code, table, and link edits.
- [x] Add live-surface stability coverage that checks quote, unordered list,
  ordered list, task list, code block, and table surfaces before parse,
  during prediction, and after parse adoption.
- [x] Route structured live block editors through explicit source edit ranges
  when their editable body is not equivalent to a simple projected text range.
- [x] Harden fenced-code projection and live code body edits so closing fences
  and following blocks cannot be consumed by body text replacement.

## Current Next Step

Current release boundary:

1. Phase 10 visual-golden hardening has passed the full local release gate.
2. Phase 11 public-surface/dependency/docs hardening has passed the full local
   release gate.
3. Phase 12 removed the transition-only compatibility layer and added an iOS
   simulator integration flow for the v2-only example app.
4. Phase 13 added a polished macOS desktop dogfood app and verified it with
   the shared example integration flow on the macOS device.
5. Phase 14 added rendered-in-place v2 live editing while keeping the same
   source-first projection/render-plan architecture.
6. Phase 15 promoted live editing from styled text plus chrome to editable
   block widgets for tasks, code fences, and tables.
7. Phase 16 records the prompt-to-artifact quality audit in
   `docs/architecture/v2/quality_audit_2026-05-05.md` and has verified the
   current worktree with analyzer, full non-benchmark package tests,
   benchmark budgets, and the full release-readiness gate.
8. Post-audit hardening closed prioritized engineering gaps: live code blocks
   now handle Tab/Shift-Tab indentation through source transactions,
   reference-style links/images resolve through Comrak metadata, and read-only
   previews render image runs as default cards.
9. The browser target loads the packaged Comrak WASM bridge and the promoted
   widgets now require Comrak by default instead of silently falling back to a
   second Markdown implementation.
10. Phase 17 adds
    `docs/architecture/v2/markdown_test_matrix_2026-05-08.md` and
    `test/v2/markdown/sovereign_markdown_feature_matrix_test.dart` as the
    coverage contract for Markdown features across parser, projection,
    render-plan, command/input, widget, web, and example lanes.
11. Phase 18 closes the HTML entity boundary with replacement-capable
    projection spans, Comrak-emitted `htmlEntity` ranges, live-edit source
    mapping, and feature-matrix/widget coverage.
12. Phase 19 hardens predictive live-edit invariants with render-plan
    descriptor prediction, live surface stability coverage, explicit structured
    source edit ranges, and fenced-code boundary protection.
13. Final engineering polish adds source-first image card actions for open,
    copy, and source editing, plus copy actions for preview and live-rendered
    code fences.
14. External publishing remains blocked on owner decisions for license,
    screenshots, `publish_to`, and canonical repository/documentation URLs.

No Flutter imports are allowed in the v2 core.
