# RFC 025: v4 design review brief

> **Historical review record.** The review has now produced an accepted
> architecture: [RFC 026](rfc_026_flark_v4_product_architecture.md). The
> [v4 build plan](../v4/build_plan.md) is the current execution contract. This
> brief remains useful for its measurements and challenges; its open-decision,
> package-boundary, platform-scope, and milestone language is non-normative.

**Purpose:** external review. Written for a principal engineer with no prior
context on this project. Consolidates RFC 022 (grammar monopoly), RFC 023
(incremental engine) and RFC 024 (the current design) into one self-contained
account of what was tried, what was measured, what was decided, and what we are
least sure about.

**Status:** design decided, build not started. **Round 1 of external review
complete — amendments accepted and folded in below.** 2026-08-06.

**How to review this:** §7 lists our open questions ranked by how much damage a
wrong answer does. Everything in §3–§5 is measured unless explicitly marked
otherwise.

### Round 1 review outcome (accepted in full)

Four material corrections, all adopted. Recorded here so a second reviewer sees
what has already been challenged.

1. **"Stale but never wrong" was not a correctness model.** We said we would
   show "last-certified structure mapped forward". Counterexample: `*hello*`,
   delete the closing `*`. Mapping the emphasis node forward keeps hiding the
   opening marker and styling the text — contradicting what the parser is about
   to say. *An untouched source range does not prove semantic validity under a
   new revision.* Replaced with current-revision certification (§5.4).
2. **Our Q1 framing could have recreated the v2 failure.** Asking "does the
   incremental parser earn its place at all?" invites an open-ended
   re-litigation whose natural endings — a size threshold, a runtime strategy
   switch, a fallback — all mean **two implementations of Markdown semantics**,
   which is exactly what killed v2. Reframed in §7 with the burden of proof
   corrected.
3. **Fuel-bounding the parser is not bounding the frame.** Rope insertion,
   rebalancing, reference-index updates, allocation and reclamation, query
   construction, UTF conversion and result destruction were all unbounded on the
   interactive path.
4. **Deferring all floor-device evidence to the last milestone repeats this
   program's original mistake.** Device measurement and a physical Android/iOS
   input slice moved into M0.

Also accepted: "exact parser" language qualified until conformance is actually
reached; our "M2 may be weeks rather than months" estimate withdrawn as
unsupported; "nothing document-sized in Dart" narrowed to the latency-critical
path, with queries required to be *batched* under explicit caps.

---

## 1. The product

A Flutter package providing a live Markdown editor — you see formatted text and
edit it directly, with syntax markers hidden except where the caret is. Think
Obsidian's Live Preview, as a Flutter widget.

**One hard constraint shapes everything: the Markdown source string is the
document truth.** There is no private rich-text model. The bytes on disk are
never rewritten, reordered or normalised behind the user's back. Editors that
keep a structured model and treat Markdown as import/export — ProseMirror,
Lexical, Tiptap, Slate — demonstrably damage files on round-trip; their own
issue trackers document lost frontmatter, spurious escapes, and destroyed list
structure. For git-tracked `.md` that is disqualifying.

**The target contract:**

| Property | Target |
| --- | --- |
| Document size | 1 MB (~160,000 words), derived from measured corpus p99s of 24–71 KB with headroom |
| First meaningful paint | < 200 ms at 1 MB |
| Typing | p99 frame < 8 ms on a mid-range phone (120 Hz is now common) |
| Correctness | Nothing on screen may contradict what the parser will say |
| Conformance | Full CommonMark + GFM, long-tail commitment |

