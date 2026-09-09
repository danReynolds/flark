# Fleury first-host-slice review — 2026-09-08

Base: main `274172f7eecc339e9731e1753085bc1102d31a55`; work on
`codex/flark-fleury-host`. Local review and validation, no CI or merge.

## Architecture result

The shared model holds for this first slice. FlarkEditor remains the single
source, selection, command, composition-transaction and history authority.
The initial host slice needed **zero kernel or parser changes**. Subsequent
dogfooding exposed a shared bare-prefix presentation gap and an empty-heading
range extraction bug, corrected for both hosts (see the follow-ups below).
Fleury consumes
projected rows, segments, shell metadata and source/display mappings directly.
It owns cell widths/wrapping, viewport movement, focus, paint and input routing.

The original plan's proposed TextInput/controller wrapper was the wrong seam:
Fleury's text controllers carry their own plain-text editing model and history.
Using the public TextInputClaimant, PasteEventClaimant and composition contracts
avoids mirroring state. A custom cell render object is normal host work; it
does not recognize Markdown. No Flutter, Fleury private import, MarkdownText,
second highlighter, universal replay driver or common widget abstraction was
introduced.

One genuine ownership correction: the nonvisual FlarkTreeSitter command adapter
and its 48 pure editing/highlighting/closer scenarios moved from flark_flutter
to flark_tree_sitter. Flutter retains a small compatible asset-loading wrapper.
Each host owns its coloring worker/lifecycle; both use the same analyzer,
language catalog, edit proposals and source-qualified asynchronous results.

The adapter occupied approximately 1,100 production Dart lines at the time of
this paragraph, against M4's 3,000-line budget, excluding the shared snippet adapter, tests and
example. Table/resource work remains, so this is headroom rather than a final
size prediction.

## Review findings and fixes

- Source may change between input events without an intervening paint. A
  vertical move now refreshes old source geometry before mapping the caret.
  Its regression intentionally types and moves without pumping/painting between.
- Fleury delivers the first drag movement to onDragStart. Wiring only
  onDragUpdate lost short selections. A direct cross-line code drag now checks
  selected source, painted selection, replacement and the surviving fence.
- Cached/repositioned widgets need current screen geometry even without a
  paint walk. Public BoundsObserver supplies the caret/pointer origin update;
  a moved RepaintBoundary regression verifies the click and next typed character.
- Picker help changed intrinsic height on focus and shifted buttons. The
  playground reserves that space, with a pointer press/release export regression.
- Style lookup advances through projection segments and snippet tokens in one
  pass; selection paint maps its endpoints once per projected row, avoiding
  repeated full-span scans for every glyph. Full performance qualification is
  still separate.
- Task hit testing is limited to the painted checkbox; continuation indentation
  places a caret without toggling an off-row checkbox.
- Home/End chooses the kernel's outer anchors at hidden-markup boundaries.
  Theme changes, controller ownership and narrow/wide layouts preserve editing
  state rather than reconstructing the document.

These were host input/layout integration issues, not obscure Markdown cases.
The first-frame, unpumped-input, actual-pointer and browser checks proved useful
precisely because semantic command success alone would miss them.

## Local evidence

- flark_fleury: analysis clean; 18 direct editor tests, including first-frame
  bold/backspace/space and pointer reentry, blank rows, wrapped selection,
  wide/combining text, paste identity, code indentation, composition transactions,
  read-only mutations, source mode, focus, scrolling and native coloring worker.
- Example: three tests, including 100x32 and 40x24 layouts, theme/focus/next input,
  pointer stability and copied configuration.
- flark_tree_sitter: analysis clean; 355 Dart tests including the migrated
  48 shared Flark adapter scenarios. No Rust/grammar changes.
- flark_flutter: analysis clean; all 795 remaining Flutter package tests passed
  after the adapter extraction. The 48 moved scenarios also passed through the
  original Flutter wrapper before their relocation.
- Standalone dart2js build with local Wasm parser, Wasm snippet analyzer and a
  browser coloring worker. Browser interactions confirmed multiline paste,
  fence-scoped Cmd+A, Ruby Enter indentation and typed `end` outdent. Visible
  swatch clicks updated the link color and a single pointer click copied the
  chosen Dart configuration. The browser semantic mirror is offscreen; those
  physical checks used the visible grid, not clicks on mirror DOM nodes.
