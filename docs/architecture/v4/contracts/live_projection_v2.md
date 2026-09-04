# Live projection v2 contract

**Profile ID:** `flark-live-v2`  
**Semantic profile:** `flark-gfm-0.29-v2`  
**Edit profile:** [flark-edit-v1](../../v5/edit_profile_v1.md)

**Owner:** [RFC 027](../../rfc/rfc_027_continuously_rendered_markdown.md)  
**Status:** normative executable contract; T1 and T2 product checkpoint implemented.

## 1. Scope

This contract defines what every visible frame may show while exact Markdown
source, parser certification, platform input, selection, and viewport state
change. [The rendered-editing profile](../../v5/edit_profile_v1.md) separately defines
what logical editing commands mean. This contract does not change any GFM
parse result or invent source-edit semantics.

The v2 product has two Flutter consumers:

- `FlarkEditor`, continuously rendered and source-editable; and
- `FlarkMarkdownView`, read-only and optionally selectable.

Both consume the same current-revision projection facts and internal render
plan. v1's focus-triggered marker reveal is explicitly not inherited.

## 2. Coordinate types

The implementation must not use an unqualified integer for a position. The
following spaces are distinct:

- source byte offset at revision;
- source UTF-16 offset at revision;
- source grapheme boundary plus affinity;
- projection run and display-text offset;
- shaped glyph-cluster position;
- visual bidi caret position; and
- viewport coordinate.

Conversions name the source revision and are bounded. Canonical selection is a
pair of source anchors with affinity. Display selections and carets are derived
values.

## 3. Presentation states

Every visible source-owned range is in exactly one state:

| State | Authority | Presentation |
| --- | --- | --- |
| `certified_projected` | Current-revision certified semantic facts or an unexpired transaction-bound result-presentation proof for that exact result revision/range | Rendered runs with parser-owned markers hidden or replaced |
| `certified_literal` | Current-revision proof that source is literal or editor source-material | Exact source with intentional literal styling |
| `pending_exact` | Current source, no current semantic proof | Neutral exact source for the authenticated pending range |
| `composition_exact` | Current source transaction plus a bounded parser-authored composition result proof and the platform composing value/range | Exact composing payload only in its active mapped range; the proof retains the surrounding owner shell/runs and hidden delimiters, and unaffected facts remain projected |
| `source_gap_exact` | Exact source is available but a projection query cannot cover it | Neutral exact source within the bounded gap |
| `fault_exact` | Session retains readable source after a typed semantic fault | Exact source plus surfaced fault state; no semantic facts from the faulted range |

Focus, selection, hover, and caret movement do not change these states.

## 4. Projection run types

Every `certified_projected` snapshot is an ordered sequence of typed runs.

### 4.1 Identity run

Visible text is source-exact. Every legal display grapheme boundary maps to its
corresponding source grapheme boundary.

### 4.2 Marker cut

A parser-owned delimiter or structural marker is hidden. Its source range maps
to legal boundary affinity at one or both ends. No interior marker offset is
reachable through ordinary display navigation, and the marker does not add an
otherwise motionless navigation step.

### 4.3 Replacement run

A source range renders as different text, for example a character reference or
code-span normalization. The runtime provides endpoint ownership and any legal
interior mapping. Without interior mapping the replacement is atomic for caret
movement and deletion.

### 4.4 Synthetic run

A visible glyph or widget has no identity text in the source display, such as
a bullet, checkbox, or thematic rule. It still names a parser-authored source
owner, hit behavior, and allowed semantic edit recipe.

### 4.5 Object run

A semantic object such as an image or future media element occupies one visual
selection unit. Its source range, before/after caret stops, node-selection
behavior, and actions are explicit.

### 4.6 Exact island

The visible text is current exact source because the range is incomplete,
pending, composing, a source gap, source material, or faulted. Exact islands do
not advertise Markdown styling that lacks current authority.

## 5. Mapping invariants

1. Runs cover their declared display text without overlap or gaps.
2. Every source-owned visible run names one current revision and a nonempty
   source range, except a parser-authored zero-width synthetic boundary.
