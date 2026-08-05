# Source-bound projection composer contract

Status: **candidate production seam under executable falsification**, 2026-07-16.

This contract connects the parser-site source ledger, projection codec,
resumable arena build, and convergence/adoption protocol. It is deliberately
stricter than the current public mechanism types in `v3_runtime_slice`.

## Current verdict

The unified `SourceProjectionRun` model remains selected: one source-ordered
record owns exact physical source and independently describes its contribution
to a terminal's logical input. The streaming Program encoder has proved exact
typed transforms, bounded 4 KiB payloads, deterministic output, virtual
attachment, Unicode metrics, and zero-logical Programs. The follow-up 7/7
`SourceStore`-lineage suite now also supports flat stable-reset locality; no
current receipt requires a nested persistent Program sequence.

The production-shaped flat-reset slice now also survives its first storage and
authority falsifiers. Packed schema v6 assigns one canonical `0x08` bit after
the exact physical run; v5 and unknown descriptor bits fail closed. A typed
join accepts only a source-bound sealed run plus matching parser permit,
poisons after any consumed-input failure, and keeps `SemanticEnvelopeEnd`
distinct. A restart-role storage mechanism can find the preceding reset from
an explicitly biased adjacent-Coverage observation without a global reset
table or a structural open-path reconstruction.

Three production claims remain on hold:

1. raw metrics, IDs, Programs, chunks, and generic `finish()` calls remain in
   mechanism/test surfaces, and the source-bound composer does not yet sink
   its sealed capability directly into the arena writer;
2. the current composer flushes only on an envelope-key change or EOF. A
   parser-safe `flush_at_reset` must be owned by that same composer, including
   its pending-Virtual/finality state; the separate post-seal join is only a
   mechanism proof; and
3. the persistent checkpoint index, exact parser/open/projection continuation,
   and exact retained-suffix splice are not integrated end to end.

Pure greedy maximum fill remains rejected as the incremental partition
authority: a one-byte insertion in a 200,000-byte indivisible transform stream
leaves zero reusable greedy suffix pages. The real-lineage reset model repairs
that failure with bounded local split/merge. Over 24 repeated 1 KiB prefix
inserts it changed at most 2 groups/19 pages; a 33,768-byte deletion across
four resets changed 1 group/8 pages; and a byte/UTF-16-divergent Unicode edit
changed 1 group/4 pages. Dense NUL/tab/CRLF growth, underflow/replenishment,
both exact-reset affinities, and CRLF-invalidated resets also pass in debug and
release. Full receipts are in `PROJECTION_SOURCE_RESET_RESULTS.md`.

## Exclusive construction rule

No production API accepts any of these as authority:

```text
SerializedMetric
ProjectionPiece
ProjectionProgram
ProjectionChunk
LogicalContribution
CoverageId
BlockId
source offsets or a source descriptor
```

They are codec data or proof-harness values. Only a candidate-owned composer
may turn exact source into retained projection output:

```text
CandidateWriter
  LiveCandidateEpoch
  BoundSourceCursor
  ArenaBuildTicket
  CandidateSourceLedger
  OpenBinding stack
  ProjectionComposer
      one ProjectionPageEncoder
      one typed physical tail packet
      optional pending Virtual
      stable-reset continuation
```

The eventual Rust visibility must reflect this rule. Raw constructors remain
available only to an explicit mechanism-test feature or in-module tests.
Runtime code receives query values and source-bound writer operations, not a
way to manufacture a retained Program or `SourceProjectionRun`.

## Source-authoritative claims

One parser claim starts from a non-cloneable source-range capability minted by
the active candidate's exact source cursor. The composer derives byte and
UTF-16 metrics while validating that capability. Parser code never supplies
the aggregate.

Typed physical recipes validate their exact source bytes before storage:

| Recipe | Required physical source | Logical result |
| --- | --- | --- |
| Identity | a nonempty UTF-8 scalar-bounded span | the same scalar sequence |
| Hidden | a nonempty UTF-8 scalar-bounded span | empty, with explicit affinity |
| Tab | exactly `\t` | 1--4 spaces from certified parser column state |
| CRLF | exactly `\r\n` | one LF |
| Lone CR | exactly `\r` | one LF |
| NUL | exactly byte zero in the valid Rust string | U+FFFD |
| Virtual LF | no physical bytes | one LF, attached by an authorized envelope boundary |

Physical ownership and logical action remain orthogonal. A `TERMINAL` does not
imply `None`; a continued CRLF can be `CONTENT` plus an atomic canonical
newline, while a paragraph's final LF is `TERMINAL` plus `None`.

Finishing a claim consumes a fresh build-scoped coverage permit. Emitting an
Enter consumes a fresh block permit and returns an open binding. A logical
action must name a compatible binding from the same candidate epoch; scalar
IDs cannot substitute for either capability.