- AOT terminal bundle built with Dart native assets. Background PTY smoke
  exercised native startup, real terminal Ctrl+A decoding, bracketed paste,
  subsequent typed input and clean Ctrl+Q exit. This is not a terminal emulator,
  physical keyboard, IME or sustained performance qualification.

Tests use immediate render checks for edited frames. Waiting is reserved for
explicitly asynchronous clipboard/worker completion.

## What this does not establish

M4 remains open. Tables currently render as sequential cells, and link/image
activation, replaceable resource controls and image presentation are next.
Large-source behavior, drag autoscroll, richer gestures/shortcuts, accessibility
editing actions, physical composition/lifecycle and input-to-present performance
require further qualification. Flutter's native qualification also remains
separate and open.

The adjacent Fleury checkout at `a9eb740aacc60c8fd8483abaa1aa8ce49f78c3c0`
contains unrelated uncommitted changes and its
companion packages are unpublished. Tests used ignored local path overrides;
no Fleury files were changed. Clean-machine/pinned-dependency installation is
not proved. Before landing or calling M4 complete, select a reviewed Fleury
revision and repeat qualification against it.

Next focus: a real Fleury table surface and replaceable resource controls. Those
will test compound block layout and host-specific UI customization more strongly
than extending the already shared editing command set.

## Dogfood follow-up: source-authored prefixes

The user's first `*` painted as a bullet before the rest of `**word**` was
typed. This was reproducible in the shared projection: Comrak recognizes a
one-character unordered marker as an empty list item. It likewise recognizes
bare hashes as empty headings, hiding them while the user chooses a level.
The source was not corrupted, but that intermediate presentation was confusing.

The shared projection now keeps those bare, empty prefixes visible and editable
as paragraphs. Space commits their block presentation. It uses parser-owned
block ranges/kinds and heading level, with no source scanner, parser fork,
host-specific Markdown rules or deferred document state. This is an explicit
editing-profile choice: imported bare prefixes have the same presentation.
Comrak's model remains unchanged; completed constructs retain normal rendering.

Heading syntax itself worked through both web text input and physical shifted
hash key events. Fleury still uses one cell style for all six heading levels;
it cannot express Flutter's font-size hierarchy in a fixed cell grid. Distinct
per-level theme styles and optional heading-level gutter cues are a presentation
follow-up. The reported lost-heading behavior has not been reproduced beyond
the disappearing bare prefix and the lack of visual level differentiation.

This is a test-methodology gap, not an exotic Markdown edge case. Initial host
checks disproportionately edited preformatted source. New tests construct
headings, emphasis, strong and lists character by character, checking immediate
source, caret, text/style and next input. Flutter also checks the same sequences
without intervening frames. Core cases retain outside prose/quote shells,
exercise Undo/Redo and assert that the parser model and projection invariants
remain intact. One existing empty-list fixture now includes the committing space.

Local follow-up evidence: analysis clean in flark, flark_flutter and
flark_fleury (including its example); 705 core, 811 Flutter, 29 Fleury editor,
3 example and 355 Tree-sitter tests passed. The rebuilt dart2js preview passed
real shifted `*`/`#` input, bold completion, heading conversion, list
continuation/exit and following prose in a separate browser tab. The user's
original draft tab was not reloaded. CI, native executable rebuild and physical
IME/performance qualification were not part of this follow-up.

## 2026-09-09: empty-heading caret and list marker

The user's next screenshot exposed a remaining first-frame defect after `## `:
the hashes disappeared, but a separator space was projected as content and the
caret painted at column one. The ATX extraction started from a paragraph range
whose trailing whitespace had already been trimmed. Its opening-separator loop
therefore stopped before the space in an otherwise empty heading. It now consumes
that prefix whitespace up to the physical line end. Comrak is unchanged. The
source caret remains after the prefix; its display position is column zero.

The previous heading test used `trimRight()` and only asserted caret presence;
those checks concealed precisely the missing-space geometry. Host regressions
now assert the exact empty row and caret origin before any content is typed.
Fleury also checks the actual inverse caret cell, backspace/Undo/next-character
behavior, and wrapped-list pointer mapping. Native extraction regressions cover
all six heading levels, spaces/tabs, outer containers, line endings and following
Unicode text with editable trailing whitespace.

