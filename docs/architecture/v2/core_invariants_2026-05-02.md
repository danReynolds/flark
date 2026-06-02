# Sovereign v2 Core Invariants

Status date: 2026-05-02

## Scope

These invariants describe the first v2 headless core slice under
`lib/src/v2/core`. They are intentionally narrow and should be updated whenever
the transaction, selection, history, projection, or command contracts change.

## Framework Boundary

- `lib/src/v2/core` must not import Flutter, `dart:ui`, or widget-layer types.
- Flutter text input, `EditableText`, gestures, layout, focus, overlays, and
  platform behavior belong in adapter packages outside the core.
- Core tests include a static import-boundary check.

## Document

- Markdown source text is canonical.
- `SovereignDocument.markdown` returns the durable source text.
- `SovereignTextBuffer` indexes source offsets as Dart/Flutter-compatible
  UTF-16 code units.
- Line starts are derived from source text and recomputed with immutable buffer
  replacement.
- Document revisions increment only when source text changes.

## Selection

- `SovereignSelection` is source-offset based.
- Selection offsets are validated against document text length.
- A selection may be collapsed or ranged.
- `start` and `end` are normalized views; `baseOffset` and `extentOffset`
  preserve direction.

## Source Operations

- `SovereignSourceOperation` is currently a replace-range primitive with
  insert/delete convenience factories.
- Operation ranges use offsets in the document state before the transaction.
- Replacement text length and deleted range length define the operation delta.
- Insertion-boundary mapping supports upstream and downstream affinity.

## Transactions

- A `SovereignTransaction` is an atomic list of non-overlapping source
  operations.
- Multi-operation transactions are applied against original offsets, not
  sequentially shifted offsets.
- Operations may be provided out of order; the transaction sorts them before
  validation and application.
- Overlapping operations are rejected.
- If `selectionAfter` is absent, the previous selection is mapped through the
  transaction.
- Transaction inversion uses the original document to recover deleted source
  text and produces undo transactions with `addToHistory: false`.
- Transaction metadata is typed through `SovereignTransactionMetadata`.

## History

- `SovereignHistoryStack` is immutable.
- History is a companion state object, not a field on `SovereignEditorState`.
- A future engine/runtime state should compose editor state, history, command
  state, and extension state.
- Recording a transaction stores both redo transactions and inverse undo
  transactions.
- Recording a new history transaction clears redo entries.
- Transactions with `metadata.addToHistory == false` are ignored by history.
- Transactions whose operations do not change source text are ignored by
  history.
- Adjacent transactions with the same non-null undo group id merge into one
  undo entry.
- Undo applies inverse transactions in reverse edit order.
- Redo applies forward transactions in original edit order.

## Open Design Questions

- Whether atomic multi-operation transactions need a formal `ChangeSet` type
  before table and paste commands are ported.
- Whether source text needs a piece-table or rope implementation after
  benchmark data exists.
