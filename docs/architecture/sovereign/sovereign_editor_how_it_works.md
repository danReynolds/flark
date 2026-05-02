# Sovereign Editor: How It Works

**Status date**: 2026-05-02
**Package**: `sovereign_editor`
**Runtime backend**: native `comrak` (macOS/iOS/Android)

This document is the practical implementation overview: what the editor does,
how the runtime pipeline works, and what is currently supported.

## 1) Runtime Architecture (end-to-end)

1. `SovereignEditor` (editable) and `SovereignMarkdownView` (read-only)
   both use `SovereignController` as the parse/projection/render driver.
2. For editable mode, user input is routed through input-intent policies
   (enter/tab/arrows/backspace).
3. `EditOperationPipeline` computes edit ops and undo grouping.
4. A predictive reconciliation pass updates hidden markers/styles immediately so typing feels live.
5. `SyntaxParseScheduler` runs authoritative parse requests with single-flight behavior:
   one in-flight parse + one latest pending parse.
6. Native `comrak` parse result is normalized into `SyntaxSnapshot`.
7. `SyntaxProjectionCoordinator` reconciles authoritative ranges/tokens with local predictive state.
8. Rendering uses `SovereignTextRenderer` + painter overlays (quote rails,
   code blocks, checkbox visuals in edit mode, link actions as configured).

## 2) Core Modules and Ownership

- `controllers/`
  - orchestration, input-intent coordination, lifecycle, wiring.
- `core/pipeline/`
  - edit diffing, value mutation coordination, undo grouping policy.
- `core/syntax/`
  - predictive reconciliation, selection guards, projection mapping.
- `engine/`
  - scheduler, syntax adapter, native bridge/backend, UTF-8/UTF-16 mapping.
- `core/structure/`
  - markdown line helpers, fence/list/quote/table/indented-code helpers.
- `core/rendering/` + `presentation/`
  - span building, marker visibility, code highlighting, overlays/painters.
- `commands/`
  - externally callable typed markdown commands (`controller.commands.*`).
- `presentation/sovereign_markdown_view.dart`
  - read-only package surface for sovereign-backed rendering in app post detail screens.

## 3) Parsing Profiles + Contracts

Profiles are defined by the public `MarkdownSyntaxProfile` enum in
`lib/widgets/sovereign/engine/syntax_engine.dart`:

- `commonMarkCore`
- `commonMarkGfm`

Key contracts:

- authoritative parse semantics come from backend snapshot reconciliation;
- externally visible offsets are UTF-16 code-unit indices;
- predictive behavior is UX-only and must converge to authoritative parse.

## 4) Supported Markdown Features (current state)

For full matrix with evidence and gaps:

- `docs/architecture/sovereign/sovereign_editor_markdown_support_matrix.md`

Current headline status:

- **Supported**: headings, blockquotes, unordered/ordered lists, task lists,
  fenced code blocks, inline bold/italic/code, links/autolinks, strikethrough,
  thematic breaks, indented code blocks, escapes/entities, reference links, and
  source-first GFM tables.
- **Supported with rich live UX**: fence enter/exit/indent/backspace policies,
  quote/list continuation/exit, command-layer toolbar actions, cursor-safety
  under predictive/authoritative reconciliation, table continuation/navigation,
  and table row/column commands.
- **Supported policy**: images stay markdown-source-first with network previews
  and non-network placeholders; raw HTML is preserved as literal text and never
  executed or rendered as HTML.

## 5) Platform Support

- Native runtime parser path is supported on:
  - macOS
  - iOS
  - Android
- Web is out of scope for the native parser path.

## 6) Command Surface (consumer API)

Primary action surface is `SovereignMarkdownCommands` via:

- `controller.commands.toggleInlineStyle(...)`
- `controller.commands.deactivateInlineStyle()`
- `controller.commands.setHeadingLevel(...)`
- `controller.commands.toggleQuote()`
- `controller.commands.toggleBulletList()`
- `controller.commands.toggleTaskList()`
- `controller.commands.insertHorizontalRule()`
- `controller.commands.insertFence(...)`
- `controller.commands.insertLink()`
- `controller.commands.insertTable(...)`
- `controller.commands.insertTableRowBelow()`
- `controller.commands.deleteTableRow()`
- `controller.commands.insertTableColumnRight()`
- `controller.commands.deleteTableColumn()`
- `controller.commands.resolveLinkEditContext()`
- `controller.commands.applyLinkEdit(...)`
- `controller.commands.capabilitiesAtSelection()`
- `controller.commands.runInTransaction(...)`

Detailed interface notes:

- `docs/architecture/rfc/sovereign_editor_command_interface.md`

## 7) Validation and Confidence Gates

Primary package gate:

```bash
flutter test
```

Fast maintenance gate:

```bash
./scripts/verify_package_confidence.sh
```

Native packaging gate:

```bash
./scripts/verify_native_editor_ci.sh
```

Conformance/parity and release-readiness details:

- `docs/production_readiness/public_api_inventory_2026-05-01.md`
- `docs/production_readiness/native_packaging_2026-05-01.md`
- `docs/production_readiness/execution_log.md`