Fleury's default unordered marker is now a fuller `●` with full text contrast.
It retains one marker cell and one gap cell, including wrapped text; a surface
that measures this glyph as two cells uses an ASCII marker to preserve geometry.
Clients can still override marker styling through the existing theme field.

Local verification: native parser tests passed; rebuilt Wasm and native render
models matched across all 1,322 corpus cases; 705 core, 811 Flutter, 31 Fleury
editor and 3 example tests passed. Flutter/Fleury analysis and diff checks passed.
The rebuilt browser candidate was inspected with real shifted hash/asterisk keys:
empty heading caret at the content origin, first letter, full-size bullets and
list continuation. Browser reload on 127.0.0.1 retained old subresources despite
the server serving fresh bytes; verification and handoff used the fresh
`http://localhost:8820/` origin. The user's original draft was preserved. Repeatable
preview cache invalidation remains a development-tooling follow-up; a reload alone
is not evidence of which candidate was exercised. No CI or merge was performed.

## 2026-09-09: generated host geometry and a table-cell extraction bug

This round mined for defects rather than following a screenshot. Two generated
contracts now cover the host layer that had none, and one shared extraction bug
they surfaced is fixed at the parser.

**Generated cell geometry.** `flark_fleury` gains its own matrix: the kernel's
random command sequences, with the cell layout rebuilt at 4, 7, 12, 40 and 200
columns after every command and its contract checked — glyphs stay inside the
grid and advance monotonically, a row's visual lines partition its display text
with only a line break skipped between them, every column resolves to an
in-range source offset, and the caret is always addressable. It also toggles
source mode mid-sequence. It runs 60 sequences by default and reuses
`FLARK_MATRIX_ITERATIONS`/`FLARK_MATRIX_SEED`; 1,200 and 600-sequence runs on
separate seeds are clean.

**Pointer identity.** Clicking a painted glyph must leave the caret on that
glyph. The check drives the real `hit` → `PlaceCaret` → `positionFor` path over
both upstream corpora at three widths. Across every case the only disagreements
were virtual whitespace — tab expansion inside indented code, where no caret can
sit between the cells — which the check excludes by requiring an exact segment,
and one genuine defect below.

**Painted selection.** A third check renders through `FleuryTester` and compares
every inverse cell against the source range, over wrapped rows, hidden markup,
wide glyphs and tabs: 4,516 selected cells agreed.

**The defect: a table row short of the header's columns.** Comrak fills the
missing cells with nodes whose sourcepos is the row's closing `|` — or its line
break when the row has none — and repeats that one position for every missing
cell. The extraction took those ranges literally, so `| c |` under a two-column
header projected a second cell whose text was `|`, `| c` projected a cell whose
text was a line break and whose range crossed into the next line, and two
missing cells produced two blocks at an identical range. Clicking the phantom
cell moved the caret into the previous one.

A cell's content can begin at neither an unescaped pipe nor the line end, so the
correction clamps a cell range to its own line and empties a childless cell that
starts at a pipe. Genuinely empty cells (`|||`, `|  |  |`, `| c ||`) and escaped
pipes are unchanged. Registered in `REGISTER.md`; regressions cover containers,
line endings, column counts and the pipeless row in Rust, and the projection
contract in Dart.

Local verification: Rust gates pass; the rebuilt Wasm matched native across all
1,322 corpus cases; 727 core (2,000 extra matrix sequences on a fresh seed),
811 Flutter, 44 Fleury editor/geometry/pointer/paint, 3 example and 355
Tree-sitter tests passed, with analysis clean in every package. No CI, merge,
browser or device qualification was part of this round.

Not defects, but noted while mining: an indented code line's expanded tabs are
virtual, so a click left of the first character lands at the content start
rather than in the whitespace; `_literalResourceText` escapes every ASCII
punctuation character, so a link synthesized from a URL carries `[http\:\/\/x\.y]`
in the source — deliberate canonical serialization, and it validates through the
parser, but it is what the author sees in source mode.

## 2026-09-09 (second round): a corpus contract, and seventeen defects

The first round's generated checks covered host geometry. This round put the
same treatment on the kernel: a **corpus contract** that runs every upstream
case in four forms — LF, CRLF, quoted and indented — against the invariants a
host relies on but the projection invariants alone never stated.

