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
  including after repeated spaces and Undo/Redo. That word continues the span:
  its closing syntax moves past the word, so a phrase typed word by word is
  one span (`**one two**`, not `**one** **two**`). The parser must own that
  syntax as the span ending where the spaces begin, and must see one span from
  the same opener afterwards; otherwise the word takes its own delimiters.
  Erasing the separating spaces returns to the surviving owner. Source mode
  retains literal source editing.
- Completing source-authored delimiters may atomically turn literal text into a
  rendered construct. The inserted delimiter itself must not flash as an
  unrelated intermediate state.
- A parser-authenticated bare empty heading prefix (`#` through `######`) or
  unordered item marker (`*`, `-`, `+`) stays visible and editable as a
  paragraph. A separating space commits its block presentation. This lets
  `*word*` and `**word**` be authored without a temporary list bullet and keeps
  heading markers visible while choosing the level. The source and Comrak
  model stay unchanged: the shared projection makes this presentation choice
  from block kind, range and heading level, without recognizing Markdown in
  either host. Loaded bare prefixes use the same presentation; completed
  headings, lists, fences and thematic breaks retain their normal behavior.
- Typed `-` or `=` on the empty line under a paragraph would be a setext
  underline: the paragraph would become a heading and the underline would be
  hidden markup with no caret position, sending the next character to the
  heading's line. The kernel inserts a blank line (keeping container markers)
  before the typed line instead, so it starts its own block: the bare marker
  above, a paragraph, or a thematic break for `---`. The parser identifies the
  underline and confirms the separation. Paste, IME preedit and source mode
  keep Markdown's literal meaning.
