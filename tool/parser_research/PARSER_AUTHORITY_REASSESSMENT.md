# Parser authority reassessment

Status: directional decision after symmetric falsification, 2026-07-14. This
selects the parser implementation lane to fund without reopening the
persistent, incremental, virtualized editor architecture.

## Current decision update

The later symmetric Comrak/Pulldown seams, inline extraction, packed/restart
spikes, and donor-neutral gates supersede the Comrak-exclusive outcome below.
The current decision is:

- keep the Flark-owned persistent parser contracts and the overall v3 editor
  architecture;
- make the pinned CommonMark/GFM/Flark profile normative, with Comrak,
  Pulldown, and cmark-gfm as differential peers;
- reject stock Comrak, stock Pulldown, the narrow adapter, and the clean-room
  grammar as production seeds;
- use Pulldown 0.13.4 as the leading inline-algorithm donor, without selecting
  it as donor for the whole core, while permitting localized Comrak/cmark-gfm
  algorithms behind one Flark runtime state/fact model; and
- require one integrated real-grammar, packed-state, checkpoint-restart,
  persistent-delta slice before broad parser implementation or product
  integration.

The inline experiment passed its algorithm-seam question but failed its
retained-memory representation. Independent packed-state and exact-restart
experiments passed their narrow mechanism questions. Gate A and Gate B are now
executable and hardened, but no parser candidate passes either complete gate.
The remaining risk is the composition of those results, not the absence of
another donor candidate.

The evidence and exact go/revise/stop criteria are in
[PARSER_DONOR_BAKEOFF.md](PARSER_DONOR_BAKEOFF.md). The remainder of this file
is retained as the intermediate reassessment and evidence chain; its claim that
Comrak had already earned exclusive primary-donor status is historical.

## Superseded intermediate outcome

**Pursue a Flark-owned, Comrak-derived persistent parser core. Do not grow the
current clean-room trial into the production grammar, and do not productionize
the current arena-backed Comrak adapter. Preserve Comrak's grammar/scanner
lineage while replacing its state, input, work, and output model.**

This is neither a clean-room parser nor a surgical add-on. Start from one
canonical Rust donor: Comrak's parser/scanner functions, extension profile,
tests, and semantic decisions. Refactor or port them into Flark `Input`, compact
value/index state, sub-line work budgets, source facts, and a persistent chunk
sink. Preserve function-level correspondence and provenance where practical.
Keep an unmodified pinned Comrak path as the clean-parse oracle; use cmark and
cmark-gfm for independent/pathological cases rather than as coequal code donors.

The lane still has a hard stop/go gate before it becomes the shipping parser.
That is an execution-risk guardrail, not an open-ended three-way bakeoff. If it
fails, the fallback is not automatically the current narrow Comrak patch: under
the unchanged 10 MB and oversized-construct contract, that path still requires
deep ownership changes. The failure must reopen the implementation and
product/SLA decision rather than quietly selecting whichever prototype remains.

## Corrections to the earlier evidence

### 1. The public Comrak handle is not a complete semantic editor parser

The extracted `IncrementalDocument` drives Comrak's real block parser, so it
does not introduce a second predictive grammar. That is an important positive.

But the external 11/16/20 microsecond p50/p95/p99 result stops at
`finish_blocks_only`. It does not inline-parse changed leaves or return inline
syntax deltas. Its public restart unit is a top-level safe chunk. The strong
in-container list, table, and fence receipts are separate proof helpers and
tests; they are not integrated into that public handle. Budgets and
cancellation are also absent.

Direct public-handle measurements on approximately 1 MB constructs were:

| Shape | Apply time | Reparsed bytes | What it means |
| --- | ---: | ---: | --- |
| Many ordinary top-level blocks | 78–82 us | 13 | The current fast path is real |
| One fenced block | 5.75–6.46 ms | 1,000,024 | In-container fence checkpoint not integrated |
| One list | 95–110 ms | 1,000,012 | In-container list proof not integrated |
| One table | 70.7–72.9 ms | 1,000,017 | In-container table proof not integrated |
| One paragraph | 269–293 us | 1,000,008 | Misleadingly cheap because inline parsing is omitted |

The 53-existing-line patch metric is also incomplete as a coupling metric.
The extracted 3,316-line module uses Comrak's private parser internals via
`use super::*`; much of it is the earlier proof/test engine moved out of
`parser/mod.rs`. Isolation improves reviewability, but upstream internal drift
can still affect the entire module. One successful 0.50-to-0.54 rehearsal is a
useful receipt, not a lifetime maintenance proof.

