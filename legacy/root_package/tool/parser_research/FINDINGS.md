# Live Markdown engine research findings

Status: historical, non-normative evidence record from temporary prototypes,
2026-07-13, with dated follow-up findings through 2026-07-18. **Its early donor
ranking and implementation sequence are chronology, not the current
recommendation.** Do not implement either the Pulldown-led composition or
bounded real-Comrak runtime service described in those historical sections.
The proposed architecture is now maintained in
[`RFC 023`](../../docs/architecture/rfc/rfc_023_incremental_live_markdown_engine.md);
its current evidence, acceptance state, and ownership rules are maintained in
[`ARCHITECTURE_PROOF_LEDGER.md`](ARCHITECTURE_PROOF_LEDGER.md),
[`ARCHITECTURAL_COHERENCE_AUDIT.md`](ARCHITECTURAL_COHERENCE_AUDIT.md),
[`DIRECT_PARSER_GREEN_COMPOSITION_GATE.md`](DIRECT_PARSER_GREEN_COMPOSITION_GATE.md),
[`PACKED_SERIALIZED_GREEN_GATE.md`](PACKED_SERIALIZED_GREEN_GATE.md),
[`ARCHITECTURE_STATE_PARTITION.md`](ARCHITECTURE_STATE_PARTITION.md),
[`DART_SOURCE_STATE_PARTITION.md`](DART_SOURCE_STATE_PARTITION.md), and
[`V3_PACKAGE_MIGRATION_BOUNDARY.md`](V3_PACKAGE_MIGRATION_BOUNDARY.md).

This file remains useful as the chronological evidence chain that led to the
current gates.
The follow-up pressure test updated the record for liveness, Dart/UI jank,
parser choice, and oversized layout. The subsequent
[Phase 0 feasibility receipts](PHASE0_FEASIBILITY.md) pressure-test input,
selection, semantics, and complex-script layout. None of the prototype data
structures are production implementations. Where this record sketches an
architecture, it describes the proposal at that point in the investigation;
later dated findings and RFC 023 supersede earlier “next step” language.

## Decision at the first measurement checkpoint (historical)

Large-document support changes the architecture from the first layer upward.
The right direction is a new internal editor engine that keeps byte-exact
Markdown as canonical source, but represents source, syntax, projection, and
layout as persistent indexed sequences and propagates explicit deltas through
all four layers.

This is a core rewrite, not a clean-room package rewrite. Keep the public API,
behavioral corpus, command semantics, Comrak/cmark-gfm conformance lineage,
IME knowledge, and reusable block widgets. Replace the global String,
whole-document bridge payload, prediction-owned grammar state, global
projection, and eager/whole-host editor surfaces.

The parser implementation direction is now a Flark-owned, donor-derived
persistent core. The pinned CommonMark/GFM/Flark profile is normative;
Comrak, Pulldown, and cmark/cmark-gfm are differential peers. Pulldown 0.13.4
has earned the lead for inline algorithms, not selection as donor for the whole
core: Flark owns resumable state, segmented input, source-backed facts,
reference symbols, budgets, persistent output, and deltas, while donor choice
remains per seam. The inline extraction, packed-state spike, and true
checkpoint-restart spike each pass their narrow mechanism question, but no
candidate composes them with real block/inline grammar or passes Gate A/B. The
next gate is one integrated production-shaped slice; broad grammar accumulation
and product integration remain on hold. The current evidence and stop criteria
are in
[`PARSER_DONOR_BAKEOFF.md`](PARSER_DONOR_BAKEOFF.md).

The liveness review also narrows the UI claim. A 10 MB document made of
ordinary bounded blocks is plausible with indexed source, worker parsing,
bounded adoption, virtualization, and one active input island. A single 10 MB
paragraph is a separate unresolved layout contract. Native needs one long-lived
parser isolate and web needs an explicit Web Worker; input/caret source state
must not wait for grammar work. Source, grammar/projection, and per-shard layout
revisions advance independently, with latest-wins worker coalescing and at most
one bounded adoption batch per frame. The existing layout probe uses a
post-hoc monolithic oracle and Flutter exposes no resumable paragraph state, so
neither a runtime checkpoint classifier nor a bounded plain/no-wrap fallback is
yet proved.

The extracted Comrak 0.54 patch is less complete than the headline implied.
Only 53 additions and five deletions touch existing files, but the 3,316-line
module remains coupled to private parser internals. Its 16 microsecond p95 over
10,000 external 1 MB edits is block-only; inline parsing is absent, and the
strong in-container list/table/fence receipts are separate proofs not integrated
into the public handle. Direct public-handle edits in single 1 MB lists and
tables took roughly 110 ms and 73 ms respectively.

The owned deep-emphasis result was also wrong as decision evidence. Maintaining
the active same-marker delimiter links reduced the 70 KB/5,000-layer case from
roughly 158–206 ms to roughly 2.9–4.0 ms while preserving 338/652 CommonMark
and 120/132 emphasis scores. A later cmark-derived code-span run index raised
the aggregate to 343/652 and reduced a 320 KB pathology from 19.6 seconds to
roughly 0.66–0.79 ms. A follow-up reference slice changed one winning
definition across 20,000 distant uses with under 80 reparsed bytes and about
0.21 ms p50 apply time, but its temporary side scanner deliberately fails the
one-authority gate. A coarse unified quote/list/setext/table transition now
emits checkpoint state, editor chunks/facts, and retroactive promotion together
and keeps a 910 KB list edit to two parser lines, but exact list/table/HTML
behavior remains a commitment gate. Separate resource probes reject that
clean-room crate as the production seed. The corrected decision and criteria
are in [`PARSER_AUTHORITY_REASSESSMENT.md`](PARSER_AUTHORITY_REASSESSMENT.md).

The follow-up prototype suite crossed the **data-flow** gate and the
**test-surface parser-to-paint composition** gate. It now includes an exact
persistent suffix splice, checkpoints inside million-byte lists, tables, and
fenced blocks, dependency-local inline parsing, a persistent
first-definition-wins reference index, a revisioned native/WASM source handle,
an actual Rust/Dart binary delta codec, a 50,000-block Flutter viewport, and a
real Comrak-to-projection-to-`EditableText` slice for both delimiter-hidden and
active-syntax-reveal modes. This is enough to choose the model; it is not yet a
production editor engine. The product slice still full-parses one bounded
active shard instead of consuming the incremental fork's delta. Automated
Phase 0 probes now support the input-lease, selection-overlay, and paged-
semantics mechanisms, while complex-script measurements reject arbitrary
independent layout shards. Physical-device selection, screen-reader, and
real-IME gates remain open.

Launch implication: if "robust live Markdown editor" includes large documents,
v2 is not the architecture to launch as the finished promise. It can still be
published as an explicitly bounded preview/small-document release, but a
general-availability claim should wait for the parallel v3 core. The desired
product scope in this investigation treats large-document behavior as
table stakes, so the recommendation is to do the internal core rewrite before
that launch rather than defer scale behind a future trigger.

## Claims from the position paper that did not survive measurement

The position paper was right about retaining Markdown source truth, keeping a
single grammar authority, and treating cmark-gfm compatibility as a moat. It
overreached from small-document and 80-block evidence to architectural fitness:

- Rebuild isolation at 80 blocks does not establish scale. The current live
  editor mounts every block in a `Column`; 5,000 blocks mounted 5,001
  `EditableText`s, took 7.09 seconds to initialize, and took 582.50 ms per
  edit pump in the debug test VM.
- "Block-local Comrak" is not equivalent to incrementality. Markdown container
  state can cross arbitrary block boundaries, and one top-level list or code
  fence can be megabytes. Exact checkpoint state, convergence, and suffix reuse
  are required.
- The immutable whole-document `String` is semantically suitable as source
  truth but not a suitable large-document representation. A 10 MB localized
  insertion took 17.27 ms before parsing or rendering.
- Comrak is not effectively the only live grammar authority while Dart maps
  every old range and synthesizes fences, lists, definitions, and provisional
  render structure before the next parse.
- IME adoption is not the only interaction unknown. A real mouse drag across
  two current block editables requested source `2..11` and ended as a collapsed
  `11..11` caret. Existing cross-block tests inject controller selection and do
  not prove the physical gesture path.
- The dense UTF-8/UTF-16 mapper is itself a large-document blocker. At 5 MB
  ASCII it built 10,000,002 list slots and took 212.92 ms.

