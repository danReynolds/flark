# RFC 016: Sovereign Markdown Command Layer

## 1. Summary

Define a first-class markdown command API inside `sovereign_editor` and move toolbar/edit-intent behavior out of app-local `MarkdownFormatting`.

Detailed API/module spec:

- `docs/architecture/rfc/sovereign_editor_command_interface.md`

This keeps:

- **UI intent orchestration** in app code (mobile/desktop toolbar widgets, dialogs).
- **Markdown mutation semantics** in Sovereign (single source of truth).

Primary outcome: one canonical implementation path for markdown actions across mobile and desktop, with fewer cursor/selection regressions.

---

## 2. Problem

Today, app-level `lib/routes/posts/create/logic/markdown_formatting.dart` performs direct text surgery for:

- inline wrappers (bold/italic/code),
- block transforms (quote/list/heading/fence),
- link editing insert/replace behavior.

Sovereign simultaneously owns projection, marker hiding, cursor safety, and edit policies. This split causes drift:

1. UI commands can violate engine assumptions (cursor projection, hidden marker boundaries).
2. Mobile/desktop behavior can diverge unless both explicitly normalize state.
3. Fixes require cross-layer patches (composer + editor internals) instead of one engine change.

---

## 3. Goals

1. **Single behavior authority** for markdown editing commands.
2. **Shared UX semantics** between mobile and desktop toolbars.
3. **Cursor-safe mutations** that are engine-native and testable in package tests.
4. **Composable intent API** so app UIs can wire actions without text-level logic.

## 4. Non-goals

1. Replacing Sovereign rendering architecture.
2. Rewriting native parser integration.
3. Introducing feature flags for migration (greenfield assumption).

---

## 5. Proposed Architecture

### 5.1 Public API shape (consumer-facing)

Expose a single typed facade in package API:

- `class SovereignMarkdownCommands`
- `enum SovereignInlineStyle { bold, italic, inlineCode }`
- `enum SovereignBlockStyle { quote, bulletList, taskList, heading, fence }`
- `class SovereignLinkEditContext`
- `class SovereignCommandCapabilities`
- `sealed class SovereignCommandResult`
  - `Applied`
  - `NoOp`
  - `Rejected`
  - typed reason code (`SovereignCommandReasonCode`) + stable wire string

Primary methods:

- `toggleInlineStyle(SovereignController, SovereignInlineStyle)`
- `deactivateInlineStyle(SovereignController)`
- `getInlineStyleAtSelection(SovereignController)`
- `setHeadingLevel(SovereignController, int? level)`
- `getHeadingLevelAtSelection(SovereignController)`
- `toggleQuote(SovereignController)`
- `isQuoteActiveAtSelection(SovereignController)`
- `toggleBulletList(SovereignController)`
- `toggleTaskList(SovereignController)`
- `insertHorizontalRule(SovereignController)`
- `insertFence(SovereignController, {String language = 'plain'})`
- `insertLink(SovereignController)`
- `resolveLinkEditContext(SovereignController)`
- `applyLinkEdit(SovereignController, ...)`
- `capabilitiesAtSelection(SovereignController)`
- `runInTransaction(SovereignController, action)` (group multiple commands into one undo unit)

Optional ergonomics (no new semantics):

- controller extension: `controller.commands.toggleInlineStyle(...)`.

### 5.2 Internal layering (modularity-first)

The facade must remain thin. It routes to domain handlers:

1. `inline_commands.dart`
2. `block_commands.dart`
3. `link_commands.dart`
4. `fence_commands.dart`

Shared infrastructure:

- `command_context.dart` (safe text/selection snapshot)
- `command_transaction.dart` (atomic commit path)
- `command_selection.dart` (clamp + cursor-safe helpers)
- `command_ranges.dart` (parsers for wrapper/link/line ranges)

Rule: handlers are pure edit-semantic modules; they do not import UI/presentation code.

### 5.3 Execution contract

All commands MUST:

1. Read from normalized context (text, safe selection, composing state).
2. Produce at most one atomic mutation payload (`nextText`, `nextSelection`).
3. Commit through a shared transaction helper only.
4. Return `SovereignCommandResult` (no silent failures).

Transaction helper MUST guarantee:

1. Mutation via `controller.value = ...` only.
2. Composing safety (`composing: TextRange.empty` unless IME-specific path).
3. Existing undo boundary behavior preserved.
4. Selection ends in projector-safe location.

### 5.4 Anti-spaghetti boundaries

