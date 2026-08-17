# Scale-alignment audit — 2026-08-17

**Status:** review artifact, uncommitted. Authored by the architecture review
at Dan's request after the full-Green falsification. Purpose: (1) formalize
why the whole-document Green direction was knowable-in-advance, (2) audit the
current architecture and remaining plan against the product goal, and (3)
propose execution rules that prevent recurrence. Adopt into the build plan /
RFC 029 or discard; this file is not itself a controlling document.

**The goal being audited against** (RFC 029 §2): fast open, foreground work
proportional to the input event and visible window, revision work proportional
to the affected region, honest typed behavior for pathological inputs — at
every declared envelope size, on the slowest qualified device.

## 1. The failure class, named

The full-Green miss was an instance of one class: **an implicit
document-proportional prerequisite on a foreground path**. The class has three
disguises, all of which have now appeared in this program's history:

- a document-sized *readiness gate* (the whole-document Green root before
  first paint; falsified 2026-08-16);
- a document-wide *invalidation scope* (whole-document PENDING per keystroke;
  caught at M0, 2026-08-06);
- a document-sized *transfer or copy* (the Dart open path committing complete
  source before the first query; named 2026-08-16, fix declared in RFC 029).

Rule 4 ("bound everything synchronous") did not catch the first instance
because it bounds each pump, and a bounded pump does not bound total work —
RFC 029 §2 now states this explicitly. The gap was procedural: no rule
required declaring the *total work class* of an operation, as a function of
document size, before building it.

## 2. The paper test that was skipped

The full-Green readiness cost was computable before the durable layer was
built:

> ordinary Markdown ≈ 0.25 Green events/byte (2.47 M events / 10 MiB, and any
> early fixture would have shown the same order) × per-event durable cost on
> the order of 1 µs (any authenticated-write microbenchmark) × Pixel factor
> 3–20× ⇒ tens of seconds at 10 MiB on the qualified device.

Twenty seconds of arithmetic against the frozen 10 MiB floor refutes the
design; the program instead learned it from a 93-second device receipt after
the layer was built. The empirical surprise (the 5.6× authentication
separation) affected only the *magnitude*, not the conclusion — even the
optimistic constant fails the mobile budget on paper.

**The generalized test:** for every operation on a foreground or
readiness path, write down `units-of-work(n, shape) × unit-cost-on-slowest-
device` at the envelope, before implementation. If the product exceeds the
frame or gate budget, the design is wrong regardless of implementation
quality.

## 3. Alignment ledger

Work-class audit of everything that exists or is planned. "Proven" means a
receipt exists; "declared" means the contract/gate exists but no receipt;
"gap" means neither.