The paper's strongest assets still stand: byte-exact source truth, Comrak/cmark
compatibility, the regression/conformance lattice, parser-as-judge commands,
IME knowledge, and honest fallback rules should all be retained. What failed is
the inference from those strengths to the fitness of the current global data
flow. Architectural malleability and green conformance say the package can be
evolved safely; they do not show that global strings, whole-document payloads,
eager hosts, or prediction-owned transient syntax scale.

## Measured current architecture

All Flutter timings below are debug test-VM measurements. They are useful for
relative scaling and architectural fan-out, not device release latency. Rust
parser timings are optimized release builds on the same machine.

| Operation | 100 KB | 1 MB | Larger evidence |
| --- | ---: | ---: | ---: |
| Current editor-state edit | 153 us | 1.25 ms | 10 MB text-buffer insert 17.27 ms |
| Current native parse + decode + Dart mapping | 44.08 ms | 459.67 ms | 1 MB payload 8.15 MB |
| Raw whole `EditableText` pump | 19.89 ms | 232.02 ms | inherently global text value/layout |
| Current live-rendered pump | 38.15 ms | 370.20 ms | plain paragraphs fall back to whole host |
| Current projection `predictAfter` | — | — | 50,000 segments 29.34 ms |
| Dense UTF offset index | — | 37.89 ms | 5 MB 212.92 ms |

The profiled 1 MB bridge spent approximately 51 ms in native parse/payload,
131 ms decoding JSON, and 218 ms mapping the result in Dart. Parser replacement
alone cannot fix this path.

## Follow-up pressure test: jank, liveness, and parser choice

### Dart work is bounded only if the API stays local

The disposable persistent Dart document was stressed beyond the intended
product range while retaining 256 structurally shared undo snapshots. Each
sample applied a one-character replacement near the middle and read a
128-character viewport slice.

| Source size | Initial construction | Edit + local slice p50 | p95 | p99 | p999 | max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 MB | 22.53-49.17 ms | 1 us | 5-7 us | 20-37 us | 127-279 us | 2.38-3.22 ms |
| 10 MB | 137.98-161.52 ms | 2 us | 3-4 us | 4-10 us | 114-162 us | 333 us-1.02 ms |
| 100 MB | 1.30-1.66 s | 101-106 us | 116-237 us | 153-515 us | 381 us-1.44 ms | 1.06-2.43 ms |

The ranges are from two runs and deliberately retain their long-tail
variability. These desktop-VM numbers do not establish a Flutter frame budget,
but they challenge the idea that total document length must determine typing
latency.
The hot operation can instead be bounded by edit size, tree height, and leaf
capacity. Initial load, whole-source materialization, search, export, retained
history, and garbage collection still scale with the document.

That makes the existing synchronous whole-document `markdown` getter an API
hazard: a listener that reads it after every edit would restore O(document)
work. The production API needs revisioned snapshots, range reads, deltas, and
streaming/asynchronous export. Whole-source materialization must be an explicit
cold operation, not an incidental controller read.

### An isolate is an accelerator, not the liveness model

A long-lived Dart isolate's empty sequential round trip measured 2 us p50,
14 us p95, 41 us p99, 120 us p999, and 992 us max on the final desktop-VM run.
This says a persistent native worker can be useful for initial parsing and rare
global work. It does not include parsing, message payloads, UI queue
contention, or a real device. One `Isolate.run` per keystroke would also add
lifecycle and copying costs.

More importantly, Flutter web has no Dart isolates: `compute` runs on the main
thread there. The portable design therefore needs a preemptible parser with a
small synchronous deadline and resumable cooperative continuation. Native can
optionally move the continuation to a long-lived isolate; web can use bounded
main-thread slices or an explicit Web Worker. Correctness and immediate common
edits cannot depend on either offload mechanism.

### The parser-to-paint surface is promising

The first 50,000-block Flutter slice used an identity parser delta and fixed
one-line extents. A second 2.97 MB slice replaced that synthetic presentation
path with a real synchronous Comrak parse of the edited active shard,
`FlarkProjection`, variable-height lazy rows, one persistent `EditableText`, and
actual styled paint. It exercised both delimiter-hidden and
active-syntax-reveal modes.

Across repeated debug test-VM runs:

| Focused syntax | Source edit + parse + projection p95 | Edit pump p95 |
| --- | ---: | ---: |
| Hidden | 0.87-1.30 ms | 5.69-8.32 ms |
| Revealed/dimmed | 0.54-0.73 ms | 3.32-6.27 ms |

Completing `**bold**` produced an authoritative strong token before the next
paint, typing the third backtick changed block styling, and an `é` composition
revealed source without replacing the input host. Only the active row rebuilt;
an adjacent mounted row stayed at one build. The hidden mode consistently did
more work and had worse debug tails, including 10-20 ms urgent-path outliers,
which reinforces active-syntax reveal as the safer launch baseline, though it
is not yet a physical-device verdict.

The web result corrected one assumption in this investigation: an asynchronous
API boundary is not automatically a frame boundary. After preloading the WASM
module, the current bridge's Promise continuation completed, published the
authoritative strong style, and scheduled the next frame without an
intermediate paint; the same input host painted revision 1 on the next pump.
The v3 web API therefore needs explicit preload/readiness and bounded work, but
does not strictly need to pretend that WASM is synchronous. Long continuation
work can still move to a Web Worker; ordinary urgent work can finish in the
browser microtask checkpoint.

This remains a composition proof, not the final liveness gate. It uses a local
full parse rather than the stateful fork, Flutter's debug test runtime rather
than a floor device, and ASCII/default-font fixtures rather than the complete
IME, bidi, shaping, and accessibility corpus.

### The bridge can be slower than the grammar

A token-dense long paragraph initially appeared to indict Comrak on web. It did
not. Pure Comrak WASM parsed 64 KB at 4.89 ms p95, while the packaged bridge took
617.11 ms before Dart mapping. The bridge expanded a full-line range for every
AST descendant by scanning backward and forward through the line, making a
long inline-rich paragraph quadratic. Reusing its existing line-start index
reduced the packaged parse/payload phase to 6.42 ms p95 with all 26 bridge tests
still passing.

After that fix, the warmed Chrome pipeline measured:

| Active shard | Shape | Total p95 | Bridge p95 | Dart mapping p95 |
| --- | --- | ---: | ---: | ---: |
| 1 KB | token-dense | 1.70 ms | 0.50 ms | 1.10 ms |
| 4 KB | token-dense | 7.00 ms | 1.20 ms | 3.40-5.50 ms |
| 16 KB | token-dense | 23.10-25.00 ms | 4.20-4.50 ms | 17.90-20.90 ms |
| 64 KB | token-dense | 72.70-73.10 ms | 14.30-14.60 ms | 56.00-56.10 ms |
| 64 KB | plain | 14.80-15.60 ms | 1.00-1.10 ms | 9.80-10.60 ms |

This is the strongest argument against defining the urgent unit as merely
"the enclosing Markdown block." A block is unbounded, and rebuilding its
entire JSON result, UTF map, and Dart projection can dominate parsing. The v3
unit must instead be bounded parser/projection leaves with compact deltas and
persistent indexes. Around 1 KB already fits the proposed urgent budget in the
current inefficient whole-result path; 4 KB becomes plausible once unchanged
tokens and coordinate maps are not rebuilt; larger work must be resumable.

### The live path needs an explicit input-to-photon contract

The ordinary-edit path should be one revisioned transaction:

1. Apply the platform text delta to the Dart source tree and mirrored parser
   rope.
2. Advance the authoritative parser from a checkpoint until it converges or
   exhausts a byte/node/deadline budget.
3. If it converges, apply syntax, projection, source/display selection, and
   active-shard deltas atomically before paint.
4. If it exhausts the budget, preserve exact source, keep the unresolved dirty
   region exact-stale or plain, and resume in short cooperative slices. A newer
   revision cancels the old continuation.

There should be no debounce on this urgent path. Completing the last `*` in
`**bold**`, a closing backtick, or an ordinary fence transition should normally
converge and render authoritatively in the same frame. Incomplete constructs
remain literal. No intermediate frame may show guessed grammar or a flash of
raw markers for syntax that the authoritative transaction already completed.

Proposed device gates are deliberately stricter than a 60 Hz average:

- ordinary-edit CPU work on the UI isolate: p99 at or below 2 ms and a measured
  hard tail below 4 ms on the floor device;
- input delta to correct painted pixels: p50 within one refresh and p99 within
  two refreshes;
- total frame time below 8.33 ms on target 120 Hz devices and 16.67 ms on the
  60 Hz floor;
- zero semantically wrong intermediate frames, lost composition ranges, or
  input-connection churn in the interaction corpus.

Measure the entire event-queue-to-paint interval and p99/p999 tails, not only
parser duration or Flutter build/raster averages.

### The UX can remove some of the hardest correctness risk

Fully hiding Markdown delimiters under the caret is not the definition of
"live"; it is the choice that creates the hardest source-to-display mapping,
selection, and composition problem. The strongest launch baseline is an
active-source island: reveal or dim syntax around the caret/composing range,
style the content authoritatively as the user types, and fully render inactive
regions. This is the same broad compromise used by Obsidian's live preview.

That mode keeps the active `EditableText` in source coordinates and makes an
IME edit much harder to mis-map, while still making bold, emphasis, code, and
block transitions feel live. Prototype it against the fully hidden mode with
the same interaction and device gates. If fully hidden wins without losing
correctness or feel, keep it; otherwise the active-syntax-reveal mode is the
more robust product optimum, not a fallback for parser weakness.

### One product invariant could overturn the whole direction

If byte-exact Markdown is merely an import/export format, a structured editor
model in the ProseMirror/Lexical family is a simpler rich-text architecture:
the tree and marks are canonical, edits are transactions, and Markdown is
parsed on import or generated on export. It avoids most source-to-display
mapping problems.

It also normalizes or loses arbitrary Markdown spelling, whitespace, malformed
constructs, and source-level cursor intent. As long as Flark promises that the
user is editing their exact Markdown source, this is not the global optimum;
it is a different product. The source-canonical incremental direction should
be reconsidered only if that product invariant changes.

### `markdown-rs` is the strongest alternate, but not a viable runtime base

`markdown-rs` is safe Rust with a byte-accounting state machine and strong
CommonMark/GFM claims. Through the same canonical cmark-gfm comparison used for
the other candidates, against cmark-gfm commit `499789b`, it reached 660/670
main examples and 25/30 extension examples. Nine main differences were
nested-strong cases that Flark explicitly supports; the likely Flark-profile
gaps are mainly a few autolinks. This is materially stronger compatibility
than Pulldown's 649/670 and 22/30.

Its stock cost is the counterweight. In the final 11-sample 100 KB run,
Comrak's full AST parse was 4.31-4.80 ms p50 across the two shapes while
`markdown-rs::to_mdast` was 147-153 ms p50. A temporary
event-level instrumentation removed mdast construction but still measured
20.93 ms p50 at 100 KB and 521.00 ms p50 at 1 MB. It exposes useful internal
pause points but no public edit-checkpoint, convergence, or persistent-splice
API.

Switching would therefore mean doing the same incremental-parser surgery while
also recovering a materially slower event-parser baseline and closing the
remaining compatibility gaps. A later mixed/reference event-only run measured
roughly 51 ms at 100 KB and 2.86 seconds at 1 MB; a 100 KB unmatched opener
shape took roughly 0.88 seconds, and its 500 KB variant was terminated after
more than 20 seconds. The project's own guidance recommends an approximately
500 KB cap for pathological input. `markdown-rs` remains useful state-machine
prior art, not a runtime integration candidate for the 10 MB SLA.

The cost of that judgment is owning a real downstream fork. The research patch
targets Comrak 0.50 and does not apply directly to 0.54; parser files changed by
roughly 1,000 insertions and 65 deletions across that interval. Production work
needs a deliberately small public stateful API, upstreamable seams where
possible, differential conformance in CI, and a repeatable upstream-merge
policy. A casually rebased private patch is not acceptable infrastructure.

### A purpose-built parser is viable, but correctness—not speed—is the gate

The architecture can be designed directly for Flark: a rope-backed line input,
value-semantic container checkpoints, persistent source-relative block chunks,
independent inline leaves, a first-definition-wins symbol index, stable IDs,
binary deltas, and budgeted `advance`. This removes Comrak's mutable arena and
whole-document finalization impedance without introducing a second live
grammar.

The executable 357-line kernel parsed approximately 1 MB initially in 2.124 ms
and processed 10,000 local edits at 292 ns p95 while reparsing one line/46 bytes
at p95. Its HTML-comment and fence cases correctly propagated state to the real
closing boundary or end of document. These numbers measure only checkpoint
machinery over a deliberately incomplete grammar and are not comparable to the
full Comrak parser.

The same kernel's naïve checkpoint-per-line representation occupied 1.89 MB
for a 1.00 MB document. The owned design must therefore separate the ability
to checkpoint at any line from the density actually retained, using compact
periodic state and aggregate line summaries rather than one heap-heavy object
per line.

The scope evidence prevents both under- and overestimating the rewrite. The
current fixture set has 652 CommonMark and 670 cmark-gfm main examples, with 132
emphasis/strong, 90 link, 74 list/list-item, and 64 block/inline HTML examples.
Mature parser cores range from Lezer Markdown's roughly 2,313 TypeScript source
lines to MD4C's 7,718 parser/API lines; Comrak 0.54 has 8,569 parser lines before
the Flark incremental module. Flark only enables autolink, strikethrough,
tables, tagfilter, and tasklist, but it requires exact source positions,
malformed-input behavior, and incremental equivalence.

The best owned direction is not a clean-room grammar invention. It is a
licensed Rust adaptation of mature algorithms onto Flark's source, state,
symbol, fact, and persistent-sequence contracts. Later symmetric seams make
Pulldown the leading inline-algorithm donor while preserving per-seam use of
localized Comrak/cmark-gfm algorithms. Those implementations remain CI
differential peers and are never shipped as simultaneous live authorities. See
[`PURPOSE_BUILT_PARSER_FEASIBILITY.md`](PURPOSE_BUILT_PARSER_FEASIBILITY.md)
and [`PARSER_DONOR_BAKEOFF.md`](PARSER_DONOR_BAKEOFF.md).

### Symmetric donor seams changed the implementation ranking

The Comrak-derived value-state seam proves genuine 4 KiB yielding inside 10 MB
paragraph/setext lines and can avoid persistent per-newline state when its
research trace is drained. It is already 2,117 core lines for 718 mapped
upstream line spans and still omits exact tightness, HTML, tables, references,
inline parsing, rope input, and integrated persistent output.

The Pulldown-derived seam is 1,839 physical core lines. Its narrow block subset
coalesces a 2 MB million-`a\n` paragraph into one chunk and two checkpoints,
bounds a 10 MB-line advance to 4 KiB/848 bytes of transient state, and matches
250/250 clean-vs-resumed edits with 4,120 maximum reparsed bytes. Its flat
`String`, eager `Vec` suffix shift, simplified grammar, and self-reported
resource counters remain disqualifying production gaps.

Stock Pulldown is separately falsified: its eager first pass created roughly
2,000,001 nodes and 192 MB of capacity for the million-line paragraph, and
roughly 24.2 MB of node capacity for about 1 MB of dense inline input. The
positive seam result comes from retaining selected algorithms while rejecting
that representation. A fresh-process public-API receipt independently measured
99,811,328 bytes maximum RSS for the 2 MB million-line shape and 24,297,472
bytes for the 1.008 MB dense-inline shape; the latter spent about 148 ms in
event resolution. The 10 MiB plain line was sparse but remained one 4.37 ms
uninterruptible constructor call.

The subsequent inline extraction removed `Tree<Item>` and `TreeIndex` while
preserving selected Pulldown emphasis, code-span, link, and reference
algorithms under segmented, fuelled execution. It matched 5,000 generated
emphasis cases and 5,000 generated code-span cases, but its retained
representation failed: a 10 MiB unmatched delimiter-dense leaf kept
100,663,296 bytes of token capacity and reached about 129 MB external RSS.
This is an algorithm-seam pass, not a representation or parser pass.