The guarantee is deliberately narrower than "never stale", because exact
Markdown has non-local effects — typing ` ``` ` can reinterpret the rest of the
document, so no exact parser can bound worst-case work:

> **The foreground never blocks. Current source text is always visible.
> Semantic formatting is applied only where certified for the current revision;
> everything else renders source-faithfully until it is.**

*(Round 1 revision. The earlier wording — "structure may be briefly stale, never
wrong" — was too weak: mapping old structure forward does not make it true. See
§5.4.)*

---

## 2. Why this is hard

A live Markdown editor must show formatted text the instant a key is pressed.
But knowing what text *means* — is this `*` emphasis or a literal asterisk? does
this line open a fence? — requires parsing, and parsing takes time.

So there is a gap between keystroke and truth. **The entire design space is what
you put on screen during that gap.** The field has three answers:

1. **Never parse.** Keep a structured model; Markdown is I/O only. (ProseMirror,
   Lexical, Tiptap, Slate, and in Flutter `super_editor`, `flutter_quill`.)
   Ruled out by our source-fidelity constraint.
2. **Show stale-but-true, or partial.** Incremental parse, bounded in-frame
   slice, publish an admittedly incomplete tree. (CodeMirror/Obsidian, VS Code,
   Zed.)
3. **Guess, then reconcile.** Predict the structure locally, correct it when the
   real parse lands.

**v2 chose (3). As far as our survey found, nobody else does.**

---

## 3. v2 — what we built, and how it failed

**Design:** whole-document parse via Comrak (Rust, over FFI/Wasm) on a debounce
after each keystroke. Because that answer is delayed ~80 ms, a Dart-side
"projection" layer predicts the structure locally so the screen can update
immediately.

**The failure mode:** the prediction and the parse disagree. Our own RFC 022 §1
states it plainly:

> every correctness bug shipped to that point clustered at one seam: places
> where Dart code re-derived markdown structure from source characters instead
> of consuming what the comrak parser already knew, or guessed structure the
> parser would later contradict.

Prediction is in v2's *public API* — `FlarkProjectionPrediction`,
`FlarkRenderPlanFidelity {authoritative, predicted, stale}`.

**The remediation ladder**, each rung added because the one below could not be
trusted: markers flashed → a caret-local re-hiding pass that "lied outright in
edge cases" → **a second CommonMark implementation written in Dart** (1,687
lines of flanking rules and delimiter placement) so Dart could pre-validate its
own writes → a parser-as-judge to veto commands → a debug counter for the cases
the judge cannot cover. Roughly 4,100 lines of v2 exist only because the
authoritative parse is delayed. v2 contains 35 regexes over Markdown syntax.

**Measured, on clean `main`, 2026-08-05:**

- **The dangerous tier is clean.** Zero contract violations across ~21,000
  corpus keystrokes. The judge closed the class where Dart writes Markdown the
  parser refutes.
- **The display tier is not.** 1 in 11 corpus keystrokes and ~1 in 9
  real-document keystrokes paint a structure the parse then refutes; >90% hide
  text that should be visible; spread across 44 of 56 grammar sections.
- **The suspected cause was wrong.** RFC 022 assumed fence topology. Fences are
  ~26 cases. The actual drivers are inline code spans, emphasis flanking,
  links/autolinks and reference definitions.

**Scale limits, measured (full parse + projection + render plan, workstation):**

| Document | Markdown-dense | Plain prose |
| --- | --- | --- |
| 1 KB | 3.1 ms | 0.06 ms |
| 25 KB | 8.5 ms | 2.0 ms |
| 50 KB | 22.1 ms | 3.1 ms |
| 100 KB | 39.0 ms | 9.6 ms |

Content matters by ~8×; markdown-dense text is far more expensive per byte than
prose. Every per-keystroke cost in v2 — parse, marshal, derived state, widget
build, layout, paint — is O(document), on the UI thread.

**Crucially, ~68% of v2's cost is not parsing.** At 1 MB: decode 243 ms +
result mapping 406 ms of 952 ms total. The Dart-side marshal dominates.

---

## 4. v3 — what we built, and what happened

**Design:** an incremental Rust parser owning the document, in a background
isolate (native) or Web Worker; revisioned publication over a wire protocol to a
Dart host store; a Flutter adapter mounting exactly **one** `EditableText`
"input island" over otherwise-painted blocks, with `SelectionArea` for
document-wide selection.

**What worked — and it is genuinely strong:**

- Bounded work per edit: **11 µs and ~63 bytes reparsed** for a single-character
  edit in a 1 MB document.
- 481 of 652 CommonMark examples render byte-exact.
- **It solves global reference-definition resolution incrementally** — the case
  `@lezer/markdown` explicitly abandoned ("it doesn't validate link
  references") to obtain incrementality.

**What failed:**

- **The integration layer could not be driven.** A jank harness could not obtain
  a single frame timing: markdown-dense 5 KB faulted; dense 25 KB threw an
  *uncaught* out-of-authority exception from the routine viewport path and
  killed the app.
- **A bisect then cleared the engine.** Driving the runtime directly — pure
  Dart, no Flutter — over every construct at every failing size: **22/22 pass**,
  full mixture at 25 KB in 32 ms. The fault is entirely in the Flutter layer.
- **`SelectionArea` is unusable here.** `EditableText` does not participate in
  Flutter's selection protocol; `SelectableRegion` cannot select content that
  isn't built (fatal under virtualization); and its output is rendered text, not
  source offsets.
- **Four distinct silent-stop states**, plus one status code (`0x0111`) standing
  for at least four unrelated faults.

**A correction to our own record, which we ask reviewers to hold us to:** we do
**not** have "exact incremental CommonMark". 481/652 is structural admission
plus semantic replay. The *incremental* path covers paragraphs, blanks, code
blocks, headings, thematic breaks and depth-one tight lists — block quotes,
nested and loose lists, and tables fail closed. We conflated these in our own
summaries for weeks.

---

## 5. v4 — what we resolved to build

**Keep** the Rust engine core (grammar, source rope, packed green tree,
reference occurrence index, fuel machinery). **Delete** the endpoint protocol,
wire codecs, publication path, host store, the `EditableText` island and every
`SelectionArea` use, and v2's projection prediction and scanners. **Build** a
lean FFI, an in-frame pump, and an own-painted surface.

### 5.1 One in-process, fuel-bounded engine — no isolate

Dart calls the parser synchronously from a frame callback with a work budget.
Completes in budget → adopt in the same frame, zero staleness. Exhausts budget →
resumable state, continue next frame, paint last-certified structure meanwhile.

Rationale: the FFI was *already* synchronous and fuel-bounded — the isolate only
ran the poll loop. Removing it deletes the wire protocol, the publication
handshake and the host store, which is ~30k of ~137k production Rust lines. This
is CodeMirror's scheduling model (Haverbeke keeps parsing on the main thread
deliberately) with an exact parser underneath.

**Measured (G3):** with a 4 ms budget, **113 of 120 single-character edits reach
exact structure in one pump**; none exceeds two. Sustained typing p99 3.53 ms,
max 3.73 ms. Unbudgeted, p99 is 18.8 ms and max 23.8 ms — **the budget is
load-bearing, not decoration.** It converts the tail into a second frame instead
of a dropped one.

Also measured: the existing wire protocol costs **62 poll round-trips and 2.9 MB
encoded per 1 KB document** — which is why the lean FFI is M0 work, not an
optimisation.

### 5.2 Bounded queries — nothing document-sized in Dart

Dart never materialises the document; it asks bounded questions. This removes
the 68% marshal term that dominates v2 (§3).

### 5.3 Own-painted surface with model-range selection

No `EditableText`. One document-level `DeltaTextInputClient`; we paint text,
caret and selection; we own hit-testing. Selection is `Position(block, offset)`
— never a widget query. Anchors become logical at pan-start so the anchor block
may be destroyed by scrolling.

**This was decided by a bake-off, not preference.** Both variants were built
over one shared selection layer against an identical suite. **50/50 tests pass;
both clear all eight acceptance cases**, including composition with a live
cross-block selection during scroll.

The editable island lost on four independent axes:

1. **Its premise is false.** It exists to get "IME, autocorrect, accessibility
   and platform services for free". Programmatic diff of the two Actions maps:
   `only in B: []` — **zero intents come free**. Pointer selection is
   hand-written in both. Accessibility is one block of 400 under virtualization.
   The selection toolbar and magnifier are lost *to the island specifically*
   (their handlers are direct `EditableTextState` calls with no Intent in the
   path) while the own-painted variant can use the same public widgets.
2. **Two intents cannot be intercepted at all.** `ReplaceTextIntent` and
   `UpdateSelectionIntent` reach the controller regardless of the Actions
   override — proven in both directions. **The island therefore cannot guarantee
   source authority.** Writes bypass it with no way to stop them.
3. **It cannot close soft-keyboard backspace at block start.** The island hands
   the platform one block with the caret at buffer offset 0, so the deletion is
   *unreportable*. On a phone that is the only backspace there is.
4. **It cannot live inside the virtualized list** — mutation-proved, failing
   with `hasAnyClients == false`. It needs a hand-positioned overlay plus a
   full-document layout oracle.

Cost: the own-painted variant is ~217 lines more, and *faster* — select-all +
copy over 1.1 MB / 34,000 blocks is 43.7 ms vs **19.9 ms**.

This also matches the field: **zero of four Flutter editors use `EditableText`**;
`super_editor` and `appflowy_editor` both own their text layer.

### 5.4 Required invariants

- **No silent stops.** Every terminal or quiescent state carries a discriminated
  reason that reaches the embedder. Four instances of this class have already
  been found; it is a design requirement, not a bug to fix a fifth time.
- **Current-revision certification** *(revised in round 1; this replaced
  "last-certified structure mapped forward", which was unsound — see the
  `*hello*` counterexample at the top)*. Semantic formatting and syntax hiding
  apply only to structure certified for the **current** revision. The engine
  distinguishes proven-reusable, invalidated, dependencies-not-yet-evaluated,
  and newly-certified. Uncertified ranges render source-faithfully — raw syntax
  may briefly appear, which is correct and strictly better than painting a
  structure the parser will reject. Measured one-to-two-pump convergence should
  keep it rare and brief. Proposed metric: *uncertified visible character-frames
  per edit*.
- **Bounded synchronous execution, not merely bounded parsing.** No operation
  reachable from the interactive frame path may perform document-proportional
  non-yielding work — including rope insertion and rebalancing, reference-index
  updates, allocation and reclamation, query construction, UTF conversion and
  result destruction. A large paste uses the *same* parser and source state,
  admitted resumably across pumps. The 4 ms budget is a *share* of the 8 ms
  frame, not the allowance.
- **Batched bounded queries.** Thousands of individually-bounded FFI calls still
  destroy frame time. Every viewport request carries caps on blocks, source
  bytes, render runs, mapping entries and total result bytes, targeting a small
  fixed number of native calls per frame.

---

## 6. What we are NOT claiming

We ran a deliberate attempt to find a larger thesis and **falsified three of
four claims**. Recorded so reviewers do not have to re-derive it:

- **"Best substrate for agent-edited Markdown"** — dead. `content.replace(old,
  new)` is already byte-exact; incrementality buys nothing at agent edit
  frequency. Cursor *chose* whole-file rewrite for model-side reasons. Notion
  moved agents off its own block model onto Markdown find-and-replace.
- **"Streaming Markdown is unsolved"** — dead. Six-plus incremental streaming
  parsers exist; one already solves the reference-definition-during-streaming
  case we would have claimed.
- **"Collaborative Markdown over source is novel"** — dead. It is the
  mainstream choice (Peerdraft, Relay, HedgeDoc, Ink & Switch).
- **"Ship the engine independently"** — dead economically. See question 1.

What survived is narrow: keystroke-frequency concurrency with source as truth
(genuinely unoccupied — tools either use a rich-text CRDT or take a lock), the
giant-unstable-tail-block streaming case, editability during a stream, and
**Flutter, where none of this exists.**

---

## 7. Open questions — please attack these

### Q0. Can current-revision certification actually deliver the guarantee? *(highest correctness risk)*

Round 1 replaced "stale but never wrong" with: semantic rendering only where
certified for the current revision, source-faithful neutral presentation
elsewhere. That needs a formal reuse/invalidation/fallback model distinguishing
structure proven reusable, structure invalidated, structure whose dependencies
are not yet re-evaluated, and newly certified structure.

*What we want from you:* is that model sufficient? What does it cost visually in
practice — how often, and how visibly, does a real editing session show raw
syntax? We propose measuring **uncertified visible character-frames per edit**.
Is there a better metric?

### Q1. Is there decisive evidence to replace the incremental engine — not "does it earn its place"?

**v4 ships exactly one parsing strategy.** The incumbent is the incremental
engine, because it already demonstrates bounded edit work (11 µs, ~63 bytes
reparsed at 1 MB), resumability, incremental reference resolution, and one-pump
convergence for 113 of 120 edits. Whole-reparse is a *plausible simplification
based on extrapolated throughput*, not a proven replacement.

So it gets **one disposable challenge** on a throwaway branch, before grammar
work — no parser abstraction, no runtime selection, no size threshold, no
fallback, no second conformance suite. **The loser is deleted.**

The comparison measures the whole chain on the floor device: apply edit → parse
complete source → build tree and reference state → determine viewport changes →
answer bounded queries → allocations and retained memory → layout and paint.
Across prose, dense Markdown, giant paragraphs, reference-definition changes,
sustained typing, streaming append, large paste, and 1 MB.

Whole-reparse replaces the engine only if it wins **decisively on all three**:
performance margin within its share of the frame; substantially easier route to
full conformance; and enough machinery deleted to materially cut maintenance.
Merely matching at 24–71 KB while keeping the same tree, invalidation,
reference, scheduling and query machinery is **not** winning.

*Context:* third-party benchmarks put exact whole-file parsing at ~275 MiB/s
(~0.25 ms at 71 KB, ~3.6 ms at 1 MB). But our own v2 measured ~6 ms of "parse"
for 25 KB dense — ~65× off — and that figure includes FFI marshal and result
mapping. **We do not currently know our own pure parse cost.**

*What we want from you:* are those three win conditions the right ones, and is
the disposable-challenge structure sufficient to stop this becoming two
architectures?

### Q2. Is "no isolate" right, or are we trading robustness for simplicity?

We remove the background isolate because the FFI is already synchronous and
fuel-bounded, and because the boundary costs 62 round-trips per keystroke.
Haverbeke made the same call for CodeMirror deliberately. But VS Code took ~6
months of race bugs moving tokenization *to* a worker, and Zed uses a
purpose-built copy-on-write structure to make off-thread parsing clean.

*Are we right that in-process is simpler and sufficient, or is a single
unbounded-latency edge case (a pathological 1 MB paste) going to force the
isolate back and cost us the simplification anyway?*

### Q2b. What exact text and coordinate model connects source to the platform IME?

We claim both "Dart never materialises the document" and "one document-level
`DeltaTextInputClient`". Those are only compatible if the platform sees a
**bounded window**. Our prototype built one (blocks the selection touches,
`\n\n` separator, a two-character invisible prefix so backspace at offset 0 is
reportable, capped at 2048 UTF-16 units) but it is prototype-grade and
unspecified.

Separately, `Position(block, offset)` is underspecified in both halves: `block`
must not be a parser node (editing a fence can replace block structure while the
user's position stays meaningful — we now propose stable source anchors plus
affinity), and `offset` must not be an untyped integer across source bytes,
UTF-16 code units, grapheme boundaries, glyph clusters and visual/bidi
positions.

*What we want from you:* what breaks first in a bounded input window — window
movement during composition, autocorrect outside the immediate word, selection
larger than the window, resynchronisation after connection loss?

### Q3. Own-painted text is a six-year project at `super_editor`. Are we underestimating it?

We chose it on measurement, and the suite passes — but that suite drives a
*simulated* IME. Real Gboard, CJK composition, swipe-typing and predictive bars
are untested, and we now own the entire platform connection. Touch selection
handles and the magnifier are built by neither variant.

*How badly do teams typically underestimate this? What breaks first?*

### Q4. Is the 1 MB ceiling the right commitment?

Derived from corpus p99s with headroom, and it beats every rich-text peer
(Slate ~1,000 blocks; `super_editor` ~15,000 words; Lexical ~1 s/keystroke at
500 KB). But we have **zero floor-device measurements** — every number here is
workstation-class. A mid-range phone could force the ceiling down.

*Is a stated ceiling even the right way to express this, versus a graceful
degradation curve?*

### Q5. Are we wrong to keep source-as-truth?

It rules out the approach every dominant WYSIWYG editor uses, and it is the
reason we cannot simply adopt ProseMirror or Lexical. Our justification is
round-trip fidelity for git-tracked files, and the field splits cleanly —
dominant *rich-text* editors are model-as-truth, dominant *Markdown* editors
(Obsidian, VS Code, GitHub, iA Writer, Typora, Bear) are source-as-truth.

*Is fidelity worth the cost we are paying for it?*

---

## 8. Plan and status

Build plan: `docs/architecture/v4/build_plan.md`.

- **M0 — walking skeleton.** Lean FFI, in-frame pump, own-painted surface on the
  real engine. Exists because v3's integration failure would have been caught
  here. Done when typing is correct and the jank harness is green on desktop.
- **M1 — the surface.** Acceptance suite against the engine, virtualization,
  clipboard, undo, block split/merge, marker-free rendering.
- **M2 — grammar.** Q1 is the first task. May be weeks rather than months.
- **M3 — platform.** Touch handles, magnifier, IME device matrix, accessibility,
  five platforms. *Where timelines historically go.*
- **M4 — scale.** Viewport-first cold open, indexed reference lookup, the 1 MB
  contract verified on a phone.

**Known open bugs:** a 32 KB paste goes quiescent without converging (source
intact, no error surfaced); over-window lines beginning with a marker character
fault; lazy continuation into an open list item faults.

**Evidence:** all measurements above are reproducible from harnesses in the
repo — `example/lib/g2_jank_harness.dart`, `g2_dense_bisect.dart`,
`g3_headless_probe.dart`, `example/lib/g4/`. Raw logs in
`docs/architecture/v4/`.