### 1a. The narrow Comrak adapter fails the production state/output contract

New probes exposed failures below the public-handle headline:

| Probe | Receipt | Implication |
| --- | ---: | --- |
| Closed list prefix | full `tight=false`, resumed `tight=true` | The cloned open spine omits exact prefix semantics |
| Convergence after tightness edit | identical state hash for `false` → `true` | The current proof can falsely converge |
| 25,000-deep quote checkpoint | 3,400,272 AST bytes for 50,008 source bytes | Checkpoint cost follows arena-node weight, not compact parser state |
| Midpoint of open paragraph | 510,000 retained content bytes and 10,000 line offsets | Giant-leaf checkpoints retain the entire prefix |
| One-word edit in a 1 MB list | 14,141,795 estimated delta bytes | Top-level arena-tree conversion is not a bounded editor delta |
| One-word edit in a 1 MB table | 3,553,017 estimated delta bytes | Nested rows are copied instead of persistently spliced |

The list-prefix failure can be repaired with a small persistent aggregate, and
the prototype already sketches one. The class of failures cannot. The complete
fix requires source-backed immutable block frames, persistent nested fragments,
exact equality-comparable continuation state, and direct parser events instead
of copied arena output. Comrak's inline `Subject` likewise holds arena node
pointers, pointer-linked delimiter storage, brackets containing nodes, and a
whole-leaf resolution phase. Making it resumable requires replacing those with
source cursors, index-based stacks, explicit phase state, and a persistent
chunk sink.

That crosses the ownership-sensitive block and inline core. It is technically
viable, but it is a Comrak-derived Flark parser rather than a surgical fork
that can comfortably track upstream. The useful assets are the grammar
decisions, scanners, tests, and algorithms—not the arena representation. Exact
probe receipts and the minimum redesign are in
[`COMRAK_STATE_OUTPUT_FALSIFICATION.md`](COMRAK_STATE_OUTPUT_FALSIFICATION.md).

### 2. The owned pathological result was an implementation bug

The owned delimiter pass originally retained marker-specific predecessor
links after consuming delimiters. Deep nesting repeatedly walked dead entries,
making 5,000 layers take roughly 158–206 ms.

Maintaining both directions of the active same-marker chain removed the stale
walk. Later adapting cmark's delimiter-run index for code spans both corrected
five examples and changed a 320 KB increasing-backtick case from roughly 19.6
seconds to roughly 0.66–0.79 ms. The trial now reaches 343/652 CommonMark
examples, including 20/22 code-span examples, and parses a 3.13 MB version of
that pathology in roughly 5.95–7.76 ms. This is direct evidence for the “owned
representation, mature algorithms” model.

The opposite result is equally important. The trial's clean-room recursive
block-quote materialization takes roughly 193–202 ms at depth 5,000, while
Comrak takes roughly 0.34 ms. An owned core does not excuse naive algorithms or
copied substrings; every adopted mechanism needs conformance and adversarial
gates.

At 1 MB, clean-parse p50s from the same source generators were:

| Shape | Owned | Comrak | Caveat |
| --- | ---: | ---: | --- |
| Typical mixed document | 30.3 ms | 29.2 ms | Owned omits lists, links, tables, and other semantics, so equality is not a win |
| Giant inline leaf | 19.2 ms | 35.2 ms | Owned is incomplete but has plausible algorithmic headroom |
| Giant fence | 0.43 ms | 0.91 ms | Both cheaply classify inert raw payload |

These results reject “an owned parser is obviously too slow.” They do not
establish production performance or correctness.

### 2a. The target unified transition is mechanically plausible

A research-only slice replaces the earlier conceptual split for a narrow
quote/list/setext/table subset. One iterative line transition emits value-semantic
restart state, source-relative chunks, authoritative ancestry, markers, leaf
identity, list-looseness facts, and retroactive leaf-promotion facts. Stable IDs
are explicitly adopted when a new parse converges on an old suffix.

Twelve focused tests cover the mechanisms in CommonMark examples 232, 242, 250,
259, and 294, nested-list edits, ID adoption, setext promotion, a bounded GFM
table activation, and valid/invalid delimiter toggles. Across the captured
release runs, a local edit in a 910 KB single list reparsed two lines and reused
69,999 chunks in roughly 1.3–4.8 ms. That timing includes `O(n)` temporary
`String`/`Vec` shell work and is not a production latency claim.

