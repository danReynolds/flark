# RFC 027: Continuously rendered Markdown editing

**Status:** ACCEPTED FOR IMPLEMENTATION. 2026-08-11.

**Amends:** [RFC 026](rfc_026_flark_v4_product_architecture.md).

**Execution contract:**
[live projection v2](../v4/contracts/live_projection_v2.md) and the
[v4 build plan](../v4/build_plan.md).

## 1. Decision

Flark is a continuously rendered Markdown editor, not a source editor that
renders only unfocused rows.

Valid, current-revision Markdown stays rendered while it is focused, selected,
and edited. Moving the caret into a heading, emphasized span, link label, list,
code block, or table cell does not reveal its delimiters or replace the block
with raw Markdown. The user edits the rendered content directly while exact
Markdown remains the sole document authority.

Literal syntax is visible when it is itself the current authoring content:

- a construct is incomplete or malformed;
- a current-revision projection is not yet certified;
- an IME composition or newly typed delimiter is still in progress;
- GFM has no rendered editing representation for a source-only construct; or
- the user deliberately enters a future explicit source mode.

Fallback is local. An uncertain construct may become an exact-source island;
focus alone may not turn a certified row, block, or document into raw source.
ABI 4.31 does not yet complete that target: unsupported typing currently paints
the whole active row as exact source. That fallback is authority-safe, but its
unrelated marker flash is a known product gap pending parser-authored minimal
dependency islands.

`flark` also exposes a dedicated `FlarkMarkdownView` for read-only rendering.
The view and editor consume one internal projection and layout contract. They
are separate public widgets with different interaction machinery, not separate
Markdown renderers and not one editor widget with a deep `readOnly` branch.

The existing `flark-live-v1` profile is retained as historical evidence for
the passive-rendered/active-source prototype. It is not the product target.
`flark-live-v2` owns continuously rendered editing.

## 2. Product examples

### 2.1 Inline completion

Typing progresses through literal current source:

```text
*
*h
*hello
```

After the closing `*` commits and the current revision is certified, the same
content becomes rendered emphasis. The delimiters disappear, `hello` remains
editable, and the canonical caret remains after the same source insertion.

### 2.2 Editing stable presentation

Given `before **bold** after`, clicking `bold` keeps it bold. Inserting `er`
produces exact source `before **bolder** after` without a focus transition or a
rendered frame containing the whole raw row.

### 2.3 Breaking a construct

If an operation makes a construct incomplete and the runtime cannot certify a
rendered interpretation, only the affected construct becomes literal current
source. Unrelated certified text retains its current presentation.

### 2.4 Block completion

Typing `# ` at a paragraph start is visible while it is incomplete authoring
syntax. Once the runtime certifies an ATX heading, the marker becomes hidden
and the rendered heading remains directly editable. The same rule applies to
list markers, quotes, fences, and other GFM structures.

### 2.5 Read-only parity

At the same source revision, theme, width, and text scale, a quiescent editor
with no editing affordances and `FlarkMarkdownView` produce the same render-plan
hash. The view omits caret, input, mutation, history, and editor-only
source-material affordances.

## 3. First principles

The design follows ten invariants.

1. **Exact source is the product truth.** Rich presentation is a derived view,
   never a mutable document model.
2. **Rust is the only Markdown authority.** Dart and Flutter may consume typed
   facts and recipes but may not recognize delimiters or infer structure.
3. **Focus is presentation-stable.** A selection-only or focus-only change
   cannot alter rendered Markdown.
4. **Every edit is a source transaction.** The platform may address a rendered
   caret, but the committed mutation names exact source coordinates.
5. **Every semantic glyph is revision-certified.** Stale facts are not mapped
   forward and presented as current.
6. **Uncertainty is visible and local.** Current exact source replaces only the
   smallest runtime-authenticated uncertain range available.
7. **Position mapping is explicit.** Source bytes, source UTF-16, graphemes,
   display text, glyph clusters, and visual bidi positions are distinct.
8. **Input remains platform-native.** Flutter consumes delta input and
   composing ranges rather than inventing a keyboard or IME protocol.
9. **Editor and view share rendering, not interaction plumbing.** Parser,
   projection, layout, and paint parity are mandatory; caret/input overhead in
   read-only mode is not.
10. **Work stays bounded.** Continuous rendering cannot reintroduce
    document-sized layout, projection, mapping, or teardown work.

These principles deliberately separate three things that rich-text editors
often combine: exact Markdown source, certified semantic presentation, and
ephemeral platform input state.

## 4. Selected model

### 4.1 Source-backed input, projected painting

The bounded Flutter `TextInputClient` window continues to contain exact source
text. Platform deltas, selections, and composing ranges therefore remain
defined against one lossless string.

Painting does not have to mirror that string. A current-revision projection
snapshot maps source ranges into visible runs, caret stops, objects, and
synthetic presentation. The custom surface paints that snapshot and maps hit
tests back to source positions before invoking `flark_core`.