Two independent spikes then challenged whether that failure was intrinsic. A
fixed-page packed toy grammar kept its worst 10 MiB adversary to 86,705,329
accounted bytes and 78,495,744 bytes external RSS under the 96 MiB
falsification ceiling, though its slowest clean run took about 1.18 seconds and
it has no Markdown semantics. A separate checkpoint prototype restarted before
a balanced 10 MiB middle edit, scanned 4,096 bytes, compared exact state plus
immutable source-tail identity, and attached the suffix; changing persistent
open state correctly forced a full 10 MiB scan. Its state/facts are unpacked
and its page-root composition remains linear. Together they prove mechanical
possibility, not composition.

At that checkpoint, the donor-neutral Gate A contract contained 189 normative block/table fixtures
and more than 400 intermediate revisions. Gate B adds 398 normative
inline/reference fixtures, 11 histories and 687 scalar-safe revisions, exact
segmented maps, 10 MiB dense-leaf resources, cancellation/supersession, and
compact reference invalidation. Its defined/undefined lane fuelfully
re-resolves 5,000 dependent leaves while adopting compact output/dependency
roots instead of sending a 5,000-object Dart delta. Both harnesses self-falsified
known cheats; no parser candidate then passed either complete interface.

The remaining discriminating evidence was one integrated slice combining real
block-to-inline segments, Pulldown-derived inline semantics, packed pages and
stacks, exact restart/convergence, and persistent delta/reference roots. More
isolated block coverage or another independent representation spike would not
answer that question.

### Parsing is not the only unbounded algorithm

A document can contain one multi-megabyte wrapped paragraph. Even an exact
incremental Markdown parser does not stop a monolithic `TextPainter` or
`EditableText` layout from rewrapping that paragraph. Full `TextPainter`
relayout after a one-character edit measured:

| Wrapped paragraph | p95 across two runs | Observed max |
| --- | ---: | ---: |
| 4 KB | 1.41-2.10 ms | 9.03 ms |
| 16 KB | 3.50-3.99 ms | 4.11 ms |
| 64 KB | 13.69-22.18 ms | 23.39 ms |
| 256 KB | 51.20-86.34 ms | 121.60 ms |
| 1 MB | 182.50-306.38 ms | 424.54 ms |

An incremental wrapping probe then invalidated from two visual lines before an
edit and searched for an old/new break convergence point through fixed 1 KB
windows. Common same-length edits converged after 68-100 code units. Insertions
and deletions in a repetitive 1 MB paragraph legitimately changed wrapping for
699-786 KB of the suffix and took 56-70 ms in total, but each synchronous
window stayed at or below 1.12 ms in the observed run.

So wrap propagation cannot be assumed local, but it can be made jank-bounded:
lay out the active viewport first, update stable pixels and the scroll anchor,
then continue the height/break index in sub-millisecond chunks. Production
needs a persistent break/height tree so suffix offsets are adjusted lazily;
the probe's flat result list still materialized every changed suffix break.
Complex scripts, bidi, ligatures, variable fonts, and style changes remain an
explicit shaping gate. Until this layer exists, the architecture is
large-block incremental only in parsing, not in input-to-pixels behavior.

## Prototype evidence

### Text storage

The disposable persistent rope stores chunk-local line metadata and reuses
unchanged subtrees.

| Localized insertion | Current `FlarkTextBuffer` | Prototype rope | Full rope materialization |
| --- | ---: | ---: | ---: |
| 1 MB | 1.61 ms | 12 us | 0.75 ms |
| 5 MB | 8.58 ms | 10 us | 3.51 ms |
| 10 MB | 17.27 ms | 9 us | 8.90 ms |

The final column is the warning: a rope provides no benefit if the next layer
immediately requests a complete `String`.

### Projection

A persistent segment tree stores source and display lengths in internal nodes,
with projection spans local to leaves. A suffix shift changes one path rather
than every later range.

| 50,000 projection segments | Current global ranges | Segmented prototype |
| --- | ---: | ---: |
| Apply one local edit | 29.34 ms | below 1 us timer resolution |
| Materialize display | 3.04 ms globally | local leaf only, below 1 us |

### Viewport rendering

| Surface | Mounted editables | Initial pump | Edit pump median |
| --- | ---: | ---: | ---: |
| Current live editor, 1,000 blocks | 1,001 | 2.24 s | 86.88 ms |
| Current live editor, 5,000 blocks | 5,001 | 7.09 s | 582.50 ms |
| Lazy prototype, 5,000 blocks | 22 | 53.96 ms | 7.91 ms |
| Lazy prototype, 50,000 blocks | 22 | 28.19 ms | 5.54 ms |

A second prototype placed a document-level pointer coordinator over 50,000
lazy local input shards and produced the requested cross-block selection while
mounting 22 editables. It proves the ownership model is viable, not that
autoscroll, touch handles, accessibility, and IME coexistence are finished.

### Exact parser resumption

#### Pulldown 0.13.4

The first-pass parser was temporarily instrumented inside the crate so the
real tree spine, list state, blank-line state, allocations, and definition
label sets could be paused and resumed. Resuming to the end produced exactly
the same first-pass tree and allocations as a clean parse.

Across 250 deterministic edits in a 200 KB document:

- convergence distance: p50 65 bytes, p95 153, p99 250, max 292;
- bytes reparsed: p95 167;
- timed incremental work: p50 5 us, p95 11 us, max 19 us.

An unclosed HTML comment correctly propagated through the remaining document.
The experiment did not implement persistent suffix splicing; Pulldown's flat
tree and borrowed allocations need replacement.

#### Comrak 0.50

The real line-oriented Comrak block parser was then instrumented. At safe
checkpoints it retained the parser object and prefix AST, resumed on edited
input, detected convergence, and continued to an AST signature exactly equal
to a clean full Comrak parse.

Across 250 deterministic edits in a 200 KB document:

- convergence distance: p50 34 bytes, p95 144, p99 169, max 176;
- restart distance: p95 58 bytes;
- timed work after the persisted checkpoint: p50 4 us, p95 9 us, max 18 us.

Curated 1 MB local edits converged in 1–43 us. An unclosed HTML comment parsed
the full remaining megabyte in 5.22 ms. Unmodified Comrak still finalized and
inline-parsed the whole mutable arena, so resumed-to-end cost remained roughly
21–27 ms for normal documents. Persistent subtree storage, incremental inline
caches, and semantic indirection are the required fork work.

#### Follow-up persistent Comrak fork probe

The next probe replaced the retained mutable prefix with a fresh parser restored
from checkpoint state, a persistent mirrored byte rope, an AVL-like persistent
syntax sequence, source-relative nodes, and an explicit reference-definition
index. Every edit was checked against a clean Comrak oracle.

For 10,000 sequential edits in a 1 MB document:

- zero block-tree or reference-winner mismatches;
- reparsed bytes: p50 70, p95 211, p99 213, max 512;
- block parser: p50 8 us, p95 22 us, max 408 us;
- persistent splice: p50 22 us, p95 49 us, max 2.51 ms;
- parser delta: p50 161 bytes, p95 1,059, max 2,346;
- p95 replaced chunks: 2.

The expensive 505-second soak duration came from deliberately performing a
fresh 1 MB oracle parse after every edit; it is not incremental latency.

The important oversized-container receipts are stronger than a top-level
checkpoint result:

- a local edit in one 1 MB list converged after 83 bytes, replaced two list
  items, reused 10,111 suffix items, and equalled a clean block parse;
- a local edit in one 1 MB table converged after 37 bytes and reused 18,253
  suffix rows exactly;
- a checkpoint halfway through a 1 MB fenced block retained five bytes of
  accumulated state, converged after the changed line, and produced the exact
  literal through persistent payload splicing;
- a 16-byte composable list-tightness aggregate matched Comrak across 4,000
  items, avoiding a whole-list finalization scan.

An additional 180-transition corpus repeatedly opened and closed HTML and code
blocks, changed list indentation and table delimiters, edited and removed the
winning duplicate reference definition, inserted block boundaries, and changed
inline delimiters. Both the persistent block/reference result and independent
per-leaf inline result matched a clean full parse after every transition.

The full patched suite passed 563 unit tests and 170 doctests, with only the
pre-existing ignored test plus the explicit 10,000-edit soak ignored in the
ordinary run.

### Inline and reference semantics

Parsing each inline-bearing leaf independently in document order produced the
same complete Comrak tree as a clean full parse. Each changed leaf declared the
reference labels it looked up, including misses, so adding or removing a
definition invalidates the right candidate leaves.

