# Rendered editing behavior

**Profile:** `flark-edit-v1`
**Status:** active product contract. v5's kernel covers the currently supported
headless rules with direct scenarios in `packages/flark/test/`; host revision,
paste, composition, and pointer geometry are M3 surface obligations.
**Product goal:** [Flark North Star](../../../NORTH_STAR.md)

## Purpose

This contract moved from `docs/architecture/v4/contracts/` and was rebased onto
v5's synchronous execution model. RFC 030 §6 records how the kernel realizes
its caret and boundary rules.

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
- Typing a space advances the source and painted caret; the next word stays
  after that space. Editable trailing whitespace, including table-cell padding,
  remains represented. A parser-authenticated hard break stays one atomic
  rendered unit when moving or deleting across the break.
- Typing ordinary whitespace after an emptied inline owner exits that owner
  unless a supported construct explicitly retains whitespace.
- Typing whitespace at an existing emphasis/strong/strike content edge moves
  that whitespace outside the parser-owned delimiters before publication.
  Surviving text stays styled and the next word retains the typing context,
  including after repeated spaces and Undo/Redo. Erasing the separating spaces
  returns to the surviving owner. Source mode retains literal source editing.
- Completing source-authored delimiters may atomically turn literal text into a
  rendered construct. The inserted delimiter itself must not flash as an
  unrelated intermediate state.

### Replacement

- Replacing a rendered range inserts the provided text once and selects or
  places the caret at the logical replacement end.
- A replacement wholly inside compatible formatting retains that formatting.
- Selecting a word at an owner's visible edge may include that edge's hidden
  delimiter. When the visible selection stays inside the owner, edit its
  content; the delimiter anchor must not make ordinary replacement fail.
- In rendered mode, the first Select All within one fenced code region selects
  its projected body, excluding fence markers and surrounding prose. The next
  Select All expands to the document. Other editing/navigation commands restart
  this sequence. In an empty fence, the first command stays at its empty body;
  pasting then must not replace the document. Source mode, prose and selections
  spanning regions select the document immediately.
- Whole-document Select All preserves the exact noncollapsed `0..source.length` range,
  including leading/trailing block syntax. Replacing or deleting that range
  replaces the whole document in one undoable transaction; the next typed
  character uses the new document's context. Ordinary caret placement still
  legalizes to rendered content. This whole-document action is supported even
  when partial cross-block transformations are not.
- A replacement crossing unsupported owners fails before mutation rather than
  guessing at Markdown closure.

### Backspace and Delete

- Backspace removes the previous rendered grapheme; Delete removes the next
  rendered grapheme.
- Hidden opening and closing delimiters are never separate deletion steps.
- Deleting content from a styled span preserves unaffected surrounding source,
  styling, and block presentation.
- Word deletion uses the same word boundaries as word navigation. It is one
  history action, followed by an independently undoable typed character.
- If deletion exposes whitespace against emphasis/strong/strike delimiters,
  move that whitespace outside the owner so surviving words retain their
  style. Keep a legal caret in surviving leading content, and retain typing
  intent when deletion exits an owner's trailing whitespace.

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
- Up and Down cross every empty visual line and row boundary in both directions,
  preserving the horizontal goal through short lines. Shift extends the original
  selection base, and the next key edits at the reached line.
- Pointer placement chooses a parser-authored target using actual glyph
  geometry. At the start or end of a word beside whitespace or a row edge,
  choose that word's formatting, including when the hit falls slightly across
  the painted caret. Clicking after the following space chooses its own
  context. Between adjacent non-whitespace glyphs with different formatting,
  the glyph half distinguishes their targets. Keyboard navigation keeps its
  separate context-preserving rules and explicit formatting toggles.
- Selection direction and affinity survive controller, platform-input, layout,
  and paint mapping.
- Double-click selects the laid-out visible word. Replacement and Undo use
  the same semantic selection rules as keyboard selection.
- Command-Up/Down and Control-Home/End move to the document edges. Shift
  extends from the existing selection base, retaining the complete source
  when the range reaches both document edges.
- Collapsing a range chooses the appropriate visible edge and immediately
  establishes the semantic context for the next command.
- A caret target or formatting context bound to an older source revision or
  selection generation is rejected.
- An editable checkbox uses the click pointer over the same hit region that
  activates it. Text and read-only regions keep their appropriate cursors.

## Structural editing

Return and Backspace operate on the visible block structure:

- Return splits a paragraph or heading at the caret;
- Return continues or exits supported list and quote structures;
- terminal Return creates one writable following paragraph;
- table Return moves to the next row in the same column, then exits the table;
  cell-boundary deletion rejects atomically, and table restructuring uses source mode;
- Backspace at a supported block start merges, lifts, or removes the structural
  boundary users see; and
- repeated Return or Backspace followed immediately by typing must leave one
  live caret and accept the next input.

Structural source markers remain hidden when the current parse recognizes them;
intentionally literal or incomplete syntax remains visible authoring content.

### Typed fence creation

In rendered mode, typing the third backtick (or tilde) on an otherwise bare
opening-fence line immediately creates one empty code line and a matching
closing fence. The caret starts inside that line. A writable gap follows the
block; existing following prose, headings and code blocks stay outside it.
The parser authenticates the opener and the completed Markdown before the
single publication. Quotes and list items retain their continuation prefixes.