This avoids making a hidden-marker display string into a second editing
authority. It also keeps the platform input connection compatible with exact
undo, paste, autocorrect, and composition state. The physical input matrix must
still prove that surrounding Markdown syntax does not break platform behavior.

### 4.2 Projection topology, not string replacement

A projected row is not merely `source.replaceAll(marker, '')`. It is an
ordered topology of typed runs:

- identity source text;
- hidden parser-owned marker cuts;
- non-identity replacements such as character references and normalized code
  span content;
- synthetic presentation such as bullets, checkboxes, and thematic rules;
- atomic/object presentation such as an image or future embedded media; and
- exact-source islands for incomplete, composing, source-only, pending, or
  faulted ranges.

Each run names its current source revision, source range, display range,
mapping kind, styles, and legal caret stops. Hidden source offsets collapse
onto display boundaries with explicit upstream/downstream affinity. Synthetic
content maps to a parser-authored source owner and edit recipe; Flutter does
not reverse-engineer Markdown from the glyph it painted.

Implementation checkpoint: ABI 4.15 activates the previously reserved
projected-row lane for ordered identity segments separated only by
parser-certified hidden container coverage. The first admitted shape is a
depth-one multiline block quote: repeated `> ` prefixes stay in the exact
platform input window but are absent from painting, hit testing maps the
collapsed boundary with affinity, and conservative literal typing retains the
segment topology through a transaction-bound continuity receipt. The fixed
128-byte row record is unchanged; new callers opt into the grouped 32-byte
segment payload with `SEMANTIC_PROJECTED`, while the older `SEMANTIC` query
continues to receive an unavailable row. Nonempty Return is also admitted:
Rust resolves the exact certified physical segment, commits the quote prefix,
and Core constructs a bounded projected transition that hides it through
recertification. Prefix Backspace on a later physical line now publishes an
ordered quote/plain surface set. The predecessor implementation mapped literal
successors through that set; section 4.4.1 supersedes that retention path
because a structural receipt contains no result-revision inline-fact proof.
ABI 4.16 closes the empty
continuation boundary: the parser publishes a synthetic zero-length row only
when a BlockQuote container marker remains unrepresented after its last
renderable child. Runtime derives the exact final-line prefix and caret, so an
empty Return exits through the same authoritative receipt without Flutter
inferring source structure.

### 4.3 Canonical selection and display selection

`flark_core` retains the canonical source-anchored selection. Flutter derives
a display selection from it for painting and maps gestures back through legal
caret stops. A hidden delimiter does not become an independently navigable
character merely because it exists in source.

Two source caret states may share one visual x-position at a hidden boundary.
Affinity distinguishes “outside the formatted span” from “inside at the first
content position.” Arrow, Backspace, Delete, insertion, selection extension,
and hit testing use that topology rather than an arbitrary nearest integer.

### 4.4 Certification and immediate feedback

The runtime continues to fail closed: no semantic fact is current without a
proof for the committed revision and range.

The T2 implementation first attempted the smallest design:

1. commit the source edit;
2. spend the existing bounded foreground pump/query budget;
3. paint a current certified projection if available; otherwise
4. paint a local exact-source island while unrelated certified runs remain.

The profile spike falsified recertification-only presentation: all 120 measured
ordinary edits could be observed without an active projected row even though
editor work remained far below the frame budget. T2 therefore implements the
allowed Rust-authored edit-presentation continuity receipt. Rust marks inline
constructs whose content can safely retain presentation for conservative
plain-text transactions and contiguous rows whose block presentation can
safely survive a plain-text insertion. `flark_core` binds either policy to the
exact edit and revision; Flutter may splice only the authorized exact-content
run. Syntax-like input, marker edits, autolinks, reference links, and row-level
deletions or replacements fail closed to current exact source until parser
recertification.

A receipt is retired only when a certified viewport at or after its result
revision covers the authorized content range. A profile-mode Mac development
receipt then observed 0 raw/missing active projections across 120 measured
edits in a 1 MiB dense-inline document. That is the T2 continuity proof, not a
mobile, IME, wall-clock latency, or full performance-matrix claim.

### 4.4.1 Amendment (2026-08-18): parser-published literal-safe envelopes

The continuity-permission design above is superseded. It is replaced, not
loosened: the invariant that nothing is painted without a proof for the
committed revision and range is retained exactly, and the mechanism that
decides *which* edits keep their presentation moves entirely into the parser.

**What falsified it.** The first dogfood typing session reproduced a flicker
in three keystrokes: typing an emphasis run and then an ordinary space
reveals the row's raw delimiters for a frame before settling back to the
rendered presentation. The cause is structural, not a coding slip. Rust
publishes only a binary per-row permission (`PlainTextEdit` or `None`) and
delegates the per-edit decision to "the host's bounded validator" — so
`flark_core` must reconstruct the geometry of that permission itself, using a
hardcoded Markdown-sensitive character list and a rule that refuses any edit
whose caret touches an inline fact's source range, boundary-inclusive. A
keystroke immediately after a closing delimiter therefore counts as touching
the construct and is refused, even though the grammar can prove it harmless.

