# RFC 030: Flark v5 — the synchronous core

**Status:** PROPOSED. 2026-09-02.

**Supersedes for execution:** RFC 026, RFC 027 §4 onward, RFC 028, RFC 029,
and the v4 build plan. **Retains the product intent of:** NORTH_STAR.md, the
product examples in RFC 027 §2,
[`edit_profile_v1.md`](../v5/edit_profile_v1.md), and
`live_editor_test_strategy.md`. Those active contracts are updated where v5
removes asynchronous parsing and very-large live editing.

**Requirements this RFC is built around** (owner's words, 2026-09-02):
blazing fast on reasonably sized documents on mobile; live Markdown editing
that never shows unexpected raw markers; never janky or laggy; runs on
Flutter, Flutter web via Wasm, and Fleury (terminal and browser).

## 1. Decision

Flark v5 is a live Markdown editor whose parse result is always known before
a frame paints. Every keystroke runs one synchronous chain inside the input
callback:

```text
current model + command → source splice → parse candidate (unmodified comrak)
                        → render model → projection → publish snapshot
                        → host paint
```

There is no incremental engine, no certification, no pending presentation,
and no parser-result revision race. Those mechanisms existed to paint frames
before the parser had answered. In v5 the parser has always answered. Platform
input still owns ordinary delivery concerns such as composition, duplicate
callbacks, and stale host selections.

The product bet is unchanged and is the reason the projection layer exists:
Markdown source is the document, and delimiters of a complete construct are
never shown at the caret. No shipping editor offers both. The engine was
never required for the bet; the projection is.

## 2. Why synchronous, in one paragraph

The strict promise, "never shows unexpected raw markers", is a statement
about the frame after a keystroke. Any asynchronous design either paints
that frame from stale structure, which shows the closing `*` literally for
one frame, or proves ahead of time that the keystroke cannot change
structure, which is the hundred thousand lines v4 spent on envelopes, plans,
and certification. A synchronous parse makes the promise a structural
property: the frame is the parser's answer for the current source, always.
Measured full-document comrak parse with sourcepos and GFM extensions on an
M1 Pro: 0.5 ms at 25 KB, 2.1 ms at 100 KB, 25 ms at 1 MB. Real corpora top
out under 100 KB at p99. The qualified sync tier covers the live-rendered
product; the rest opens in source mode.

## 3. Envelope

| Tier | Limit (provisional) | Behavior |
| --- | --- | --- |
| Sync | initial qualification floor: ≤ 16 KiB on phones, ≤ 32 KiB on desktop, and within a receipt-defined shape budget | Full live rendering, all promises hold |
| Source mode | outside either byte or shape limit, or a typed extraction deviation prevents a trustworthy live model | Monospace source editing with a visible notice; no live rendering; nothing silent |

The size limits are measured in UTF-8 bytes. A byte ceiling alone is not a work
bound: a flat list can create thousands of blocks well below the byte cap, and one
giant line can move cost into layout. Admission therefore also has a small,
published shape budget (physical lines, maximum line length, and model
rows/runs), calibrated by the same adversarial receipts. Cheap byte and line
checks run before parsing; a candidate outside the post-parse model budget is
not published as live-rendered state. The initial 16/32 KiB limits are
qualification floors, not yet product claims. They reflect the corrected M2
kernel rather than the earlier parse-only and pre-correction spikes. Ordinary
prose may support 32 KiB on phones and 64 KiB on desktop, but those are stretch
candidates raised only by a production-path device receipt. The never-stale
promise applies inside the sync tier. Source mode is V5's terminal behavior
beyond it, not a placeholder for an automatic async live-rendered path.
For an in-envelope extraction deviation, initial open fails closed into source
mode, an existing source-mode document remains writable there, and an edit to
an existing live snapshot is rejected atomically. No untrustworthy projection
is published.

## 4. Packages and dependency direction

```text
flark_fleury    Fleury editor + view widgets (cells)
flark_flutter   Flutter editor + view widgets (RenderBox)
      \           /
       \         /
        flark           pure Dart kernel: document, projection, commands,
          |             history, FlarkEditor facade, parse transports
          |
   native/flark_parse   Rust: unmodified comrak → render model
                        C ABI (FFI) and wasm32 (js_interop), four exports
```

- `flark` has no Flutter import. The boundary test from v4 stays.
- The parse transports live inside `flark` behind conditional imports, as
  v2 did: `dart:ffi` on the VM (macOS, iOS, Android, Linux, Fleury
  terminal), `dart:js_interop` on the web (Flutter web under dart2wasm,
  Fleury browser under dart2js). One package for consumers.
- Rust is the only authority that recognizes Markdown meaning. Dart operates on
  ranges the render model hands it; an explicit semantic command may emit a
  canonical spelling, but the candidate parse must prove the requested result.
  Dart may not infer a construct by scanning source delimiters. The model must
  carry every range a host needs.

## 5. The parse crate and the render model

`flark_parse` depends on comrak as an ordinary crate, unforked and
unpatched, with `sourcepos` on and the GFM extensions the profile pins. It
exports four functions on both targets: `version`, `alloc`, `parse(bytes) →
buffer`, and `free`. Parse is a pure function of the bytes. No session, no
state, no fuel.

The output is the **render model**, one flat little-endian buffer:

**Blocks**, in document order, each with: kind (paragraph, heading with
level, fenced code with info range, indented code, block quote, list with
ordered/start/tight, list item with task state, table, table row, table
cell with alignment, thematic break, HTML block, footnote definition,
reference definition); parent index; source range in bytes; and one
**content range per physical line** the block owns.

The per-line content ranges are what let hosts hide structural prefixes
(`> `, `- `, `1. `, fence lines) without scanning. comrak does not expose
them per line, but the stripped content of each line is always a suffix of
the physical line after tab expansion, so the extraction derives each
line's content start by suffix alignment. A differential test over the
CommonMark and GFM corpora proves the derivation: for every block and line,
source slice at the derived range equals comrak's own line content.

**Inline runs**, in document order within each leaf block, each with: kind
(text, emphasis, strong, code, strikethrough, link with destination and
title ranges, image, autolink, backslash escape, entity or replacement with
display text, hard break, soft break, inline HTML, footnote reference);
block index; parent run index; source range; content range. Hidden bytes
are exactly source minus content. This is v4's `DocumentInlineFact`
vocabulary and v2's bridge arithmetic, reused as a specification.

**Reference definitions** get their own record because comrak exposes no
node for them; v2's `reference_definitions.rs` scanner is salvaged verbatim.
They render as a source-only construct, which the North Star already
permits.

**Coordinates.** Every range is emitted in both bytes and UTF-16 code
units by Rust. v2 rebuilt a byte-to-UTF-16 mapper in Dart on every parse,
two integer lists the length of the document; v5 never builds one.

**Derive, then validate.** Anything the extraction derives rather than
reads from comrak, the per-line content ranges and the reference
definitions, is checked against comrak's own output in debug builds and in
the conformance differential, and a mismatch is a hard failure. This is
the pattern v2's reference-definition scanner already used, and it is the
rule that keeps the extraction from becoming a second parser.

**Marshal rule.** The buffer for a 100 KB document is on the order of 100
KB and decodes into typed-data views, not objects. Dart materializes a block
or run only when a host asks for it. The v2 postmortem found two thirds of
its per-keystroke cost in Dart-side decoding of a whole document into
objects. That is the one place this design can quietly fail, so it is a
spike with a number before any kernel code exists.

## 6. The Dart kernel

Six concepts. A new engineer should be able to hold all of them in one
sitting.

**`FlarkDocument`** is an immutable value: source string, selection, the render
model derived from the source, and its projection. `FlarkEditor` owns command
application and history and publishes a new document only after the complete
candidate succeeds.

**`FlarkProjection`** turns the render model into what a host draws: rows
(one per leaf block, plus container shells for quotes and lists) of runs,
each run carrying display text, its source range, an exactness flag, and
merged styles. Overlapping inline facts are cut at every boundary and their
styles merged, which is v2's inline segmentation module, salvaged. Hidden
ranges never become runs. Replacement runs, such as entities, carry display
text that differs from source and map to an edge by affinity. Bidirectional
offset mapping is v4's `FlarkSurfaceTextRun`, about fifty lines, salvaged.

*Pointer refinement (2026-09-07).* The active editing profile now chooses the
word's context at a visible edge beside whitespace or a row boundary. Hit-half
selection remains for boundaries between adjacent non-whitespace glyphs. This
replaces the subpixel-dependent word-edge behavior described in the historical
M2 account below; the source-anchor model and keyboard rules are unchanged.

**The caret model** is a visible offset plus a source anchor. One visible
position can have several legal source anchors, before or after a hidden
range, and which one the selection holds is the semantic context: typing there
stays bold or leaves bold. Navigation sets the anchor by rule: arriving by
typing keeps the current context; arrow keys cross a boundary on the first
press and never produce a stop that does not move; pointer placement uses
the glyph half, exactly as `edit_profile_v1` already states. The caret is
never inside a hidden range. This is the essential complexity of the
product and it is small.

*As built in M2 (2026-09-03), amending the paragraph above.* The caret is
a plain source offset: a legal offset is one not strictly inside a hidden
run range and lying on a row's caret span, and the anchors of a display
position are the legal offsets that display there. No separate caret type
exists; `FlarkSelection` plus the document's legality and anchor queries
is the whole model. The rules as shipped: movement keeps the context it
came from (moving right takes the smallest anchor, moving left the
largest, so the context changes only by crossing a glyph of another
style); Home and End take the outermost anchor; pointer placement takes
the outer anchor for a leading glyph half and the inner for a trailing
one. One deliberate exception to "never a stop that does not move": Right
at the end of the document, and Left at its start, step to the outermost
anchor without moving, because otherwise a styled run at the end of a
document could only be left by a formatting command. Fence lines of a
code block hold no caret, since an edit there would be invisible; the info
string is a host affordance. A formatting toggle at a collapsed caret
inside a span unwraps the whole span rather than splitting it, because
Markdown's rule of three makes `**a****b**` not parse. Delete-to-empty
leaves the removed delimiters as pending typing intent, which history
restores on undo.

**`FlarkCommand`** is the closed set of logical actions: insert text,
delete backward, delete forward, newline, replace range, set selection,
move caret by grapheme, word, line, or rendered row, undo, redo, toggle task,
indent and outdent, paste, toggle emphasis, strong, strikethrough, or
inline code on the selection or the word at the caret, and set heading
level. Every platform route, delta, full value, key, or
IME, is classified into one of these before any Markdown is involved. That
is `edit_profile_v1` rule 1 and it stays.

**Command semantics** implement `edit_profile_v1` as range arithmetic over
the render model. Deleting the last styled grapheme removes the run's
hidden ranges with it. Return at a list item continues, exits, or splits
using the block's line content ranges. Backspace at a block start lifts the
prefix using the same ranges. Rust alone recognizes Markdown meaning; Dart may
read parser-bounded source to preserve or emit canonical syntax for an explicit
semantic command, and the candidate parse must prove the requested result.
Every rule has a headless test of the form: source, caret, command, expected
source, caret, and projection.

**Incomplete syntax needs no concept.** `*hello` is a paragraph with a
literal asterisk, because that is what the parser says. There are no exact
islands, no uncertainty, no fallback. The literal delimiter is visible
because it is the current authoring content, which is RFC 027 §2.1 word for
word. v5 does not auto-close delimiters; the parser-recipe auto-close from
v4 can return later as a feature if dogfooding wants it. The one
candidate exception is the opening fence: typing three backticks makes the
parser read everything below as code until a closing fence exists, which
is correct and document-wide. v2 spent a thousand lines reconciling an
auto-closed fence against IME echoes; in a synchronous kernel the same
auto-close is one command with no echo to reconcile. Whether to enable it
is decided by dogfood, not in this RFC.

**The hide model needs three affordances, and they are in scope.**
Because delimiters of a complete construct are never shown, the user cannot
remove formatting by deleting a marker, cannot see whether the next
keystroke is bold at a boundary, and cannot edit a link's URL in place.
The kernel therefore exposes formatting commands (above), a
`typingContext` on the facade that names the styles the next keystroke
will inherit so hosts can show a cue, and hosts ship link and image
popovers, which v2 already built. Reveal-at-caret, the Obsidian and Typora
behavior, is retained as a projection option that shows the hidden ranges
of the run containing the anchor. It is off by default and is promoted to a
user-facing toggle only if dogfooding in Dune shows the boundary cue is not
enough. The anchor model is the same in both.

**History** salvages v4's session rules: a one-second typing coalescing
window, composition joins the open group, one logical action is at most one
entry, undo restores exact source and selection.

**`FlarkEditor`** is the facade and the only thing a host constructs:
`snapshot`, `apply(command)`, `typingContext`, a change listener, and the parse
backend. Its sealed snapshot is either live, with a document and projection, or
source-only, with exact text and selection but no stale render model to paint.
The v4 core exports 113 types and no facade, and the audit counted that as its
single largest defect for a second consumer. As built, the v5 facade library
exports 26 concepts on either platform, 27 in the union of its conditional FFI
and Wasm exports, with the command set counted once and the union gated at 27
by the recursive boundary test. The render model with its schema constants is a second library,
`package:flark/render_model.dart`, for hosts and tests that read it.

## 7. Flutter surface

Salvaged nearly whole from v4, then pruned: a custom `RenderBox` with
`TextPainter` per row, a `DeltaTextInputClient`, a 16 KB input window of
source text around the caret so platform offsets are source offsets, the
newline-as-action deduplication, the composition base, the hand-built
semantics tree, and `FlarkMarkdownView` sharing the same surface without
input. super_editor makes the same top-level choices; they are mainstream
for this problem.

Two changes. **Scrolling** uses a real `Scrollable` and `ScrollPosition`
over a multi-child layout instead of the 32-row page snap, which restores
scrollbars, ballistics, `ensureVisible`, and accessibility scrolling.
**Rows are a protocol**, not one painter: each block row implements caret
rect, hit test to source offset, and selection rects, so tables get borders
and cell navigation, code blocks get a background and syntax highlighting,
images get a real image row, and lists and quotes get drawn shells. v2's
inline image, link and image popovers, syntax highlighting, theme, and
table and task descriptors are the salvage for those rows. Rendering
fidelity is where the effort goes, because the engine no longer needs any.

Removed: successor lineage, the reconciliation map, the certification
barrier and second notification channel, the per-callback SHA-256 of the
window, the viewport pager, and every generation counter but one.

## 8. Fleury surface

Fleury is a retained-mode cell UI for the terminal and the browser. Its
terminal host is the Dart VM, so the FFI transport applies through the same
`hook/build.dart`, with prebuilt binaries fetched by the hook so a Fleury
app does not need a Rust toolchain. Its browser host is dart2js over a DOM
grid, so the wasm transport must load through `dart:js_interop` alone; the
v2 loader already does this and only its `dart:ui_web` asset lookup goes.

`flark_fleury` provides an editor over Fleury's `TextInput` and
`TextEditingController`, and a view. Rows become Fleury `TextSpan` cells;
hidden ranges are simply not emitted; the caret maps through the projection
exactly as in Flutter. Quote bars, list bullets, code backgrounds, and
table borders use box-drawing cells and Fleury's own table widget. Fleury's
existing `MarkdownText` stays for its tiny-renderer niche; the view is the
full-fidelity option.

Fleury is the second host that proves the kernel's editing semantics are
portable. Its browser surface shares the Wasm transport with Flutter web.
An early Flutter-web smoke is part of M3a; full Fleury delivery is M4 and
full Flutter-web qualification remains M5, as detailed in the build plan.

## 9. Web

One comrak, compiled to `wasm32-unknown-unknown` by the salvaged v2 script,
loaded by the salvaged v2 loader, called synchronously. Under Flutter's
dart2wasm and Fleury's dart2js the call shape is identical. Native and Wasm
equivalence is therefore not an argument but a test: the conformance corpus
runs through both transports and the render models must be byte-identical.

## 10. Performance model

Provisional per-keystroke allocation for a phone at 120 Hz (8.3 ms), at the
16 KiB qualification floor:

| Stage | Budget | Basis |
| --- | --- | --- |
| Source command, validation, and history | 0.5 ms | bounded source copy and one history snapshot |
| Parse and marshal | 2.5 ms | corrected M2 desktop p99 under 0.8 ms at 16 KiB, with floor-phone reserve |
| Projection and caret legality | 3.0 ms | complete immutable rebuild; corrected facade p99 is the controlling measurement |
| Layout and paint | 2 ms | visible rows only; painters reused on identical spans, salvaged from v4 |

The bar is v2's own measurement, not the parse alone. v2 measured parse
plus projection plus render plan at 8.5 ms for 25 KB of dense Markdown and
39 ms at 100 KB on a workstation, with two thirds of that in Dart. Under the
de-scoped live tier, the corrected V5 facade must stay under 4 ms p99 at 16 KiB
on the M1 Pro before the floor-phone full-frame test is attempted. Parser-only
or pre-correction spikes do not clear that gate.

The 2026-09-03 M2 baseline measured 1.50–1.58 ms p50 and 2.2–2.5 ms
p99 at 25 KiB, but later correctness work added Unicode legality and immutable
snapshot ownership to the hot path, so that number is historical rather than a
current exit receipt. On the uncommitted correction-pass tree on 2026-09-04,
the 16 KiB-class structural fixture (16,750 bytes) measured insert 1.78/2.18 ms p50/p99 and
Backspace 2.60/3.14 ms; parse-and-marshal was 0.64/0.80 ms and projection
0.30/0.54 ms. Those local timings do not close the current kernel gate or
qualify layout and paint. The [build plan](../v5/build_plan.md#evidence-before-closeout)
records the later review, semantic failures, and Backspace budget miss; M2b
requires a passing receipt on the corrected named commit.

Flatness across sizes is not a claim v5 makes. The claim is that every document
inside the published byte-and-shape tier is under budget on the named phone,
with a receipt that names the commit, library hash, device, display rate, and
document shape. The v4 receipt format is salvaged.

## 11. Testing

The smaller state space makes testing cheaper. The product contract defines
expected behavior; tests and their observation helpers can still be wrong.
The [testing strategy](../../testing/live_editor_test_strategy.md) defines
fault-detection checks, early real-input sessions, and the recurrence rule.

- **Direct kernel scenarios.** Typed Dart cases start from source and issue
  production commands, with acceptance, expected source, caret, projection,
  and relevant styles/container membership after every step and follow-up input.
  There is no serialized journey vocabulary or replay framework. Every live
  edit is a clean parse; the tests must also prove the requested editing intent.
- **Projection invariants**, asserted on every step of every scenario:
  display text contains no source code unit from a hidden range; display text equals
  source minus hidden ranges plus replacements; the caret is never inside a
  hidden range; offset mapping round-trips.
- **Generated matrix.** Seeded random command sequences over the corpus
  documents preserve uninterrupted histories, with undo/redo probes separate
  or occasional. A failure prints its seed and command log so it can be
  minimized into a directly named regression. M2a corrects the current generator
  that inserts an undo/redo probe after each mutation.
- **Conformance.** The 652 CommonMark and 670 GFM cases run through the
  render model, comparing rendered text and ranges to the reference, on
  both transports.
- **Actual paint** stays for the Flutter layer, for paint, geometry,
  focus, and rapid unpumped bursts. Those bursts now test the input bridge,
  not the engine, which is the only thing they were ever able to find.
- **Native canaries** and the dogfood milestone stay as written.

Two rules inherited from v2's failure. **No test that asserts on the
edited frame may settle first.** v2's suite ended every step with a settle
that drained the debounce and the isolate, so it never saw the one frame
the promise is about. In v5 one immutable coherent snapshot must be available
before the input callback returns, and the first subsequently produced frame
must use it; settling before that assertion is forbidden. **Budgets are
frames.** v2 asserted 75 to 250 ms per
step, four to fifteen frames; v5 budgets are the single frame in §10.

A dogfood bug becomes a direct regression at its owning layer, with a paint or
native case when that provides additional evidence. Another variant of a
purported family-level fix triggers review of the rule and its observation
before further feature expansion.

## 12. Salvage manifest

| Keep | From | Use |
| --- | --- | --- |
| NORTH_STAR.md, DOGFOOD_MILESTONE.md, edit_profile_v1 (now `docs/architecture/v5/edit_profile_v1.md`), test strategy | v4 | product contract, rebased to the v5 execution model and live envelope |
| Sourcepos to byte-range arithmetic, reference definition scanner, payload layout | v2 `native/comrak_bridge/src/{parser,reference_definitions,payload}.rs` | core of `flark_parse` |
| Wasm build script, `hook/build.dart`, js_interop loader | v2 `scripts/`, `hook/`, `native_comrak_bridge_factory_web.dart` | transports |
| Inline segmentation (covering model) | v2 `render_plan/flark_inline_segmentation.dart` | projection |
| `FlarkSurfaceTextRun` offset mapping, hidden-range projection, caret normalization | v4 `surface_projection.dart`, `surface_projector.dart` | projection and caret |
| History grouping rules | v4 `editor_session.dart` | history |
| Semantic command admission shape | v4 `editor_semantic_command_planner.dart` | command semantics, rewritten over ranges |
| Render surface, delta input client, input window, semantics | v4 `flark_flutter/lib/src/{render_surface,editor,platform_input_bridge,editor_input_state,markdown_view}.dart` | Flutter surface |
| Block widgets: image, popovers, syntax highlighting, theme, table and task descriptors | v2 `lib/src/v2/flutter/`, `render_plan/` | block rows |
| GFM and CommonMark fixtures, deviation registers, receipt format | both | conformance and receipts |
| Inline fact vocabulary | v4 `flark_runtime` `DocumentInlineFactKind` | render model spec only |

## 13. Retire manifest

Archived under `legacy/`, with the technical note the August review
recommended: `flark_engine` (persistent tree, measured sequence, reference
root and journal), `flark_parser` (donor fork, reimplemented inline phase,
persistent sessions, inline projection job), `flark_runtime`, `flark_abi`
v4.38, the pending presentation stack and its counterfactual plans, literal
safe envelopes, input transaction lineage and reconciliation, the viewport
pager, v2's speculative projection, live block reconciler, and projected
editable text.

## 14. Risks and the spikes that retire them

Each spike is a day or two and ends in a number in the repo.

| Risk | Spike | Pass |
| --- | --- | --- |
| comrak inline sourcepos is wrong inside containers, links, or entities | differential over the 1,322 conformance cases: source slice at every run's range equals the expected delimited text | zero unregistered deviations; known ones enter the register with a rule |
| Per-line content derivation fails on tabs or lazy continuation | same differential, per line | zero |
| Floor-phone live frame | complete input-to-raster path on a mid-range Android at 16 KiB, then stretch at 32 KiB | every admitted edit inside one 120 Hz frame at 16 KiB |
| Corrected kernel keystroke | 16 KiB structural document, command plus parse, marshal, projection, caret, and history; FFI and Wasm | under 4 ms p99 on an M1 class machine |
| Adversarial in-tier shape | giant line, flat list, dense table, deep quote/list nesting, reference-heavy and Unicode-heavy documents at each candidate limit | every admitted shape stays inside the complete frame budget; over-budget shapes enter source mode |
| Fleury browser transport | comrak wasm loaded from a dart2js page, parse a document | works, startup under 100 ms |
| Fleury native packaging | a scaffolded Fleury app consuming `flark` without Rust installed | prebuilt fetch works |

**Results, 2026-09-02, M1 Pro** (code and register under `spikes/v5/`):

| Spike | Result |
| --- | --- |
| Sourcepos and per-line differential, 1,322 cases | 15 cases in four classes, all with deterministic corrections, registered in `spikes/v5/SOURCEPOS_REGISTER.md`; zero unexplained. **Pass** |
| End-to-end keystroke, FFI, 25 KB dense | 0.97 ms p50, 1.19 ms p99 against v2's 8.5 ms. **Pass** |
| Marshal, 100 KB | decode plus projection 0.54 ms, 0.43 ms with the per-block memo. **Pass** |
| Wasm under dart2js, 25 KB, warm | 1.2 ms p50, 2.0 ms p99; parse 0.8 ms, equal to native; instantiate 15 to 29 ms. **Pass** |
| Earlier phone parse/marshal kernel, iPhone 16 on iOS 18.7, profile build | 25 KiB 0.69 ms p50 / 0.76 p99; 64 KiB 1.75 / 1.83; 100 KiB 2.78 / 3.84. Useful capacity evidence, but it predates the corrected Dart caret/projection path, excludes layout and paint, and uses a flagship; it does not set the V5 live limit |
| Fleury native packaging | not yet run; needs prebuilt binary hosting |

At 25 KB over FFI the parse is 78 percent of the keystroke and the marshal
is 14 percent. The relationship v2 had is inverted.

If the sourcepos differential fails in a way the register cannot express,
the fallback is a bounded Rust-side delimiter locator over comrak's own
inline tree, not a second parser. That decision is made only on evidence.

## 15. Sequence

1. **M0 Spikes.** The table above.
2. **M1 Parse.** `flark_parse`, render model, both transports, conformance
   through the model on both. Exit: 670 of 670 GFM byte-identical native and
   Wasm.
3. **M2 Kernel.** M2a closes semantic/test gaps; M2b qualifies the corrected
   kernel with reproducible performance, local checks, and named-commit CI.
4. **M3 Flutter.** M3a proves the paragraph/inline input-to-paint path,
   composition/history, shape admission, source recovery, and an early
   Flutter-web Wasm smoke. M3b completes rows/scrolling and qualifies the full
   shape envelope. M3c integrates Dune and completes sustained dogfood.
5. **M4 Fleury.** Editor and view, terminal and browser. Exit: the same
   semantics pass direct host tests; browser transport receipt. May overlap
   M3c once the shared contracts stabilize.
6. **M5 Flutter web.** Wasm build, parity run, mobile receipts on the named
   devices. Exit: envelope limits published from receipts.
7. **M6 Release.** Accessibility, device input/lifecycle, packaging, and released
   artifacts consumed by Dune and Fleury. Windows remains later work.

The full ladder with exit criteria, budgets, and sizing is the
[v5 build plan](../v5/build_plan.md).

## 16. Decisions requested

1. Sync tier limits and source mode above them, as in §3.
2. The never-stale promise stays strict inside the tier and is the reason
   for the synchronous design.
3. No delimiter auto-close in v5.
4. Package names: `flark`, `flark_flutter`, `flark_fleury`; Rust crate
   `flark_parse`.
5. Fleury before Flutter web.
6. The hide model, with formatting commands, a typing-context cue, and
   link and image popovers as required affordances, and reveal-at-caret
   kept only as an off-by-default projection option. **Agreed
   2026-09-02.**
7. The caret model as built in M2 (§6, "as built"): a plain source offset
   with anchors as the legal offsets sharing a display position; movement
   keeps the context it came from; Home, End, and the document ends take
   the outermost anchor; fence lines hold no caret; a collapsed toggle
   inside a span unwraps it. **Decided in implementation 2026-09-03; open
   to revision from dogfood.**

## 17. What the earlier generations teach, and how v5 answers

Each row is a failure tied to code in the previous versions, and what v5
does about it. Three of them are not removed by architecture and are
listed as rules instead.

| Generation | Failure, tied to code | v5 answer |
| --- | --- | --- |
| v2 | The parse was late, an 80 ms debounce plus an isolate for anything over 4 KB, so Dart predicted structure; 1 in 11 keystrokes painted a frame the parse then refuted | Removed structurally: the parse is synchronous, there is nothing to predict |
| v2 | Guessing needed a second grammar in Dart, 1,687 lines of flanking and delimiter rules, plus a mapping layer that re-derived blocks the payload did not carry | Removed only if the render model carries every range a host needs. Rule: Dart never recognizes Markdown by scanning delimiters; canonical syntax emitted for an explicit command is accepted only when the candidate parse proves its meaning. A missing range is a parse-crate bug, not a Dart workaround |
| v2 | Marshal was two thirds of the keystroke: JSON over FFI, three object tiers, a document-length UTF-16 mapper per parse | Not removed by sync; made more urgent. Typed-data views, one object tier, UTF-16 from Rust, lazy per-block decode, and the end-to-end spike in §14 |
| v2 | One EditableText per block: identity guessing on every reparse, five echo-resync reasons, a thousand-line fence policy | Removed structurally: one render surface, one input client, source-offset selection, no echoes to reconcile |
| v2 | The suite settled before asserting, so it never saw the intermediate frame, and budgets were four to fifteen frames | Not removed by architecture. Rule in §11: no settle before the assertion, budgets are single frames |
| v2 | A position paper declared the architecture fit for purpose eight weeks before measurement refuted it | Rule: no claim without a receipt naming commit, device, and number |
| v3 | The engine was strong and the integration layer could not be driven; a jank harness never obtained one frame; four silent-stop states | Vertical slice through paint by M3, fault containment as typed errors, never a silent stop |
| v3 | SelectionArea and EditableText could not express source-offset selection over virtualized content | The v4 surface already solved this; v5 keeps it |
| v3 | Claims were conflated: structural admission reported as exact conformance | Denominators stay separate: conformance through the render model, direct kernel scenarios, and receipts each report their own count |
| v4 | Never-stale over an async engine required certification, envelopes, and counterfactual plans; roughly 100k lines | Removed structurally: never-stale is a property of the sync chain |
| v4 | Scope drifted from editor to large-document engine to streaming runtime | Rule: no concept without a direct case, a conformance case, or a named consumer, and the consumers are Dune and Fleury |
| v4 | Real IME, CJK, swipe typing, handles, and the magnifier were never exercised; native canaries stayed unrun at the stop | Device session in M3, and mobile handles named as unbudgeted work in §7 rather than assumed |

Two v2 techniques are adopted as concepts, not just code. The reference
definition scanner's discipline of deriving a fact textually and then
validating it against comrak's own output becomes the rule for every
derived range in §5. And the delta adapter's insistence that every
platform route reduce to one logical command survives as rule 1 of the
edit profile.

## Appendix A: asynchronous live rendering is outside V5

V5 uses explicit writable source mode beyond its qualified synchronous tier.
An asynchronous live-rendered tier would weaken the never-stale promise and
reintroduce range mapping, result adoption, and another visible representation.
It therefore requires a named consumer and a separate RFC; document size alone
must never enable it.

[RFC 031](rfc_031_code_color_worker.md) specifies a separate, optional code-color
worker. The Flutter workbench implements it for qualification: only snippet
colors may arrive later; Markdown structure, source, indentation, caret and
history remain synchronous. This does not change source-mode admission or
qualify asynchronous Markdown rendering.