- Text typed, pasted or put (one line) on an empty row starts a block on that
  line, in the containers the row shows. An empty line in a list item or
  footnote may lack their indentation, so the line gets the innermost
  container's continuation prefix; an item marker the projection hides with
  no space after it (`1.`, `-` after another item, `- [ ]`) gets the
  separating space, so the item keeps the text; and an empty line after or
  before the typed line keeps the blocks around it apart where Markdown would
  read the text into them (a paragraph in another container read on lazily, a
  table, an HTML block) or them into the text (indented code, an empty item, a
  definition, a setext underline or rule, a footnote's lazy line). Whitespace
  after the row's container prefix, which the row does not show, stays
  unshown: where text run into it would show it (a tab's columns before the
  code its indentation makes of the text, or a line of code or literal HTML
  the text joins), the text goes where the prefix ends, an empty item's marker
  padded with one space, unless no spelling keeps the rest that way (literal
  HTML that runs on to the end of the document). Typing never shortens the
  source: the whitespace becomes spaces a paragraph does not show, or the
  blank line after or before the text. An empty item stays one rather than
  underlining the text, and under a fence with no body and no closing fence
  the fence closes first, so the text is not its code. The kernel
  commits the first spelling the parser reads with every other row keeping
  its kind, the kinds of its containers and its shown text, nothing the
  projection hid painted, and the typed text in the row's containers or in
  containers it opened (or whose prefix the line already carries, as Return
  leaves a footnote's next line). A paragraph in the same containers may take
  the typed line as a line of its own, as Return then typing continues it, a
  line under a table may start its first body row, and text that fills an
  empty item takes back the blocks indented for it, which the empty item had
  left out (a lazy paragraph does not count). Where no spelling qualifies the
  edit is refused; at a limit a typed underline is typed as it is.
- Text typed on a thematic break starts a block on the line after it, in the
  rule's containers; the rule stays a rule. Text typed beside a bare marker
  shown as text joins it as paragraph text, with an empty line after it when
  the block after would otherwise read on as part of it.
- A lazy line shows inside its containers without their prefix. Text typed on
  it commits as it is unless it would open or move a block, which then gets
  the prefix of the paragraph's first line, so a typed `# ` makes a heading in
  the item rather than ending the list.
- A space or tab typed where a line's content starts (alone, inside the
  hidden syntax that starts it, or over a selection from there, even one
  reaching the row's later lines) is indentation or marker padding Markdown
  does not show; where it would move a block (an item's content column
  re-nesting its children, a paragraph becoming indented code, an emptied
  item dropping the blocks after it) it is refused.
- A pending style's delimiters must pair around the typed text and hide;
  where they cannot (after a backslash, inside an autolink, against another
  delimiter run) the text is typed without the style.
- Typed text can complete block markup that hides the caret's own line. A
  table delimiter row gets a line break after it when the lines below would
  become its rows (see the table rules), and a fence marker that would make a
  line with text after it an opening fence, hiding that text in its info
  string and turning what follows into code, is escaped.
- A heading's opening separator belongs to its hidden prefix, including when
  it has no content yet. Its empty rendered row is exactly empty and its caret
  sits at the content origin; first-frame checks must not trim away a misplaced
  separator or merely assert that a caret exists.

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
- A setext heading cannot be empty: left behind, `===` would be painted as
  text and `---` read as a rule, and taken with the text, the underline could
  leave a list item empty and move the blocks after it. Deleting all of its
  text, replacing all of it with whitespace, or Return over all of it
  respells it as an empty ATX heading of the same level in the same
  containers, as a level change does, with the caret where typing continues
  the heading (after Return, on the new line). The parser must keep the other
  blocks where they were. Other text typed or pasted over all of it keeps the
  heading and its underline.
- A deletion inside a row changes that row only: every other row keeps its
  kind and containers, the row's remaining text its kind and containers, a
  table row its cells, and an emptied line shows none of its prefix. Where
  Markdown would read the result otherwise, the deletion respells what it
  emptied, and the parser must confirm the respelling: text deleted at a
  line's start takes the spaces after it, so an item's content column and the
  blocks nested at it stay put (`- The plan` less `The` is `- plan`); an
  emptied line of a longer row goes with its line break (deleting `b` from a
  setext heading `a`, `b` keeps `a` above its underline; an emptied lazy line
  goes); an item's emptied first line takes the blank lines after it, as an
  item can start with at most one, so its later blocks stay in it; an emptied
  item under a paragraph's line, which can neither interrupt the paragraph
  nor be its underline, takes a blank line in the outer containers before
  it, as does a line a deletion leaves as a setext underline (`-` under `a`);
  and an emptied cell of a table row written without its leading or trailing
  pipe keeps one (`| | Value`, `D||`, a body row `||`), the caret in it. When
  no spelling keeps the structure, a deletion that empties its line or a cell
  and would change another row, show its line's prefix or change its table
  row refuses (`#` between `- b` and `  [` stays: `[` would join the item);
  any other goes ahead as Markdown reads it, literal HTML and definitions
  included, so Backspace from the end and Delete from the start still empty a
  document.
- If deletion exposes whitespace against emphasis/strong/strike delimiters,
  move that whitespace outside the owner so surviving words retain their
  style, and retain typing intent when deletion exits an owner's trailing
  whitespace. Past a line break the opening syntax moves after the next
  line's container prefix, which would otherwise follow it as text. Deleting
  an owner's first word leaves the caret at the visible deletion point,
  before the whitespace that now leads the owner, with the owner's typing
  intent: a word typed there joins the owner (`**new two**`, not
  `**new** **two**`), and erasing that whitespace returns into it. At a line
  start, where Markdown displays no leading whitespace, the caret stays in the
  surviving content.

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

- Return splits a paragraph or heading at the caret. A heading's setext
  underline or ATX closing sequence stays with the part before the caret, so
  Return at the end of its text opens a paragraph. A setext heading cannot be
  empty: Return over all of its text leaves the empty ATX heading deleting it
  leaves, before the new line, and over the whole last line of a longer one
  refuses, since the underline would have to move up to the lines before;
- Return continues or exits supported list, quote and footnote structures. On
  an empty line each Return leaves one container: the empty item, then the
  quote, outer item or footnote its list ends. A continuation line repeats
  quote markers; an item or footnote definition that opens on the caret's line
  is continued at its indent (a footnote's four columns, counted from the end
  of the containers around it rather than from an indented label) rather than
  by repeating its marker, which would open another;
- Return shows only its line break. Over a selection it continues the
  containers as at the selection's start; a lazy line takes the prefix it
  lacks where it would otherwise leave its quote. Text moved to the new line
  or left before it stays text: a marker that would start a block there, or
  syntax that would end one (`> b`, `1. b`, `=`, `a\`, `# a #`), is escaped,
  and a hard break before the caret gives way to the new break. A blank line
  keeps the next block apart from the text of a split heading, now a
  paragraph, as for a lift, and an item whose later blocks follow blank lines
  continues after them, where an empty item would drop them from the list.
  Where nothing keeps what it splits (an autolink, a reference's label,
  delimiters that would pair anew) Return refuses, as it does when leaving an
  empty container line would move the block after it into another container;
- Return, including in code, inserts the line ending of the caret's line, so a
  CRLF document stays CRLF;
- terminal Return creates one writable following paragraph;
- table Return moves to the next row in the same column, then exits the table;
  a typed `|` is escaped as cell text, and a typed `\` that would escape a
  cell's delimiter (GFM reads any backslash before a pipe as escaping it) is
  refused; other text typed, pasted or put on one line in a cell keeps its
  row's cells (whitespace that would indent the row out of its table, or a
  pasted or replacing pipe that would split it, is refused); cell-boundary
  deletion rejects atomically, a deletion in a cell shows none of its row's
  other source (one that would leave a backslash escaping the cell's delimiter
  refuses), emptying a cell keeps its row's cells (a pipe spells the empty
  cell, so a cell the table drops stays dropped), text put in a cell a short
  row never wrote adds the pipes it needs right after the row's last cell
  (which shows no new space; after a backslash, which would escape a pipe, a
  space goes first), and table restructuring uses source mode. In the editing
  view a table without body rows shows its delimiter row as its source, a row
  of its own under the header: the row being typed keeps the caret, edits that
  would dissolve the table (typed, deleted, replaced or pasted, lines
  included) are refused, Return at its end opens the next line in its
  containers, and the first body row typed there hides it. Read-only views
  show only the header;
- Backspace at a supported block start merges, lifts, or removes the structural
  boundary users see. A lifted line stays in its outer containers, and when
  the block after it would read on as part of it (a list numbered past 1,
  indented code, a rule that would underline it) a blank line keeps that block
  apart. An empty line or rule before a row goes as a whole line, so the row
  keeps its markup and containers: Backspace after a rule, or Delete on it,
  removes the rule (one that opens a list item leaves the item's marker to
  the next row); joining after a setext underline or ATX closing sequence
  moves that markup after the joined text; and removing the empty line after
  a heading or code leaves the caret at the end of its text, not past its
  hidden underline or closing fence. Backspace at the start of a heading makes
  it a paragraph in the same containers (a blank line keeps the line after
  it from reading on into a quote's new paragraph; `# >` keeps its heading,
  as its text would open a quote); an empty heading whose marker cannot go
  without changing the block before it (an emptied item's `- ` would
  underline a paragraph above) goes with its line, as Delete at the end of
  that block takes it. Indented code lifted becomes the paragraph its text
  reads as, in no new container (`>a` would be a quote, `- a` an item, so
  those refuse). No join crosses a fence line: Backspace at the start of code
  removes an empty line or rule above it and otherwise refuses, and nothing
  joins onto a closing fence. A join or lift whose result would change
  another block's kind or paint markup the projection hid (removing the blank
  line between a paragraph and `---` would make a setext heading) refuses, as
  does one that would move any other block, however far after it, into or out
  of a quote or list item (`b` after `> a` and an empty line would read on
  lazily inside the quote; `c` after `- # a`, `b` and an empty line would
  join the item once `b` joins `a`), except the blocks of a container whose
  marker the lift removes, which leave it with that marker; and
- repeated Return or Backspace followed immediately by typing must leave one
  live caret and accept the next input.

Structural source markers remain hidden when the current parse recognizes them;
intentionally literal or incomplete syntax remains visible authoring content.

### Typed fence creation

In rendered mode, typing the third backtick (or tilde) on an otherwise bare
opening-fence line, before, between or after the other two, immediately
creates one empty code line and a matching closing fence. The caret starts inside that line. A writable gap follows the
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

Code is literal: edits the code delegate does not propose (typing it
declines, deletion, replacement, paste and composition) change the projected
body as given. Every code edit gives new lines the edited line's container
prefix, and text put on an empty line that omits the indentation of its list
item or footnote takes the prefix the fence's own lines continue with. Text
typed on an empty line of indented code that lacks the code's indentation
takes the indentation of the block's first line, so it stays code. When an
edit leaves a body line the parser would read as the closing fence (a typed
or pasted fence character, a run a deletion joins, an outdented run), the
fences grow past the body's longest run of their character instead; while the
parser still reads the block unchanged, they keep their length.

Untagged fences receive automatic syntax coloring without changing Markdown.
When the caret is inside a fence, the toolbar offers Automatic, Plain text and
a language override. A manual choice edits only the first info-string token,
retains metadata and body text, maps the selection, and is one Undo action.
Automatic removes the language token; with remaining metadata it uses `auto`
to preserve the metadata's position. Unknown tags remain intact and uncolored.

Enter carries existing leading whitespace and parser-owned container prefixes.
On a blank final body line after another code line, Enter instead exits the
fence and removes that final blank line, including auto-indentation. Thus
Enter twice after code finishes the snippet, with the next character entering
a separate paragraph. The enclosing list or quote remains intact. A newly
empty fence needs two Enters; blank lines in the middle remain code.
Shift+Enter deliberately keeps a blank line inside the fence. Selected
replacement, paste, composition and source-mode input retain literal breaks.
Exiting an imported unclosed fence adds its matching closer. Each exit is one
atomic Undo action and preserves following content and existing fence metadata.

An opening brace, bracket or parenthesis increases the indentation, and Enter
between a matching pair puts the closer on its own line. Python's trailing
colon also increases indentation. Recognized comments, strings and regex literals do not
trigger those rules. The step is two spaces, four for Python, or an existing
tab. Tab/Shift-Tab indent/outdent selected code lines without touching their
container prefixes, and without the code delegate Tab leaves blank lines as
they are; a collapsed Tab inserts a step at the caret. Commands
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
- One logical user action creates at most one history entry. A command that
  leaves the source, selection and typing intent as they are (the current
  heading level in any spelling, the current code language, a link's own
  destination) is an inert, successful no-op, like a repeated SetStyle: it
  publishes nothing, records no history and is not reported as refused.
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