## Two distinct boundaries

The generic chunker currently has one `finish()` operation. Production needs
two non-interchangeable capabilities:

- `ProjectionResetCapability`: a source-proven safe storage reset inside an
  open semantic envelope; and
- `SemanticEnvelopeEnd`: parser authority that the logical consumer really
  ends at this boundary.

`flush_at_reset(reset)` may seal a bounded page or reset group, but it must not
apply EOF rules. A pending interior Virtual stays right-biased and travels
with the following physical packet. This operation must enter the same
`SourceBoundProjectionComposer` that classified and sealed the run; a parallel
join with a manually mirrored pending-Virtual bit is not an admissible runtime
architecture. Any validation failure after consuming a sealed run or parser
permit terminally poisons that candidate's composer/writer lane.

`finish_envelope(end)` consumes the semantic end capability. Only this
operation may attach a trailing Virtual leftward to the preceding physical
packet. An arbitrary poll boundary, page limit, line lease, or caller-supplied
metric cannot become an envelope end.

This distinction makes virtual attachment independent of scheduling and
prevents clean and incremental paths from assigning the same synthesized byte
to different coverage identities.

## Stable-reset large-envelope algorithm

Greedy 4 KiB fill remains the within-group packing mechanism. It is not the
incremental partition authority.

### Clean construction

1. Stream typed pieces directly into the one-page encoder.
2. Seal pages as they fill; sink each page immediately into the active arena
   build rather than retaining a vector of page payloads.
3. After a bounded number of encoded pages, mark the next safe physical packet
   boundary as a stable reset.
4. Store the reset in the persistent source-ordered output itself. It is a
   source-relative capability, not an absolute offset table or a second source
   identity system.
5. Replenish resets in newly built regions so no retained reset interval grows
   without a configured encoded-page bound.

Direct Identity/Hidden/Atomic runs remain compact. Program schema version 2 is
required for physically anchored, zero-logical compound Programs.

### Incremental construction

1. A stored restart checkpoint supplies its exact event-side parser cut and an
   explicitly biased adjacent-Coverage anchor. From that anchor, scan a bounded
   number of predecessor leaves for the prior stable reset using the immutable
   base output. Do not enter through the generic green cursor: reconstructing
   its open path can scale with Markdown nesting.
2. Use source-store lineage to map the first eligible old reset after the edit
   and its complete retained tail. One bundled storage proof maps the boundary
   and suffix; the parser does not map every page individually.
3. Resume the persisted parser, ledger, open-binding, and projection
   continuation at the preceding reset. A source coordinate or adjacent
   Coverage observation cannot reconstruct that state because zero-metric
   Enter/Exit events make the event side ambiguous.
4. Rebuild bounded pages until an eligible mapped reset is reached.
5. Compare the complete parser/projection continuation required at that reset,
   not only bytes or a payload hash.
6. If it converges, the storage transaction splices the newly built group and
   retains the exact old suffix pages and coverage IDs wholesale.
7. If it does not converge, continue to the next reset under explicit fuel.

There is no production operation that maps all retained resets after an edit.
The executable probe does that only as a reuse-measurement oracle. A global
reset vector would turn a prefix edit into O(document groups) work even though
the resulting page identities look local. Production resolves the bounded
restart/convergence/tail capabilities, splices changed flat runs, and retains
the suffix persistent subtree without visiting it.

A local edit may churn IDs inside its changed reset group. It must not churn a
converged distant suffix. Fresh-open equivalence compares decoded Markdown,
physical ownership, logical segments, mappings, and facts; process-local
coverage IDs and persistent-tree packing are intentionally history-specific.

### Bounds

One poll retains only:

- one 4 KiB Program body buffer;
- a fixed stack header and one 1--3-piece tail packet;
- constant reset/checkpoint state; and
- arena-owned pages already journaled to the candidate build.

Allocation, page sealing, reset comparison, and suffix attachment remain
fuelled. A reset interval is bounded by encoded pages, not merely physical
bytes: a huge coalesced Identity span is cheap, while a dense transform stream
gets frequent resets. A semantic change that genuinely reclassifies a giant
span may require giant total work, but it remains resumable and source-visible
instead of blocking input.

## Canonical semantic output

Within one newly encoded run:

- adjacent Identity pieces coalesce;
- adjacent Hidden pieces coalesce only with equal affinity;
- Atomic pieces never split or merge;
- a one-piece Identity/Hidden/Atomic uses its inline contribution rather than
  a one-piece Program; and
- Program encoding has one schema-versioned canonical representation.

Storage page boundaries are not Markdown semantics. Decoder/query oracles must
compare normalized source/projection streams across clean and incremental
builds, while the incremental gate separately checks eligible suffix page and
ID reuse. Public raw constructors must be sealed so alternate encodings cannot
enter production by bypassing the composer.