- a legal caret round trips through its display and back through its anchors;
- offsets sharing a display position share an anchor set, so one painted caret
  never means two unrelated contexts;
- rows are in display order, their per-line ranges stay on their own lines, and
  their segments stay inside them;
- grapheme, word, line and vertical movement never stall and never repeat;
- every command at every legal caret either refuses without a change or
  changes the source, keeps every invariant, and undoes exactly;
- Backspace from the end empties the document;
- source mode round trips, and extending a selection and coming back returns
  the caret to the same painted place.

It lives at `packages/flark/test/corpus_contract_test.dart` and prints the
source and rule of each failure. Minimized cases are in
`packages/flark/test/caret_contract_test.dart`.

### What it found

Four in the parse crate:

1. **Cells filling a short table row carried a delimiter.** A row short of the
   header's columns projected a cell whose text was `|`, or a line break whose
   range crossed into the next line; several missing cells repeated one range.
   Clicking the phantom cell moved the caret into the previous one.
2. **Adjacent entities collapsed.** `&amp;&amp;` displayed one `&` from the
   second entity and nothing from the first, because the piece splitter treats
   a following entity as an immediate resync. Two legal offsets then shared one
   display position. `&lt;&gt;`, `&nbsp;&nbsp;` and `&#10;&#10;` were the same.
3. **The splitter could emit a piece with no source.** Display text at an
   offset no caret can reach; only a leading piece (a partially consumed tab's
   virtual spaces) may do that.
4. **A trailing thematic break owned the blank lines after it.** Each of their
   line ends became a caret painting on the rule, so Enter after `---` left the
   caret where it was.

One more in the parse crate, found through a refused edit rather than the
contract: **a link title closed by a literal backslash** (`[foo]: /url "a\"`)
is a definition to comrak but not to the definition mirror, so the line
belonged to no block and every edit near it was refused as a deviation. The
generated scanner lets a backslash be an escape or an ordinary character and
takes the longest match; the mirror now does the same.

Six in the projection:

6. **A setext underline held a caret** that painted at the end of the heading
   text. Loading a document with a saved offset inside `===` put the caret
   there, and the next character destroyed the heading. Fence lines already
   held no caret for exactly this reason; nothing else does now either.
7. **A blank line inside indented code held a caret** that painted at the end
   of the line above, so Enter there looked like nothing happened.
8. **A line ending inside inline HTML was swallowed**: `a <b\nc> d` displayed
   as `a <bc> d`, gluing two source lines together. The rule is now general —
   only a gap the markup already hides displays nothing.
9. **Whitespace on a blank line outside any container was called prefix**, so a
   document that was one tab had no caret before it and nothing Backspace could
   remove.

Five in the editor:

10. **Forward movement could not leave a table** whose last row was short of
    its columns: the empty cell anchors back to the offset the caret already
    holds, and the step reported a move that could not happen.
11. **Backward movement stalled** at the start of tab-indented code for the
    same reason — virtual leading spaces anchor to their own offset.
12. **Down stalled forever** on a row nothing displays. Vertical movement now
    keeps looking until it actually leaves the caret's row.
13. **A caret could sit between a backslash and the character it hides.**
    Clicking just before the `*` in `a\*b` and typing gave `a\Z*b`: the
    backslash unhidden, the asterisk armed. An escape is one caret unit.
14. **A fence that displays nothing could not be deleted.** Its joins refused
    because it has no content record, so `~~~` on a line was markup the
    document could never be rid of — and neither could the line break after it.
    A row that displays nothing but owns source now goes whole.

### Observed and left alone

- Typing a character that completes an inline construct may hide it — typing
  into `[a](|<b)c` makes a link. That is the editor working.
- An indented code line's expanded tabs are virtual, so a click left of the
  first character lands at the content start.
- Backspace on the empty line after a table is refused, because neither
  direction may lift a pipe or a delimiter row. The document is still erasable
  forward or through Select All; changing it would need the table deletion
  contract reopened, which this round did not.
- `_literalResourceText` escapes every ASCII punctuation character, so a link
  synthesized from a URL carries `[http\:\/\/x\.y]` in the source.

### Local evidence