The proof is intentionally coarse: table support is one-line-header-only and
not code-span-aware; it does not certify exact HTML for all list/table rules,
HTML close classes, inline brackets/references, mid-line yielding, or persistent
fact-tree integration. It does prove that retroactive interpretation fits the
target representation without a shadow AST or grammar-sensitive side scanner.

This is architectural evidence, not the production seed. The real integrated
machine should inherit the donor's exact grammar rather than expand this
clean-room slice feature by feature.

### 2b. The current owned prototypes fail production resource contracts

Red-team probes found three independent failures:

| Probe | Receipt | Required correction |
| --- | ---: | --- |
| 10 MB one-line edit with a nominal 64-byte budget | 37–48 ms and `Converged` in one call | Sub-line cursors and fuel checks in block/inline scanners |
| 2 MB / one million `a\n` lines in `RevisionedDocument` | 473 MB maximum RSS | Packed/adaptive checkpoints instead of one heavyweight record per line |
| Same-boundary repeated insertion | order-key panic on insertion 307 | Relabeling or tree-native order without finite midpoint exhaustion |

The unified line-record slice still reached 209 MB RSS on that 2 MB dense-line
shape. These are prototype-design failures, not small optimizations. The full
receipts and strengthened kill gates are in
[`OWNED_PROTOTYPE_FALSIFICATION.md`](OWNED_PROTOTYPE_FALSIFICATION.md).

### 3. Global references do not force global parsing

The owned source/checkpoint sequence was extended with source-backed reference
facts, a persistent first-definition-wins occurrence index, and lookup
dependencies. On a 609 KB document containing 20,000 distant uses of one
label, changing the winning definition reparsed under 80 bytes and reported
all 20,000 semantic dependents. Across 1,000 edits, apply time was about 0.21 ms
p50 and 0.66 ms p95 in the captured run; p99 varied around 1.3–2.1 ms.

This proves the index/delta mechanism, including forward definitions, duplicate
winners, missing-to-present transitions, definitions in a quote, and inert
definition-like text in a fence.

It deliberately does **not** pass the owned architecture gate. The temporary
slice scans reference facts beside the batch semantic parser. Shipping that
shape would recreate the dual-authority problem. In a qualifying owned parser,
the single block machine must emit checkpoint state, semantic chunks, and
definition facts together; the inline machine must emit lookup dependencies.

### 4. `markdown-rs` is prior art, not a viable production base

`markdown-rs` initially appeared to be a third path because it exposes an
explicit tokenizer and `push`/`flush`. Inspection shows append-feed resumption,
not arbitrary-edit checkpoints: the tokenizer retains a whole event vector and
the parser repeatedly subtokenizes and rewrites linked events after definitions
are known.

An event-only hook, excluding HTML/mdast compilation, measured roughly 51 ms at
100 KB and 2.86 seconds at 1 MB on a mixed document with references. A 100 KB
unmatched-bracket/image/emphasis case took roughly 0.88 seconds; the 500 KB case
was terminated after more than 20 seconds. The project's own security guidance
recommends an approximately 500 KB cap and a stoppable thread for these shapes.
Retrofitting partitioned events, exact checkpoints, persistent splice, and
budgets would be another invasive parser project starting from a materially
slower base. Use its state-machine ideas and tests as references; do not fund a
runtime integration spike.

## What is decided

The product and parser boundary are now:

1. exact UTF-8 Markdown source is canonical;
2. one Flark-owned, Comrak-derived parser core emits persistent source-relative
   syntax chunks;
3. restart state includes every unresolved structural and semantic dependency;
4. parsing is revisioned, budgeted, cancellable, and resumable;
5. references use symbol indirection and explicit dependency deltas;
6. projection, layout, semantics, and the viewport consume stable-ID deltas;
7. unmodified Comrak remains the canonical clean-parse oracle; cmark/cmark-gfm
   add independent conformance/pathological evidence, never simultaneous
   runtime authorities.

The implementation should preserve Comrak parser/scanner lineage with
attribution while changing the ownership boundary. It should not reproduce an
upstream AST merely to convert it into the values the editor needs.

## Commitment gate

Fund one concentrated Flark-owned, Comrak-derived core lane, in dependency
order. The first exact slice must replace—not supplement—the batch block parser,
checkpoint scanner, and reference side scanner for its supported mechanisms.

### Gate A — exact retroactive block state