Two consequences follow. The host holds Markdown knowledge it must not hold,
which is the one surviving breach of the one-grammar rule. And the decision
can only ever be conservative, because `flark_core` cannot distinguish a
space after a closing delimiter (harmless) from a space after an opening one
(destroys the construct) — the same character at adjacent positions with
opposite meanings.

The T2 receipt did not catch this because it could not: its workload types
alphanumerics strictly inside an already-certified `**strong**` run, which is
the single shape the permission retains. No measurement typed a delimiter,
created a construct, or edited at a construct boundary.

**Selected replacement.** At certification, the parser publishes, per row, the
exact source ranges within which a literal edit of a declared character class
provably cannot change any published fact of that row — a *literal-safe
envelope*. The host's entire decision becomes a containment test:

- an edit contained in an envelope for its class retains the row's
  presentation, with every parser-published range in that row transformed
  through the edit;
- an edit outside every envelope must eventually present a minimal exact-source
  island around the affected dependency range until recertification. The
  current 4.30 implementation conservatively presents the whole active row,
  which is safe but remains a no-marker-flash product gap.

This is one decision procedure with two defined outcomes, both governed by
parser-published data. There is no timeout, no second mechanism, and no path
in which the host guesses. Presentation retained through an envelope is
*proven* correct for the new revision: the parser computed the proof in
advance and the host applied it, so section 4.4's invariant holds unchanged.

**ABI 4.26 minimum (2026-08-20).** The first implementation deliberately exposes
only two insertion classes. `asciiWordInsertion` covers one non-empty insertion
made entirely of ASCII letters or digits inside an eligible inline fact whose
complete, non-empty content slice is itself one flat ASCII letter/digit word
(with no punctuation, whitespace, nested syntax, or code normalization).
This narrow whole-slice rule prevents an outer fact from certifying across a
nested or latent delimiter boundary. `singleAsciiSpaceInsertion` covers exactly one U+0020
at the editable row end when an eligible inline fact's outer closing boundary
is also the row end. The latter envelope is zero-width and one-shot: after that
space, fresh parser authority is required because a second space can form a
hard line break. Thematic-break and table rows publish neither class.

**ABI 4.27 closure amendment (2026-08-21).** Capability
`LITERAL_SAFE_ENVELOPE_CLOSURE_V1` lets the parser publish a non-empty
`singleAsciiSpaceInsertion` range and lets Core carry a successful proof set
through successor insertions. The current reusable bundle is limited to a
canonical single-line ATX heading with authoritative empty inline facts,
identity coordinates, and ASCII word/space content bounded by word bytes. A
non-empty space envelope admits positions `start < p < end`, never either
endpoint. A separate zero-width envelope may
authorize the exact editable-row endpoint once and is consumed by that edit.
Reusable space authority ends before an existing trailing-space run, and the
parser publishes no terminal zero-width proof when the row already ends in
U+0020. These rules keep a second trailing space, and therefore a possible hard
line break, outside carried authority.

A matched non-empty envelope grows through its declared edit. Non-empty
envelopes with exactly equal byte and UTF-16 geometry are one parser-authored
closure bundle and grow together. An unmatched envelope strictly crossed by a
foreign-class insertion is dropped; a range before the edit stays, and a range
at or after it shifts. A matched zero-width envelope is consumed. Any mismatch
or failed transform drops continuity and waits for fresh parser authority. Core
does not inspect source, classify Markdown, or reconstruct proof.

This closes the demonstrated plain-heading burst only. It does not authorize
the unsupported edit classes or replace the required parser-authored minimal
dependency-island tranche.

**ABI 4.28 projection edit cells (2026-08-21).** Capability
`PROJECTION_EDIT_CELLS_V1` adds kind-16 records whose source range is the
parser-authored affected closure and whose content range is the edit trigger.
The host may retain only the declared block shell and independent runs outside
that closure; it replaces the closure with one exact current-source run and
does no Markdown classification.

The first complete cell covers canonical top-level single-line ATX content
with authoritative empty inline facts. It accepts any non-noop splice without
CR/LF, including deletion, replacement, and non-ASCII text, and chains only by
transforming that complete cell. The first local cell covers one conservatively
isolated flat Strong fact: one U+0020 at its opener/content boundary makes only
that fact's source closure exact, retains independent outside inline facts, and
is consumed immediately. A fresh complete result-revision inline publication
supersedes either temporary presentation.

