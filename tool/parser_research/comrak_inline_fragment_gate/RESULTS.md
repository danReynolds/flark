# Inline-fragment gate results

## Decision

**Conditional GO for the real bounded inline hook; not yet a GO for the whole
editor architecture.** The prototype proves that a certified paragraph,
heading, list-item paragraph, or table-cell leaf can use Comrak's definitive
inline grammar without invoking its block parser or maintaining a predictive
marker grammar. It also proves exact source projection is possible with a
targeted parser annotation patch.

The remaining architecture decision depends on the separate exact block-spine
gate. Physical-device native/WASM tails, the scheduler contract, large-leaf
policy, and retained fact density remain launch gates.

## Resulting contract

1. The block spine owns block boundaries, definition precedence, leaf kind,
   logical bytes, and segmented logical-to-physical origin mapping.
2. The inline service runs a real Comrak `Subject` directly. It never calls
   `parse_document` and never predicts emphasis, code, link, entity, or escape
   semantics itself.
3. Parser-owned annotations emit hidden-marker, replacement, exact link-span,
   and reference-query facts while the definitive parse decisions happen.
4. Semantic and projection ranges are leaf-logical. A separate segmented
   origin map handles stripped quote/list prefixes, CRLF atoms, and virtual or
   replacement runs. A single physical start/end pair was falsified because it
   incorrectly claimed stripped prefix gaps inside multiline spans.
5. Text facts are source-backed: range plus a flag, with no copied literal.
   Entity/escape normalization is reconstructed through projection facts.
   Code literals, direct-link destinations, HTML literals, and explicit
   replacements remain owned where source slicing is not yet the proven wire
   contract.
6. Reference Link/Image facts carry an 8-byte stable document symbol ID, not a
   cloned URL/title. The resolver exposes only symbol ID, defined state, and a
   presence generation. Value-only winner changes update the symbol table and
   do not invalidate leaf structure; undefined/defined transitions do.
7. Snapshot root identity/generation is diagnostic only. It must not be used as
   a global leaf-staleness key.
8. Every result is revision-tagged. Stale work is rejected before arena setup,
   and stale completed work can be discarded at the worker boundary.
9. Task-list recognition is a post-inline operation with a block certificate:
   the leaf must be the first paragraph child of an Item under a List. List
   tightness controls HTML paragraph wrappers only; it is not part of the
   semantic certificate. The service then requires the first parsed child to
   remain Text, uses Comrak's generated strict task scanner, and emits both an
   exact-symbol semantic fact and a full-prefix hidden-marker projection.

`logical` includes the block-spine-owned inline content, all interior line
endings, and optionally the terminal line ending/trailing spaces accumulated by
the block parser. The service mirrors Comrak's `rtrim`: a terminal suffix emits
no break or marker fact; an interior CRLF remains a two-byte logical range.

## Assumptions that did not survive

- **An 8 KiB cap is not a proven launch constant.** It is only a candidate
  urgent-path ceiling. The 16/64 KiB measurements support a worker/background
  tier, not permanently plain rendering for a 9 KiB paragraph.
- **Reference URLs cannot live in leaf facts.** That would make value-only
  definition edits rewrite every dependent leaf and could clone a multi-MiB
  definition through a tiny leaf. The parser now sees only bounded dummy values
  for grammar and the wire stores symbol IDs.
- **Text payloads should not duplicate the document.** Source-backed Text is
  exact against the tested corpus and materially lowers retained output.
- **Delimiter markers were not correct merely because curated examples
  passed.** Source-backed reconstruction exposed `*b***b*` hiding its literal
  middle `*`. Tracking residual delimiter source consumption fixed it.
- **Per-allocation GFM post-processing was not donor-equivalent.** The complete
  scorecard exposed GFM example 631 (`a.b-c_d@a.b`): underscore processing
  split the address, so only its tail autolinked. The facade now mirrors donor
  adjacent-Text coalescing, source-run accounting, and bracket-context
  suppression before email autolinking.