## Direct sink requirement

Returning a `Vec<ProjectionChunk>` is a test convenience, not the production
path. The composer must hand each sealed Program payload directly to the
resumed `ArenaBuildSession`, receive a build-scoped typed Program owner, and
immediately emit or buffer only the single green event that references it.

The sink receipt reports both compact encoded bytes and actual scratch
capacity. No claim based only on final payload length is accepted as a memory
bound. Cancellation transfers every allocated Program/green owner through the
arena build journal and performs no synchronous page walk.

## Falsification gates

The algorithm/locality portion now passes 7/7 executable real-lineage tests:

- 24 repeated prefix inserts: resets 6 to 8, 2 minted, worst 2 changed groups
  and 19 changed pages, maximum 16 KiB/17 pages per group;
- insert exactly at one reset: both affinities remain local, while `After`
  preserves 100% of the mapped right suffix and changes 2 pages versus 10 for
  `Before`;
- 33,768-byte deletion across four resets: 1 changed group/8 pages and 35/42
  mapped suffix pages eligible for exact reuse;
- 40,960-byte dense NUL/tab/CRLF insertion: 4 local resets minted, 5 changed
  groups/36 pages proportional to inserted data, and 89.4% suffix eligibility;
- underflow followed by 20 KiB growth: one local merge, two later splits, and
  the hard group bound retained;
- a mapped reset that becomes the interior of CRLF is rejected locally; and
- byte/UTF-16-divergent Unicode: exact totals, 1 changed group/4 pages, and 90%
  suffix eligibility.

Those percentages are fixture-size dependent; the decision receipt is the
absolute changed-group/page bound. The suite consumes actual
`LineageMappingProof` values. Its per-reset/page jobs are measurement oracles,
not the production mapping algorithm. It also materializes and rebuilds a
clean comparison layout, so this gate proves identity eligibility and bounded
partition churn, not yet proportional production CPU time.

The production-shaped codec/query mechanism adds these receipts:

- packed schema v6 round-trips the reset bit, rejects the same bit under v5,
  and rejects every still-reserved logical-descriptor bit;
- the source-bound join rejects cross-build, wrong-source, wrong-generation,
  wrong-parser, and pending-Virtual inputs, and is terminally poisoned after a
  consumed linear-input failure;
- Unicode byte/UTF-16 totals, CRLF atomic projection, right-biased Virtual
  ownership, implicit source zero, and cross-manifest staleness remain exact;
- the checkpoint mechanism distinguishes `BeforeFollowing` from
  `AfterPreceding` at the same source coordinate across zero-metric structural
  events and explicitly reports that neither observation is a sequence cut or
  parser restart state; and
- a 24,000-run multi-page fixture returns a typed one-page bounded miss, then
  finds the distant reset under an explicit page budget by persistent-sequence
  leaf descent, without a document-wide reset directory or green open path.

The reset codec/query suite passes 6/6 in debug and release, the typed
checkpoint-boundary suite passes 8/8, and the existing projection regression
matrix remains green at 24/24 (2 chunk-edit-stability, 5 chunker, 7 source-reset,
and 10 packed-green tests). These are mechanism receipts, not evidence that the
current composer can yet request a reset flush or that a parser checkpoint can
be minted in production.

The full flat stable-reset model passes only when all of the following are
true:

1. a real `SourceStore` lineage proof, not echoed ranges, maps reset and suffix;
2. repeated prefix edits keep reset density bounded and reuse a distant suffix;
3. insertion at a reset exercises both boundary affinities without duplication
   or loss;
4. deletion/replacement across many resets rebuilds only the changed span plus
   the first nonconverged group;
5. Unicode byte/UTF-16 metrics, CRLF/lone-CR, tabs, NUL, hidden affinities, and
   Virtual attachment survive reset boundaries exactly;
6. the one canonical source-bound composer owns reset flushing, pending-Virtual
   state, final-chunk knowledge, and terminal poison; no parallel join mirrors
   that state;
7. no public constructor can create a retained alternate encoding;
8. a persistent checkpoint supplies the exact event-side cut and complete
   parser/open/projection continuation rather than reconstructing either from
   a source coordinate or committed green;
9. direct clean and incremental output normalize to the same complete source
   and logical stream;
10. a huge dense single-line/table-cell fixture remains page-bounded and
   cancellable; and
11. exact old suffix pages/coverage IDs are reused after convergence without
   retaining the retired source root.

Use an explicit transformed-envelope/reset-group size gate while these remain
open. Promote Program to a persistent mini-sequence only if source-authoritative
reset density, convergence, or top-level coverage fanout fails these tests. Do
not add the extra tree merely to repair pure greedy chunking before the smaller
reset design is exercised.