That experiment found one stock-Comrak behavior that cannot survive in the new
engine unchanged: resolved references consume a cumulative 100 KB clone budget.
The result of a later leaf can therefore depend on how much reference URL/title
text earlier leaves cloned, even when the leaf uses a different label. This is
traversal-order state, not Markdown semantics, and breaks viewport-order lazy
parsing.

The robust fork should intern normalized reference labels and values. Inline
reference nodes store a label ID; definition value changes update the symbol
table without reparsing leaves, while definition presence changes reparse only
dependent leaves. Resource limits should be local/per-leaf or measured against
interned storage, never against viewport traversal order. A persistent index of
all definition occurrences preserved Comrak's first-definition-wins behavior,
including value-only winner changes when the first duplicate is removed.

### Revisioned ABI and compact delta

The UI-side persistent source now aggregates UTF-16 length, UTF-8 length, line
breaks, and a shared UTF-8 byte fingerprint. A Unicode hash mismatch found
during the probe was corrected: hashing Dart UTF-16 units and native UTF-8 bytes
would have falsely rejected valid non-ASCII edits.

Measured on the Dart VM and Chrome:

- a localized source edit plus coordinate mapping was p95 81 us at 1 MB and
  102 us at 10 MB on the VM; Chrome p95 was 68 us and 102 us;
- the source edit request for one byte was 29 bytes;
- 2,000 sequential Unicode edits matched a Dart `String` oracle;
- the native/WASM persistent source handle matched a Unicode oracle across
  10,000 Rust edits and rejected stale revisions, wrong hashes, invalid UTF-8,
  and scalar-splitting ranges.

The syntax delta is an actual little-endian binary codec, not a JSON-size
estimate. Rust and Dart share a 78-byte Unicode/ranges/replacement golden and
both fail closed on truncation or version skew. An identity-projection leaf
delta is 56 bytes; edit plus syntax response stayed at 85 bytes through 1,000
sequential edits instead of growing with the edited leaf's text.

The same source handle compiled to `wasm32-unknown-unknown` and ran under V8:

- 5,000 edits in a 1 MB mirrored rope: p50 5.0 us, p95 11.1 us;
- 10,000 complete stock-Comrak parses of a bounded changed fragment: p50
  6.5 us, p95 12.9 us.

The latter is a WASM cost receipt for bounded Comrak work, not a claim that the
checkpoint fork is already exported as the shipping WASM parser.

### End-to-end Flutter viewport slice

A 2.09 MB, 50,000-block model connected the persistent source, revisioned edit,
binary syntax delta, persistent block/projection aggregate, lazy viewport, and
bounded `EditableText` shard:

- 22 editables mounted;
- one composed `é` survived the source edit and parser response with the same
  composing range and input connection;
- only the active shard rebuilt; an adjacent mounted shard stayed at one build;
- 1,000 local edit/encode/decode/apply loops were p95 96 us on the VM and
  109 us in Chrome;
- 60 edit-and-pump cycles were p95 4.54 ms on the VM and 4.13 ms in Chrome;
- the separate document gesture coordinator selected source `2..372` across
  lazy shards while keeping 22 editables mounted.

These are test-runtime receipts, not physical-device profile-frame or real-IME
certification.

### Parser fidelity

The initial Pulldown result understated its compatibility by treating the older
GFM 0.29 core corpus as normative after Flark selected CommonMark 0.31.2 and by
counting excluded footnotes. The corrected profile receipts are:

- canonical CommonMark 0.31.2: 652/652;
- current GFM extension corpus with Flark's five flags: 22/30, where three
  failures are excluded footnotes, two are bare-autolink examples, two interop
  examples depend on bare autolinks, and one is tagfilter rendering; and
- table, strikethrough, and task-list extension fixtures pass in the selected
  profile.

Tree-sitter Markdown explicitly says its inaccuracies make it unsuitable when
correctness matters. Lezer has an excellent reusable-tree API but its Markdown
semantics and JavaScript runtime would make it a second grammar. Both remain
useful design references, not authorities.

This no longer makes a Comrak fork the automatic lower semantic-risk option.
Both donors require a new persistent input/state/output model. Pulldown needs a
bounded bare-autolink addition and explicit Flark deviations; Comrak needs a
substantial ownership/continuation rewrite. The final discriminator is the
source-backed inline seam, not aggregate clean-parser fidelity.

### Oversized syntactic constructs

Raw release Comrak full parses:

| Shape | 1 MB | 5 MB | 10 MB |
| --- | ---: | ---: | ---: |
| One paragraph with inline syntax | 33.08 ms | 142.75 ms | 280.38 ms |
| One fenced code block | 0.80 ms | 5.29 ms | 8.67 ms |
| One list container | 42.50 ms | 207.28 ms | 401.38 ms |

Document virtualization cannot stop at parser top-level blocks. Layout shards
must have a bounded size, and parser checkpoints must be possible inside large
containers. Giant paragraphs require an incremental inline pass eventually;
until then they must edit and scroll locally while exact inline styling catches
up asynchronously, with no guessed grammar state.

## Recommended engine model at that checkpoint (historical)

### 1. Canonical text and coordinates

- Dart owns a persistent piece tree because Flutter needs synchronous, frequent
  viewport slices without FFI round trips.
- The native/WASM parser owns a mirrored rope, updated by the same revisioned
  edits. Mirrored storage is not dual syntax ownership.
- Every chunk aggregates UTF-16 length, UTF-8 length, line breaks, and hash.
  Flutter/API offsets remain UTF-16; parser offsets remain UTF-8 bytes; mapping
  is logarithmic plus a bounded leaf scan.
- Initial load transfers the document once. Steady-state calls transfer edits,
  viewport requests, and parser deltas only.

Ropey 2 demonstrates the desired byte/char/UTF-16/line metrics, but it is
currently beta. VS Code's piece-tree experience also supports keeping the hot
UI text API in the UI runtime instead of crossing a native boundary per line.

### 2. Stateful exact parser service

`DocumentParser.applyEdit` should:

1. validate document handle, base revision, and source hash;
2. apply the edit to the mirrored rope;
3. restore the predecessor parser checkpoint;
4. run the Comrak-derived block machine until old/new source alignment and
   exact parser state converge;
5. splice new persistent block segments and reuse the unchanged suffix;
6. re-run inline parsing only for changed leaves;
7. update a persistent reference-definition symbol table;
8. return a compact binary delta with stable IDs and local projection facts.

Checkpoints must include container state and be available inside large lists,
quotes, code/HTML blocks, and periodically by byte count. The AST should use
source-relative offsets within persistent segments; aggregate trees provide
global source/display positions without rewriting the suffix.

Reference links need semantic dependency handling separate from structural
convergence. Candidate reference nodes should retain normalized label IDs and
resolve through a symbol table, so changing one definition does not rewrite
20,000 otherwise unchanged inline nodes. Visible consumers resolve lazily.

### 3. No prediction-owned grammar

The immediate edit path may keep the previous authoritative tree outside a
dirty range, but it must not synthesize a competing Markdown interpretation.
Ordinary localized edits should parse synchronously before the next frame. If
work exceeds a small budget, the parser returns `pending` and continues in
bounded slices or on a worker. The dirty region renders honestly until the
authoritative delta arrives.

Commands may still propose source transformations, but parser state supplies
their context and a cheap persistent candidate parse judges the result. Dart
scanners may perform text mechanics; they do not own syntax classification.

### 4. Segmented projection and viewport

- Parser leaves own local hidden/replacement spans and source-to-display maps.
- A persistent aggregate tree maps global source/display offsets in `O(log n)`.
- Rendering requests only visible/overscan shards; there is no global
  `projectText`.
- Parser blocks and layout shards are distinct. A large parser block can be
  split only at boundaries that are both syntax-safe and proven to preserve
  shaping and line-break context. Parser approval alone is insufficient.
- Use a lazy sliver plus a height index of estimated/measured shard extents.
  Preserve scroll by stable anchor ID and intra-shard pixel offset.

### 5. Input, selection, and IME

- Keep Flutter's `EditableText` for a bounded active/visible shard. Receiving a
  complete local `TextEditingValue` is cheap at a few kilobytes and preserves
  Flutter's mature platform input behavior.
- Do not start by implementing a custom `DeltaTextInputClient`; that would also
  require replacing caret, selection, semantics, autofill, and platform action
  behavior.
