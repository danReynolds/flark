# Flark Editor Command Interface

**Status**: implemented (source-of-truth API reference)  
**Status date**: 2026-05-02
**Related RFC**: `docs/architecture/rfc/rfc_016_sovereign_markdown_command_layer.md`  
**Scope**: live markdown edit actions invoked by external consumers (toolbars, menus, shortcuts)

---

## 1. Why this layer exists

Flark should own markdown mutation semantics, not app surfaces.

The command interface gives consumers a stable API:

- UI triggers intent (`toggle bold`, `insert quote`, `apply link`).
- Flark performs mutation atomically with cursor/selection safety.
- Mobile and desktop get the same behavior path.

---

## 2. Design principles (best-practice baseline)

The interface follows patterns used by mature editor stacks (for example ProseMirror, Lexical, Slate, CodeMirror):

1. **Command facade, not text surgery in UI**
   - UI should not manually splice markdown strings.
2. **Deterministic transform contract**
   - same `(text, selection, args)` must produce same result.
3. **Atomic transaction**
   - multi-step edits commit once (single undo unit).
4. **Typed command results**
   - explicit `Applied | NoOp | Rejected` instead of hidden failures.
5. **Strict module boundaries**
   - inline/block/link/fence handlers are isolated.
6. **Engine invariants first**
   - command output must remain projection-safe and cursor-safe.

These are standard command architecture traits, not custom novelty.

---

## 3. Public API shape

```dart
enum FlarkInlineStyle { bold, italic, inlineCode }
enum FlarkBlockStyle { quote, bulletList, taskList, heading, fence }

sealed class FlarkCommandResult {
  const FlarkCommandResult();
}

class FlarkCommandApplied extends FlarkCommandResult {
  final TextSelection selection;
  const FlarkCommandApplied(this.selection);
}

class FlarkCommandNoOp extends FlarkCommandResult {
  final String reason; // stable wire string, e.g. "no_active_inline_style"
  final FlarkCommandReasonCode reasonCode;
  const FlarkCommandNoOp(this.reason);
}

class FlarkCommandRejected extends FlarkCommandResult {
  final String reason; // stable wire string, e.g. "ime_composing"
  final FlarkCommandReasonCode reasonCode;
  const FlarkCommandRejected(this.reason);
}

class FlarkCommandCapabilities {
  final bool isComposing;
  final bool canMutate;
  final FlarkInlineStyle? activeInlineStyle;
  final int? activeHeadingLevel;
  final bool quoteActive;
}
```

Facade entry points:

- `toggleInlineStyle(controller, style)`
- `deactivateInlineStyle(controller)`
- `getInlineStyleAtSelection(controller)`
- `setHeadingLevel(controller, level)`
- `getHeadingLevelAtSelection(controller)`
- `toggleQuote(controller)`
- `isQuoteActiveAtSelection(controller)`
- `toggleBulletList(controller)`
- `toggleTaskList(controller)`
- `insertHorizontalRule(controller)`
- `insertFence(controller, {language})`
- `insertLink(controller)`
- `insertTable(controller, {columns, bodyRows})`
- `insertTableRowBelow(controller)`
- `deleteTableRow(controller)`
- `insertTableColumnRight(controller)`
- `deleteTableColumn(controller)`
- `resolveLinkEditContext(controller)`
- `applyLinkEdit(controller, context, label, url)`
- `capabilitiesAtSelection(controller)`
- `runInTransaction(controller, action)`

Optional convenience:

- `controller.commands.toggleInlineStyle(...)` (extension wrapper only)

---

## 4. Internal modular structure

```
commands/
  sovereign_markdown_commands.dart      // public facade
  models/
    sovereign_inline_style.dart
    sovereign_block_style.dart
    sovereign_link_edit_context.dart
    sovereign_command_result.dart
  internal/
    command_context.dart                // safe snapshot helpers
    command_transaction.dart            // single commit helper
    command_selection.dart              // clamping + selection math
    command_ranges.dart                 // wrappers/links/line parsers
    inline_commands.dart
    block_commands.dart
    link_commands.dart
    fence_commands.dart
    table_commands.dart
```

Rules:

- `internal/*` cannot import UI/presentation widgets.
- Handlers can depend on shared internal helpers, never on each other directly.
- Facade coordinates; handlers mutate.

---

## 5. Supported action catalog

### 5.1 Inline actions

| Action | Inputs | Behavior | Selection contract | Result notes |
| --- | --- | --- | --- | --- |
| `toggleInlineStyle(bold)` | caret or selection | wrap/unwrap/switch bold wrapper | selection remains inside active wrapper when activating; moves outside when deactivating non-empty wrapper | `NoOp` if state already canonical |
| `toggleInlineStyle(italic)` | caret or selection | wrap/unwrap/switch italic wrapper | same as above | prevents delimiter run collisions |
| `toggleInlineStyle(inlineCode)` | caret or selection | wrap/unwrap/switch code wrapper | same as above | preserves atomic undo |
| `deactivateInlineStyle()` | caret in wrapper | exit active wrapper | collapsed selection outside suffix | `NoOp` if no active wrapper |
| `getInlineStyleAtSelection()` | caret/selection | inspect enclosing wrapper | no mutation | returns style or null |

### 5.2 Block actions