**ABI 4.29 literal-cell extension (2026-08-21).** Capability
`PROJECTION_EDIT_CELLS_V2` extends kind 16 with matcher codes 2, 4, and 5. The
chainable literal cell covers parser-authored ASCII word/space source
segments in top-level paragraphs, simple list and block-quote content, and
plain table cells. Its trigger excludes dependency boundaries. ASCII word
insertion/replacement, or a space strictly inside the trimmed trigger,
therefore paints the changed literal exactly while
retaining the row shell and independent styled facts; a separate one-shot
proof admits one safe Backspace. Rust compares admitted edits and representative
carried successors with a fresh final-source parse, including paragraph,
list, quote, table, and ATX shell shapes. This is the path exercised by the
real dogfood paragraph—including typing, selection replacement, and
Backspace—rather than a test-only heading fixture.

Matcher 5 covers the final physical-line plain gap, including the punctuation
in the reported `locally.` dogfood path, and owns only a zero-width append
trigger. It chains ASCII-alphanumeric text, bounded ASCII prose punctuation,
and single separator spaces while a
host-carried state bit prevents two terminal spaces. A fresh row ending in one
space republishes that blocked state; two spaces or other terminal whitespace
receive no such cell. The current Plain physical line must begin with an ASCII
letter after ordinary paragraph padding, so appending a space cannot complete
a list, heading, or quote opener. The exact closure therefore cannot create a hard line
break while earlier Strong content remains rendered.

This is the first bounded minimal-island implementation, not a general inline
dependency graph. Unsupported delimiter families, structural edits,
non-ASCII/punctuation composition islands, nested or multi-line shells, and ambiguous
closures still fail closed to the current whole-row exact path and remain a
product gap against the north star.

**ABI 4.30 Strong-asterisk envelope (2026-08-21).** Capability
`LITERAL_SAFE_ENVELOPES_V2` adds edit class 3 without widening the V1 classes.
For one flat Strong fact with no escaping asterisk dependency or overlapping
fact, the parser publishes its complete content as a one-shot envelope. Exactly
one collapsed `*` insertion strictly inside that content transforms the current Strong run, so the
delimiters stay hidden and the style remains rendered. The proof and every
same-geometry sibling are consumed; they do not authorize a successor or any
other delimiter shape.

**ABI 4.32 parameterized dependency cell (2026-08-22).** Capability
`PROJECTION_EDIT_CELLS_V3` keeps the existing kind-16 layout and adds matcher
code 6, `INSERT_EXACT_SCALAR_AT_POINT`. Rust supplies a complete affected
closure, one zero-width trigger, and one scalar parameter; Core only compares
the declared parameter and transforms the declared ranges. The initial bounded
emitter handles `[` inside one isolated flat Strong fact on a single
physical-line Plain row when the parser has an
exhaustive no-existing-bracket certificate. It exposes only the transformed
Strong source exactly, retains the paragraph shell and outside projection, and
is consumed after one edit. This is the first use of the generic parser-owned
component seam, not a host bracket rule or general bracket-graph claim.

The same exact-scalar matcher covers the frozen D0 prose punctuation set
(`.`, `,`, `;`, `:`, `!`, `?`, apostrophe, double quote, `(`, `)`, hyphen,
en dash, and em dash) only at an ASCII-alphanumeric guard pair inside a
fact-free prefix before one authoritative Strong fact. The complete prefix is
the exact affected closure, the outside Strong fact remains projected, and the
record is consumed after one edit. This adds parser-owned proof breadth without
changing the ABI vocabulary or teaching Core punctuation semantics.

The frozen D0 syntax-construction cells use the same one-shot scalar protocol.
Rust emits `*`, backtick, `[` or `]` only beside one Emphasis sibling and `_`
or `~` only beside one Strong sibling, with a fact-free ASCII prefix, an
alphanumeric guard pair, and no occurrence of the inserted marker in the
current source. Brackets additionally require exhaustive parser bracket
classification. The prefix becomes exact while the different-marker sibling
remains projected; no successor authority is inferred.

The same V3 seam may map a parser-authored complete fact-free physical-line
gap onto the existing guarded ASCII-literal matcher, with a maximal ASCII
prose run as its trigger. One nonempty ASCII-alphanumeric/U+0020 replacement
is admitted only strictly inside the
declared trigger and must contain an alphanumeric unit. This permits a bounded
multiword paste while unchanged guards protect the outside inline partition;
it does not authorize punctuation, deletion, line boundaries, or host-side
Markdown classification.

**ABI 4.33 result-shell transition cell (2026-08-22).** Capability
`PROJECTION_EDIT_CELLS_V4` keeps kind 16 and query kind 6, and adds matcher codes
7 and 8, `EXACT_SPLICE_REPLACE_BLOCK_SHELL` and
`SIMPLE_BLOCK_PREFIX_PLAN`. The parser supplies a complete bounded
physical-line closure, one exact insertion point or deletion range, and the
typed clean-result Plain, ATX heading, depth-1 BlockQuote, or simple ListItem
shell. Core compares the declared splice mechanically; Flutter presents the
current result content under that shell through the existing pending snapshot.
The exact-splice proof is one-shot, retains no predecessor shell, and is
superseded by prefix-inclusive fresh parser certification.