Per-module responsibility:

- Inline: wrappers, inline active-state detection, switch/deactivate behavior.
- Block: heading/quote/list/task prefix transforms.
- Link: resolve/apply markdown link edits only.
- Fence: fence insertion helpers (not navigation/exit policies).

Hard boundaries:

1. Inline handlers cannot edit block prefixes.
2. Block handlers cannot inspect inline wrappers.
3. Link handlers cannot invoke modals or UI prompts.
4. Multi-step transforms (e.g. bold->italic switch) must commit atomically in one transaction.

### 5.5 Ownership split

- App code keeps:
  - toolbar button state,
  - modal presentation,
  - UX choreography (which dialog to show).
- Sovereign keeps:
  - markdown mutation logic,
  - active-inline detection logic,
  - wrapper exit/switch edge cases.

### 5.6 Inline-mode model

Near-term (parity): retain current wrapper semantics but move implementation into package.

Follow-up (hardening): remove dependence on expanded placeholder selection as mode indicator; replace with collapsed-caret-safe inline-mode representation to avoid cursor-visibility edge cases on iOS.

### 5.7 Suggested package layout

```
packages/sovereign_editor/lib/widgets/sovereign/commands/
  sovereign_markdown_commands.dart
  models/
    sovereign_inline_style.dart
    sovereign_block_style.dart
    sovereign_link_edit_context.dart
    sovereign_command_result.dart
  internal/
    command_context.dart
    command_transaction.dart
    command_selection.dart
    command_ranges.dart
    inline_commands.dart
    block_commands.dart
    link_commands.dart
    fence_commands.dart
```

Only facade + model types are exported. `internal/` remains package-private.

---

## 6. Migration Plan

### Implementation checklist (PR-sequenced)

- [x] **PR1 Scaffold**
  - add command package structure (`facade`, `models`, `internal` helpers),
  - export command types from package API,
  - no behavior changes yet.

- [x] **PR2 Inline parity port**
  - move inline wrapper behavior into package command handler,
  - preserve existing semantics (toggle/switch/deactivate),
  - add package parity tests for inline flows + undo units.

- [x] **PR3 Block command port**
  - move heading/quote/list/task transforms into package command handler,
  - add package tests for line transforms + selection stability.

- [x] **PR4 Link + fence command port**
  - move link resolve/apply + fence insertion into package handler modules,
  - add package tests for insert/replace contexts and caret placement.

- [x] **PR5 App adapter flip**
  - convert app-local formatter methods to thin pass-through adapters,
  - keep signatures stable to avoid call-site churn.

Current status note:

- app adapter flip completed for inline + block + link + fence actions,
- mobile/desktop/legacy composer toolbars now invoke `controller.commands` directly,
- app-local `markdown_formatting.dart` adapter was removed during cleanup.

- [x] **PR6 Surface unification**
  - mobile and desktop toolbars consume the same command APIs/results,
  - remove surface-specific markdown mutation logic.

- [x] **PR7 App cleanup**
  - remove deprecated adapter cruft,
  - retain only UI orchestration helpers if needed.

- [x] **PR8 Hardening gate**
  - package command tests comprehensive and green,
  - app smoke tests green,
  - IME/cursor/undo regressions explicitly covered.

---

## 7. Test Strategy

### 7.1 Package unit/widget coverage (required)

- Inline toggles:
  - collapsed caret insert,
  - selection wrap,
  - switch bold↔italic↔code,
  - deactivate behavior.
- Block toggles:
  - quote/list/task/heading/fence enter/exit flows.
- Link commands:
  - resolve existing link,
  - replace link,
  - insert new link.
- Cursor invariants:
  - no hidden-marker cursor traps,
  - no invalid selection after command.

### 7.2 App integration coverage (thin)

- Mobile toolbar calls command API.
- Desktop toolbar calls same API.
- Link modal applies edits through command API.

---

## 8. Risks and Mitigations

1. **Behavior drift during port**
   - Mitigation: parity tests before/after port, temporary adapter layer.
2. **Undo grouping regressions**
   - Mitigation: explicit undo tests per command.
3. **IME edge regressions**
   - Mitigation: compose-state guard tests for command execution.

---

## 9. Acceptance Criteria

RFC is complete when:

1. Mobile and desktop markdown actions invoke only package command APIs.
2. App-local markdown text surgery is removed.
3. Command behavior is covered by package tests (not route-specific tests).
4. Cursor visibility and selection stability regressions are green on iOS and Android.
