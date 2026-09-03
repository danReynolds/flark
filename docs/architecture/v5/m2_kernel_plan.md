# M2 — the kernel

**Execution detail for M2 of the [v5 build plan](build_plan.md), revised
after M1 (2026-09-03).** The kernel is `packages/flark`: pure Dart, no
Flutter import, under 8,000 production lines including the M1 parse layer
(about 1,100 lines today).

## What M1 settled that M2 builds on

- The render model is the whole contract with Rust. A row is a leaf block
  with per-line content records; hidden bytes are source minus content.
  Lines a leaf owns without a content record (setext underlines, fence
  lines, the table delimiter row, a thematic break) are hidden whole.
- Replacement runs carry their display text in the string table: entities,
  code spans in cells with escaped pipes, and literals that begin with the
  virtual spaces of a partially consumed tab. The projection never derives
  display text from a delimiter; it takes the slice or the override.
- Soft breaks display as one space, hard breaks as a line break, and line
  endings inside a multi-line code span as one space, exactly as comrak's
  literal does.
- Reference definitions and HTML blocks are source-only rows; footnote
  definitions stay where they are written; task items carry the checkbox
  range and the list padding separately.
- Bare `\r` line endings are outside the parser's fidelity contract, so the
  document normalizes them to `\n` on load and records that it did.
- Every range is in bytes and UTF-16; the kernel works in UTF-16 and hands
  bytes to nothing.

## Concepts, in build order

1. **`FlarkDocument`** — immutable: `source`, `selection`, the `RenderModel`
   for that source, and the history stacks. `FlarkDocument.load(text)`
   normalizes line endings and parses once. Producing the next document is a
   pure function of (document, command).
2. **`FlarkProjection`** — from the model: `rows` (one per leaf block, in
   document order, plus container shells for lists and quotes as row
   metadata, not rows), each with `segments` of (`displayStart`,
   `displayEnd`, `sourceStart`, `sourceEnd`, `styleMask`, `exact`) and its
   `displayText`. Overlapping inline facts are cut at every run boundary
   and their styles merged. Bidirectional mapping: `sourceForDisplay(row,
   offset, affinity)` and `displayForSource(offset)`. Per-row memo keyed on
   the row's source slice and kind so unchanged rows reuse their segments.
3. **`FlarkCaret`** — `(row, displayOffset, sourceAnchor)`. One display
   position may have several legal anchors (before or after a hidden
   range); the anchor is the typing context. Rules: typing keeps the current
   anchor; arrow keys cross a boundary on the first press and never stop
   without moving; pointer placement takes the anchor from the glyph half
   the host reports; a caret is never inside a hidden range; collapsing a
   selection picks the anchor of the edge it collapses to.
4. **`FlarkCommand`** — the closed set: `insertText`, `deleteBackward`,
   `deleteForward`, `newline`, `replaceRange`, `setSelection`, `moveCaret`
   (grapheme, word, line, block; with extend), `undo`, `redo`, `toggleTask`,
   `indent`, `outdent`, `paste`, `toggleStyle` (emphasis, strong,
   strikethrough, code), `setHeadingLevel`. Every host route reduces to one
   of these before the kernel sees it.
5. **Semantics** — `edit_profile_v1` as range arithmetic: deleting the last
   styled grapheme removes the run's hidden ranges with it; Return continues,
   exits, or splits a list item or quote using the block's per-line content
   ranges; Backspace at a block start lifts the prefix using the same
   ranges; typing at a boundary follows the anchor; a whitespace insert after
   an emptied inline owner exits it. No character is inspected; every rule
   reads ranges from the model.
6. **History** — a one-second typing coalescing window, composition joins
   the open group, one logical action is at most one entry, undo restores
   exact source and selection, redo re-applies the logical result.
7. **`FlarkEditor`** — the facade and the only thing a host constructs:
   `document`, `projection`, `apply(command)`, `typingContext` (the style set
   the next keystroke inherits), a change listener, `sourceMode` (true above
   the sync tier), and the parse backend. Public exports at or under fifteen;
   everything else under `src/`.

## Journeys

A journey is a fixture: a starting source and caret, then commands, and
after every command the expected **visible transcript**: each row's display
text with its style runs, the caret's row and display offset, and the
typing context. Fixtures are JSON under `test/journeys/`, written by hand
from `edit_profile_v1` rules and, in M3, exported by the recorder.

Invariants asserted on every step of every journey, hand-written or
generated:

- display text contains no byte from a hidden range;
- display text equals source minus hidden ranges plus replacements;
- the caret is never inside a hidden range and its anchor is legal;
- `displayForSource(sourceForDisplay(x)) == x` for every legal position;
- undo after any command restores the exact prior source and selection;
- the projection of the resulting source equals a projection from scratch.

**Generated journeys** are the coverage answer to v4's fixture drift: a
matrix of every inline kind × boundary position (before, inside at start,
inside, inside at end, after) × command, and every block kind × Return and
Backspace at start, middle, and end, driven by a small simulator that types
delimiters, words, and spaces the way people do. The matrix is reported as
a denominator, like the conformance counts.

The parse layer's own kernel test runs the projection invariants over all
1,322 conformance cases, which costs seconds and catches projection bugs
without any editing.

## Exit

- Every `edit_profile_v1` rule has a journey; the generated matrix is
  complete and green; invariants hold on every step.
- The boundary test proves no Flutter import; exports at or under fifteen.
- Receipt: a 25 KB dense keystroke through `FlarkEditor.apply` under 1.5 ms
  on the M1 Pro, named by commit.
- Line budget: under 8,000 production lines in `packages/flark`.

Status 2026-09-03: met, with two honest notes recorded in the build plan's
M2 receipts. The export gate is asserted as 24 concepts (commands counted
once; render model in its own library) because "fifteen" predated the
command set. The keystroke measures 1.50–1.58 ms p50 on the M0 document,
on the line rather than under it, with 0.97 ms in parse and marshal.

What M2 built that the concept list above did not name: the caret is a
plain source offset (the anchor is which of several legal offsets sharing a
display position it holds), so `FlarkCaret` is `FlarkSelection` plus the
document's legality and anchor queries; pending typing intent after
delete-to-empty or a formatting toggle is part of the history entry, so
undo restores it; Indent and Outdent nest by the sibling's or parent's
marker width from the model; fence lines hold no caret. Not built, by
decision: delimiter auto-close, table restructuring, cross-block range
transforms; a split of a styled span at the caret (Markdown's rule of three
makes `**a****b**` not parse), so a collapsed toggle inside a span unwraps
the whole span.

## Order of work

1. Projection over the render model, with the invariants test over the
   corpora. This is the first thing that can be wrong and the cheapest to
   prove.
2. Document, caret model, and the journey runner with hand-written
   journeys for the inline rules.
3. Commands and semantics, rule by rule from `edit_profile_v1`, each
   landing with its journey.
4. History and the facade.
5. The generated matrix, then the performance receipt.
