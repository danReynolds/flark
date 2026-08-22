# Spec-first owned parser trial results

Status: active corrected receipt, 2026-07-14. This evaluates the execution plan
in [`OWNED_PARSER_SPEC_TRIAL.md`](OWNED_PARSER_SPEC_TRIAL.md). The code is
temporary research under [`owned_parser/`](owned_parser/), not a package
implementation.

## Decision

**The earlier stop decision is withdrawn. The funded direction is a
Flark-owned, Comrak-derived persistent core, behind the
implementation-neutral service contract and subject to exact unified
block/inline commitment gates.**

The original receipt stopped after the foundation plus one inline slice even
though the declared architecture-stress milestone had not been implemented.
It treated missing list/HTML/link/GFM features and an implementation bug in the
delimiter chain as proof that the architecture failed. It also compared those
results with a Comrak public-handle benchmark that omits inline parsing and
does not integrate the separate in-container checkpoint proofs.

The direction has not earned shipping authority. Its batch semantic parser and
checkpoint machine are still separate consumers, and the reference-index slice
deliberately uses a side fact scanner. Exact list/HTML/table boundaries, inline
brackets, direct semantic chunks, and giant-leaf resumption remain the decisive
work.

See [`PARSER_AUTHORITY_REASSESSMENT.md`](PARSER_AUTHORITY_REASSESSMENT.md) for
the current decision and stop conditions. The clean-room code below remains
throwaway architecture evidence, not a production grammar seed. The production
core should preserve Comrak parser/scanner lineage while replacing its arena,
state, work, and output model; the current surgical adapter is not the lifetime
architecture.

## What was built

The standalone Rust crate has no production Markdown-parser dependency. It now
contains:

- a pinned CommonMark 0.31.2 fixture harness and exact test renderer;
- a 61-example foundation covering six complete spec sections;
- total, non-overlapping UTF-8 source coverage;
- a persistent `Arc` source rope with bounded leaves;
- a persistent checkpoint sequence with structural suffix reuse;
- revision/hash validation, UTF-8 boundary checks, budgets, cancellation, and
  resumable edit sessions;
- one shared block-recognition module used by clean and incremental paths;
- paragraph and setext dependency digests that prevent false convergence
  inside an unresolved semantic leaf;
- a linked delimiter pass for emphasis/strong, including the rule of three;
- a cmark-derived delimiter-run index for bounded code-span matching;
- a source-backed first-definition-wins reference/dependency index slice;
- one research-only unified quote/list/setext/table transition emitting
  checkpoint state, source-relative chunks, ancestry, markers, IDs, list facts,
  and retroactive promotion facts together;
- pinned whole-suite and adversarial scorecards; and
- full-parse and pathological benchmark binaries.

The trial is approximately 4,342 lines of library code, including the coarse
unified transition research slice, but still before exact lists, complete HTML,
links/references, GFM extensions, production persistent semantic chunks, or the
wire protocol. Test and benchmark code is separate.

## Receipts

### Conformance

| Receipt | Result |
| --- | ---: |
| Foundation gate | 61/61 exact examples |
| Whole CommonMark 0.31.2 scorecard | 343/652 |
| Emphasis/strong section | 120/132 |
| Code spans section | 20/22 |
| Pinned architecture stress manifest | 8/30 |

The aggregate 343/652 score should not be mistaken for product coverage. Some
literal fallbacks happen to render correctly. The missing mechanisms remain
large: HTML blocks are 0/44, lists/list items are 9/74 combined, and links plus
reference definitions are 20/117 combined. The stress manifest fails nested
containers, long-lived HTML, references, links, tables, tasks, strikethrough,
and extended autolinks.

The emphasis slice is the most encouraging result. Replacing the initial
recursive matcher with a spec-shaped delimiter pass raised the section from
49/132 to 120/132. Eleven remaining failures depend on unimplemented
links/raw-HTML precedence; one requires the spec's Unicode `P`/`S` general
categories. This confirms that the normative spec is sufficient to implement
the grammar.