- **The stock full-parser path was not automatically safe.** The first doctest
  run found an annotations-disabled smart-quotes underflow caused by using the
  UTF-8 replacement length as source length. Passing the scanner's source run
  length fixed it; the complete upstream and pristine differential lanes now
  pass.
- **Raw source scanning is not donor-equivalent for task markers.** Comrak
  recognizes decoded whitespace in `[ ]&NewLine;` and `[x]&Tab;done`, while a
  byte scanner sees `&`; conversely, a resolved `[x]` reference or escaped
  `\[x]` opener must win before task recognition. Scanning after the real
  inline parse preserves both rules.
- **A synthetic List/Item/Paragraph AST is unnecessary.** It reproduced the
  donor's block mutation but added a second miniature structural path. The
  selected profile carries one block certificate and reuses only the donor's
  generated scanner after inline parsing.
- **List tightness is not task context.** A loose task item's checkbox remains
  outside its first `<p>`, while the task decision is still made from that
  paragraph. A focused exact differential now covers tight, loose, mixed,
  nested, non-paragraph-first, and second-paragraph cases.
- **Post-processing must be observational on a miss.** Adjacent Text is
  coalesced for donor-equivalent scanning. The full GFM scorecard caught an
  unmatched-backtick regression where the expanded source position was not
  persisted after a task miss; the implementation and a task-context equality
  regression now preserve every byte.

## Correctness receipts

- Inline gate: 20/20 focused tests.
- Task-list phase gate: 8/8 focused tests, including strict/relaxed boundaries,
  entities, references, escapes, CRLF/tabs/Unicode origins, incomplete typing,
  and unmatched adjacent-Text preservation.
- The adjacent value-block renderer has an exact six-case Comrak differential
  proving first-item-paragraph certification independently of list tightness.
- Selected product learning-record lanes are 47/47: curated fixture loading,
  the Markdown feature matrix, task-marker auto-spacing, live activation
  transitions, and ordered/task block-exit behavior.
- 603 filtered upstream CommonMark/GFM single-paragraph fixtures exactly match
  stock Comrak semantic nodes, ranges, and reconstructed payloads.
- 484 exhaustive syntax-atom combinations match stock Comrak.
- GFM example 631 is an explicit adjacent-Text email-autolink regression.
- 1,322 complete upstream fixture documents produce byte-identical HTML and
  CommonMark XML (AST/source positions) between patched and pristine Comrak
  with annotations disabled.
- Upstream default-feature unit suite: 649 passed, 1 pre-existing ignored.
- Upstream default-feature doctests: 203 passed.
- Upstream no-default-feature unit suite: 488 passed, 1 pre-existing ignored.
- Upstream no-default-feature doctests: 67 passed.
- Strict Clippy passes for patched Comrak, the harness, bins/tests, and WASM
  probe.
- A 100,000-definition snapshot resolves a local reference with one lookup.
- A 10-byte reference leaf backed by a 4 MiB URL plus 4 MiB title does one
  presence lookup, emits an 8-byte symbol payload, and stays under 256 output
  bytes. Definition values cannot cross the resolver API.
- Hit and miss dependencies are deduplicated and preserve stable symbol ID,
  normalized-label collision guard, defined bit, and presence generation.
- Terminal LF/CRLF/hardbreak suffix and interior LF/CRLF handoff cases are
  explicit tests.

## Hot-leaf performance

Release builds, Apple Silicon workstation, 2,000 samples after 100 warmups.
P50 was stable; wall-clock P99 varied while the workstation was heavily loaded,
so both a representative low-contention result and the repeated range are
shown. These are feasibility receipts, not device acceptance numbers.