Enter continues code, including on an empty line inside a quote or list. Down
from the last code line reaches the following gap, where typing creates prose.
Backspace in an empty closed code block removes the fences and retains its
container context. Completion is its own Undo step: Undo restores the two
typed markers and their caret; Redo restores the empty bounded block.

This is a typing convenience. Paste, range replacement, source mode and IME
preedit preserve their literal input. Existing language tags are preserved;
under immediate creation, characters typed after the third marker enter the
code body. Opening-line padding and CRLF at the insertion site are retained.

### Code editing

Code selection is painted above the block background. Pointer selection,
replacement, copy/cut and history use the same projected text and source
coordinates as other rows.

Untagged fences receive automatic syntax coloring without changing Markdown.
When the caret is inside a fence, the toolbar offers Automatic, Plain text and
a language override. A manual choice edits only the first info-string token,
retains metadata and body text, maps the selection, and is one Undo action.
Automatic removes the language token; with remaining metadata it uses `auto`
to preserve the metadata's position. Unknown tags remain intact and uncolored.

Enter carries existing leading whitespace and parser-owned container prefixes.
An opening brace, bracket or parenthesis increases the indentation, and Enter
between a matching pair puts the closer on its own line. Python's trailing
colon also increases indentation. Recognized comments, strings and regex literals do not
trigger those rules. The step is two spaces, four for Python, or an existing
tab. Tab/Shift-Tab indent/outdent selected code lines without touching their
container prefixes; a collapsed Tab inserts a step at the caret. Commands
crossing a code-block boundary reject atomically.

Typing `}`, `]` or `)` on an indented, otherwise blank code line aligns it with
its matching opener's leading whitespace. A shared balanced-delimiter scan
skips literal token ranges, including enclosing string/comment/regex ancestry.
It operates on parser-owned code content and preserves quote/list prefixes.
The closer and whitespace change publish together as one Undo action. Inline
closers, unmatched/mismatched pairs, selected replacement, paste, composition
and source-mode input retain literal behavior. Unknown and Plain text languages
also retain literal indentation. The current YAML grammar marks flow punctuation
as string text, so its automatic closer falls back to literal input. The twelve
registered grammars each have a declared regression case; these examples are
coverage boundaries, not a guarantee for every construct in those languages.
This is a bounded editing aid, not a formatter or arbitrary-language parser.

Syntax decoration is pure Dart and theme-free. The initial grammar set is Dart,
Python, JavaScript, TypeScript, Rust, Go, JSON, YAML, SQL, shell, HTML/XML and CSS.
Detection examines at most 1,024 UTF-16 units. Blocks above 8,192 units fall back
to plain text; the host's narrower admission envelope still applies. Cached
tokens are bounded and must reconstruct the exact projected body text.
Decoration failures cannot alter or reject source input.

## History and platform input

- Undo restores the exact prior source, selection, and semantic typing intent.
- Redo reapplies the logical result using fresh current-revision authority.
- One logical user action creates at most one history entry.
- Equivalent full-value, delta, key, paste, and composition delivery routes
  produce the same accepted logical command.
- Duplicate platform callbacks must not duplicate source mutations.
- Browser copy/cut exports visible selected text in rendered mode and exact
  selected source in source mode. Copy Markdown remains the full-source export.
  A browser cut is one semantic deletion; native DOM source replacement must
  not also delete the range.

Paste, composition, clipboard, dictation, and platform-specific selection
behavior require native qualification in addition to Core and mounted tests.

## Presentation result

### EP1-RESULT-PRESENTATION-001

Inside the live tier, every accepted source mutation returns enough parser-owned
information to paint the complete current result. That result is bound to the
committed source revision. Outside the configured UTF-8 byte-and-shape
admission envelope, the editor publishes a source-mode snapshot and does not
retain a full parsed projection. A typed extraction deviation also keeps an
initially opened or already-source-mode document in source mode rather than
publishing an untrustworthy projection; the same deviation rejects an edit to
an existing live snapshot atomically. M2 implements the byte gate; M3 adds
shape admission before this becomes a product-qualified live boundary.

Flutter may validate and render this information. It may not reconstruct the
result with delimiter scans, character allowlists, or stale row structure.

Source, selection, rendered runs, block presentation, caret target, geometry,
semantics, and available actions publish atomically. The synchronous parser
answer is part of that publication; there is no later parser result to adopt.

## Required D0 behavior

| Area | Required coverage |
| --- | --- |
| Inline owners | Emphasis, Strong, Strikethrough, Inline Code, representative nesting, and escaped-literal controls |
| Commands | Insert, Backspace, Delete, range replacement, Return, selection collapse, Undo, and Redo |
| Boundaries | Inside, outside, opening edge, closing edge, pointer placement, and arrow traversal |
| Sequences | Author formatting, partially delete, type repeated spaces, continue a word, erase separators, and Undo/Redo; also delete-to-empty then type, repeated Return then type, terminal-gap Backspace then type |
| Presentation | Current source, rendered text, style, block presentation, caret, selection, geometry, and no unrelated marker exposure on every paint |
| Scale | The supported document presets, viewport movement, resize, live/source transitions, adversarial admitted shapes, and rapid input budgets in the dogfood milestone |

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