- Add a document-level gesture and selection coordinator for cross-shard drag,
  autoscroll, select-all, clipboard, and selection painting.
- Pin the active shard and its input connection during composition. Queue
  parser-driven reshaping of that shard until composition commits, then
  reconcile by stable ID and source selection.

## Product-scale contract

Treat scale as work shape, not only document length:

- **Up to 10 MB / hundreds of thousands of ordinary blocks:** full-featured
  live editing with the same ordinary-edit input-to-photon gates regardless of
  length. Initial parse, rare global propagation, search, and export may be
  asynchronous, but scrolling and typing remain responsive.
- **10–100 MB:** a stretch tier for responsive open, local editing, and
  viewport rendering, possibly with non-visible enrichment deferred. The Dart
  source probe makes this credible; it is not a launch SLA until memory, load,
  far-scroll, parser, layout, and device tails pass together.
- **Oversized single constructs:** text mutation and viewport rendering remain
  local. Code/HTML/list containers use internal checkpoints and shards. Giant
  inline paragraphs need incremental inline parsing and incremental word
  wrapping plus context-preserving shaping. Until then they may show an
  exact-but-stale/plain dirty region, or a bounded/no-wrap source treatment for
  an uncertified bidi/grapheme/shaping region, while authoritative passes
  complete—never guessed formatting or independently shaped arbitrary
  substrings.
- Widget count and layout work are bounded by viewport plus overscan, not
  document blocks.
- Per-edit allocation is proportional to changed text/nodes plus tree depth,
  not document length.

## Rewrite boundary identified at that checkpoint (historical)

Retain:

- public controllers, transactions, selections, and source-fidelity contract;
- command behavior and the regression/IME/conformance corpus;
- Comrak grammar algorithms and cmark-gfm differential oracle;
- visual block components where they can consume local segment models.

Replace:

- `FlarkTextBuffer` immutable full strings and global line arrays;
- full-document JSON parse requests/results and dense UTF offset maps;
- `FlarkProjection.projectText` and global range prediction;
- global render-plan offset remapping and provisional syntax;
- debounce-first whole-document parse scheduling;
- whole-document `EditableText` fallback and `SingleChildScrollView + Column`;
- per-block-only gesture/focus ownership.

Build this as a parallel v3 engine behind the existing API. Do not gradually
mutate v2 until both models are entangled.

## Gate result and remaining risks at that checkpoint (historical)

The **data-flow gate** passes. The piece tree, revisioned handle, exact safe and
in-container checkpoints, persistent suffix, inline/reference dependency model,
compact binary delta, 50,000-block viewport, 10,000-edit differential, and
adversarial transition corpus all have executable receipts. Local source,
incremental block-parser, splice, codec, and viewport operations are small
enough to justify the model.

The **test-surface product-feel composition gate** also passes. A real Comrak
result now drives exact hidden/styled projection, active input, IME composition,
a fence transition, and the next paint in a 50,000-block VM surface; a separate
warmed-web slice proves the async bridge can publish authoritative styling
before its next paint. The **production product-feel gate** remains open because
the large slice does not consume the incremental fork's real delta and has not
passed target-device accessibility, touch, or real-IME tests. The complex-
shaping spike produced a useful negative result: Arabic joining and Latin
ligature measurements prove that arbitrary independent layout shards are not
exact, so the layout design now requires shaping/line-break-safe boundaries and
shared context. The follow-up checkpoint differential then passed monolithic
line/geometry equivalence for the ordinary mixed-script corpus with bounded
leading/trailing context, while deliberately rejecting long-lived bidi state
and an oversized grapheme. That supports a certified-checkpoint plus fallback
design without claiming Flutter exposes a universal resumable paragraph API.
The engine must still commit an ordinary local edit, authoritative syntax,
projection, source selection, and the active shard atomically before the next
paint. If an explicit work/deadline budget is exhausted, it should keep an
exact stale-or-plain dirty region and resume; it must never show guessed
grammar.

That does not make the engine launch-ready. The next work should be a narrow
productionization program, with these blockers made explicit:

1. Land the indexed full-line-range bridge fix independently, add a pathological
   long-line performance receipt, and audit every parser-adapter pass for hidden
   rescans. This is a current-package quality fix, not a reason to retain v2's
   whole-result architecture.
2. Move the probe into a maintained Comrak fork with a public stateful
   native/WASM handle, explicit byte/node/time budgets, cancellation, and
   revision supersession. The current fork code is a reproducible patch, not
   the package bridge.
3. Feed that fork's real binary delta into the product-feel slice. Keep the
   active-syntax-reveal mode as the baseline and retain the hidden mode as an
   evidence-gated enhancement. Run both under real typing, selection,
   composition, touch, and accessibility on floor and 120 Hz devices.
4. Replace Comrak's global reference clone budget with interned symbols and
   local resource accounting, then run cmark-gfm plus Flark's full corpus on
   incremental and full paths.
5. Productionize the proved contextual line-checkpoint path and define the
   classifier for uncertified bidi/grapheme state. The honest fallback remains
   a bounded exact-stale/plain or no-wrap dirty shard while authoritative inline
   and layout passes catch up; this is the largest remaining UX compromise.
6. Generalize parser checkpoint summaries beyond the proved list/table/raw
   cases to every enabled Flark extension, with fuzzing over checkpoint
   serialization/deserialization and edit convergence.
7. Replace the test viewport with a sliver/height/scroll-anchor implementation;
   validate touch handles, autoscroll, clipboard, accessibility, undo/redo,
   and selection across mounted/unmounted shards.
8. Remove whole-document reads from the hot public API and measure initial
   open, far scroll, large paste, find/replace, memory retention, checkpoint
   density, undo history, and global propagation at 1 MB, 10 MB, 100 MB, and
   oversized-single-construct fixtures.

Do not merge this by incrementally teaching v2 more prediction rules. Build a
parallel v3 core behind the existing public API and retire v2 paths only as
behavioral and device gates move over.

### Phase 1 Comrak maintenance update — historical evidence, 2026-07-14

The first representative fork upgrade improved the parser confidence without
clearing the production gate. The exact research patch rebased from Comrak
0.50.0 to 0.54.0 with two small parser-state conflict regions and one AST-field
adaptation. The full upgraded upstream suite, the focused incremental suite
with and without default features, and the 10,000-edit full-parse oracle soak
passed. The soak reparsed 211 bytes at p95 and 512 at maximum over a roughly
1 MB document.

This does not justify shipping the patch as-is. Approximately 3,000 added lines
remain a `cfg(test)` engine embedded in `parser/mod.rs`; it proves algorithmic
survival, not a clean stateful fork API. The next parser step is extraction into
small Flark-owned modules with a narrow document handle, budgets,
cancellation/supersession, and native/WASM differential CI. See
[`COMRAK_MAINTENANCE_REHEARSAL.md`](COMRAK_MAINTENANCE_REHEARSAL.md).

Later state/output probes superseded the front-runner implication of this
receipt. The current checkpoint can falsely converge across a list-tightness
change, cloned spines become larger than deeply nested source, open paragraphs
retain their entire prefix, and local giant-container edits produce
multi-megabyte deltas. Fixing the class requires persistent source-backed block
and inline ownership—the selected Flark-owned representation. The later donor
bakeoff reopened which implementation lineage best supplies those algorithms.

The parser-independent v3 source substrate was promoted separately into an
unexported package module. It now exercises bounded leaf ownership, exact
UTF-16/UTF-8/line/hash coordinates, atomic multi-edit batches, revision/hash
validation, conservative grapheme certification, large deletion, and a 10 MB
local edit in both the Dart VM and Chrome. This work remains valid if the
parser choice is reopened.

The follow-up extraction improved the visible maintenance surface. The net
Comrak 0.54 patch adds 3,316 lines in a dedicated Flark-owned module while
changing existing upstream files by 53 insertions and five deletions. Its
external document API returns stable-ID block chunks and reference-definition
changes; a clean 10,000-edit 1 MB run measured 11/16/20 microseconds at
p50/p95/p99. That timing excludes inline parsing, and the module's broad use of
private parser internals means the existing-file diff is not a complete
coupling measure. The complete 663-test unit suite, 203 doctests,
no-default-feature focused suite, clean patch replay, and WASM build passed.