| backend | dense leaf | p50 | latest scripted p99 | repeated p99 range |
|---|---:|---:|---:|---:|
| native | 1 KiB | 46 us | 78 us | 0.08-0.33 ms |
| native | 4 KiB | 187 us | 820 us | 0.52-4.82 ms |
| native | 8 KiB | 377 us | 1.21 ms | 0.94-20.76 ms |
| raw WASM/Node | 1 KiB | 55 us | 162 us | 0.16-0.64 ms |
| raw WASM/Node | 4 KiB | 222 us | 1.03 ms | 0.87-1.18 ms |
| raw WASM/Node | 8 KiB | 439 us | 1.51 ms | 1.13-3.08 ms |

The WASM linear memory after each dense run was about 1.4, 1.8, and 2.4 MiB.
Node RSS (roughly 40-57 MiB) is mostly runtime baseline and is not the parser's
retained memory.

### Research-only 16/64 KiB direct-inline results

| shape | size | p50 | repeated p99 range | compact output |
|---|---:|---:|---:|---:|
| plain prose | 16 KiB | 102-109 us | 0.14-0.68 ms | 15.3 KiB |
| plain prose | 64 KiB | 412-417 us | 0.80-1.04 ms | 61.0 KiB |
| ordinary Markdown | 16 KiB | 515-546 us | 1.79-7.52 ms | 111.9 KiB |
| ordinary Markdown | 64 KiB | 2.23-2.25 ms | 6.64-18.48 ms | 448.7 KiB |
| dense Markdown | 16 KiB | 734-738 us | 1.32-1.68 ms | 189.1 KiB |
| dense Markdown | 64 KiB | 3.06-3.12 ms | 5.72-6.99 ms | 757.5 KiB |

Syntax density, not byte count alone, controls cost. A 64 KiB prose paragraph
is plausible as exact background work; 64 KiB ordinary/dense Markdown is not an
urgent synchronous operation.

## Cold streamed 10 MiB document

The document contains 106,997 distinct 96-byte ordinary Markdown paragraphs.
Each leaf is parsed once with a fresh arena; each compact result is consumed and
dropped immediately.

- 10,271,712 inline bytes parsed in 366 ms: 26.8 MiB/s and 292,565 leaves/s.
- First cold leaf: 134 us in this run.
- 6,526,817 allocations and deallocations: about 61 calls/leaf.
- 1.31 GB cumulative allocator traffic: about 12.3 KiB/leaf, but only 10,091
  bytes maximum transient live heap above baseline.
- Retained live delta after all 106,997 leaves: exactly 0 bytes.
- Current RSS moved from 12.04 to 12.62 MiB; max RSS was 12.62 MiB.

The arena/Subject fixed cost is not dominant. On the same 10 MiB plain-prose
document, 96/512/2048-byte leaves achieved 97/106/131 MiB/s. A rough linear fit
puts fixed per-leaf cost near 0.26 us (about 27% of a 96-byte plain leaf). Reuse
of `typed_arena::Arena` is unsafe as an optimization because it has no reset and
would retain ASTs across leaves. Safe batching can amortize worker-message and
scheduler overhead while still creating/dropping one arena per leaf; an arena
pool should wait for evidence and a reset-with-no-retention proof.

The high cumulative allocator traffic is still a performance risk worth
profiling on floor devices.

## Source-backed Text retention result

10 MiB, 106,997 96-byte leaves, retaining every compact leaf:

| corpus | wire | protocol output | retained live heap over baseline | current RSS |
|---|---|---:|---:|---:|
| plain | owned Text | 16.69 MB | 40.32 MB | 54.23 MB |
| plain | source-backed Text | 6.42 MB | 25.34 MB | 36.91-36.93 MB |
| ordinary Markdown | owned Text | 63.34 MB | 112.65 MB | 81.49-119.75 MB |
| ordinary Markdown | source-backed Text | 54.89 MB | 104.09 MB | 60.42-117.85 MB |

Source backing is clearly preferable, but it is not enough. Retaining all
ordinary-Markdown leaf facts still costs 54.9 MB of protocol data for a 10 MiB
document because 2.67 million fixed-width 20-byte facts dominate. The product
architecture should avoid eager full-document retention (visible window plus
bounded cache/lazy exact parsing) and investigate a denser SoA/delta/varint wire
before claiming 100 MiB document readiness.