3. Every legal display caret stop resolves to one source grapheme boundary and
   affinity.
4. Every canonical source selection endpoint has a deterministic display
   projection or a typed offscreen/unpresented result.
5. A display range maps to a source range plus endpoint affinities; it is not
   reconstructed by searching visible text.
6. Marker, replacement, synthetic, and object ownership comes from Rust facts
   or recipes, never a Dart/Flutter Markdown scan.
7. An edit invalidates older mapping snapshots. A stale mapping may not commit
   a source transaction.
8. Mapping and shaping are capped to the requested viewport/fragment budget.

### 5.1 Caret-context topology

Every writable projected caret stop publishes a bounded set of parser-authored
`CaretContextAuthority` alternatives. Each alternative is stop-specific and
binds the current revision, support domain, topology and stop identities, one
exact source anchor, rendered edit side, and adjacent visible/object targets.
It references a shareable opaque `ContextPlanAuthority` for the ordered
existing semantic-owner path and insertion plan. Stops may share that plan but
never their stop ID, anchor, side, or adjacent targets. Neither record contains
a recipe for an owner that does not exist in this projection.

Hit testing and navigation return one alternative identity; Core may adopt that
exact identity for its new selection generation but cannot synthesize, widen,
or combine alternatives. The geometry may be shared by inside and outside
sides while their authority remains distinct. A styled side with no current
alternative cannot be published as writable: adoption fails without moving the
canonical selection, or the surface is explicitly read-only/no-target. Baseline
caret alternatives outrank optional projection facts under output caps. Their
absence is never permission to label a stop plain, infer context from painted
styles, or accept a caret whose next ordinary insertion cannot be authorized.

## 6. Focus and selection rules

### LP2-FOCUS-STABILITY-001

Moving focus or a collapsed caret into a certified projected span does not
change visible text, style, wrapping, block geometry, or render-plan identity.

### LP2-SELECTION-STABILITY-001

Creating, extending, reversing, or collapsing a selection does not reveal
markers. Selection paint follows visible glyphs while endpoints remain
canonical source anchors.

### LP2-HIDDEN-BOUNDARY-001

At a display boundary shared by hidden syntax and visible content, the caret
topology distinguishes inside/outside affinity without exposing the marker as
an extra caret step. Mouse/touch placement and arrow navigation choose the
construct default and directional context defined by `flark-edit-v1`.

### LP2-CROSS-RANGE-001

A selection may cross inline runs, blocks, viewport pages, exact islands, and
synthetic/object runs. Replacement produces one canonical source transaction
over the resolved source selection and one new revision.

## 7. Edit rules

### LP2-INSERT-CONTENT-001

Inserting ordinary content inside a certified rendered span updates exact
source and remains rendered when current certification is available by the
paint deadline. A focus-triggered or row-wide raw transition is forbidden.

### LP2-INCOMPLETE-COMPLETE-001

Incomplete Markdown remains literal current source. When a committed edit
creates a valid construct and returns either current certification or the
transaction-bound result-presentation proof defined below, the affected range
becomes projected without changing canonical selection or source. A completed
construct does not wait in `pending_exact` merely because background
certification has not caught up.

### LP2-COMPLETE-INCOMPLETE-001

When a committed operation produces an incomplete or unproven construct, the
smallest available authenticated affected range becomes an exact island.
Unrelated certified presentation remains.

### LP2-REPLACE-CONTENT-001

Replacing selected visible content inside a semantic span preserves only the
source syntax required by the `flark-edit-v1` result and semantic closure.
Flutter does not expand, dissolve, or repair Markdown delimiters on its own.

### LP2-DELETE-BOUNDARY-001

Backspace/Delete traverse legal visible caret stops. A hidden marker is not
deleted as an invisible character. Boundary edits that require structural
behavior use the current-revision semantic result defined by `flark-edit-v1`
and proved by a parser-authored edit recipe. Literal source mechanics apply
inside a current authenticated exact/literal island using its canonical
mapping even when richer semantics are pending; they do not claim hidden-owner
traversal or closure.