- Exact normalized-HTML and source-fact equality for the pinned quote/list
  cases, including lazy continuation, interruption, indentation, tightness,
  and every-revision ambiguity edits.
- Setext and GFM-table activation/interruption in the same transition. These
  force an earlier chunk to be retroactively reclassified.
- All HTML-block open/close classes and inert-body behavior.
- Persistent source/checkpoint/chunk/fact trees, direct local deltas, stable-ID
  adoption, budgets, cancellation, and revision supersession.
- Real sub-line budgets on giant physical lines; no “one line is always
  allowed” escape hatch.
- Packed/adaptive checkpoint density that passes both byte-heavy and
  million-line memory fixtures.
- Stable sequence ordering through at least 10,000 same-boundary inserts and
  100,000 randomized edit histories without relabel failure.
- Checkpoint memory proportional to compact open state plus bounded local
  payload—not source prefix length or arena-node weight.
- Clean equivalence after adversarial edit sequences and independently against
  pinned cmark-gfm/Comrak oracles.

### Gate B — exact resumable inline/reference state

- Source-backed delimiter/bracket arrays and explicit scan/resolve/output
  phases that can yield inside a giant leaf.
- Code spans, emphasis/strong, links/images, raw inline HTML, escapes/entities,
  autolinks, and their precedence from the pinned manifest.
- First-definition-wins facts and lookup dependencies emitted by these real
  block/inline machines, with no grammar-sensitive side scan.
- Semantics invariant across work budgets, checkpoint density, cancellation,
  edit history, native/WASM, and clean parse.
- Pathological inputs from cmark/Comrak plus generated unmatched/nesting cases
  guarded by explicit work and memory ceilings.

Stop the lane if either gate requires a second grammar consumer, a general
mutable AST, whole-document event rewriting, global descendant rewrites, or
unbounded work between cancellation checks. Passing a syntax score while
violating those properties is a failure.

Only after both gates pass should the remaining Comrak parser/scanner sections
be adapted and certified section-by-section against full CommonMark 0.31.2 and
the fixed five-extension GFM profile. This is donor refactoring, not clean-room
feature accumulation. Product layers may integrate only the
implementation-neutral parser-service/delta contract until then.

## Maintenance model

“Owned” means owning the product-shaped state machine, not severing the parser
lineage:

- Pin and version the exact CommonMark/GFM/editor-fact profile. Syntax changes
  are explicit product changes, not incidental upstream upgrades.
- Use Comrak as the canonical code donor. Preserve near-function correspondence
  for block, delimiter, bracket, scanner, and extension algorithms with a
  function-level provenance/upstream-diff ledger and required notices.
- Keep an unmodified pinned Comrak executable as the clean-parse differential
  oracle and cmark-gfm as independent evidence in CI; never link either AST into
  the live runtime.
- Import their conformance, fuzz-regression, and pathological-complexity cases,
  then add every-revision edit and source-fact assertions they do not provide.
- Expand by complete spec sections and fail closed on unsupported profile
  versions; do not grow a collection of literal-fallback “passes.”
- Use cmark/cmark-gfm as independent algorithm/corpus references, not a second
  implementation lineage mixed ad hoc into production code.
- Monitor upstream semantic/security fixes and port relevant Comrak changes under
  dedicated differential changes instead of rebasing private parser internals.

Keep v2/Comrak available during the gated build and compatibility migration.
Do not delete it until the owned core has full profile parity, native/WASM
parity, inherited editor-flow parity, and the launch performance/device gates.

## Confidence

- High: whole-document parsing per keystroke and a prediction grammar are not
  acceptable for the 10 MB contract.
- High: the parser-service, persistent-source, delta, scheduling, and viewport
  architecture is the right product model.
- High: the current “surgical” Comrak fork is the wrong lifetime representation
  for the unchanged oversized-container/leaf contract.
- High: `markdown-rs` is not a better retrofit base for that contract.
- Moderate-high: a Flark-owned persistent core with algorithms selected per
  seam is the best implementation direction and deserves one integrated
  commitment slice. Pulldown currently leads the inline seam; no donor has
  earned the complete core.
- Moderate: the packed, restart, real-grammar, reference, and persistent-delta
  mechanisms will compose while clearing the exact unified block/inline gates.
  This integration, rather than another isolated mechanism, is the deliberate
  falsification point.

This is enough confidence to stop parallel donor/representation exploration and
choose the integrated gate, without pretending that separate prototypes have
already earned production authority.
