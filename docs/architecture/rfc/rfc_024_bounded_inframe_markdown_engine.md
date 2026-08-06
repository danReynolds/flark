# RFC 024: Bounded in-frame live Markdown engine

**Status:** DRAFT — design proposed; G4 DECIDED (own-painted), G3 partially run, G2 blocked-then-bisected,
G1 instruments ready. Input surface resolved 2026-08-06; see §6.1.1. Remaining
open risk is real-device IME, touch handles and the magnifier (G1). See §6.1 for what the gates have
returned so far.
**Supersedes:** the integration strategy of
[RFC 023](rfc_023_incremental_live_markdown_engine.md). RFC 023's *engine*
selection stands and its falsification record carries forward; what this RFC
replaces is how that engine reaches the screen.
**Relationship to [RFC 022](rfc_022_parser_grammar_monopoly.md):** RFC 022's
rule — *grammar belongs to the parser, everything else is geometry* — is the
invariant this design enforces structurally rather than by discipline.

## 1. The primary directive

> **Correct live Markdown editing with no jank, up to a stated document size.**

Two words carry the weight.

**Correct** — the Markdown source is the truth, byte-exact, always. Nothing on
screen may contradict what the parser will say. Scaling curves are not the
metric; a good asymptote that still drops frames is broken.