The code-span slice demonstrates the intended implementation model. Correcting
longer-run matching initially exposed roughly 19.6 seconds of work on a 320 KB
increasing-backtick input. Adapting cmark's run-length index reduced that case
to roughly 0.66–0.79 ms, a 3.13 MB version to roughly 5.95–7.76 ms, and raised
the section to 20/22. The algorithm is attributed in
`THIRD_PARTY_NOTICES.md`.

### Incremental correctness

The initial checkpoint model was wrong even though its first locality tests
passed. It could converge at the second unchanged line of a changed multiline
paragraph because `Paragraph` state did not include unresolved inline input.
It could likewise retain a setext underline whose heading text had changed.

Adding a rolling paragraph digest and a one-checkpoint setext closure state
fixed those cases. Current tests also show:

- a local edit among 20,000 one-line paragraphs reparses at most four lines and
  reuses more than 30,000 line checkpoints;
- a payload edit inside a roughly 1 MB fence reparses less than 160 bytes;
- changing that fence's opener correctly invalidates more than 900 KB; and
- sessions yield to tiny work budgets, cancel, reject stale revisions/hashes,
  and match a clean checkpoint scan after adoption.

These are useful algorithms, but they are not yet incremental semantic parsing.
The candidate has no persistent semantic chunk tree or direct syntax delta.
Its batch parser and checkpoint consumer share recognition rules but are still
separate structural consumers. It therefore fails the trial's one-state-model
and direct-persistent-delta criteria today.

A follow-up added source-backed reference facts to the persistent checkpoint
sequence. Changing the winning definition in a roughly 609 KB document with
20,000 distant lookups reparsed under 80 bytes and reported every dependent;
1,000 repeated edits measured about 0.21 ms p50 and 0.66 ms p95 in the captured
run. Forward, missing-to-present, duplicate-winner, quoted-definition, and
fenced-inert cases pass. This proves the symbol/dependency data structure, but
the temporary fact scanner is beside the semantic parser and therefore does
not clear the one-state-model gate.

A second follow-up adds a deliberately narrow unified
quote/list/setext/table transition. The same iterative line step emits restart
state, source-relative chunks, ancestry, markers, stable leaf/container
identities, list-looseness facts, and retroactive leaf-promotion facts. Twelve
focused tests cover the mechanisms in CommonMark examples 232, 242, 250, 259,
and 294, incremental ID adoption, setext promotion, bounded one-line-header GFM
table activation, and valid/invalid delimiter toggles. A local edit in one
910 KB list reparsed two lines and reused 69,999 chunks. The shell still clones
a `String`, scans/rebuilds `Vec`s, and asserts structure rather than full HTML
conformance, so its approximately 1.3–4.8 ms captured host results are not a
production latency claim.

### Performance and representation

Release-mode host samples from this machine:

| Shape | Owned | Comrak | Interpretation |
| --- | ---: | ---: | --- |
| 100 KB typical, p50 | 2.88 ms | 2.81 ms | Not comparable coverage; owned omits major constructs |
| 100 KB inline-heavy, p50 | 1.21 ms | 2.23 ms | Owned has grammar headroom, not a feature-complete win |
| 1 MB typical, p50 | 30.3 ms | 29.2 ms | Similar throughput is encouraging but not semantic parity |
| 1 MB inline-heavy, p50 | 19.2 ms | 35.2 ms | Incomplete owned grammar remains the caveat |
| 1 MB fence, p50 | 0.43 ms | 0.91 ms | Both make inert raw blocks cheap |
| 5,000 nested emphasis layers, 70 KB | about 2.9–4.0 ms | about 5.9 ms | Corrected active delimiter links remove the prior gap |
| Increasing backtick runs, 320 KB | about 0.66–0.79 ms | about 0.77 ms | Owned adapts cmark's bounded run index |
| 5,000 nested block quotes | about 193–202 ms | about 0.34 ms | Current recursive owned block materialization is unacceptable |