That result first changed the parser question from “is a Comrak fork feasible?”
to “which candidate has the cleaner lifetime boundary?” The subsequent
symmetric and composition probes now support one narrower conclusion: keep the
Flark-owned persistent core, use Pulldown as the leading inline-algorithm donor,
select other algorithms per seam, and require one integrated packed/restart/
real-grammar candidate before broad Phase 1 implementation. Neither the current
narrow Comrak adapter, stock Pulldown, nor the clean-room trial is the automatic
fallback under the unchanged SLA.

### Active-leaf locality finding — 2026-07-16

The real pinned Comrak chronology narrows the retroactivity problem more than
the earlier generic-mutation discussion suggested. For CommonMark 0.31.2 plus
the selected GFM profile, Setext, reference-prefix removal, and whole/split
table promotion all target the currently provisional Paragraph. No selected
block-spine transition reaches back into an unrelated, already-closed sibling.

The result is not equivalent to “delay the Paragraph Enter.” The same
provisional Paragraph can finalize as one Paragraph or Heading, zero wrappers
after reference-only removal, or a preface Paragraph plus an open Table.
Restart can also begin inside a canonical old Heading, reference-only Gap, or
Table body. A fresh-only deferred fragment therefore fails the large-document
restart case unless it also gains exact retained-base provenance and range
authority.

Those observations select an opaque normalization group as the proposal, not
as a new finding: one group manifest records the final structural,
source/projection, checkpoint, identity, and reference footprint and is shared
by every checkpoint inside the provisional Paragraph. The complete proposed
contract and reject gates live in
[`LEAF_NORMALIZATION_GROUP_GATE.md`](LEAF_NORMALIZATION_GROUP_GATE.md).
Description lists and footnote postprocessing are explicit profile-boundary
counterexamples because they can reparent or move already-closed structure.

### Integrated-authority finding — 2026-07-16

A representation-neutral capability model can be internally consistent while
proving the wrong architecture. The first normalization-group model allowed a
future suffix to be supplied at checkpoint creation, treated caller-assembled
source runs as authoritative, asserted prefix compatibility, and simulated a
10 MiB group with one length-only leaf. It could therefore certify convergence
that the real editor has not earned. The outcome algebra remains useful, but
the executable model is rejected as evidence. Restart and convergence claims
must now consume real Crop lineage, real packed-green sequence cuts, real
parser state, and the existing `ArenaBuildSession` journal.

The fenced-code integration demonstrates why that stricter rule matters. Once
the real direct parser was joined to the real writer, two authority holes
appeared that isolated models had not exposed: logical metrics were updated in
one consume path rather than at the single accepted-piece boundary, and a bare-
EOF all-marker line could finish its physical range without the authoritative
source cursor having observed EOF. Centralizing metric accounting and adding a
resumable line-EOF confirmation fixed both without adding parser-side offsets
or another state machine.

The resulting fence seam is notably small: the parser emits fence grammar
facts plus two semantic marks; the writer derives exact bounded byte/UTF-16
info and literal slices from its own constant-size fold. Unicode, CRLF, all
line-ending forms, long runs, bare EOF, nesting, BOM, sequential fences,
failure/cancellation, and adversarial mark/fact cases now pass through the real
v3 composition path. This strengthens the general architecture—typed semantic
decisions crossing into one source/storage authority—while simultaneously
raising the evidence bar for Setext: only the real 10 MiB restart/convergence
splice can validate the normalization-group hypothesis.

The first real persistent-cut work strengthens that direction without yet
closing the gate. A resumable `ArenaBuildSession` split now visits only one AVL
boundary path, reuses preflighted join scratch, allocates at most one branch per
poll, and preserves the exact old leaf identities at every tested cut,
including a worst-shaped 987-leaf tree. Forced packed-green leaf barriers also
distinguish zero-source-metric structural positions by leaf plus event rank.
An adversarial audit caught one authority overstatement: a ready cut could be
extracted after its build entered abort. Requiring the matching live session
fixed that escape, but the value remains only a build-local observation. It
becomes restart authority only when the writer consumes it into the same
candidate's composite checkpoint entry and the committed manifest later binds
that entry to source, grammar, generation, parser path, and adjacent leaf
identity. This is useful evidence precisely because it narrows, rather than
hides, the remaining integration work.

The corresponding donor-control boundary is now executable rather than an
assumption. Immediately after an acknowledged physical line, the direct parser
can capture a versioned pause containing only its open block path, current
depth, line cursor, child folds, and deferred terminator/blank-gap state. It
retains no source bytes or donor positions, rebuilds fresh `NodeId`s, and
reproduces the uninterrupted suffix command stream across paragraph, list,
fence, Setext, BOM, blank-gap, and EOF witnesses. Memory grows with syntax
depth: 120 bytes at depth 2 and 1,656 bytes at depth 66, independent of a long
closed prefix. This validates `ControlContinuation` as a clean private seam,
not as a complete checkpoint. In particular, a checkpoint inside a canonical
old Heading must still restore provisional Paragraph control plus the same
stable identity through storage-owned normalization-group authority; serializing
the parser pause alone would recreate the very control/output coupling the new
architecture is meant to remove.

The persistent sequence now also has the full replacement operation that the
real gate needs. One build-owned job cuts both boundaries, releases the deleted
subtree as one journal owner, and resumably joins prefix, optional replacement,
and suffix with at most one branch allocation per poll. It preserves exact old
leaf identities across all 171 delete ranges of the adversarial 17-leaf
fixture, every insertion/replacement boundary, aligned/full/empty cases, and
forced failure or cancellation at every phase. This closes the generic tree
mechanics question. The next uncertainty is intentionally higher-level: can a
typed green working-prefix/tail cursor use that operation to normalize a real
provisional Paragraph without exposing raw roots, accumulating prefix wrappers,
or creating a second per-block representation?

### Selected-contract convergence finding — 2026-07-18

The next executable work answered that question positively for the block,
source, projection, storage, Setext, and Table-cursor seams. This does not make
the editor production-ready, but it changes the architectural characterization:
the selected topology is no longer a collection of unrelated mechanism toys.

- The Dart v3 foreground is analyzer-clean and 66/66 green. Its 17/17 focused
  widget lane routes Flutter insertion, deletion, replacement, and non-text
  deltas directly into exact ordinary or provisional-bulk source transactions.
  A 100,000-unit paste performs no foreground replacement UTF-8/chunk work and
  installs a 64-unit scalar/CRLF-safe island; global selection and active IME
  composition survive source-free handoff; randomized ordinary/bulk rebases
  match a `String` oracle. This proves the bounded control shape, not AOT/web or
  floor-device frame latency.
- Setext is 44/44 green through retained restart, the real source ledger,
  projection composer, packed-green storage, suffix splice, and identity
  authority. The 10 MiB parent-bound receipts preserve distant page identity,
  and stale/crossed/cancelled work fails unpublished. The key generalization is
  normalize-before-parent-crossing: a private deferred whole outcome is
  acknowledged before non-Paragraph Open, parent/ancestor Close, or Finish;
  Paragraph Open alone consumes the authenticated residual split.
- The authenticated Table projection cursor is 4/4 green, with seven
  differential scanner tests, five downstream tests, four two-pass tests, and
  four Table/reference/Setext/list priority tests behind it. The actor retains
  the only packed-green/Program/Crop cursors and `TableReady` is a non-cloneable
  seal. This rejects the earlier copied-row/cloneable-snapshot temptation while
  leaving parser-priority, prefix-retain, body-row, and writer integration open.
- The source projection composer line-boundary module is 22/22 green. Its
  continuation retains no source/heap payload, fits 224 bytes, and derives its
  next generation from the sealed-run count. That is a clean compact checkpoint
  member, not proof of the final composite checkpoint.
- The supporting exact-grammar lanes are green independently:
  `reference_label_service` 4/4, `pulldown_inline_gate` 16/16,
  `comrak_value_block_core` 176/176, and `oversized_block_line_gate` 16/16.
  The combined v3 run passes 392 library tests plus every integration target
  and compile-fail doctest when the intentionally open reference-finalizer
  publication test is skipped. These receipts validate algorithms and seams;
  the skip prevents them from being misreported as complete reference support.

