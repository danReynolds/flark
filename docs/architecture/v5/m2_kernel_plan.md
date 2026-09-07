# M2 — the kernel

**Execution detail for M2 of the [v5 build plan](build_plan.md), revised
after the testing review (2026-09-04).** The kernel is `packages/flark`: pure Dart, no
Flutter import, under 8,000 production lines including the M1 parse layer
(about 1,100 lines today).

## What M1 settled that M2 builds on

- The render model is the whole contract with Rust. A row is a leaf block
  with per-line content records; hidden source code units are source minus content.
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
- CRLF is preserved exactly. Bare `\r` is outside the parser's fidelity
  contract and is rejected before mutation rather than rewritten silently.
- Every range is in bytes and UTF-16; the kernel works in UTF-16 and hands
  bytes to nothing.

## Concepts, in build order

1. **`FlarkDocument`** — immutable: `source`, `selection`, and the
   `RenderModel` for that source. `FlarkDocument.load(text)` validates host
   text and parses once. Producing the next document is a pure function of
   (document, command); history is facade-owned and publishes only after the
   candidate document succeeds.
2. **`FlarkProjection`** — from the model: `rows` (one per leaf block, in
   document order, plus container shells for lists and quotes as row
   metadata, not rows), each with `segments` of (`displayStart`,
   `displayEnd`, `sourceStart`, `sourceEnd`, `styleMask`, `exact`) and its
   `displayText`. Overlapping inline facts are cut at every run boundary
   and their styles merged. Bidirectional mapping: `sourceForDisplay(row,
   offset, affinity)` and `displayForSource(offset)`. M2 rebuilds this complete
   immutable projection on each accepted live edit; memoization remains a
   measured reserve rather than a second representation in the initial kernel.
3. **Caret and selection mapping** — `FlarkSelection` stores source offsets;
   the document maps them to `(row, displayOffset)` and exposes the legal
   anchors before and after hidden ranges. The chosen source offset is the
   typing context. Rules: typing keeps the current context; arrow keys cross a
   boundary on the first press; pointer placement takes the anchor from the
   glyph half the host reports; a caret is never inside a hidden or atomic
   replacement range; collapsing a selection picks the corresponding edge.
4. **`FlarkCommand`** — the closed set: `insertText`, `deleteBackward`,
   `deleteForward`, `newline`, `replaceRange`, `setSelection`, `moveCaret`
   (grapheme, word, line, rendered row; with extend), `undo`, `redo`, `toggleTask`,
   `indent`, `outdent`, `paste`, `toggleStyle` (emphasis, strong,
   strikethrough, code), `setHeadingLevel`. Every host route reduces to one
   of these before the kernel sees it.
5. **Semantics** — [`edit_profile_v1`](edit_profile_v1.md) as range arithmetic: deleting the last
   styled grapheme removes the run's hidden ranges with it; Return continues,
   exits, or splits a list item or quote using the block's per-line content
   ranges; Backspace at a block start lifts the prefix using the same
   ranges; typing at a boundary follows the anchor; a whitespace insert after
   an emptied inline owner exits it. Rust alone recognizes Markdown meaning;
   Dart may read parser-bounded source to preserve or emit canonical syntax for
   an explicit command, whose candidate parse must prove the intended result.
6. **History** — a one-second typing coalescing window, one logical action
   is at most one entry, undo restores exact source and selection, redo
   re-applies the logical result. The facade now supplies composition
   begin/commit/cancel transactions; native IME qualification remains M3a.
7. **`FlarkEditor`** — the facade and the only thing a host constructs:
   `snapshot`, `apply(command)`, `typingContext` (the style set the next
   keystroke inherits), a change listener, `sourceMode`, and the parse backend.
   A `FlarkLiveSnapshot` owns the document and projection; a
   `FlarkSourceSnapshot` owns exact source and selection and cannot expose a
   stale projection. The conditional-export union is capped at 29 concepts:
   the original 27 plus host-required admission limits and rejection reasons.
   Revision and composition methods stay on the existing facade.

## Direct editing scenarios

Each scenario is an ordinary typed Dart test: a starting source and caret,
then production commands, with the relevant expected state asserted after every
command—acceptance, source, row display text, caret/selection, and typing context.
Assert styles and container membership when those define the command's result;
static projection tests do not replace these semantic assertions. Expected
outcomes come from the edit profile, not from copying the implementation's
output. Cases live under `test/journeys/` only as a
small organizational grouping; there is no serialized command vocabulary,
fixture decoder, recorder, or replay framework.

M2a must make the following checks hold on every step of every direct scenario
and generated discovery run. The current helper proves only part of this
contract, so its green corpus result is not a completion receipt:

- display text contains no source code unit from a hidden range;
- display text equals source minus hidden ranges plus replacements;
- the caret is never inside a hidden range and its anchor is legal;
- `displayForSource(sourceForDisplay(x)) == x` for every legal position;
- history cases prove undo and redo restore the correct logical typing group;
- every live edit projects its newly parsed model with no incremental
  projection state to reconcile.

The coverage answer to v4's fixture drift is a finite table of supported
inline kind × boundary × command and block kind × structural-command cases,
expressed directly in the typed tests rather than a separate registry. Each
covered cell has an intended semantic result and immediate follow-up input.
Seeded histories remain a bounded discovery lane; they preserve uninterrupted
editing sequences, with history round-trip probes separate or occasional.
Every discovered failure is minimized into a direct regression.