The prefix-plan form additionally supplies a finite ASCII sequence and the
parser-classified activation point. Core may carry only that exact sequence at
the declared line-start point, presenting Plain before activation and the typed
target shell afterward. This closes the same prefixes when several platform
deltas arrive before one vsync, while keeping the sequence and its Markdown
meaning entirely parser-owned.

This revision was not added for a new Markdown construct. The D0 actual-paint
matrix demonstrated that ordinary human-cadence entry of the final space in
`# `, `> `, `- `, and `1. ` exposed the old Plain row until recertification.
The generic result-shell field closes that architectural gap without a host
marker table and is reused for parser-proved removal geometry.

**Envelope semantics.** Envelopes are class-qualified because safety is
positional, not lexical. A space after an outer closing delimiter at row end is
inert; the same space after an opening run can destroy the construct. Edits
outside the two landed insertion classes fail closed to exact source. Deletion,
replacement, non-ASCII insertion, table-specific classes, and broader literal
classes remain pending; they are not covered uniformly or inferred from an
empty replacement. Chaining is authorized only by the ABI 4.27 closure rules
above and only while a transformed parser proof survives each successor.

ABI 4.26 exposes the new records only through the capability-gated
`SEMANTIC_PROJECTED_LITERAL_SAFE` query kind. `SEMANTIC` and
`SEMANTIC_PROJECTED` retain their pre-envelope record vocabulary. Because the
ABI has no per-client negotiation state, 4.26 rejects a 4.25 negotiation rather
than claiming it can preserve every legacy row flag while serving new clients.
ABI 4.28 retained query kind 6 and added record kind 16 plus capability bit 28.
ABI 4.29 adds capability bit 29 for matcher codes 2, 4, and 5. ABI 4.30 adds
capability bit 30 for literal edit class 3. ABI 4.31 adds capability bit 31
for parser-proved structural presentation continuity. Exact-minor negotiation
rejects every earlier minor rather than serving widened matcher semantics under
an older contract.

**ABI 4.31 structural presentation proofs (2026-08-21).** Capability
`STRUCTURAL_PRESENTATION_PROOFS_V1` adds receipt flag `PRESENTATION_PROVEN`.
Rust sets it only from a current Ready parser result. V1 covers a bounded Plain
paragraph Return at its editable end and a bounded Plain paragraph Backspace
merge whose separately parsed inline partitions equal the merged parse. The
host may then retain the proved block shell and runs; the new Plain successor
receives the zero-width chainable ASCII literal cell specified by that typed
proof. The cell may carry ordinary input but cannot authorize another Return;
every structural successor requires a fresh Ready parser proof. Pending,
oversized, non-ASCII, or hazard-bearing transitions and delimiter crossings
omit the flag and remain exact-source fail-closed.

The final ABI 4.32 contract extends that same post-commit flag to the existing
typed simple-list indent and outdent transitions. Rust requires a current Ready
ListItem context, a ListItem result context, and exactly one bounded ASCII-space
prefix insertion or deletion. Core then shifts the certified runs through the
prefix splice and retains the list shell; the receipt does not grant authority
to a successor input or structural action.

Exact 4.31 also permits the existing ASCII-word envelope to cover maximal
parser-authored word leaves inside eligible projected facts. Leaves require
identity coordinates, fact-edge or U+0020 guards, independence from overlapping
facts, and zero Code normalization. At most 128 literal-safe envelopes are
published per row. When the complete page baseline fits, the ABI reserves every
row's ordinary inline facts and required projection-segment group before
optional cells or envelopes consume the remaining payload, so the optimization
cannot displace authoritative presentation on later rows. Oversized baseline
groups retain the ABI's complete-group fail-closed behavior.

**Transform.** Retained presentation and its surviving proof set are
transformed by pure range arithmetic
over the ranges the parser already publishes: the row's source and editable
ranges, each inline fact's source and content ranges, and each projection
segment. Both byte and UTF-16 dimensions transform independently from the
edit's own two deltas. Ranges after the edit shift; ranges strictly
containing it grow; ranges ending exactly at its start shift rather than
grow. Because the source/display mapping is derived from those same
structures, transforming them keeps caret and selection geometry coherent
without a separate mechanism. The transform never inspects source text. The
closure-specific proof transforms are the exhaustive ABI 4.27 rules above;
ordinary presentation-range containment does not authorize a foreign-class
envelope.

**Removed from the active decision path.** The row continuity policy and its
ABI field, inline-fact policy flags, and host-side Markdown-sensitive character
classification no longer authorize presentation retention. The old inline and
table authorization entry points are removed. Under ABI 4.26 a transaction
receipt binds one parser envelope to one exact insertion. Under ABI 4.27 it may
carry only the parser proof set that survives the declared closure transform.
No Markdown decision remains in `flark_core` or `flark`.