The remaining reference restart/re-winner design is deliberately recorded as
**PROVISIONAL**, not smuggled in as a settled layer. One global occurrence
sequence is paired with an exact-label directory whose leaves own per-label
persistent occurrence sequences. A committed checkpoint authenticates each
label's prefix rank before the active Paragraph. For a contiguous
restart-to-convergence replacement, old changed occurrences delete forward at
that fixed rank and new occurrences insert in reverse at it. Winner is element
zero, so deleting it promotes an untouched suffix occurrence without suffix
enumeration or rebasing. Arbitrary move/reorder is outside the operation. A
narrow audit of the pinned Crop API found no public stable leaf/piece token,
and a second challenge found that finite edit lineage cannot make old
occurrence coordinates durable. The cleaner design is simpler:
parser-authenticated source/projection cuts exist only inside the active
Paragraph transaction. Before terminal mutation, an authenticated
random-access cursor replays each accepted destination/title range through the
pinned streaming cleaner into persistent cooked byte blobs. Published
occurrences own those blob roots; unchanged suffix occurrences reuse them by
identity, and the old projection can retire after the manifest join. Durable
go-to-definition positioning remains a separate stable-anchor or
lazy-coordinate-index gate rather than a hidden promise of the semantic
reference index. The standalone cleaner matches pinned Comrak for every
semicolon-terminated named entity and 2,000 randomized accepted values; the
generated table pins a 33-byte maximum spelling, 6-byte maximum decoded value,
and exact 6/5 expansion ratio. The current restart kernel's fixture-only raw
label IDs and heap `Vec` of changed occurrences are not production closure.
The production path must adopt the committed exact interner, stream changed
cooked occurrences into a persistent replacement spool, reverse-traverse that
spool for fixed-rank per-label insertion, and expose a bounded committed
normalized-label-to-winner query for inline consumers. The design still needs
executable insertion-before-winner, winner-deletion,
relabel/value, duplicate-order, large-suffix identity/work, fuel-one,
cancellation/fault, crossed-checkpoint, and memory tests. The parser-owned
reference-prefix finalizer/CandidateWriter join remains a separate HOLD even
if those tests pass.

The current architectural judgment is therefore stronger, not more layered:
one actor-owned candidate, one grammar controller, one source/projection
authority, and one persistent output family now explain the previously awkward
Setext and Table cases without a second parser, aggregate Paragraph string,
mutable donor AST, or cloneable snapshot. The remaining risk is whether
reference, inline, host publication, and presentation can enter that same
authority cleanly. RFC 023 is the proposed architecture; the proof ledger
controls those remaining gates, while this file remains the observational
evidence chain.

## Research references

- [CommonMark 0.31.2](https://spec.commonmark.org/): normative syntax and 652
  executable examples for the owned-parser gate.
- [GitHub Flavored Markdown](https://github.github.io/gfm/): tables,
  strikethrough, autolinks, task lists, and tag filtering enabled by Flark.
- [cmark](https://github.com/commonmark/cmark) and
  [MD4C](https://github.com/mity/md4c): compact complete-parser implementation
  scale and algorithm prior art.
- [CodeMirror system guide](https://codemirror.net/docs/guide/): persistent
  transactions, viewport-only rendering, and estimated/measured document
  heights.
- [Lezer API](https://lezer.codemirror.net/docs/ref/): reusable tree fragments,
  partial parsing, `advance`, and explicit stop positions.
- [Lezer Markdown](https://code.haverbeke.berlin/lezer/markdown): a genuinely
  incremental Markdown grammar, but deliberately does not validate reference
  links and therefore cannot be Flark's definitive grammar unchanged.
- [Flutter `ParagraphBuilder`](https://api.flutter.dev/flutter/dart-ui/ParagraphBuilder-class.html):
  a one-shot public paragraph construction surface with no checkpoint
  import/export API.
- [`unicode-bidi`](https://docs.rs/unicode-bidi/latest/unicode_bidi/): a safe
  Rust UAX #9 implementation that exposes paragraph classes/levels and may be
  useful as a classifier or oracle, but does not make Flutter consume resumed
  bidi state.
- [Tree-sitter Markdown](https://github.com/tree-sitter-grammars/tree-sitter-markdown):
  explicit correctness warning for the Markdown grammar.
- [Comrak](https://github.com/kivikakk/comrak): current CommonMark/GFM
  conformance and native/WASM parser substrate.
- [Pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark): current
  leading inline-algorithm donor; its extracted algorithms, stock eager first
  pass, and failed rich retained representation are evaluated separately.
- [`markdown-rs`](https://github.com/wooorm/markdown-rs): the strongest
  alternate Rust substrate evaluated in the follow-up bakeoff.
- [Ropey](https://cessen.github.io/ropey/ropey/struct.Rope.html): logarithmic
  edits and byte/character/line/UTF-16 metrics for huge text.
- [VS Code piece-tree retrospective](https://code.visualstudio.com/blogs/2018/03/23/text-buffer-reimplementation):
  real editor tradeoffs, line metadata, CRLF complexity, and the cost of hot
  UI/native round trips.
- [Xi minimal invalidation](https://xi-editor.io/docs/rope_science_12.html):
  explicit deltas, viewport caches, and bounded invalidation throughout the
  render pipeline.
- [Xi incremental word wrapping](https://xi-editor.io/docs/rope_science_05.html):
  why a long paragraph needs its own resumable layout algorithm.
- [Zed's rope and SumTree](https://zed.dev/blog/zed-decoded-rope-sumtree):
  persistent indexed sequences for source and derived editor state.
- [Obsidian live preview](https://obsidian.md/help/edit-and-read):
  syntax is revealed around the cursor, a useful UX option for reducing source
  to display mapping risk in the active input region.
- [ProseMirror guide](https://prosemirror.net/docs/guide/): the leading
  structured-document alternative, with immutable document state and
  transaction-based updates.
- [Flutter performance guidance](https://docs.flutter.dev/perf/best-practices)
  and [isolate guidance](https://docs.flutter.dev/perf/isolates): device frame
  budgets, long-lived workers, message costs, and the web limitation.

## Reproduction

Worktree: `/Users/dan/.codex/worktrees/flark-parser-prototypes`

```sh
flutter test --tags benchmark --reporter expanded \
  test/prototype/flark_piece_table_prototype_test.dart \
  test/prototype/flark_segmented_projection_prototype_test.dart \
  test/prototype/flark_lazy_block_editor_prototype_test.dart \
  test/prototype/flark_utf_offset_mapper_probe_test.dart

flutter test --reporter expanded \
  test/prototype/flark_cross_block_gesture_probe_test.dart \
  test/prototype/flark_document_selection_coordinator_probe_test.dart \
  test/prototype/flark_revisioned_document_prototype_test.dart \
  test/prototype/flark_incremental_delta_codec_prototype_test.dart \
  test/prototype/flark_incremental_vertical_slice_prototype_test.dart \
  test/prototype/flark_product_feel_vertical_slice_prototype_test.dart \
  test/prototype/flark_wrapped_paragraph_layout_probe_test.dart \
  test/prototype/flark_incremental_wrap_convergence_probe_test.dart

flutter test --platform chrome --reporter expanded \
  test/prototype/flark_revisioned_document_prototype_test.dart \
  test/prototype/flark_incremental_delta_codec_prototype_test.dart \
  test/prototype/flark_incremental_vertical_slice_prototype_test.dart \
  test/prototype/flark_web_wasm_urgent_path_probe_test.dart

cargo run --release --manifest-path tool/parser_research/Cargo.toml \
  --bin edit_locality
cargo run --release --manifest-path tool/parser_research/Cargo.toml \
  --bin pathological_blocks
cargo test --manifest-path tool/parser_research/Cargo.toml --lib
cargo test --manifest-path tool/parser_research/Cargo.toml --bin delta_codec

RUSTC="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc" \
  "$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo" \
  build --release --target wasm32-unknown-unknown --lib \
  --manifest-path tool/parser_research/Cargo.toml
node tool/parser_research/run_wasm_revisioned_handle.mjs
node tool/parser_research/probe_comrak_fragment_wasm.mjs
node tool/parser_research/probe_packaged_comrak_wasm.mjs
```

The exact Comrak probe is preserved as
`tool/parser_research/0001-Prototype-incremental-checkpoints-and-persistent-suf.patch`.
It applies cleanly to Comrak 0.50.0 and contains the ordinary, container,
inline/reference, adversarial, and ignored 10,000-edit soak tests. It is a
research fork patch, not production code. The earlier Pulldown private-state
probe remains only in its temporary patched crate copy.