The parse layer's own kernel test runs the projection invariants over all
1,322 conformance cases, which costs seconds and catches projection bugs
without any editing.

## Exit

- Every supported `edit_profile_v1` rule has a readable direct case; the finite
  table is complete and green; checks hold after each individual command;
  every minimized discovery failure has a deterministic regression. Each
  correction demonstrates a failing behavioral assertion before its fix.
- Representative temporary faults demonstrate detection of lost replacement
  text, incorrect styling/container membership, and wrong caret or typing
  context. Keep a short result in the closeout receipt; no permanent fault
  switches or mutation framework are required.
- The boundary test proves no Flutter import and keeps the conditional-export
  union at or under 29 concepts, with the command set counted once.
- Receipt: insert and Backspace through `FlarkEditor.apply` under 4 ms p99
  on the M1 Pro for the pinned 16 KiB-class structural fixture, named by commit,
  exact byte size/shape, runtime, parser artifact, and machine. Assert successful
  edits. This diagnostic explicitly admits its fixture even if it exceeds the
  product's 16 KiB fallback; it is not live-envelope qualification.
- Line budget: under 8,000 production lines in `packages/flark`.

Current local implementation and evidence: [implementation review](implementation_review_2026_09_04.md).
Named-commit CI and native/device qualification remain open.

Earlier status 2026-09-04: correction pass in progress after review found fail-open
derived ranges, an unversioned wire-format change, Unicode caret gaps,
non-transactional history, and adversarial projection cost. The earlier
1.50–1.58 ms p50 / 2.2–2.5 ms p99 at 25 KiB predates the corrected hot path and
no longer qualifies the current tree. A 2026-09-04 local diagnostic on the
uncommitted correction pass measured the 16 KiB-class structural fixture (16,750 bytes) at insert
1.78/2.18 ms p50/p99 and Backspace 2.60/3.14 ms, clearing the revised 4 ms
desktop-kernel gate at that point. Later review reproduced common semantic failures
despite green tests; a standard run on the dirty tree based on `9fcb092` used a
16,694-byte dense fixture and measured insert 3.38 ms p99 and Backspace 5.61 ms
p99 on the M1 Pro with Dart 3.12.2. The gate remains open. Neither local timing
is a committed receipt or full-frame live-envelope qualification.

What M2 built that the concept list above did not name: the caret is a
plain source offset (the anchor is which of several legal offsets sharing a
display position it holds); `FlarkSelection` plus the document's legality and
anchor queries is the complete model. Pending typing intent after
delete-to-empty or a formatting toggle is part of the history entry, so
undo restores it; Indent and Outdent nest by the sibling's or parent's
marker width from the model; fence lines hold no caret. Not built, by
decision: delimiter auto-close, table restructuring, cross-block range
transforms; a split of a styled span at the caret (Markdown's rule of three
makes `**a****b**` not parse), so a collapsed toggle inside a span unwraps
the whole span.

## Closeout order

### M2a — first implementation focus

Work through these families in order. Choose each expected result from the edit
profile, demonstrate the existing failure, fix the shared rule, and check the
neighboring supported cases before moving to the next family.

| Order | Family and current failure | Required result |
| --- | --- | --- |
| 1 | Return inside or at the end of a styled span exposes its delimiters; an empty list/quote exit becomes lazy continuation after typing | The supported split/exit preserves intended formatting and container membership, leaves the intended caret, and accepts the next character |
| 2 | Partial-owner selection edits accept unmatched delimiters | Supported replacements preserve unaffected formatting; unsupported Insert/Paste/ReplaceRange/Delete/Return routes reject without source, selection, intent, or history mutation |
| 3 | Deleting the final styled grapheme from outside recreates the style | Pending formatting reflects the starting context in both deletion directions; character/whitespace follow-up and Undo/Redo preserve that decision |
| 4 | Coverage and helpers overstate what a passing run establishes | Supported styles/boundaries have direct cases, every repeated command is checked, replacement values have independent expectations, and discovery preserves natural histories |

The first reviewable change is the Return family in `editor.dart` and the direct
structure/inline cases, with only the assertion support those cases require.
Correct the existing list-exit expectation to assert an actual exit after the
next character. Seed it with `*t*` at source caret 2 followed by Return, and
`- one\n- ` at source caret 8 followed by Return then `p`; both exposed the gap
in the 2026-09-04 review. Expand the small supported-style table as each family lands;
retain explicit atomic rejection tests for unsupported transformations.

Finish with the temporary fault checks from the testing strategy. If another
variant escapes a claimed family-level correction, review the general rule and
its owning layer before adding another special case.

### M2b — qualification after semantics stabilize

1. Add successful-edit checks and a small structured output to
   `tool/bench_editor.dart`; pin the measured fixture and record its exact shape.
2. Profile the corrected Backspace path and resolve its 4 ms p99 miss. Keep
   optimization within the existing parser/projection/command responsibilities.
3. Run analysis, the complete direct suite, bounded generated discovery, Rust
   gates, generated-schema checks, native/bundled/fresh-Wasm parity, and the
   Rust-free consumer smoke.
4. Record the named-commit performance result and verify CI for that candidate.
   Missing or environment-blocked checks remain open. This closes M2, not M3.

M3a owns real composition, wrapped geometry, shared shape admission, rejection
and recovery UX, and the deferred formatting-toggle experiment. These do not
expand the first M2a change into a new editor framework.