Structural receipts remain authoritative for their exact source splice and
temporary block partition only. They never preserve predecessor inline styles,
hidden inline delimiters, or character-reference projection as current. The
affected temporary surface paints exact source until result-revision facts
arrive, and an immediate ordinary successor clears the structural surface;
only a fresh literal-safe envelope can authorize inline projection retention.

**Soundness obligation.** An envelope is a claim the parser must be able to
prove exhaustively: for every admitted position in a published envelope,
applying an edit of the declared class must leave the row's published facts
unchanged. For ABI 4.27 that obligation also covers every carried successor and
every same-geometry closure bundle. This is differentially testable across the
conformance corpus and replaces a measured frame count with a structural
guarantee. The landed tests cover the two minimum classes, carried interior
word/space insertion, consumed terminal authority, and fail-closed boundaries;
an exhaustive corpus-wide differential remains required before adding more
classes. A frame receipt remains a quality measurement; it is no longer the
evidence for the continuity claim.

**Unchanged.** Genuinely uncertified regions — during load, over-cap
constructs, composition without a matching parser-authored edit cell — keep the
exact-source fallback mechanism; envelopes and cells govern certified content
awaiting recertification, a different domain. The current fallback may cover
the whole active row, while broader minimal dependency islands remain required
for the product target. Structural-edit capability remains gated on current
certification, and retained presentation never grants edit authority.

### 4.5 Composition islands

An active platform composing value is exact current source and remains stable
until the platform commits or cancels it. A matching current-ABI edit cell may
retain only its declared block shell and independent outside runs while the
affected cell stays exact. Otherwise the landed minimum treats the active row
as the exact composition island: the current ABI has no result-revision proof
that an arbitrary composition delta leaves other inline facts in that row
unchanged. Markers in the active row may therefore be visible while composition
is pending; unrelated certified rows remain projected.

General narrowing to the composing range is pending parser-authored
result-revision/dependency authority; the bounded edit-cell cases do not imply
authority for other rows or edit shapes. The surface may retain a surrounding
style only when a current runtime result or a transaction-bound receipt
explicitly authorizes it; predecessor facts plus a source splice are not such
authority. This narrower island remains part of T3 input truth below.

Projection changes do not retire or recreate the input connection merely to
hide syntax. Composing endpoints, selection, candidate/prompt rectangles, and
caret geometry map through the same topology.

### 4.6 One renderer, two public widgets

Internally, both widgets consume the same bounded `FlarkSurfaceSnapshot`
shape, layout engine, fragment virtualization, theme resolution, and paint
primitives.

`FlarkEditor` adds:

- a platform input connection and composition state;
- canonical selection/caret painting and editable hit testing;
- gestures, shortcuts, clipboard mutation, undo/redo, and editor actions;
- editor-only exact-source material for constructs with no rendered editing
  representation.

`FlarkMarkdownView` adds only read interactions:

- optional visible-text selection and copy;
- link activation and caller-provided semantic actions;
- view semantics and accessibility.

It does not instantiate an editor controller merely to disable it. Multi-MiB
guarantees apply to a bounded, virtualized viewport. Any future unbounded or
shrink-wrapped convenience mode has a separately declared small-document cap.

## 5. Ownership

### Rust runtime and parser

Rust owns all Markdown recognition, current-revision projection facts,
certification, source/display geometry facts required to construct mappings,
and any semantic edit or continuity recipe.

### `flark_core`

The headless Dart package owns source sessions, canonical selection, anchors,
graphemes, edit/history policy, typed projection models, and validation of
revision/range ownership. It does not import Flutter or decide what Markdown
means.

### `flark`

The Flutter package owns projection consumption, text shaping, display/glyph
mapping, layout, paint, caret/selection geometry, platform input, gestures,
semantics, accessibility, and the separate editor/view widgets.

Platform adapters handle only actual platform differences. Clipboard,
selection, navigation, composition policy, and rendering do not fork into
independent macOS, Android, iOS, and Windows implementations.

## 6. Alternatives rejected

### Reveal the active row

Rejected. It is the current prototype and directly contradicts presentation
stability. It also changes wrapping and hit geometry on focus.

### Make a rich semantic tree authoritative

Rejected. Converting edits back to Markdown would make formatting choices,
whitespace, delimiters, reference layout, and line endings lossy or canonicalized.
It would violate exact source truth.

### Give the platform a projected editing string

Rejected for the initial design. Hidden and synthetic text would cause platform
deltas, composing ranges, autocorrect context, and source transactions to use
different strings. A future experiment may revisit a projected input island
only if physical input evidence proves the exact-source window inadequate.

### Speculatively carry old styling in Flutter