Protocol and counted live-heap deltas were deterministic across repetitions;
macOS resident-set accounting varied substantially for the ordinary retained
case because allocator pages were committed/reclaimed differently. RSS is
reported rather than hidden, but it is not used as the comparative verdict.

## Scheduler contract

The Comrak call is bounded but atomic. It cannot honor the old Gate B rule that
scan, resolve, emit, and finalize poll every 4 KiB. An 8 KiB leaf can therefore
cross that rule by 4 KiB; **Gate B as written does not pass**.

The viable contract is:

1. Preflight revision, leaf kind, input ceiling, protocol ceilings, and worker
   cancellation before arena allocation.
2. Charge and grant the whole atomic kernel before entering it, using the leaf
   size/tier and conservative measured cost. Cancellation occurs before/after;
   a revision mismatch discards completion.
3. If the scheduler cannot grant more than 4 KiB atomically, lower the urgent
   cap to 4 KiB. Do not describe an 8 KiB call as cooperative streaming.
4. Run the atomic kernel only on a dedicated native isolate or Web Worker, so a
   scheduling tail cannot block Flutter/web UI frames.
5. Treat 8 KiB as a candidate urgent-worker tier, 16-64 KiB as fully charged
   background exact work, and larger pathological leaves as source-visible,
   cancellable deferred work with a separately calibrated final cap.

The p50/sub-ms receipts make this model plausible. Floor-device native/WASM
P99, typing-through-backlog behavior, and stale-result pressure remain launch
gates.

## Patch and maintenance inventory

The isolated patch against pristine Comrak 0.54.0 is
`patches/comrak-inline-fragment-0.54.0.patch`:

- 6 files, 1,132 insertions, 10 deletions.
- 926 lines are the new isolated service/wire wrapper.
- `parser/inlines.rs` is the sensitive existing-file change: 200 insertions and
  9 deletions for opt-in annotations, exact residual source spans, and the
  inline-only reference resolver.
- The remaining changes are two module/export lines and research feature lines
  in `Cargo.toml` plus its upstream `Cargo.toml.orig` source.
- No block parser or table parser change belongs to the inline patch. The
  shared vendor checkout contains unrelated block-facade work, which the patch
  intentionally excludes.

The selected task phase changed the previous 775-line wrapper by 167 additions
and 16 deletions (net +151). The rejected synthetic block-mutation variant was
940 lines and would have made the isolated patch 1,146 insertions/10 deletions.
The selected profile is therefore smaller, but line count is not the deciding
reason: it avoids synthesizing List/Item structure and keeps block ownership in
the spine. The raw pre-inline scanner was rejected for semantic inaccuracy,
not size.

A clean pristine checkout accepted the patch with `git apply --check`, applied
without fuzz, and passed `cargo check --lib --no-default-features`. The patch is
larger than the original “tiny surgical hook” hope. Maintenance is acceptable
only with version-pinned patch-application CI, complete upstream unit/doctest
lanes, the pristine differential, and feature-gating so annotations remain off
for ordinary Comrak users. Upstreaming a general inline-parser/source-event API
would be preferable to carrying this indefinitely.

## What remains

- The exact block-spine prototype must prove block boundary and definition
  invalidation semantics independently.
- Run native and WASM tests on floor physical devices under realistic UI and
  worker contention; workstation P99 volatility is not a launch receipt.
- Calibrate urgent/background/final leaf ceilings from a real corpus. Over-cap
  content must stay visible and eventually exact, not silently become plain.
- Prototype visible/lazy compact-fact retention and a denser fact wire; full
  eager retention is currently too expensive.
- Measure end-to-end edit-to-render latency, stale-work cancellation, worker
  queue collapse, and Flutter application cost. Parser time alone is not UX.
- Replace the stateless empty-snapshot hash in production with a collision-free
  document symbol interner; normalized labels remain a collision guard in this
  prototype.