| # | Operation | Work class (target) | Bounded by | Falsifying gate | Status |
|---|---|---|---|---|---|
| 1 | `openUtf8Stream` admission | O(page) per grant; rope share O(log n) | 8 Ki UTF-16 seed pages; append-publish roots | §10.1 512 KiB-before-viewport, ≤2 MiB ingress, no derived copy | **Proven (engine)** — A1 receipts, 575–1165 MiB/s, `seal_reused_root` |
| 2 | `open(String)` convenience | O(chunk) per grant after RFC 029; **today O(n)** | lazy bounded encoding (declared) | same §10.1 caps measured on public API | **Declared** — A3 must prove |
| 3 | Seal / promotion | O(1) | root move, no rebuild | A1 "promotion copies nothing" | **Proven (engine)** |
| 4 | First certified slice | O(slice): ≤32 rows / 64 KiB / 8192 events | closed-rows-only + hazard defer | probe: flat across 1/10/40 MiB | **Proven (engine)** — 2,240 bytes, 0.33–0.52 ms; caveat: link-free fixture |
| 5 | Early certification proof | O(slice) hazard scan | conservative `[` scan | differential vs sealed oracle | **Proven**; needs GFM first-winner refinement before A3's "ordinary" fixture is honest |
| 6 | EOF compact indexing | O(n) **background only** | §7 grants + deadlines; never prerequisite for edits | probe 4 typing/scroll-during-index | **Declared** — compact-path probe 4 not yet run |
| 7 | Compact index storage | O(n) at ~0.006 % of source + reference state | ≤ ceil(n/4096)+2 restarts; 12 MiB component cap | envelope fixtures | **Proven at envelope** (0.612 MiB @10 MiB; 9.7 MiB refs @5 MiB stress) |
| 8 | Post-edit convergence | O(replay ≤64 KiB + tree height) | predecessor checkpoint + suffix splice | §10.3: no BOF restart, no size trend | **Gap** — Experiment B not built; see F2 |
| 9 | Reference-winner edits | O(labels touched) | label-scoped index + lazy fragment revalidation | §10.3 reference gate | **Declared** |
| 10 | Cold jump / cache miss | O(replay-to-closure) + typed fallback | closure caps, fallback families | §10.4 100 ms p99 / 200 ms max | **Partial** — ordinary 11.3 ms proven; nested 89.2 ms, see F1 |
| 11 | Fragment cache | O(cap) | 8 MiB hard cap, typed miss | memory gates | **Declared**, partially proven |
| 12 | Typing foreground | O(event + window) | 16 Ki input window, anchors ≤4096 | Tier B frame contract | **Proven on current path** (8.3 ms p50 flat across sizes/shapes); must re-prove over compact path (probe 4) |
| 13 | Selection / copy / export | O(near endpoints), async chunked bulk | source anchors + surrogate window | §8 contract | **Declared** — input-window receipts partially cover |
| 14 | Virtual viewport layout | O(visible tiles) | estimates + anchor preservation | §10.4 mobility | **Declared** — Experiment C |
| 15 | Accessibility traversal | O(near endpoints) *claimed* | §8 prose only | **none frozen** | **Gap** — see F3 |
| 16 | Whole-document queries (find, word count, outline, spellcheck scope) | absent from RFC | — | **none** | **Gap** — see F4 |
| 17 | Undo of large paste | O(edit size), not O(doc) | history + background recert | known 0.9 s residue, listed target | **Acceptable class** |
| 18 | Streaming retirement | O(unshared chunks) per drop | retirement budget + typed error | probe 4 supersession rules | **Declared**; wiring must drain per grant — logical-byte accounting overstates shared-root cost (pacing nit, not correctness) |
| 19 | Close / lifecycle | reaches zero | fuel-drained close | existing gates | **Proven pattern** (asserted in current suites) |

Summary: rows 1–5 (Experiment A engine half) are genuinely proven bounded;
rows 6, 8–14 are declared with falsifiable gates — the architecture is
*aligned on paper* and the gates would catch the failure class if it recurs;
rows 15–16 are the only places where document-proportional work could enter
without any gate noticing.

## 4. Flags (ranked)

**F1 — DECOMPOSED 2026-08-17: the cost is the slice query layer, not the
restart.** Instrumentation splits the depth-4 nested 84.6 ms as decode
0.03 ms + replay 2.25 ms + Green slice build 3.2 ms (restart machinery
exonerated at 5.5 ms) versus `locate_renderable_rows` 26.1 ms and 32×
`prepare_m11_recursive_green_slice_inline_leaf` 50.7 ms (~1.6 ms/row via
the per-point row-fence resolver); inline capture itself is 2.3 ms. The
optimization target is one shared ancestor-context walk per
materialization in `recursive_green` queries instead of a fresh fence
resolution per row — it pays on every deep viewport, not just cold jumps.
Remains open as a named engine optimization ahead of Pixel qualification
(projected ~250 ms on Pixel unoptimized vs the 100/200 ms gate).

**F2 — RESOLVED 2026-08-17: durable payloads are relocatable as stored.**
The falsification probe ran before any convergence machinery was designed
(build-plan receipt "Experiment B relocatability receipt"). Under a one-byte
BOF insertion at 10 MiB, both the ordinary cell (2,532 checkpoints) and the
nested cell (1,013 checkpoints, all inside open containers with writer
records) produced byte-identical parser and writer payloads, identical
stream lengths, shift-stable checkpoint selection, and uniformly shifted
manifest metadata. The feared absolute-block-start baking does not exist in
the payload bytes — absolutes are confined to the entry manifest.
Experiment B's storage work therefore narrows to the §5 measure-tree
manifest (O(log n) coordinate updates over ~2,500 entries per 10 MiB) plus
two follow-ups: the same probe for the compact reference index's absolute
source ranges, and a multi-byte insertion cell.

**F3 — Accessibility has prose but no gate.** §8 claims traversal loads only
geometry near active endpoints, but §10 freezes no gate for it. Platform
accessibility APIs (macOS AX, Android `AccessibilityNodeInfo`) will request
arbitrary ranges and full-text summaries; an unbounded reply is a hidden
O(n) foreground path that no current receipt would catch. Freeze a gate in
§10.4: an accessibility traversal step at the envelope performs bounded work
and never materializes complete document text.