Rejected. An inserted delimiter or non-local reference edit can invalidate
semantics outside the obvious character range. Continuity requires current Rust
authority.

### Maintain a second read-only renderer

Rejected. Parser and visual drift are guaranteed maintenance debt. The public
view is separate; its render contract is shared.

### Reuse the legacy v2/v3 implementation

Rejected. Older code and RFCs are design evidence only. v4 keeps its direct
runtime, ABI, `flark_core`, and custom `flark` path.

## 7. Adversarial review and refinements

| Challenge | Consequence | Refined decision |
| --- | --- | --- |
| Hidden delimiters create several source positions at one visual boundary. | A scalar offset map gives incorrect arrows, insertion affinity, and deletion. | Model legal caret stops plus affinity as a topology. |
| The parser may not recertify before the next frame. | Raw syntax could flash during ordinary styled typing. | The Mac spike proved the gap; use a transaction-bound Rust-authored continuity receipt and retire it only after covering recertification. |
| Composition may span a marker or replacement run. | Reprojecting can corrupt the IME transaction or candidate rectangle. | Freeze the active row as an exact composition island; narrow it only after Rust supplies result-revision authority for the surrounding facts. |
| A certified reference definition edit has non-local dependents. | Retaining old link presentation would be stale. | Pending dependency ranges become exact/local; unrelated certified presentation remains. |
| A valid construct has no editable rendered text, such as a reference definition. | Hiding it makes exact source unreachable. | Editor shows an explicit source-material affordance; read-only mode follows GFM output. |
| Link destinations and image metadata are hidden in rendered content. | Direct text editing cannot address every source field. | Use parser-authored semantic actions/popovers; retain a later explicit source mode. |
| Tables combine cell editing with two-dimensional layout and nested scrolling. | Treating a table as one text row recreates raw-source focus or unbounded layout. | Give tables a dedicated later slice with cell-owned source mappings and gesture arbitration. |
| Trackpad, mouse, touch, stylus, code scrolling, and table scrolling compete. | A generic pan recognizer selects while the user scrolls. | Define a pointer-kind gesture matrix and test Flutter gesture-arena outcomes. |
| Read-only Markdown is often embedded in an unbounded parent. | Full layout of a multi-MiB document defeats virtualization. | Large-document guarantees require a bounded view; cap any shrink-wrap mode separately. |
| Accessibility may need both semantics and source editing context. | Painting correctly is not enough for screen readers or platform text services. | Make semantic ranges and actions part of each slice; physical device qualification remains mandatory. |
| Copy behavior differs between authoring and reading. | One hidden-marker policy surprises one of the surfaces. | Initial editor copy follows its canonical source selection; view copy follows visible text. Rich/multi-flavor clipboard is a later explicit capability. |

The review leaves no unresolved architectural blocker. It does leave two early
falsification points: same-frame projected continuity and exact-source-window
IME behavior. Both are tested before broad construct coverage.

## 8. Implementation tranches

This is five product tranches, not one slice per syntax construct.

### T1 — Contract, topology, and shared surface

- [x] Materialize `flark-live-v2` as an executable matrix.
- [x] Add typed projection runs and legal caret-stop topology through Rust, ABI,
  `flark_core`, and Flutter only where current facts are insufficient.
- [x] Extract one internal snapshot/layout/paint path from the existing passive
  renderer.
- [x] Introduce `FlarkMarkdownView` as the read-only consumer of that path.
- [x] Preserve v1 fixtures as historical tests; do not mutate their meaning.

Exit: editor-passive and view render-plan parity passes for the supported v4
GFM facts, source/display mappings are total at legal stops, and no second
parser or renderer exists.

### T2 — Continuously rendered inline editing and gestures

- [x] Keep emphasis, strong, strikethrough, code, autolinks, and link labels
  rendered while active.
- [x] Implement display hit testing, insertion affinity, arrow movement,
  selection, replacement, Backspace, and Delete across hidden markers.
- [x] Implement incomplete-to-certified and certified-to-incomplete transitions.
- [x] Replace generic pan-to-selection with the desktop pointer/scroll matrix.
- [x] Measure the same-frame continuity decision on the Mac profile app.

Exit: the product-tour document can be edited without focus reveal, common
inline edits do not flash a raw row, scrolling cannot mutate selection, exact
source histories pass, and the focused performance trace stays within the
existing v4 frame gates.

**Product checkpoint:** resume user dogfood here before expanding breadth.

### T3 — Input truth and cross-range editing

- Exercise real Mac composition, dead keys, autocorrect replacement, emoji,
  ZWJ, combining marks, bidi, affinity, and composing rectangles.
- Complete multi-block/page selection, clipboard, paste, undo/redo, and
  selection transformation while projection changes.
- Pin exact input-window resynchronization under hidden markers.

Exit: the input and hidden-marker matrices pass through the real runtime and
surface; no composition is lost, duplicated, reordered, or visually detached.

