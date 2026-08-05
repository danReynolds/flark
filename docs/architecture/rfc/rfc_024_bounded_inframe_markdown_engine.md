# RFC 024: Bounded in-frame live Markdown engine

**Status:** DRAFT — design proposed, gates not yet run, 2026-08-05.
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
  paint last-certified structure meanwhile.

No isolate, no Worker, no wire protocol, no publication handshake, no
independent host store. Cold open spreads across frames, viewport first.

This is CodeMirror's scheduling model with an exact parser underneath. RFC 023's
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

### 4.4 Degradation is cosmetic

Above budget, the pre-parse window shows **last-certified structure mapped
forward** — never a guess, never a block type the last authoritative parse did
not produce. CodeMirror's fallback is invisible (uncoloured text); ours must be
too. This is a tested invariant, not a principle.

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

## 6. Gates

No build commitment until these run. Each is bounded and decisive.

| # | Gate | Cost | Decides |
| --- | --- | --- | --- |
| G1 | IME device matrix on v2 (existing 9-row protocol) | 1 day | Baseline for "IME correct"; unblocks v2 Phase 4; acceptance suite for any v4 input prototype |
| G2 | Jank harness on real phones — sustained typing at 5 KB/25 KB/100 KB/1 MB, p99 frame + dropped frames | 3 days | Whether the §2 contract is reachable at all, and where v2 actually breaks |
| G3 | In-frame sync spike — drive the existing endpoint from a frame callback with a budget | 3 days | §4.1. Cheap because the FFI is already synchronous |
| G4 | Input-surface UX suite (see §7) against an editable-based prototype | 2 weeks | The largest work item in the plan |
| G5 | Reference cold-open fix — indexed winner lookup, re-measure 100k-reference | 1 week | Whether "minimal open time" survives adversarial documents |
| G6 | Inline contiguity design pass | 3 days | Whether full conformance is reachable on this representation |

G1, G2 and G5 are worth running **whichever architecture wins** — no-regret.

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

## 8. Open decisions

**D1 — v2 while we build.** Recommendation: **fix it**. Enforce never-guess,
land Phase 2/4, repair the block-range regression this branch introduced. A few
weeks. Rationale: shipping something correct preserves the feedback loop that a
multi-year rebuild otherwise loses. Alternative is to freeze v2 and accept a
known-wrong shipped editor.

**D2 — conformance commitment.** Do we promise 100% CommonMark + GFM, or a
stated subset? The tail is inline raw HTML (37 examples, unimplemented) and the
inline-leaf contiguity limit (22 examples, architectural). Decide deliberately
in this RFC; do not discover it later.

**D3 — what survives from v3.** Proposed: **keep** the parser core, source rope,
packed green representation, reference index, conformance corpus, fuel/abort
machinery. **Retire** the endpoint protocol, publication path, independent host
store, wire codecs, and the `EditableText` island adapter. Confirm before the
retired surface accrues more work.

## 9. Stop conditions

Reopen this design if: G2 shows the contract unreachable on a floor device at
p90 document sizes; G4 fails and own-painting proves larger than the whole
remaining plan; G6 shows full conformance unreachable *and* the product requires
it; or in-frame scheduling cannot hold the budget without an isolate, in which
case §4.1 reverts and the wire protocol comes back.

## 10. Next steps

1. Decide D1, D2, D3.
2. Run G1 and G2 (no-regret, ~4 days).
3. Run G3 (~3 days) — the core new claim.
4. Run G5 and G6 in parallel with G4.
5. Rewrite §4 with measured answers; promote this RFC to SELECTED or revise.