| Action | Inputs | Behavior | Selection contract | Result notes |
| --- | --- | --- | --- | --- |
| `toggleQuote()` | current line(s) | add/remove `> ` prefix | keeps column as stable as possible | no inline mutations |
| `isQuoteActiveAtSelection()` | caret/selection | detect quote marker context | no mutation | supports toolbar active-state |
| `toggleBulletList()` | current line(s) | add/remove list marker | caret/selection mapped to transformed lines | marker style normalized |
| `toggleTaskList()` | current line(s) | add/remove or toggle task marker | preserves line anchoring | delegates checkbox semantics to editor rules |
| `setHeadingLevel(level)` | current line(s) | set or clear heading marker | keeps logical content column stable | `level == null` clears heading |
| `getHeadingLevelAtSelection()` | caret/selection | detect active heading level | no mutation | supports heading picker state |

### 5.3 Fence action

| Action | Inputs | Behavior | Selection contract | Result notes |
| --- | --- | --- | --- | --- |
| `insertFence(language)` | caret or selection | insert fenced block scaffold | selection targets inner body | does not own enter/exit navigation policies |

### 5.4 Link actions

| Action | Inputs | Behavior | Selection contract | Result notes |
| --- | --- | --- | --- | --- |
| `insertLink()` | caret or selection | insert base markdown link scaffold | caret placed for immediate editing | used by toolbar quick-insert path |
| `resolveLinkEditContext()` | caret/selection | detect existing link span or insertion point | no mutation | used by UI modals |
| `applyLinkEdit(context, label, url)` | validated dialog input | replace/insert `[label](url)` | caret moved after link | `Rejected` on invalid URL/text constraints |

### 5.5 Table actions

| Action | Inputs | Behavior | Selection contract | Result notes |
| --- | --- | --- | --- | --- |
| `insertTable(columns, bodyRows)` | caret or selection | insert source-aligned GFM table scaffold | caret placed in first body cell | clamps to at least two columns and one body row |
| `insertTableRowBelow()` | caret in established table | insert aligned empty body row below current editable row | caret placed in new row first cell | `NoOp` outside established tables or fenced code |
| `deleteTableRow()` | caret in body row | delete current body row and realign remaining table | caret moves to adjacent editable row/cell | `NoOp` on header/separator rows |
| `insertTableColumnRight()` | caret in established table cell | insert aligned empty column to the right | caret placed in inserted cell | preserves separator alignment markers |
| `deleteTableColumn()` | caret in established table cell | delete current column and realign remaining table | caret moves to nearest remaining cell | `NoOp` when deletion would leave fewer than two columns |

### 5.6 State snapshot and transactions

| Action | Inputs | Behavior | Selection contract | Result notes |
| --- | --- | --- | --- | --- |
| `capabilitiesAtSelection()` | caret/selection | returns composing/mutation readiness + active inline/block state | no mutation | one-call toolbar snapshot |
| `runInTransaction(action)` | callback with command facade | executes multiple commands as one undo unit | command-defined | keeps mutation logic in Flark |

---

## 6. Command transaction contract

Every mutating command must:

1. Build a normalized context (`text`, clamped selection, composing state).
2. Return early with `Rejected("ime_composing")` when IME composing is active (unless command explicitly supports composing).
3. Produce one mutation payload:
   - `nextText`
   - `nextSelection`
   - `composing = TextRange.empty`
4. Commit once through transaction helper (`controller.value = ...`).
5. Return `Applied` with final selection.

No command may perform multiple direct `controller.value = ...` writes.

---

## 7. Invariants and boundaries

### Required invariants

1. Selection always within `[0, text.length]`.
2. Selection should not land in known hidden-marker trap ranges.
3. Multi-step edits are a single undo unit.
4. Command output is independent of UI surface.

### Boundary rules

1. Inline handler cannot edit heading/list/quote prefixes.
2. Block handler cannot parse inline wrappers.
3. Link handler cannot launch modal/dialog.
4. Fence handler cannot own arrow/enter exit navigation.
5. Table handler owns source-first GFM row/column transforms, not a separate
   grid widget model.

---

## 8. Result and error taxonomy

Standardize reason strings + typed reason codes to make telemetry and debugging consistent:

- `NoOp` examples:
  - `already_active`
  - `no_active_inline_style`
  - `empty_input`
- `Rejected` examples:
  - `ime_composing`
  - `invalid_selection`
  - `invalid_arguments`

This avoids ambiguous "nothing happened" behavior in consumers.

---

## 9. Consumer integration contract

### App code should do

1. Keep toolbar active-state UI.
2. Open dialogs (link, insert media, etc.).
3. Call command facade with validated inputs.

### App code should not do

1. Direct markdown string splicing for editor actions.
2. Manual caret offset arithmetic for command effects.
3. Divergent mobile/desktop mutation logic.

---

## 10. Testing requirements

### Package tests (authoritative)

1. One suite per action family (inline/block/link/fence).
2. Selection/cursor invariants verified after each command.
3. Undo grouping checks for every mutating command.
4. IME guard behavior coverage (`Rejected` cases).

### App tests (thin integration)

1. Toolbar button invokes expected command.
2. Dialog outputs map to command inputs correctly.
3. No duplicate mutation logic in app layer.

---

## 11. Migration checklist (from app-local formatter)

1. Introduce package command facade and handlers.
2. Move existing mutation logic into package with parity tests.
3. Convert app `MarkdownFormatting` to pass-through adapter.
4. Deprecate and remove adapter once call sites migrate.
5. Keep only UI orchestration in app.

---

## 12. Future extension model

When adding a new markdown command:

1. Add typed command API and result behavior.
2. Add handler in one domain file only.
3. Add package tests for behavior + invariants.
4. Wire consumer UI through facade.
5. Do not bypass transaction helper.