### LP2-ARROW-BOUNDARY-001

Left/Right movement advances through legal grapheme caret stops in logical
source order, with bidi visual movement following the qualified platform rule.
Movement cannot strand the caret inside a hidden delimiter or UTF-16 scalar.

### LP2-PLATFORM-SELECTION-NORMALIZE-001

A platform-originated selection or non-text delta may name a raw source offset
inside hidden syntax because the input service sees the exact source window.
Before installation as canonical selection, Flutter resolves it to the adjacent
legal caret stop using movement direction and affinity, then synchronizes the
corrected exact-source `TextEditingValue` back to the platform. The hidden raw
offset never becomes a display caret or edit origin.

### LP2-UNDO-PROJECTION-001

Undo/redo restores exact source and the complete interaction state required by
`flark-edit-v1`, including selection/affinity and Core-owned `TypingContext`.
Projection is recomputed/certified at the resulting revision; presentation
state is not an independent history entry. History restores semantic typing
intent, while the native history transaction emits a fresh context directive
and recipe bound to the new result revision; an old authority token is never
replayed.

## 8. Immediate visibility and certification

An accepted edit must expose its exact source, caret, and selection on the next
rendered frame under RFC 026's performance contract.

For semantic presentation, the surface may use only:

1. facts certified at the committed revision and range;
2. a one-shot parser-authored literal-safe envelope for a literal edit that
   cannot change the retained semantic facts; or