**No jank** — the frame budget is never blown while typing. Note the achievable
form of this guarantee, because exact Markdown has non-local effects (typing
` ``` ` can reinterpret the rest of the document, and no parser can bound
worst-case work while staying exact):

> **The foreground never blocks. Structure may be briefly stale — never wrong.**

## 2. The stated ceiling: 1 MB

Derived, not chosen. Measured p99 across real corpora: Obsidian's help vault
28.6 KB, github/docs 24.0 KB, vscode-docs 71.4 KB. Every real Markdown corpus
tops out well under 100 KB. 1 MB is ~160,000 words — a novel in one file — and
gives 15–40× headroom over every measured p99.

Peer ceilings for context: Slate advises under 1,000 blocks; `super_editor`
under 15,000 words; ProseMirror degrades at 50–75 pages; Lexical takes ~1 s per
keystroke at 500 KB. CodeMirror exceeds 1 MB comfortably but buys it by
degrading fidelity — `@lezer/markdown` does not validate link references.

**The contract:**

| Property | Target |
| --- | --- |
| First meaningful paint | < 200 ms at 1 MB |
| Fully current structure | < 500 ms at 1 MB |
| Typing, p99 frame time | < 8 ms on a mid-range phone (120 Hz is now normal) |
| Beyond 1 MB | degrades visibly and honestly — never silently, never janky |

## 3. What we learned that changed the design

Recorded so a later reader can tell new information from drift.

1. **v2's speculative projection is the bug source, and it is not historical.**
   Measured on clean `main`: 1 in 11 corpus keystrokes and ~1 in 9 real-document
   keystrokes paint a structure the parse then refutes. Over 90% hide text that
   should be visible. RFC 022 §1 already named this seam. **But fence topology
   is a small minority** — the drivers are inline code spans, emphasis flanking,
   links/autolinks and reference definitions. Phase 4 is mis-scoped if it
   targets fences.
2. **The asserted tier is clean** — zero contract violations over ~21,000
   corpus keystrokes. The judge closed the dangerous class; what remains is
   transient display divergence.
3. **~68% of v2's per-adoption cost is Dart-side marshal, not Rust parse**
   (1 MB: decode 243 ms + mapping 406 ms of 952 ms). Moving the parser
   off-thread relocates the minority. Bounded queries eliminate the majority.
4. **`SelectionArea` is unusable for this product, and it — not the input
   island — was the mistake.** `EditableText` does not participate in Flutter's
   selection protocol; `SelectableRegion` cannot select unbuilt content; and its
   output is rendered text, not source offsets. All three vanish under
   model-range selection.
5. **Cross-block selection over virtualized content is solved twice in Flutter**
   (`appflowy_editor`, `re_editor`): logical `Position(path, offset)` anchors
   taken at pan-start, visible-window hit-testing, type-registry selectables.
6. **The FFI is already synchronous and fuel-bounded.** The isolate runs the
   *poll loop* off the UI thread; it is not required by the call shape.
7. **v3's engine is materially better than its own records claimed** —
   481/652 byte-exact once harness gaps are closed, ahead of the JS reference
   implementation of this technique.

## 4. Design

### 4.1 One in-process, fuel-bounded engine

The document lives in Rust. Dart calls it synchronously from a frame callback
with a work budget. Two outcomes from one code path:

- **completes within budget** → adopt in the same frame. Zero staleness window.
- **exhausts budget** → returns resumable state; continue next frame, and
  meanwhile paint per §4.4 — certified structure where it is certified for
  *this* revision, source-faithful neutral presentation everywhere else.

No isolate, no Worker, no wire protocol, no publication handshake, no
independent host store. Cold open spreads across frames, viewport first.

This is CodeMirror's scheduling model with a single authoritative parser
underneath — one intended to converge to full CommonMark/GFM conformance, which
it has not yet reached (§6.1). RFC 023's
fuel machinery, built for the Worker's benefit, is precisely what makes the
Worker unnecessary.

*Deferred, not rejected:* a background isolate for cold-open parallelism on
native remains available as an optimisation once the in-frame path is proven.

### 4.2 Bounded queries, no document-sized Dart state

Dart never materialises the document. This is what removes the dominant cost
term (§3.3) and what makes the ceiling a cold-open question rather than a
typing question.

### 4.3 Virtualized view with model-range selection

Selection is `Position(path, offset)`, never a widget query. Anchors become
logical at pan-start so the anchor block may scroll away and be destroyed.
Hit-testing is restricted to the visible window. Commands resolve through a
type registry, so select-all and copy work across blocks that were never built.

### 4.4 Current-revision certification, not "mapped forward"

*Revised 2026-08-06 after external review. The previous formulation — "show
last-certified structure mapped forward" — was **wrong**, and the counterexample
is simple:*

```
*hello*     →  user deletes the closing *  →  *hello
```

*Mapping the emphasis node forward preserves its geometry, but the node is no
longer valid. The editor would keep hiding the opening marker and styling
`hello` as emphasis — directly contradicting what the parser will say. An
untouched source range does **not** prove semantic validity under a new
revision. The same propagates further through reference definitions, fenced
code, lazy continuations, HTML blocks, delimiter runs and link destinations.*

The correct model:

> **Current source text is visible immediately. Semantic formatting and syntax
> hiding are applied only to structures certified for the current source
> revision. Uncertified ranges use a source-faithful neutral presentation until
> certified.**

The engine must therefore distinguish four states, not two:

1. structure **proven reusable** for the new revision;
2. structure **invalidated** by the edit;
3. structure whose **dependencies have not yet been re-evaluated**;
4. **newly certified** structure.

For a node to retain semantic rendering, the engine must prove it remains valid
under the new revision — not merely that its bytes were untouched.

This means raw syntax may briefly appear in an invalidated region. That is
**correct behaviour**, and strictly preferable to hiding literal source or
painting a block type the parser will reject. Given measured one-to-two-pump
convergence it should be rare and short-lived.

**Product metric:** *uncertified visible character-frames per edit* — it
captures both how often and how visibly we degrade, which parser duration alone
does not.

## 5. Confidence, honestly

**High** — source-as-truth; the Rust incremental parser; incremental parsing as
an approach; document-in-Rust with bounded queries; the reference occurrence
index; virtualized rendering; model-range selection. Each is either built and
measured here or verified in shipping code elsewhere.

**Medium** — in-frame synchronous scheduling (approach validated by CodeMirror,
untested across this FFI); cosmetic degradation (declared, never enforced);
conformance tail.

**Low** — the input surface, entirely. Editable-vs-own-paint; document-level
IME; the editing surface (split/merge, clipboard, undo, commands); cold open on
pathological documents; **and all floor-device behaviour, where we have zero
data.**

*Revised 2026-08-06 after the gates.* The engine moves **up** — it survived
every construct at every size the integration layer died on (§6.1). In-frame
scheduling moves **up**, from medium to high-medium, on measured evidence.
The existing integration layer moves **down to disqualified**: it is not merely
suboptimal, it demonstrably cannot be driven.

**The input-surface question is now DECIDED (§6.1.1): own-painted.** It moves
from *low* to *high* for everything the suite covers — selection, clipboard,
block split/merge, undo routing, cross-block replacement, and composition with a
live selection during scroll, all passing against a virtualized list with
unbuilt blocks asserted.

What remains **low** is narrower and now precisely named: real-device IME
(Gboard, CJK, swipe, predictive bars) — B's largest exposure since B owns the
whole connection; **touch selection handles and the magnifier, which neither
variant built and which §7 requires**; accessibility semantics, built by
neither; and cold open on pathological documents. Floor-device behaviour is
still entirely unmeasured — G2 has never produced a frame timing.

## 6. Gates

No build commitment until these run. Each is bounded and decisive.

| # | Gate | Cost | Decides |
| --- | --- | --- | --- |
| G1 | IME device matrix, run against v2 as a *reference implementation* | 1 day | What Flutter's own text input handles correctly today — the behavioural baseline the new input surface must match, and the acceptance suite for G4 |
| G2 | Jank harness on real phones — sustained typing at 5 KB/25 KB/100 KB/1 MB, p99 frame + dropped frames, against the engine | 3 days | Whether the §2 contract is reachable at all. **The program has zero floor-device data of any kind** |
| G3 | In-frame sync spike — drive the existing endpoint from a frame callback with a budget | 3 days | §4.1, the core new claim. Cheap because the FFI is already synchronous and fuel-bounded |
| G4 | Input-surface UX suite (see §7) against an editable-based prototype | 2 weeks | The largest work item in the plan, and the lowest-confidence area |
| G5 | Reference cold-open fix — indexed winner lookup, re-measure 100k-reference (currently 71 s on Chrome) | 1 week | Whether "minimal open time" survives adversarial documents |
| G6 | Inline contiguity design pass | 3 days | *How* to reach the §8 D2 commitment on this representation, not whether to |

G2, G3 and G5 exercise the kept engine directly and are the critical path. G1
runs against v2 purely to extract behaviour, since v2 has no other role.

## 6.1 Gate results to date (2026-08-06)

Full detail in `docs/architecture/v4/`.

**G3 — in-frame sync pump: the core claim HOLDS at 1 KB; not passed overall.**
With a 4 ms budget, **113 of 120 single-character edits reach exact structure in
one pump**, none needs more than two, and sustained typing gives p99 3.53 ms /
max 3.73 ms — inside an 8 ms frame. Fuel-abort held budget on a 32 KB paste with
source byte-intact. **Budgeting is load-bearing, not decoration:** unbudgeted,
p99 is 18.8 ms and max 23.8 ms, which drops frames. The budget converts the tail
into a second frame instead of a dropped one.

Two things it exposed. The wire protocol costs **62 poll round-trips per
keystroke and 2.9 MB encoded for a 1 KB document** — measured support for §8 D3's
lean direct FFI, and the reason 100 KB and 1 MB could not be reached. And the
32 KB paste **never converged**: 100,000 pumps, `exact=false`, source intact, no
error ever surfaced. G3 does not pass until that is understood.

**G2 — jank harness: BLOCKED, and the blockage is the finding.** Zero of eight
configurations produced a frame timing. Markdown-dense 5 KB reached structure,
never painted, then faulted (`parserFailure: 4`); dense 25 KB threw an *uncaught*
out-of-authority range receipt from the routine viewport progress path and killed
the app.

**Then the bisect cleared the engine.** Driving `FlarkV3DocumentRuntime`
directly — pure Dart, no Flutter — over every construct the dense fixture uses,
alone and mixed: **22 of 22 pass**, including the full mixture at 25 KB in 32 ms.
So the fault is not the parser. It is one defect in the Flutter viewport layer,
which requests windows the host rejects as out-of-authority and thereby faults
the runtime.

**That result is the strongest evidence this RFC has:** it clears the component
§8 D3 keeps and convicts the component §8 D3 deletes. The engine handles
realistic Markdown at 25 KB in 32 ms; the `EditableText`-island integration
layer cannot survive being driven at 5 KB.

**Still unmeasured: everything about a real device.** G2 has never produced a
frame timing, so the §2 contract remains unverified on any hardware.

## 6.1.1 G4 — input surface: DECIDED. Own-painted (Variant B).

Both variants were built over one shared model-range selection layer and faced
the identical acceptance suite. **50/50 tests pass; both clear all eight §7
cases**, including case 8 (composition with a live cross-block selection during
scroll) which this RFC predicted would break. Verified independently by re-run.

**Variant B wins, and A's entire justification fails on measurement.**

*A's premise was "Flutter gives IME, autocorrect, accessibility and platform
text services for free". It does not.*

- **The Actions map is the same size.** Programmatic diff of the two maps:
  `only in B: []`. **Zero intents come free to A.**
- **Pointer selection is hand-written in both** (`rendererIgnoresPointer: true`).
- **Accessibility is one block out of 400** under virtualization — i.e. nothing.
- **The toolbar and magnifier are lost to A specifically**, because their
  handlers are direct calls on `EditableTextState` with no Intent in the path.
  B can use the same public widgets (`AdaptiveTextSelectionToolbar`,
  `TextMagnifier`) with its own commands. *The things A sacrifices are things B
  keeps.*

*Two intents cannot be intercepted at all.* `ReplaceTextIntent` and
`UpdateSelectionIntent` reach the controller regardless of the Actions override
(proven by test, both directions). **Variant A therefore cannot guarantee the
source-authority invariant** — there are writes that bypass it and no way to
stop them. That is the disqualifier: it is precisely the silent-correctness-bug
class of §6.2, unfixable by construction.

*B closes a hole A cannot, and it is not a corner case.* Soft-keyboard backspace
at block start. A hands the platform one block with the caret at buffer offset
0, so the deletion is **unreportable** — the IME has nothing to send. On a phone
that is the only backspace there is. A's own route out of it ends at
`DeltaTextInputClient`, i.e. at B's machinery. Proven differentially against
both variants in one file.

*The structural argument.* A's editable **cannot live inside the virtualized
list** — mutation-proved, failing with `hasAnyClients == false`. It must be a
hand-positioned overlay, which for variable block heights means owning a
full-document layout oracle that answers "where is block N" for blocks never
built. That is strictly more machinery than B needs.

*Cost:* B is ~217 lines more. Select-all + copy over 1.1 MB / 34,000 blocks:
**A 43.7 ms, B 19.9 ms** — B is also faster, because A pays to marshal through
the editable.

**Decision: own-painted text, one document-level `DeltaTextInputClient`, painted
caret and selection, own hit-testing.** This matches what `super_editor` and
`appflowy_editor` both do, and the earlier peer research found that *zero* of
four Flutter editors use `EditableText`.

**Contingent on G1.** The suite drives a faithful but simulated IME. Real Gboard,
CJK composition, swipe-typing and predictive bars are B's largest exposure
because B owns the whole connection. Touch handles and the magnifier are built by
*neither* variant and are §7 criteria — half the target platforms remain
untested on the single most user-visible interaction.

## 6.2 Required invariant: no silent stops

Four distinct silent-stop states have now been observed: a runtime that faults
with no reason surfaced (G2 D-A); an engine that goes quiescent while still not
current, with no error (G3 paste); a terminal fault visible only as the word
"closed" in a diagnostic tile; and a surface parked in `awaitingActivePresentation`
forever. Separately, status `0x0111` has stood for at least four unrelated
faults, which is why each needed its own investigation.

This is a missing invariant, not four bugs. *Strengthened 2026-08-06 after
external review — "no silent stops" is necessary but not mechanically
checkable:*

> **Every pump either completes, advances a monotonic progress token, or
> returns a typed reason identifying what external condition is required for
> further progress.** A repeated state with an unchanged progress token
> automatically becomes an explicit fault.

That version is enforceable by the runtime itself rather than by discipline.
Recovery may discard derived parser state and rebuild from the same Rust-owned
source using the same grammar, and may use neutral source-faithful rendering
meanwhile. Recovery must **never** discard source edits, stop silently, or leave
the editor unable to accept further input.

A discriminated status code replaces `0x0111`.

**Consequence:** the known 32 KB paste that goes quiescent without converging
violates this invariant directly. It is an **M0 blocker**, not backlog.

## 6.3 Bounded synchronous execution — not just bounded parsing

*Added 2026-08-06 after external review. Fuel-bounding the parser loop is not
the same as bounding the frame.* The following are all reachable from the
interactive path and none was bounded by the original design: applying a large
insertion to the rope; rope rebalancing; reference-index updates; allocation and
tree reclamation; viewport-query result construction; UTF conversion; large
result destruction; and any parser loop that accidentally fails to consume fuel.

> **No operation reachable from the interactive frame path may perform
> document-proportional non-yielding work.**

A large paste therefore uses the *same* parser and the *same* source state,
admitted and processed resumably across several pumps. That is not a second
implementation and must not become one.

**The 4 ms budget is a share, not the allowance.** Parser work, bounded queries,
shaping, layout and paint all draw on the same 8 ms frame. The scheduler should
consume the frame's *remaining* parser allowance rather than assuming 4 ms is
always available.

## 6.4 Bounded queries — narrowed, and batched

*Narrowed 2026-08-06.* "Nothing document-sized in Dart" is too absolute:
select-all, copy and export are document-sized by definition, and we measure the
1.1 MB copy case deliberately.

> **No document-sized materialization or transfer occurs on the latency-critical
> edit, parse, viewport-query, layout or paint path.** Explicit bulk operations
> use separate measured APIs and may stream.

Queries must also be **batched**. Thousands of individually-bounded FFI calls
are still fatal to frame time. Every viewport request carries caps on blocks,
source bytes, render runs, mapping entries and total result bytes, and the API
targets a small fixed number of native calls per frame.

## 7. The input-surface acceptance suite

G4 passes only if an editable-based focused block handles all of:

drag-select across blocks with autoscroll · anchor scrolling out of view and
being destroyed · `Cmd+A` then copy on a 1 MB document yielding complete exact
source · shift-click extension · double/triple-click · touch handles and
magnifier · typing to replace a cross-block selection · **IME composition while
a selection exists elsewhere, during scroll**.

The last is where it is most likely to break. If it does, own-painted text is
forced — and we will know precisely why, rather than choosing on taste.

Note the standing hazard either way: `EditableText` ships Actions (undo, cut,
paste) that write directly to its controller, bypassing source authority. This
was observed at runtime. Using an editable means owning that intercept surface
permanently.

## 8. Decisions (resolved 2026-08-05)

**D1 — v2 while we build: RESOLVED. Nothing.** v2 is pre-launch. There are no
users, no feedback loop to protect and no migration burden, so the case for
keeping it correct in parallel does not exist. **v2 is a harvest source and a
reference implementation, not a product.** No Phase 2/4 work, no never-guess
retrofit, no regression repair except where it blocks harvesting.

**D2 — conformance: RESOLVED. 100% CommonMark + GFM, long-tail, not urgent.**
Committed as the destination. Not a gate on anything else; the two architectural
items (inline raw HTML, inline-leaf contiguity) are scheduled work, not open
questions. G6 designs the contiguity fix rather than deciding whether to attempt
it.

**D3 — what survives: RESOLVED by fit, not lineage.** Nothing is kept because
of where it came from. Each component is judged only against §1. Applying that
test:

*Keep — because nothing better exists.* The Rust parser core, source rope,
packed green representation, reference occurrence index, and the fuel/abort
machinery. This is not sentiment: an **exact, incremental CommonMark engine does
not exist anywhere else**. Every alternative is either non-incremental (comrak,
pulldown, markdown-rs) or non-conformant (`@lezer/markdown`, tree-sitter). This
engine is 481/652 byte-exact with the hardest case — global reference-definition
resolution — already solved, which is precisely the case the JS reference
implementation abandoned. Rebuilding it would take years and land in the same
place.

*Delete — because it exists only to cross a boundary we are removing.* The
endpoint protocol, wire codecs, publication path, independent host store, and
the candidate/session machinery. Roughly 30k of the ~137k production Rust lines
are boundary infrastructure. **Removing the isolate slims the Rust as well as
the Dart**, and makes the remaining engine materially more reviewable — which
directly addresses RFC 023's own reopen condition about core size.

*Delete — because it is disproven.* The `EditableText` island adapter and every
use of `SelectionArea` (§3.4).

*Delete — because it is the bug source.* v2's projection prediction, its
reconciliation machinery, and all 35 Dart-side Markdown scanners.

*Harvest as knowledge, not code.* v2's command semantics, IME device protocol,
golden-test corpus and conformance fixtures. The behaviour is valuable; the
implementations sit on a whole-document model that does not fit §4.

*Build fresh.* The virtualized view, model-range selection, input surface, and
the lean synchronous FFI that replaces the wire protocol.

Net: this is **v3's engine with a new integration layer** — not v3 continued,
not v2 evolved, and not a from-scratch rewrite, because only one of the three
existing assets survives the fit test on its merits.

## 9. Stop conditions

Reopen this design if: G2 shows the contract unreachable on a floor device at
p90 document sizes; G4 fails and own-painting proves larger than the whole
remaining plan; G6 shows full conformance unreachable *and* the product requires
it; or in-frame scheduling cannot hold the budget without an isolate, in which
case §4.1 reverts and the wire protocol comes back.

## 10. Next steps

1. ~~Decide D1, D2, D3.~~ Resolved 2026-08-05 (§8).
2. **G2 and G3 first** — they exercise the kept engine and test the core new
   claim. ~6 days.
3. G1 in parallel (needs hands on devices), producing the G4 acceptance suite.
4. G5 and G6 alongside G4.
5. Rewrite §4 with measured answers; promote to SELECTED or revise.

Because v2 is pre-launch (§8 D1), there is no parallel maintenance track and no
migration plan. The only deliverable is the destination.
