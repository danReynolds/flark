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
ordered quote/plain surface set; literal successors map that set through their
exact splice without dropping an unaffected peer. ABI 4.16 closes the empty
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

### 4.5 Composition islands

An active platform composing range is exact current source and remains stable
until the platform commits or cancels it. Projection outside the composing
island remains current. The surface may retain a certified surrounding style
only when the current runtime result or a continuity receipt authorizes it.

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
| Composition may span a marker or replacement run. | Reprojecting can corrupt the IME transaction or candidate rectangle. | Freeze an exact composition island and project around it. |
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