Rust gates pass: spec HTML conformance 1,321/1,322 with example 354 registered,
and the extraction with zero deviations plus the schema invariants, now
including the new sibling-overlap and content-line checks in
`check_invariants`. The
rebuilt Wasm matched native across all 1,322 cases. 740 core tests (with 1,500
extra matrix sequences on a fresh seed), 811 Flutter, 51 Fleury, 3 example and
355 Tree-sitter tests passed, analysis clean in every package. No CI, merge,
browser or device qualification was part of this round.

## 2026-09-09 (review round): what the contracts missed

A ten-angle review of the branch — two of the angles differentially executed
this kernel and `main` over every legal caret in both corpora — found that
several of the fixes above had bought their symptom at the cost of a worse one.
The corrections, and what they say about where each fix belonged:

**Deleting `_fillLineEnds` was too blunt.** It served three different cases at
once. A line the row genuinely cannot show (a setext underline) must hold no
caret; a line the row *can* show (a fence body carrying only a container
prefix) must hold one, and without it `> ```` ``` ````\n> ` became a document with a
caret where nothing could be typed at all; and a blank line comrak folded into
a preceding leaf is neither — it is document structure, and belongs to nobody.
The fill is back for literal rows only, and the third case is fixed where it
started: `trim_trailing_blank_lines` in the crate takes a leaf's trailing lines
back off when they carry no content after their container prefix, so a blank
separator becomes its own row with its own caret. That one correction replaces
the thematic-break clamp, which was the same quirk seen through one keyhole.

**A join boundary is a line edge, not a caret span.** `_lastCaretEnd` returning
the last content record let one Backspace erase a whole `===` underline, and
`_bodylessFenceAnchor` let Delete splice a code line into a fence's info
string, where the editor never showed it again. It now returns the end of the
row's last physical line, and a fence that displays nothing refuses to absorb
another row's content at all.

**A block range is not reliably the whole construct.** The index-0 erase branch
trusted `sourceStart..sourceEnd` and turned `> ---` into a literal `> --`.
comrak reports a rule as one column inside a container and runs past its line
at a document's end, so the crate now derives a thematic break's range from its
own line; the Dart branch is narrowed to the one row kind that guarantee covers.

**An unclosed fence's block ends at its opening line**, so the `closed` guard is
back: deleting through it orphaned a closing delimiter into a new code block.

**Presentation choices still have to answer to commands.** A bare `-` between
two real items dropped its list shell, so it painted outside the list, refused
Indent, and stopped continuing it; the marker is authoring text only when it is
the sole item of its own list. A bare `#` is projected as a paragraph, so
`SetHeadingLevel(2)` prepended and produced `## #`; the level commands now
follow the block kind. And Return on `> ## ` took the empty-container-line exit
and deleted the heading along with the quote marker.

**`scan_link_title` was the expensive kind of correct.** The reachability
rewrite never exited early and allocated a buffer the size of the rest of the
paragraph, so a 1 MiB paragraph of titled definitions went from 1.62 ms to
15.70 s. Reachability only ever reaches the next byte or the one after, so it
fits in a two-slot window that stops when neither is live; `paragraph_definitions`
now measures 34/66/131/271/534 µs at 16/32/64/128/256 KiB — linear. (A separate,
pre-existing quadratic in extracting definition-heavy documents remains, and is
not from this branch.) In the same spirit the projection's new break fallback
walks the block's merged hidden runs with one cursor instead of rescanning per
line, and merging also stops two abutting hidden runs from faking a break.

Host corrections: a wrapped list row repeated its bullet and its `[ ]`, and the
task hit test read that prefix, so the checkbox was clickable on the wrap; the
link popover's clamp inverted at zero columns; the coloring cache had neither of
the Flutter host's bounds and rescanned every code body per keystroke; a
read-only view committed a shared editor's composition; the goal column survived
`Cmd+Up`; and the IME preedit range was assumed rather than read back from the
kernel.

Left as is, deliberately: an unclosed fence that displays nothing still cannot
be Backspaced away when it is the first row, and neither can a link with no text
— both would mean deleting through a range the model does not pin down, and
Select All or `RemoveLink` covers them. Backspace on the empty line after a
table stays refused. The empty cell a short table row never wrote is painted but
still not addressable; entering it should materialise `| `, the way a bodyless
fence materialises its body, and that is table work rather than a caret fix.