**F4 — Whole-document query features are unplanned O(n) magnets.** Find-in-
document, word count, outline/TOC, spellcheck scoping, and export will be
demanded of a product editor; none appear in RFC 029. Each has a natural
bounded implementation over the existing substrate (streaming rope search,
page-summary heading index, visible-window text services), and each has an
obvious wrong implementation (`getText()` → String). Declare the feature
class now with one rule: whole-document queries are chunked background work
with bounded foreground slices, served from Core, never from a materialized
Dart String.

**F5 — Split-CRLF at an unsealed append frontier** (from the 2026-08-17
review): `is_physical_line_start` blesses a trailing bare `\r`
(source.rs:1544) and the line cursor emits a pending CR as a complete `Cr`
ending at window end (source.rs:2779). Correct for sealed source; at an
unsealed frontier a next chunk starting `\n` becomes a phantom blank line.
Fix in the frontier-admissibility rule before wiring
`adopt_progressive_opening_append` (currently zero callers).

**F6 — Gate hygiene:** `cargo test -p flark-parser` has not compiled under
any feature configuration since ≤ a210a12 (`block_core_incremental_adoption`
calls a `pub(crate)` method), invisible because `verify_v4.sh` tests only
runtime+abi and the conformance script targets ledgers individually. Fix the
test; add `cargo check --all-targets` for engine+parser to the everyday gate.

## 5. Proposed rules of execution (additions)

Proposed as amendments to build plan §2, in its style:

9. **Declare total work class before building.** A tranche that adds or
   reroutes any foreground or readiness operation states, in its build-plan
   entry *before implementation*, the operation's total work as a function of
   document size and shape, and names the frozen gate that falsifies it. A
   bounded pump is not a bounded total. "Background" is a claim about
   scheduling, not about total work; background totals still need a declared
   class and a starvation-safe gate.
10. **Arithmetic before receipts.** Any operation whose declared class is
    O(n) in any capacity dimension must show `units × unit-cost × slowest-
    device factor` at the envelope on paper, in the RFC or tranche entry,
    before the first implementation commit. A paper fail is a design
    rejection; a receipt is not required to kill it.
11. **Cheapest falsification first.** Within an experiment, the property
    most likely to invalidate the design is probed with a disposable
    diagnostic before dependent machinery is built (current instance: F2
    page-relocatability before the convergence engine).
12. **Every performance receipt carries the detector tier.** Claim-eligible
    receipts include the 4× size tier (and the declared hostile-density
    shapes where applicable) so hidden linear foreground work cannot pass by
    fixture selection. A receipt without its detector row is a narrative,
    not a receipt.
13. **New feature classes declare their work class at proposal time.** A
    product feature touching document content (search, export, accessibility
    summaries, text services) enters the RFC with its foreground work class
    and background contract stated, before any UI work.

## 6. Goal sharpening

Two clarifications that make "best in class" testable:

- **Make the leadership envelope a number.** RFC 029 defines it as "the
  largest same-hardware comparable peer result" but no peer has been
  measured. Run the standard fixtures once through 2–3 peers (e.g. Obsidian
  live preview, VS Code markdown preview, Typora) on the development Mac and
  record open time, typing responsiveness, and the size at which each
  degrades. That number pins the stretch tier, prevents over-engineering
  past it, and becomes the marketing claim's receipt.
- **State the non-goal explicitly.** The product floor is 10 MiB ordinary /
  5 MiB hostile density. Inputs beyond the envelope get typed, recoverable
  degradation — never silent jank. This is already RFC 029 §9 doctrine; it
  belongs in one sentence wherever the goal is stated, because "scalable"
  without a declared envelope is how unbounded prerequisites sneak back in.

## 7. Verdict

The architecture as selected is aligned with the goal: everything proven so
far is bounded by construction, everything planned has a falsifiable gate,
and the two audit gaps (F3, F4) are contract omissions, not design flaws.
The direction does not need correction; it needs the flags closed in order
and the five rules adopted so the next document-sized prerequisite dies on
paper instead of on a Pixel. Status as of 2026-08-17: F5 (CRLF frontier),
F6 (gate hygiene), F3/F4 (RFC amendments), and the rules are landed; F2 is
resolved favorably by the relocatability receipt; F1 (nested cold-jump
decomposition) remains open ahead of Pixel qualification.