The first emphasis implementation scored well but took roughly 2.6 seconds on
only 100 KB because it rebuilt a token vector per match. Replacing it with a
linked delimiter structure removed that quadratic behavior. A second audit
found that unmatched delimiters produced hundreds of thousands of text nodes;
adjacent-text coalescing reduced the 1 MB case to one root inline. The apparent
deep-nesting gap was then localized to stale same-marker predecessor links.
Maintaining the active chain in both directions fixed the complexity without a
conformance regression. The prior roughly 33x-gap conclusion was wrong.

These corrections are not arguments against systems work. They are evidence
that a complete grammar core includes substantial algorithmic and
representation hardening beyond passing examples. Comrak already carries that
history.

The current owned AST also copies code literals and materializes/remaps block
quote interiors. A production design needs persistent source-backed views,
relative chunk coordinates, iterative ownership/rendering for deep trees, and
stable semantic IDs. Those improvements are possible. The Comrak incremental
layer needs the same editor-facing representation; its public handle has
proved stable block chunks but not integrated inline semantic chunks or
bounded giant-container behavior.

### Resource-contract falsification

Separate red-team probes reject the current crate as a production seed even
though its target representation remains useful:

| Probe | Result | Production requirement |
| --- | ---: | --- |
| Nominal one-line/64-byte budget on a 10 MB line | 37–48 ms and `Converged` in one call | Sub-line cursors and fuel checks |
| 2 MB / one million `a\n` lines in `RevisionedDocument` | 473 MB maximum RSS | Packed/adaptive checkpoints |
| Same input in the unified line-record slice | 209 MB maximum RSS | Density bounded by bytes and records |
| Repeated insertion at one boundary | order-key panic on insertion 307 | Relabelable or tree-native order |

These are architecture-level failures in the prototype mechanisms, not tuning
items. Exact receipts and the resulting kill gates are in
[`OWNED_PROTOTYPE_FALSIFICATION.md`](OWNED_PROTOTYPE_FALSIFICATION.md).

## Shared shell, potentially meaningful parser-core difference

The promising v3 architecture survives the parser choice:

1. persistent indexed source is canonical;
2. a single authoritative parser owns grammar;
3. restart state contains every unresolved semantic dependency;
4. parsing yields under explicit budgets and cancellation;
5. persistent syntax, reference, projection, and layout indexes consume local
   deltas; and
6. clean full parsing remains the differential oracle.

A Flark-owned, Comrak-derived core can make block/container and delimiter stacks
native to the same checkpoint and persistent-chunk machine while retaining the
donor's mature grammar lineage. The current arena-backed adapter instead mirrors
state into editor chunks and must maintain that seam across upstream changes.
That difference is material for giant containers and inline leaves, but the
target core still has to prove it can preserve donor semantics through the new
representation.

The selected lane still requires interned references, budgets, cancellation,
protocol, native/WASM differential CI, and complete grammar hardening. The
commitment gates exist to falsify its one-machine advantage before broad
feature accumulation.

## What to carry forward

- Keep the exact spec pins, profile, manifests, and scorecards as independent
  oracle infrastructure.
- Require the explicit budget/cancellation/session contract from every parser
  mechanism.
- Preserve the paragraph/setext false-convergence cases as mandatory
  differentials.
- Add Comrak/cmark pathological inputs to release CI, including unmatched and
  deeply nested delimiters, giant containers, references, tables, and HTML.
- Keep the persistent source/chunk/reference/delta contract independent of
  either parser's internal AST.
- Keep pinned Comrak/cmark-gfm oracles in CI without making their ASTs runtime
  state.
- Continue the Comrak-derived core lane only through the timeboxed,
  manifest-pinned stress gate. Do not grow the clean-room trial feature by
  feature into the production grammar.

## Reproduction

```sh
cd tool/parser_research/owned_parser
cargo test --release -- --nocapture
cargo run --release --bin owned_parser_bench
cargo run --release --bin owned_parser_pathological
cargo run --release --bin owned_parser_nested -- 5000
cargo run --release --bin owned_reference_slice

cd ..
cargo run --release --bin full_parser_bakeoff
cargo run --release --bin pathological_blocks
```
