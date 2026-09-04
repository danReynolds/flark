# Rendered editing behavior

**Profile:** `flark-edit-v1`
**Status:** active product contract. v5's kernel (M2, 2026-09-03) implements every headless rule below with a journey in `packages/flark/test/journeys/`; paste routes, composition, and pointer geometry are covered by the M3 surfaces.
**Product goal:** [Flark North Star](../../../NORTH_STAR.md)

## Purpose

This contract moved from `docs/architecture/v4/contracts/` when v5 adopted it unchanged; RFC 030 §6 records how the v5 kernel realizes its caret and boundary rules.

This document defines what common editing actions mean when users edit rendered
Markdown while exact Markdown source remains canonical. It is the active
behavior reference for implementation and tests. RFC 028 records the underlying
transaction architecture but does not add another user-facing rule system.

Flark behaves like a native rich-text editor backed losslessly by Markdown:

- users edit visible graphemes, semantic spans, and block structures;
- hidden delimiters are not independent caret stops or deletion targets;
- literal and incomplete syntax remains visible authoring content;
- exact committed Markdown is always available for export; and
- no parser, scheduler, or paint phase may contradict an accepted command.

## Small vocabulary

- **Rendered grapheme:** one user-visible deletion or movement unit.
- **Semantic context:** formatting intent at the current rendered caret, such
  as Emphasis or Strong.
- **Exact island:** the smallest parser-authorized source range that must remain
  literal because its current rendered meaning is uncertain.

These are implementation-facing descriptions of visible behavior, not
additional product principles or testing layers.

## General rules

1. A logical command is identified before Markdown interpretation. Keyboard,
   pointer, replacement, paste, composition, history, and structural actions
   are not inferred from coincidentally similar callback payloads.
2. The parser identifies the rendered owner, adjacent grapheme, semantic
   context, and affected source range. Dart and Flutter do not scan delimiters
   to invent Markdown behavior.
3. One accepted command commits source, selection, formatting intent, history,
   and presentation authority as one ordered result.
4. Source outside the affected semantic closure remains byte-for-byte exact.
5. When several source spellings preserve the same intent, retain the existing
   parser-authenticated spelling where possible.
6. Unsupported or stale commands fail before mutation. They may not partially
   change source, selection, history, or visible presentation.

## Inline editing

### Insertion

- Typing inside a semantic span continues that semantic context.
- Typing at a visible boundary uses the context selected by the caret target,
  pointer hit, or preceding navigation command.
- Typing ordinary whitespace after an emptied inline owner exits that owner
  unless a supported construct explicitly retains whitespace.
- Completing source-authored delimiters may atomically turn literal text into a
  rendered construct. The inserted delimiter itself must not flash as an
  unrelated intermediate state.

### Replacement

- Replacing a rendered range inserts the provided text once and selects or
  places the caret at the logical replacement end.
- A replacement wholly inside compatible formatting retains that formatting.
- A replacement crossing unsupported owners fails before mutation rather than
  guessing at Markdown closure.

### Backspace and Delete

- Backspace removes the previous rendered grapheme; Delete removes the next
  rendered grapheme.
- Hidden opening and closing delimiters are never separate deletion steps.
- Deleting content from a styled span preserves unaffected surrounding source,
  styling, and block presentation.

### EP1-DELETE-TO-EMPTY-001

Deleting the final rendered grapheme of Emphasis, Strong, Strikethrough, Inline
Code, or another supported inline owner removes that owner from committed
source. Empty delimiters must not remain visible as literal markers.

The caret lands at the visible deletion point and the editor remains writable.
The next ordinary character recreates the previous semantic context when the
command began inside that context; ordinary whitespace exits it. Escaped or
otherwise literal delimiters remain literal and are deleted as visible
characters.

This behavior must be tested in both directions, for nested formatting, and as
an immediate delete-then-type sequence. The reported `*t*` Backspace failure is
the smallest mounted regression case.

## Caret, selection, and boundaries

- Arrow movement advances by visible caret targets, not hidden source offsets.
- Pointer placement chooses a parser-authored target using actual glyph
  geometry. When two semantic contexts share one visual boundary, leading and
  trailing glyph halves may select different targets.
- Selection direction and affinity survive controller, platform-input, layout,
  and paint mapping.
- Collapsing a range chooses the appropriate visible edge and immediately
  establishes the semantic context for the next command.
- A caret target or formatting context bound to an older source revision or
  selection generation is rejected.

## Structural editing

Return and Backspace operate on the visible block structure:

- Return splits a paragraph or heading at the caret;
- Return continues or exits supported list and quote structures;
- terminal Return creates one writable following paragraph;
- Backspace at a supported block start merges, lifts, or removes the structural
  boundary users see; and
- repeated Return or Backspace followed immediately by typing must leave one
  live caret and accept the next input.

Structural source markers remain hidden unless they are intentionally literal
or inside the smallest exact island.

## History and platform input

- Undo restores the exact prior source, selection, and semantic typing intent.
- Redo reapplies the logical result using fresh current-revision authority.
- One logical user action creates at most one history entry.
- Equivalent full-value, delta, key, paste, and composition delivery routes
  produce the same accepted logical command.
- Duplicate platform callbacks must not duplicate source mutations.

Paste, composition, clipboard, dictation, and platform-specific selection
behavior require native qualification in addition to Core and mounted tests.

## Presentation result

### EP1-RESULT-PRESENTATION-001

Every accepted source mutation returns enough parser-owned information to paint
the complete current result or the smallest exact island plus unchanged
rendered surroundings. That result is bound to the committed source revision
and affected range.

Flutter may validate and render this information. It may not reconstruct the
result with delimiter scans, character allowlists, or stale row structure.

Source, selection, rendered runs, block presentation, caret target, geometry,
semantics, and available actions publish atomically. A later parser result may
replace that snapshot only when it belongs to the same or a newer accepted
generation.

## Required D0 behavior

| Area | Required coverage |
| --- | --- |
| Inline owners | Emphasis, Strong, Strikethrough, Inline Code, representative nesting, and escaped-literal controls |
| Commands | Insert, Backspace, Delete, range replacement, Return, selection collapse, Undo, and Redo |
| Boundaries | Inside, outside, opening edge, closing edge, pointer placement, and arrow traversal |
| Sequences | Delete-to-empty then type, repeated Return then type, terminal-gap Backspace then type, and delete/insert Undo/Redo |
| Presentation | Current source, rendered text, style, block presentation, caret, selection, geometry, and no unrelated marker exposure on every paint |
| Scale | The supported document presets, viewport movement, resize, parser backlog, and rapid input budgets in the dogfood milestone |

The exact cases live beside the production tests that execute them. There is no
separate scenario registry or conformance claim based only on fixture metadata.

## Explicitly unsupported for D0

- arbitrary cross-owner and cross-block range transformations;
- general table-object restructuring and arbitrary list nesting changes;
- every possible Markdown delimiter collision;
- full physical iOS and Android qualification; and
- performance or platform claims beyond the measured dogfood configuration.

An unsupported command must remain safe and writable, but common D0 actions may
not be relabeled unsupported merely because exact-source fallback avoids data
loss.

## Test rule

For each supported behavior, add the smallest direct test at the lowest layer
that can observe failure. Add a controller test only for delivery or publication
ordering, a mounted test only for actual paint or geometry, and a native test
only for OS-owned behavior. Generated exploration may discover cases, but each
kept regression becomes an ordinary readable test.