The corpus contract gained the rule that would have caught the first of these:
every legal caret must accept some edit. The check that a delete may not hide
more than it removes was tried and dropped — a structural reparse can
legitimately change the display by more than the source — so the fence-info
splice is pinned by a named regression instead.

## 2026-09-09 (follow-ups): the findings below the report cap

The review round reported fifteen findings — the cap — and fixed them. The ten
angles had returned roughly sixty candidates, and the remainder was merged
without being put in front of anyone as a decision. This round works that
remainder, and says plainly which parts are still open and why.

**Cell geometry is no longer rebuilt per frame.** `performLayout` allocated a
glyph for every grapheme in the document on every layout pass — each keystroke,
each scroll tick, each arriving colouring result — while only the viewport was
painted. A layout now describes the controller, width, theme, width policy and
colour revision it was built from, and both the render object and the input path
share that one test. `FlarkCellTheme` gained value equality so a host that
rebuilds its theme per frame (which `FlarkCellTheme.of` does when no extension
is installed) does not defeat it, and the controller publishes a colour revision
that changes only when colours do, which the editor's own revision does not —
it counts selection moves too. `positionFor` no longer scans the document for a
row: the constructor indexes each row's first line. A regression asserts the
counts directly — zero rebuilds for a repaint, a caret move or a selection
change, one for an edit — and that the reuse test refuses a resize, a theme
change, a width-policy change, an edit and a different controller with identical
text.

**A swapped controller invalidates what was bound to the old one.** The cached
layout carries the editor it was built from, and `_dialogOpen` stayed true if a
presenter never completed, wedging Ctrl+K permanently.

**The scope-to-role table is shared.** `codeSyntaxRole` lives beside `CodeToken`
in the kernel, and `FlarkCellTheme.syntax` is keyed by role rather than by the
analyzer's scope names. Fleury had enumerated five of them, so `built_in`,
`regexp`, `attr`, `tag`, `property`, `doctag` and a dozen more rendered as
ordinary body text in the terminal while the Flutter host coloured them.

**Both resource forms read their labels from the session.** They had drifted:
the terminal form hardcoded link wording, so it would have called an image a
link, and it never offered the `remove()` the shared session already provides.

**`CodeEditAction.values.byName(action.name)`** coupled two independently
compiled enums by string; a member added to one would have thrown on a
keystroke. It is a switch now, so that is a compile error.

**A dropped paste is observable.** `onNotice` reports a paste abandoned because
the document moved under it, or one over the source limit; both vanished
silently before.

**CI grew two jobs.** `flark_tree_sitter` (355 tests) and `flark_flutter`
(811) ran nowhere in CI, and the branch that added a whole host package left
them there. They run now. `flark_fleury` still cannot: `fleury` and
`fleury_widgets` are unpublished, and its path overrides live in a gitignored
`pubspec_overrides.yaml`, so its 54 tests are local-only. The workflow says so
in a comment, because a green CI does not cover that host.

Smaller: `_step`'s guard counter bounded only row hops, so the loop it guarded
was not the loop that could run long — it now decrements every iteration and
grows with each row entered. Two nested `clamp`s whose outer bounds could never
bind became `math.max`, a no-op rebinding left the table-cell arm, and the
erase loop's bound in the corpus contract was four times larger than the number
of steps it could ever take.

### Still open, deliberately

The largest reuse finding is not done: `FlarkFleuryController` and
`FlarkCodeColors` still run two copies of the cache-plus-worker pipeline. The
divergence that mattered — Fleury having neither the LRU bound nor the
maxCodeUnits cap, and retrying a superseded analysis forever — is fixed, but the
duplication that caused it remains. It is not a straight lift: `FlarkCodeColors`
imports `flark_tree_sitter`, which the kernel must not depend on, and it also
carries a visible-rows policy that is a Flutter viewport concern Fleury does not
share. The shared part is the cache and the worker loop, not the visibility
rule, and splitting them properly is its own change.

Also still open: the random-command generator and corpus loader are copied
across three test files, and cannot share `packages/flark/test/support/` because
a package cannot import another's `test/` — a shared home would have to be a new
public library in `packages/flark/lib/`. The deletion refusals, the unaddressable
empty table cell, and the pre-existing quadratic in extracting definition-heavy
documents are unchanged from the round above.

