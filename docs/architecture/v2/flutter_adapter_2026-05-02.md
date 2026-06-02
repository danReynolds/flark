# Sovereign v2 Flutter Adapter

Status date: 2026-05-02

## Scope

The first Flutter adapter slice introduces `SovereignFlutterController` as a
thin `ChangeNotifier` over the headless runtime, projection, and render plan.
It is intentionally not the editor core.

## Current Contract

- `SovereignFlutterController` owns a `SovereignEditorRuntime`.
- Source edits still flow through headless commands or transactions.
- Flutter text-input deltas are translated into typed source transactions by
  `SovereignTextDeltaAdapter`.
- Stale text deltas are rejected when `oldText` does not match the headless
  document.
- Projection is predicted through source transactions while parser output is
  stale.
- Render plans are explicitly marked stale until a current parse result is
  applied.
- Parse results are accepted only when revision and source text length match
  the current document.
- Current parse results rebuild projection and render plan from parser payloads.
- Undo/redo currently clears projection back to a length-safe empty projection
  when no inverse transaction is exposed by runtime history.
- `MarkdownEditor` is the promoted editing widget. It owns a
  controller from `initialMarkdown` for simple use or accepts a shared
  `SovereignFlutterController` for toolbars and split previews.
- The concrete raw-source, projected, and live-rendered editing widgets are
  implementation details behind `MarkdownEditor`.
- Flutter command actions invoke typed v2 commands through
  `SovereignCommandIntent` and `SovereignCommandAction`.
- `MarkdownEditor` can install shortcut maps that dispatch typed
  command invocations.
- `Markdown` is the promoted read-only widget. It owns a
  controller from `markdown` for standalone preview or consumes a shared
  controller's render plan in split-pane layouts.
- `SovereignMarkdownEditingMode.liveRendered` routes through
  `SovereignLiveRenderedEditableText`, which uses projected source edits plus
  render-plan-backed block widgets for parsed task lists, fenced code, and GFM
  tables.

## Why This Matters

v1 centers on a `TextEditingController`. v2 keeps Flutter as an adapter over
headless state so the source model, command semantics, parser protocol,
projection, and render planning remain testable without widgets.

## Next Flutter Work

- Decide whether runtime undo/redo should expose inverse transactions so
  projection can be predicted instead of reset.
- Continue improving specialized block widgets without introducing
  widget-local Markdown parsing or parallel document state.