3. a transaction-bound semantic result-presentation proof required by
   [EP1-RESULT-PRESENTATION-001](../../v5/edit_profile_v1.md#ep1-result-presentation-001).

### LP2-SEMANTIC-EDIT-RESULT-001

A semantic result-presentation proof is part of the same logical receipt as
the committed edit. It binds the predecessor publication, exact result
revision, base/result closure transform, and a complete affected partition of
row shells, rendered runs and owner/style paths, source/display mapping, legal
caret-context topology, semantic objects, and action authority. Unchanged
partitions are retained only by exact predecessor fact identity plus bounded
range transforms. Result anchors, semantic typing intent, fresh caret/empty-
owner directive, and history identity join the same result.

The affected and retained partitions must cover the complete command surface
without overlap or an unexplained gap. Required proof data is never truncated;
cap exhaustion fails before mutation. The proof may describe created, changed,
or removed inline facts because Rust resolved the semantic command or
recognized a completed construction; literal-safe envelopes may not.

Core validates and publishes the proof atomically through the existing
immutable pending-presentation slot and lifecycle. It does not introduce a
second result cache or third controller semantic state. Flutter does not
extend, merge, or infer it, and the proof expires when current certification
covers the closure. Without current certification or this complete proof, the
semantic command must fail before mutation rather than expose raw syntax in a
waiting frame.

The initial T2 spike attempted bounded commit, pump, and query with the older
continuity protocol. The following paragraphs preserve that experiment's
history; they are not authority for `flark-edit-v1` semantic deletion. The Mac
profile trace demonstrated missing active projection on all 120 measured
ordinary edits despite sub-budget editor work, so the runtime then published a
typed continuity policy. `flark_core` turned it into an exact
transaction/revision receipt; Flutter could update only the authorized visible
content run. The receipt remained live until a certified viewport at or after
its result revision covered the authorized content range.

Continuity is currently authorized for conservative plain-text edits to
emphasis, strong, strikethrough, code-span, and direct-link label content, plus
plain-text insertions inside parser-authored contiguous editable rows. The row
receipt preserves the active block and unaffected cached presentation through
the admission/certification interval; it does not authorize deletion,
replacement, or edits that touch an inline fact. Syntax-shaped replacements,
marker edits, autolinks, and reference links wait for current parser
certification. A profile-mode 1 MiB dense-inline Mac development receipt
recorded 0 raw or missing projected frames across 120 measured edits.
Editor-attributed p99 was 3.481 ms with no editor-attributed over-budget
samples. Flutter failed to foreground the final harness run, so its wall-clock
outliers are explicitly not claim evidence; controlled wall-clock, broad
shape/size, and device qualification remain open.

**Amendment (2026-08-18): literal-safe envelopes replace the continuity
receipt.** The clauses above are superseded by
[RFC 027 section 4.4.1](../../rfc/rfc_027_continuously_rendered_markdown.md).
The rule "does not authorize ... edits that touch an inline fact" is the
normative source of the dogfood flicker: it refuses boundary-adjacent
keystrokes the grammar can prove harmless, and it forces `flark_core` to
classify Markdown-sensitive characters itself, which the one-grammar rule
forbids. Both are contract defects, not implementation defects.

Under the amendment the runtime publishes, per certified row, exact source
ranges for typed literal edits that provably cannot change that row's published
facts. The vocabulary remains intentionally smaller than the general design:

- a non-empty ASCII letter/digit insertion inside a parser-authored maximal
  word leaf of an eligible fact. The leaf is identity-mapped, bounded by the
  fact edge or U+0020 on each side, independent of overlapping facts, and has
  no code normalization; and
- one U+0020 insertion at a parser-proved position. A non-empty space envelope
  admits positions strictly inside its range; a separate zero-width envelope may
  admit the exact editable-row endpoint once.

ABI 4.26 introduced both classes as one-shot authorities. ABI 4.27 adds the
`LITERAL_SAFE_ENVELOPE_CLOSURE_V1` proof: matched non-empty envelopes grow, and
non-empty envelopes with exactly equal byte and UTF-16 geometry form one
parser-authored bundle that grows together. The current reusable bundle is
limited to canonical single-line ATX content with authoritative empty inline
facts, identity coordinates, and ASCII word/space content bounded by word
bytes. A matched zero-width envelope is consumed. Unmatched envelopes strictly
crossed by a foreign-class insertion are
dropped; ranges before the edit stay and ranges at or after it shift. Reusable
space authority ends before an existing trailing-space run, and no terminal
zero-width proof is published when the row already ends in U+0020. The host
does not inspect source or recreate a consumed proof.

ABI 4.28 adds parser-authored projection edit cells. A complete canonical ATX
content cell admits arbitrary non-newline insertion, deletion, replacement, and
non-ASCII input while retaining the heading shell and painting only its content
exactly. A narrower one-shot cell for a conservatively isolated flat Strong
fact admits one U+0020 at its opening content boundary: only that Strong source
closure becomes exact while independent inline facts remain projected. Core
matches the declared trigger, performs range transforms, and never infers the
dependency closure from Markdown source.

ABI 4.29 capability `PROJECTION_EDIT_CELLS_V2` declares a chainable
ASCII-literal splice cell over
parser-authored plain literal segments. The initial emitter covers top-level
paragraphs, simple list and block-quote content, and plain table-cell content.
It excludes neighboring dependency boundaries, punctuation, entities,
non-ASCII input, empty or punctuation-bearing replacements, leading/terminal
space insertion, multi-unit
deletion, and structural edits. Nonempty ASCII-word replacement chains; a
U+0020 insertion chains only strictly inside the trimmed trigger, without
turning a retained leading/trailing space into indentation or a trailing hard
break. A separate one-unit deletion proof is consumed after one Backspace and is emitted
only when every admitted deletion leaves the cell nonblank. A terminal append
cell covers the parser-authored plain gap on the final physical line and admits
ASCII words plus bounded ASCII prose punctuation separated by single spaces at
its zero-width end trigger. A fresh
row ending in one U+0020 republishes blocked-space state, while two spaces or
other terminal whitespace suppress the cell. The physical line must begin,
after ordinary paragraph padding, with an ASCII letter; `-`, `1.`, `#`, and
other block-opener-shaped lines receive no terminal proof. Its carried state never admits
two consecutive terminal spaces. The affected
closure becomes one exact unstyled run—which is visually identical to the plain
rendered literal—while the row shell and all independent outside styles remain
projected. ASCII composing updates use the same declared replacement matcher.
Runtime differential tests compare admitted ranges, shell boundary cases, and
representative carried successors with a fresh final-source parse, requiring
the complete row/fact publication to match.

ABI 4.32 capability `PROJECTION_EDIT_CELLS_V3` adds one generic,
parser-parameterized exact-scalar predicate without changing the kind-16
layout. The parser supplies both the affected closure and one zero-width
trigger, stores the admitted Unicode scalar in `replacement_first`, and leaves
`replacement_second` zero. Core compares that parameter mechanically; it does
not classify Markdown. The first emitter admits `[` only at guarded interior
points of one isolated flat Strong fact on a single physical-line Plain row
when bracket classification is
exhaustive and the leaf has no existing bracket dependency. The transformed
Strong source is exact, plain source outside it remains projected, and the
one-shot authority is consumed immediately.

The V3 exact-scalar emitter also declares the frozen D0 prose punctuation set
(`.`, `,`, `;`, `:`, `!`, `?`, apostrophe, double quote, `(`, `)`, hyphen,
en dash, and em dash) at ASCII-alphanumeric guard pairs inside a fact-free
prefix before one authoritative Strong fact. The affected closure is that
complete prefix, so the punctuation result is exact while the outside Strong
fact remains certified. Each record is consumed after one edit.

V3 also declares the frozen different-marker syntax constructions as one-shot
exact-scalar cells: `*`, backtick, `[` and `]` beside one Emphasis fact, and
`_` or `~` beside one Strong fact. The current source must contain no instance
of the inserted marker, the fact-free prefix must be ASCII prose, and the
trigger must sit between ASCII-alphanumeric guards. Brackets additionally
require exhaustive parser bracket classification. The prefix is exact; the
different-marker sibling remains projected.

V3 also extends the existing guarded ASCII literal matcher with one strictly
interior, nonempty ASCII-alphanumeric/U+0020 replacement containing at least
one alphanumeric unit. The parser emits that authority only for a complete
fact-free physical-line gap and supplies a maximal interior ASCII prose
trigger. This supports the Product Tour's real multiword paste while retaining
the earlier Strong fact; Core compares only the protocol predicate and never
searches Markdown punctuation or constructs.

ABI 4.33 capability `PROJECTION_EDIT_CELLS_V4` adds matcher codes 7 and 8,
`EXACT_SPLICE_REPLACE_BLOCK_SHELL` and `SIMPLE_BLOCK_PREFIX_PLAN`, on the same
kind-16/query-6 transport. The
parser publishes one exact insertion or deletion over a complete bounded
physical line together with the clean-result Plain, ATX heading, depth-1 quote,
or simple list shell and its prefix geometry. Core compares only the exact
splice; the Flutter adapter exposes current result content exactly under that
typed shell and retains outside rows. The exact-splice record is one-shot. The
prefix plan carries a finite parser-declared ASCII sequence and activation
point, so a no-vsync `# `, `> `, `- `, or `1. ` burst can advance mechanically
to the same result shell without host marker knowledge; any different successor
fails closed.

The initial D0 matrix covers human-cadence and true unpumped construction of
`# `, `> `, `- `, and `1. ` plus structural Backspace removal. Every actual paint must carry the
exact source generation and caret, preserve an outside Strong sentinel, and
show the clean result shell. Prefix-inclusive fresh certification retires the
temporary authority, including the intermediate `>` quote before its optional
space. The host does not scan markers or infer a block transition.

ABI 4.34 capability `BOUNDED_PENDING_PRESENTATION_PLANS_V1` covers the bounded
multi-row result that kind 16 cannot express. One plan declares an exact ASCII
insertion sequence, a zero-width trigger, the predecessor-row count, and a
complete clean row snapshot for every admitted prefix. The parser owns those
rows and all retained inline facts; Core performs only protocol validation,
range transformation, and materialization into the single pending-
presentation snapshot. The initial emitter is deliberately limited to the
frozen D0 opening/closing fenced-code journey. A mismatch retires the plan
synchronously and fresh complete certification supersedes the final prefix.

Exact ABI 4.32 additionally maps the parser-certified body of a closed fenced
code block into bounded ASCII-word cells. Each affected closure is one physical
code line without its line ending, and each trigger is one maximal ASCII word.
The body publishes authoritative empty inline facts; the host keeps the fenced
code shell and paints only the changed authored line as exact current source.
Neither fence is edit authority.

General delimiter dependency graphs, non-ASCII composition islands, interior
punctuation replacements, cross-fragment multi-line replacements, nested
structural shells, and broader edit classes remain pending. Those paths still
paint the whole active row as exact source until
recertification. That is authority-safe but does not satisfy this contract's
local-island or no-marker-flash target. The 4.28/4.29 cells are the bounded
authority used by the current product-tour typing matrix; unsupported edits are
kept explicit rather than hidden behind host Markdown inference.

ABI 4.30 capability `LITERAL_SAFE_ENVELOPES_V2` adds one bounded delimiter
proof. For a parser-proved flat Strong fact with no escaping `*` dependency or
overlapping fact, edit class 3 admits exactly one collapsed `*` insertion
strictly inside the Strong content range. The host transforms the existing projected run, so
the Strong delimiters stay hidden and its style remains rendered. The envelope
and every same-geometry sibling are consumed and cannot be carried.

Exact ABI 4.31 bounds this vocabulary at 128 envelopes per row. When a fact has
multiple word leaves, each admitted leaf is published independently; exceeding
the bound drops only further optimization records, never the authoritative
inline fact set. When the complete page baseline fits, the ABI encoder first
reserves every row's ordinary facts and required projection-segment group, then
admits cells and envelopes only from the surplus payload; proof density on an
earlier row cannot make a later rendered row raw. Baseline groups that do not
fit remain subject to the ABI's complete-group fail-closed rule.

An insertion chain retains presentation only while every successor matches the
carried parser proof set and every transform succeeds; anything else uses that
whole-row fallback. Retained presentation is proven, not assumed. The old typed
row/inline policy and host Markdown classification remain removed from the
active decision path.

ABI 4.26 appends envelope records only for the capability-gated
`SEMANTIC_PROJECTED_LITERAL_SAFE` query kind. The earlier `SEMANTIC` and
`SEMANTIC_PROJECTED` query kinds keep their pre-envelope payload vocabulary.
ABI 4.28 retained query kind 6 and added record kind 16 plus the V1 capability.
ABI 4.29 adds V2 matcher semantics under bit 29 and requires exact 4.29
negotiation. ABI 4.30 adds literal edit class 3 under bit 30 and requires exact 4.30
negotiation. ABI 4.31 adds structural presentation proof receipts under bit 31
and requires exact 4.31 negotiation; earlier minors are rejected rather than
receiving widened authority.

Old facts plus a source splice are not sufficient authority. Flutter may keep
layout/cache storage internally, but it cannot paint stale semantic identity as
current. ABI 4.31 permits retained structural presentation only when Rust marks
the receipt `PRESENTATION_PROVEN` from a current Ready result: the bounded Plain
split/merge proof establishes an unchanged inline partition, and a successor
ASCII cell is a typed consequence of that Rust proof. It may carry ordinary
typing in the new empty Plain row, but it cannot authorize another structural
transition. Every unproved structural receipt paints exact source, and an
unmatched successor clears the transitional surface.

The final ABI 4.32 contract also admits the already-typed simple-list indent
and outdent transitions when the current Ready parser context certifies the
list row and the semantic resolver produces exactly one bounded ASCII-space
prefix splice plus a ListItem result context. Core may map the certified row's
runs through that prefix splice and retain the list shell. The proof is only a
post-commit presentation transition; it does not authorize a later ordinary
edit or another list action.

## 9. Composition and platform input

### LP2-COMPOSITION-BEGIN-001

The input connection retains the exact source window. Beginning composition
captures the active range and starting semantic intent. Each source-mutating
update must return a parser-authored composition result proof that confines
`composition_exact` to the mapped composing payload and retains the complete
surrounding shell/runs, including hidden delimiters. The proof may not be
reconstructed from predecessor facts plus a splice.

Without that proof the update fails typed before mutation; broadening the
complete row to exact source and revealing its markers is not a
`flark-edit-v1` fallback. ABI 4.36 lacks this general capability, so native
composition remains outside the current dogfood support domain rather than
claiming profile conformance through the older row-wide exact behavior.

### LP2-COMPOSITION-UPDATE-001

Every accepted delta updates exact source, composing range, canonical
selection, bounded composition proof, and candidate/prompt geometry in order.
Repeated updates do not duplicate, normalize, or reorder text.

### LP2-COMPOSITION-COMMIT-001

Commit applies `EP1-ATOMIC-TEXT-BOUNDARY-001`, publishes its complete result
proof, and closes the island. Cancel restores exact precomposition source and
intent with fresh authority. The input connection is not restarted solely for
a presentation change.

### LP2-INPUT-RESYNC-001

Window resynchronization uses epochs and exact source identity. Projection
offsets never enter the platform editing value. A resync preserves the
canonical composition when the platform contract permits it or returns a typed
reason that the composition was cancelled.

The executable matrix includes dead keys, combining marks, emoji/ZWJ, CJK or
another multi-stage IME available on the test system, autocorrect replacement,
dictation-shaped replacements where available, and bidi selections.

## 10. Gesture contract

| Platform input | Default result |
| --- | --- |
| Mouse primary down + drag | Extend text/object selection |
| Mouse wheel | Scroll; never mutate selection |
| Desktop trackpad scroll/pan | Scroll; never mutate selection |
| Shift + primary click | Extend selection from canonical anchor |
| Touch one-finger drag | Scroll until selection mode is deliberately entered |
| Touch long press | Enter platform selection mode |
| Selection-handle drag | Extend the selected endpoint |
| Stylus | Follow platform text-selection convention, tested separately |
| Horizontal code/table pan | Scroll the nested axis after gesture arbitration; do not create text selection |

Pointer-kind and button state are part of arbitration. A generic `onPan` route
that always activates/extends selection is nonconforming.

## 11. Construct behavior

| Construct | Editor stable state | In-progress/source access | Read-only state |
| --- | --- | --- | --- |
| Paragraph | Rendered editable text | Exact local island when pending | Rendered text |
| Emphasis/strong/strikethrough | Styled editable content; markers hidden | Typed delimiters literal until certified | Styled content |
| Code span | Monospace projected content with typed replacement mapping | Incomplete delimiter island | Monospace content |
| Heading | Styled editable content; marker hidden | Marker literal until certified | Styled heading |
| List/task | Synthetic marker/control plus editable content | Incomplete marker island; structural recipes | Rendered list/task |
| Block quote | Rendered container plus editable content | Incomplete prefix island | Rendered quote |
| Fenced/indented code | Editable code body without active raw-row switch | Incomplete fence/source metadata affordance | Rendered code block |
| Link/autolink | Editable label/content with semantic target action | Typed incomplete link literal; target action edits destination | Styled/activatable link |
| Image | Object/alt presentation with semantic action | Incomplete syntax or metadata action | Object/alt presentation |
| Table | Rendered editable cells with source-owned geometry | Incomplete row exact island | Rendered table |
| Thematic break | Atomic rule with before/after caret stops | Incomplete marker text | Rule |
| Reference definition | Editor source-material affordance | Exact source | No visible GFM output |
| Raw HTML under selected policy | Intentional literal/source-material presentation | Exact source | Literal according to product HTML policy |
| Unsupported projection shape | Typed exact island; no false semantics | Exact source | Typed exact/literal fallback |

“Source-material affordance” is a deliberate editor representation, not the
old behavior of revealing any active row.

## 12. Clipboard contract

Initial v2 behavior is intentionally narrow:

- ordinary rendered-mode editor copy/cut exports the selected visible payload
  as `text/plain`, omitting hidden delimiters and metadata. Exact/literal
  islands contribute their visibly displayed source characters and atomic
  objects contribute their declared plain-text representation. Exact Markdown
  export requires a separately named future command/MIME type;
- cut follows the prepared-deletion ordering in `EP1-COPY-CUT-001`: native
  range deletion and its complete result are validated and reserved before the
  external clipboard write, then the same single-use operation is committed
  under the unchanged command gate. Clipboard state is not falsely claimed as
  rollbackable with source;
- editor paste preserves the received plain text as exact source bytes only at
  a plain/exterior or authenticated exact/literal target; inside an existing
  semantic owner or range longest-common path, it preserves the exact visible
  clipboard payload while Rust escapes/re-delimits source according to
  `EP1-PASTE-001`;
- read-only copy uses selected visible text; and
- no rich or multi-MIME preservation claim is made.

Marker-inclusive selection expansion, rich HTML, `text/markdown`, images, and
cross-application formatting are separately versioned future capabilities.
These choices are governed by `POL-CLIPBOARD-PROJECTION`, not inferred from
Markdown grammar or platform callback shape.

## 13. Read-only widget contract

`FlarkMarkdownView`:

- uses the same runtime semantic facts, projection snapshot, shaping, layout,
  paint, theme, and virtualization code as the editor;
- creates no text input connection, undo stack, edit command router, caret, or
  editor-only source-material UI;
- supports optional visible-text selection and caller-provided link actions;
- fails closed to current literal/exact presentation when semantics are not
  available; and
- exposes a bounded viewport mode for the multi-MiB contract.

The public constructor may own a session from supplied Markdown or consume a
caller-managed document/controller. Final naming and lifecycle ergonomics are
reviewed in T1 without creating a Flutter dependency in `flark_core`.

## 14. Parity and accessibility

For a current, quiescent revision, matching width, theme, scale, locale, and
platform font environment:

- editor and view produce equal semantic projection and render-plan hashes;
- layout and paint differ only by enumerated editor affordances;
- focus and selection do not change the underlying render-plan hash; and
- semantics describe visible content and actions without announcing hidden
  markers as ordinary visible characters.

Accessibility acceptance requires live screen-reader and platform selection
inspection. Widget semantics tests are necessary but not sufficient.

## 15. Boundedness

Projection, mapping, layout, hit testing, semantics, and paint remain bounded
by viewport, fragment, fact, and byte caps. A single giant line, table, code
block, object, or selection cannot force whole-document materialization.

The Mac matrix retains ordinary prose, giant line, tiny blocks, dense inline,
tables, references, and 1/2/5/10 MiB sizes. The read-only view runs the same
render-shape matrix where interaction applies. Embedded unbounded layout has no
large-document claim unless a later contract supplies a cap and receipt.

## 16. Required executable categories

The T1 JSON profile/matrix must own at least these unique case families:

- focus and selection stability;
- hidden-boundary affinity and hit testing;
- platform-originated selection normalization at hidden boundaries;
- ordinary insertion inside every inline style;
- incomplete-to-complete and complete-to-incomplete transitions;
- replacement and deletion across hidden markers;
- split, merge, paste, undo, and redo;
- cross-block and cross-page selection;
- composition begin/update/commit/cancel;
- grapheme, emoji/ZWJ, bidi, and line endings;
- pending-to-certified and non-local dependency invalidation;
- source gaps, typed faults, and oversized projection fallback;
- heading, list/task, quote, code, link/image, table, thematic break, reference
  definition, and raw-HTML policy behavior;
- desktop pointer/scroll arbitration and prepared mobile gesture cases;
- editor/read-only render-plan parity; and
- bounded ordinary/giant-line/dense-block performance receipts.

Every step records exact source, revision, source anchors, composition,
presentation state by range, visible runs, legal caret stops, certification
authority, and terminal outcome.

## 17. Failure conditions

Any of the following fails the contract:

- moving focus reveals certified markers or changes wrapping;
- a scroll gesture changes canonical selection;
- visible semantic styling comes from a different revision;
- a hidden delimiter becomes an accidental caret stop or invisible deletion;
- an edit, selection, composition, or undo changes exact source incorrectly;
- unrelated certified content becomes raw because one local construct is
  pending;
- editor and view use different Markdown recognition or render-plan logic;
- the platform input value contains projected rather than exact source without
  a future explicit contract amendment; or
- a supported construct uses “active raw source” as its final editing mode.
