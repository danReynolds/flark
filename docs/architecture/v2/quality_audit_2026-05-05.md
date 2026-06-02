# Flark v2 Quality Audit

Status date: 2026-05-05
Scope: current standalone package worktree after Phase 15 editable block
widgets.

## Objective Restatement

Flark should be a best-in-class live Markdown editor and previewer for
Flutter:

- read-only preview mode renders Markdown efficiently and predictably;
- live editing renders as the user types without abandoning canonical Markdown
  source;
- architecture is source-first, headless where possible, parser-backed,
  transaction-oriented, and testable without Flutter;
- consumer surface is small, documented, and pleasant to integrate;
- release readiness is proven through tests, visual contracts, example
  dogfooding, benchmarks, and packaging gates.

## Current Assessment

The package is now a strong private release candidate for the stated editor and
previewer goal. The central architecture is correct: Markdown source is the
durable model, the pure Dart runtime owns transactions/history/commands,
Comrak-backed parsing feeds a projection and render-plan layer, and Flutter
widgets adapt that state into source, projected, live-rendered, and read-only
surfaces.

The live editor has crossed the important quality threshold from "styled text"
to real render-plan-backed block widgets. Parsed task items, fenced code blocks,
and GFM tables are editable UI blocks that write normal source transactions
back to Markdown. The read-only preview consumes the same controller/render
plan rather than reparsing in the widget layer.

The remaining blockers are not core architecture blockers. They are release
boundary items: owner decisions for license/URLs/pub publishing and any future
first-class extensions beyond CommonMark/GFM.

## Prompt-to-Artifact Checklist

| Requirement | Current evidence | Assessment |
| --- | --- | --- |
| Efficient read-only preview mode | `Markdown`, `FlarkReadOnlyPreview`, shared `FlarkRenderPlan`, `test/v2/flutter/flark_read_only_preview_test.dart`, `test/v2/flutter/flark_render_plan_parity_test.dart`, render-plan performance budget | Satisfied for required Comrak-backed paths |
| Render-as-you-type live editing | `FlarkMarkdownEditingMode.liveRendered`, `FlarkLiveRenderedEditableText`, block widgets in `lib/src/v2/flutter/flark_projected_editable_text.dart`, `test/v2/flutter/flark_live_rendered_editable_text_test.dart`, `flark_v2_live_rendered_editing.png` | Satisfied for current supported blocks and inline styling |
| Canonical Markdown source | `FlarkDocument`, `FlarkTransaction`, projected edit adapter, live block source-range transactions | Satisfied |
| First-principled architecture | V2 core/import boundary, parser protocol, projection core, render plan, command runtime, public API inventory | Satisfied |
| CommonMark/GFM confidence | Native Comrak backend, upstream fixture contracts, curated GFM fixtures, deviation register | Satisfied with documented policy gaps |
| Consumer delight | Top-level `flark.dart`, headless core barrel, full V2 barrel, README quick start, example desktop workbench, public API tests | Strong, with publication metadata still pending |
| Performance | `test/v2/performance/flark_v2_performance_budget_test.dart`, `scripts/verify_benchmark_lane.sh` | Current budgets pass |
| Visual/user-facing quality | Seven-PNG visual golden suite and macOS example integration flow | Strong for covered scenarios |
| Release readiness | `scripts/verify_release.sh`, release checklist, native/editor/web/example gates | Engineering gate passes; public publishing still blocked on owner decisions |

## Verification Evidence

Current-turn verification:

- `flutter analyze hook lib test`: passed with no issues.
- `flutter test test --exclude-tags benchmark --reporter compact`: passed.
- `./scripts/verify_benchmark_lane.sh`: passed.
- `./scripts/verify_release.sh`: passed end to end. The gate covered package
  and example dependency resolution, package and example analysis, dartdoc dry
  run with 0 warnings/errors, example tests, web smoke coverage, host native
  Comrak build, native editor CI, full package tests including visual goldens,
  and enforced benchmark budgets.

Recent recorded Phase 15 verification in `execution_log.md`:

- focused live-rendered widget tests passed;
- visual golden update and verification passed;
- example analysis/tests passed;
- macOS integration flow passed.

## Residual Gaps

- Public publishing is blocked until the owner chooses a license, canonical
  repository/documentation/issue URLs, screenshot policy, and `publish_to`
  change.
- Web now has packaged Comrak WASM as the required default parser path.
- Image/media support has a default read-only preview card with open, copy, and
  source-edit actions. Loading remote thumbnails or owning an app-specific media
  library remains outside the package default policy.
- HTML entity substitution is covered by replacement-capable projection spans,
  native `htmlEntity` replacement ranges, and projected/live edit tests.
- Reference-link action resolution is covered by parser-provided Comrak
  metadata; dedicated source-edit commands for reference definitions remain
  optional polish.
- Code-fence editing supports source-preserving body edits, language selection,
  syntax highlighting, copy actions, Backspace boundary behavior, vertical
  arrows, visible selection, and Tab/Shift-Tab indentation. IDE-level actions
  such as autocomplete, diagnostics, formatting, or comment toggles remain
  explicit non-goals.

## Forward Plan

1. Keep owner decisions separate from engineering readiness: add license and
   pub metadata only when those decisions are explicit.
2. If continuing engineering polish before publication, prioritize upstream web
   conformance expansion, release examples/screenshots, or explicitly scoped
   extension work.
3. Keep all new UI behavior tied to the render plan and typed transactions;
   avoid widget-local Markdown parsing or parallel mutable document state.