### T4 — Block structures and semantic objects

- Continuously edit headings, lists/tasks, quotes, fenced/indented code, hard
  breaks, thematic rules, and source-material rows.
- Add link/image target actions and parser-authored edit recipes.
- Add rendered table-cell editing, navigation, selection, and nested-scroll
  arbitration without falling back to an active raw table.
- Pin editor/view parity for every selected GFM presentation class.

Checkpoint: ordinary typing and Backspace now remain projected within one
parser-authored table cell through recertification. Cross-cell selection,
navigation, structural row/column actions, and nested-scroll arbitration remain
in this tranche and fail closed meanwhile. Pure lists with uniform two-space
container indentation now continue and outdent one level at a time at any
bounded parser depth; nonuniform and mixed-container geometry remains explicit
unsupported/fail-closed behavior. ABI 4.17 replaces the uniform-spacing
assumption with a parser-authored marker column and bounded ancestor padding
lineage. Pure lists with wide ordered markers now render, continue, and outdent
through the same framework-neutral transaction lane; tabs and mixed-container
paths still fail closed. ABI 4.18 makes the parser's exact indented-code
deindent ranges projection-safe. Return repeats that parser-owned four-column
source prefix, Backspace joins a prior visible code line by consuming its line
ending plus the hidden prefix, and Backspace on the first line lifts only the
indentation. Framework-neutral receipt transitions keep spaces, tabs, residual
visible indentation, CRLF, and a BOF BOM correct without exposing raw prefixes
during recertification.

ABI 4.19 establishes the first parser-certified semantic atom. A top-level
thematic break stays rendered while active and exposes one zero-width editable
boundary rather than its marker source. Backspace or forward Delete at that
boundary removes the complete parser-owned row through the ordinary one-splice
transaction, anchor, history, and presentation-receipt path. Nested rows and
stale semantics fail closed; ordinary typing and Return at the boundary remain
literal source edits, and deletion preserves a BOF BOM.

ABI 4.20 extends pure block-quote editing from depth one to a bounded parser-
owned container lineage. Return repeats the complete exact prefix; Return on
an empty nested row and Backspace at its content boundary remove one innermost
marker. For a later nonempty physical line, Rust emits one canonical splice
that creates a block boundary and restarts the line at the remaining quote
depth, preventing CommonMark lazy continuation from silently retaining the old
nesting. Core maps that receipt into ordered marker-free temporary surfaces,
while Flutter only chooses how each semantic depth is painted. Mixed quote/list
or quote/heading paths remain fail closed.

Exit: the complete v2 behavior denominator is exact or an explicitly scoped
unsupported product feature; “active raw” is not an accepted fallback for a
supported construct.

**Product checkpoint:** second visual/interaction dogfood before hardening.

### T5 — Production hardening and qualification

- Run the existing 1/2/5/10 MiB shape matrix for editor and bounded view.
- Finish semantics, accessibility, text scaling, themes, link actions, and
  lifecycle behavior.
- Add Android/iOS functional emulator preparation, then physical device input,
  gesture, memory, thermal, and performance qualification when hardware exists.
- Qualify Windows later without forking product behavior.

Exit: the Mac product checkpoint and later named-device gates in RFC 026 pass
without weakening source, certification, or frame contracts.

Focused tests run inside each tranche. The full `verify_v4.sh` gate runs at T2,
T4, and T5 integration checkpoints rather than after every small edit.

## 9. Required evidence

The product is not accepted from unit tests alone. Evidence must include:

- an executable v2 source/presentation/selection/composition history matrix;
- parser-to-ABI-to-Dart-to-Flutter source/display mapping parity;
- render-plan equality between quiescent editor and view;
- real-engine widget interaction tests and live Mac inspection;
- real Mac keyboard, trackpad/mouse, and IME receipts;
- profile-mode frame attribution at ordinary and adversarial document shapes;
- source-export equality after every edit history; and
- physical device receipts before mobile UX or performance claims.

Moving focus, changing selection, or toggling editor/view mode must not change
the stable render-plan hash. A rendered frame showing stale semantics, a whole
active raw row after a focus-only action, or a scroll gesture mutating selection
is a failing receipt.

## 10. Deferred

- Explicit whole-document source mode.
- Rich/multi-MIME clipboard interchange.
- Media loading and arbitrary embedded Flutter widgets.
- Collaborative editing.
- Web and Linux products.
- Another language UI SDK.

These do not block the continuously rendered editor, read-only widget, or the
selected native-platform plan.

## 11. Acceptance condition

This RFC is realized when a user can focus and edit certified rendered
Markdown without a presentation-mode transition; incomplete or unproven source
appears only in the affected current-source island; exact Markdown export,
selection, history, and IME state remain correct; scrolling never becomes
selection accidentally; and `FlarkEditor` and `FlarkMarkdownView` share one
bounded, parser-authored rendering contract.
