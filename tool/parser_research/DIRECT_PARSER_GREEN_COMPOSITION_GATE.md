# Direct parser-to-green composition gate

Status: **selected parser/source/writer substrate GO; reference/Table direct
joins, full grammar, final multi-root publication, and launch HOLD**,
2026-07-18.

The 2026-07-16 ordinary-container slice below is historical provenance, not
the current upper bound. The same real candidate path now includes durable
checkpoint/restart, exact parent-selected convergence, Setext normalization
(44/44 focused), an authenticated no-snapshot Table cursor (4/4), and a
zero-payload 224-byte source/projection continuation (22/22). The combined v3
receipt passes 392 library tests, every integration target, and three
compile-fail doctests when the single explicitly open reference-finalizer
publication gate is skipped. Current remaining work is controlled by
[`ARCHITECTURE_PROOF_LEDGER.md`](ARCHITECTURE_PROOF_LEDGER.md) and RFC 023; any
older statement below that calls architecture selection itself HOLD is
superseded by this update.

The concrete ownership, transition-ordering, failure, and first-slice sequence
for the next implementation is in
[`CANDIDATE_WRITER_VERTICAL_SLICE_PLAN.md`](CANDIDATE_WRITER_VERTICAL_SLICE_PLAN.md).

This gate defines the production-shaped seam between the
exact correspondent block machine, the restart composer, and the selected
packed serialized-green document. It exists to prevent a superficially useful
adapter from preserving the experimental parser's event history, mutable
output handles, or source-position repair protocol as permanent architecture.

## Decision

Keep the exact line machine, handler ordering, and donor-derived scanners.
Replace its proof-era AST/materialization boundary with a parser-owned typed
output port. The parser does not publish a mutable tree and does not feed a
retained event tape. It runs as a cooperative worker job and writes a
capability-bound mutation program into one revision candidate:

```text
current source revision + edit lineage + immutable base manifest
                              |
                              v
              exact parser control continuation
                              |
                    bounded semantic actions
                              |
                              v
       one resumable PageArena candidate-build transaction
                              |
          +-------------------+-------------------+
          |                   |                   |
          v                   v                   v
  packed green pages   reference/restart roots   root manifest
          |                   |                   |
          +-------------------+-------------------+
                              |
                   one ownership transfer
```

`ControlContinuation` remains parser-private. Published green pages remain
grammar-free. The top-level composer is the only component allowed to join a
new prefix to an old suffix, and it may do so only through a one-use adoption
permit bound to base-root capabilities.

## The current sink is evidence, not the seam

`comrak_value_block_core::checkpoint::StructuralEvent` currently contains:

- source leaves containing copied text;
- numeric output handles and parent handles;
- `Open`, `Update`, `Detach`, and `Close` history;
- source-position rewrites and `RepairListSourcePositions`;
- append/drain operations over materialized logical content; and
- reference occurrences.

`ResumableValueBlockParser::materialize` then replays parser history into a
`TreeMaterializer`. This proved that the grammar can resume with a write-only
consumer, but it is the wrong persistent contract:

1. numeric handles imply a mutable output directory;
2. chronological repair events encode donor AST bookkeeping rather than final
   Flark truth;
3. copied `SourceLeaf.text` duplicates the source owner;
4. `Update` permits incomplete property writes separated from block identity;
5. list position repair can walk an already-closed subtree; and
6. a general event history can express invalid intermediate states that the
   packed document must never commit.

The direct lane may keep a bounded line-local action buffer while a grammar
transition is still atomic. It may not persist or replay a document-history
event tape.

## Three action scopes, not one generic event enum

The composer accepts three typed scopes. An action cannot silently move from
one scope to another.

### 1. Streaming construction actions

These append final source-order records to transaction-owned pages:

```text
OpenNew {
  new_block_id,
  kind,
  normalized_facts_known_at_open,
  parent_open_binding
}

AppendCoverage {
  source_capability,
  byte_length,
  utf16_length,
  owner_open_binding,
  part,
  logical_projection_descriptor
}

CloseOpen {
  open_binding,
  normalized_close_time_projection_facts,
  closed_child_contribution
}
```

`OpenNew` produces a transaction-local non-cloneable `OpenBinding`; it does
not expose a numeric lookup key. It receives normalized final facts already
known at that decision site, not a mutable donor `BlockKind`. `CloseOpen`
consumes the binding and supplies code/HTML projection slices and other facts
known only at close. The close contribution is encoded on `Exit`, so list
folds do not backpatch `Enter`. Paragraph/Setext/reference/table ambiguity is
owned by the separately typed `LeafNormalizationGroup`; it atomically installs
one canonical final footprint rather than exposing general backpatching.

Coverage references the current immutable source revision. It never copies
source text into green storage. Adjacent compatible runs may coalesce, but the
builder must retain exact cut points needed by pending parser state, restart
samples, origin transforms, and edit lineage.

#### Coverage identity belongs to sealed storage, not decoder atoms

The Stage-1 source-ledger proof currently returns one debug
`ValidatedSourceClaim` and burns one `CoverageId` for each Identity, Hidden,
Atomic, or None action. That surface is intentionally not the production
composer contract. Preserving it would turn a dense Tab/NUL/CRLF source into
one packed green event per atom even though the bounded Program codec can
represent thousands of those exact transforms in one page.

The production path must separate two linear transitions:

```text
authoritative source consumption
  -> ConsumedSourcePiece
       exact root/build/range/metric
       physical owner + part
       logical target + typed projection
       no CoverageId

source-bound projection composition
  -> sealed SourceProjectionRun
       one freshly consumed CoverageId
       inline Identity/Hidden/Atomic when canonical
       otherwise one bounded Program chunk
```

`ConsumedSourcePiece` is non-cloneable and can enter only the active candidate
composer. The composer may coalesce or Program-pack adjacent pieces only while
physical owner, coverage part, logical consumer, source order, and envelope
policy remain compatible. An owner/part/consumer change, a certified reset, or
a semantic-envelope end forces a bounded flush. Each emitted chunk consumes a
fresh build-scoped coverage permit; source-piece count and retained coverage
run count are deliberately different receipts.

The base executable composer now proves owner/part/consumer flushes and EOF
chunking, but **certified-reset flushing remains HOLD**. A post-seal marker
join cannot end an otherwise compatible long envelope at the exact
parser-authorized boundary, and a multi-page finish cannot mark its final
chunk safely without finality evidence. Production must route a typed
`flush_at_reset` request through this same composer/writer and either retain
one bounded run of lookbehind or receive a chunker finality signal. The reset
codec/join prototype proves packed marker encoding and capability matching;
it is not a second post-hoc production composer.

Actor-level singleton admission alone is mechanism evidence, not manifest
authority. The executable grammar-free `CandidateWriter` now owns the composer
and packed builder together, retains the non-cloneable composer/source
completion seal, requires explicit green sink acknowledgement, and joins that
seal with the sole builder manifest and arena ticket inside one local commit.
It also flushes the preceding projection envelope *before* journaling an
intervening `Exit`/`Enter`; ledger open/close follows the builder's distinct
`ReadyForEvent` acknowledgement. This closes the ownership and ordering
falsifier for already-final typed actions. The first real CommonMark
Document/Paragraph/Quote/List/Item/blank-line path now drives that writer
directly from actual grammar decision sites. Grammar decisions first seal one
parser-private line transaction, then expose one acknowledged stack-shaped
command at a time. The transaction orders each retired frame's `Exit`
immediately after its final physical source use, so an old Paragraph or Item
can close between a retained Quote marker and a replacement List open without
exposing a mutable handle. It also verifies recognition versus authoritative
replay, exact packed output, and the local completion join. It does not yet
authorize production
publication because checkpoint, adoption, reference, fact, Unknown-range, and
inline roots are not part of the join, and most grammar remains deliberately
fail-closed.

This ordering is also the clean failure boundary. If source consumption
succeeds but Program encoding or arena journaling fails, the candidate is
poisoned and must enter its existing constant-time abort/fuelled-reclaim path.
It may not return the consumed piece, continue parsing, or publish output with
a source hole. At most one unaccepted source piece and one sealed chunk may be
pending between parser and sink; neither side retains a document-sized claim
or chunk vector.

### 2. Capability-bound normalization and local fact updates

CommonMark/GFM has a small number of exact retroactive outcomes. The parser
selects them through one opaque provisional Paragraph group, not a collection
of independently callable mutations:

```text
NormalizeLeafGroup {
  one_use_group_authority,
  scanner_certified_paragraph_outcome,
  exact structural replacement envelope,
  certified source/projection partitions,
  parent/open-path capabilities,
  reference-root capability,
  outcome-specific identity policy
}
```

The outcomes are deliberately enumerable:

- ordinary Paragraph and Setext Heading retain the primary identity;
- reference-only finalization retires the wrapper while retaining all physical
  source as parent-owned Gap coverage;
- visible-reference finalization keeps the surviving Paragraph or Heading;
- whole-table promotion retires the Paragraph and returns a new open Table; and
- split-table promotion preserves the primary identity on the preface and
  returns a newly minted open Table.

One structural replacement envelope is sufficient for the pinned profile, but
one contiguous source range is not: ancestor markers interleave, definitions
change owner, and table cells use certified interior partitions. The group
manifest carries those exact capabilities without exposing arbitrary offsets.

Other capability-bound updates remain narrow:

- Open-ancestor list/output facts rewrite through capabilities captured from
  the immutable base root or produced by the current transaction.
- An edit or reference-prefix drain may split a coverage run, retaining the
  deterministic surviving CoverageId fragment and minting the others.

No operation accepts only `BlockId`. `BlockId` is identity evidence inside a
capability; it is not a locator. See
`LEAF_NORMALIZATION_GROUP_GATE.md` for restart provenance, convergence inside
large open groups, and the decisive zero/two-wrapper witnesses.

### 3. Certified immutable adoption

The only suffix operation is:

```text
AdoptSuffix {
  one_use_permit,
  immutable_balanced_suffix,
  typed_open_boundary_recipes
}
```

The permit is issued only after edit-lineage mapping, physical-line boundary
alignment, grammar/profile identity, exact `ControlContinuation`, stable open
bindings, immutable source-tail identity, path shape, and every variant-local
semantic-prefix recipe validate. Consuming it attaches persistent pages by
owned reference. Equal bytes, equal hashes, equal control alone, or equal
`BlockId`s cannot create a permit.

## Parser job state

A live worker job retains these disjoint values:

```text
ParserJob
  ControlContinuation
  SemanticPrefixState
  StableOpenBindings
  SourceCursor
  SchedulerCursor
  CandidateBuildId
  CurrentLineActions
```

`CurrentLineActions` is bounded by a declared transition fan-out or is itself
fuelled. Dense table row/header construction and very deep one-line container
opens are current counterexamples to assuming that one physical line fits a
bounded `Vec`; their scanners and action producers require explicit phases.
Staged actions are discarded on cancellation before becoming visible. A
complete physical line is the smallest convergence boundary, but long
scanners may yield inside a line without fabricating a convergence point.

## Exhaustive per-line source ledger

The typed output port begins each line with a stable source lease. Parser
decision sites claim disjoint source intervals with owner binding, source part,
and logical contribution. `finish_line` validates:

```text
claims are ordered, non-overlapping, and cover [0, physical_line_bytes)
sum physical bytes and UTF-16 equals the bound source lease
every logical output maps to source or an explicit typed virtual recipe
```

Quote/list prefixes, ATX and Setext markers, fence opener/closer, thematic
break, table delimiter/pipes, BOM, blank gaps, line endings, and definition
source must be claimed at the parser sites where their meaning becomes exact.
Unclaimed bytes are an error. Deriving roles later from source positions,
content origins, or copied line text would recreate a second classifier.

The current `ArenaBuildTransaction<'a>` holds `&mut PageArena` for its entire
lifetime and therefore cannot survive a worker yield cleanly. The composed
lane needs an arena-owned generation-checked journal:

```text
begin_build() -> BuildId
resume_build(BuildId) -> short-lived BuildSession
yield_build(BuildSession) -> BuildId
abort_build(BuildId, fuel) -> ReclaimStatus
commit_build(BuildId, manifest_owner) -> SerializedGreenDocument
```

Only one candidate build is admitted by the latest-wins coordinator. A
`BuildId`, owner handle, open binding, range capability, and adoption permit
are non-`Copy`, non-`Clone`, generation checked, and single-use where their
operation transfers ownership. A stale or replayed value fails closed.

Drop is not the cancellation mechanism for a large candidate. Abort walks the
owner journal under fuel; the old committed root remains independently
queryable throughout.

## Exact source and logical projection

The structural stream's total byte/UTF-16 coverage is necessary but may not be
sufficient for inline and raw-block input. Every terminal logical run must let
a bounded service reconstruct:

- the exact logical byte sequence supplied to inline/reference parsing;
- physical source origins for every logical range;
- identity, tab-expansion, pipe-trimming/unescaping, entity/backslash, and
  synthetic transformations where used;
- paragraph/heading/table-cell line boundaries;
- fenced/indented-code info and literal projections; and
- HTML literal projection and trailing-blank trimming.

These values must remain source-relative and shareable across prefix edits.
They may be encoded as compact coverage-run fields plus typed external
projection-run roots. They may not be reconstructed by reparsing with a
second Markdown classifier, copied as aggregate strings, or addressed through
old absolute source coordinates.

This requirement is a codec gate. If the existing
`CoverageRun { id, metric, owner_relative_depth, part }` cannot losslessly
express it, the run schema must be extended before direct composition; the
missing information must not become a parallel mutable coverage tree.

## Atomic manifest

The committed root is a composite manifest, not merely the structural
sequence manifest. One commit binds at least:

- syntax profile and grammar revision;
- source revision and edit-lineage generation;
- parse generation and semantic epoch;
- packed structural/source root;
- exact known and `UnknownRange` coverage;
- reference occurrence and winner roots;
- restart-sample root and open-overlay state;
- available inline/presentation generations; and
- capability/codec schema versions.

All required child roots are transaction-owned edges before ownership
transfer. A structure root cannot become current while its reference or
restart state belongs to another epoch.

Inline and layout enrichment may publish later derived manifests, but each is
bound to the exact source/semantic root it describes and cannot change block
truth.

## Fresh parse and incremental parse must share the same sink

A clean parse begins with an empty candidate and streams the Document Enter,
coverage, semantic tokens, and final Document Exit.

An incremental parse begins with capabilities sampled from one immutable base
manifest, constructs a replacement prefix/range through the same action
vocabulary, and optionally consumes one certified suffix permit. It does not
route through a different parser, a mutable AST patcher, or a clean-parse
materializer followed by a tree diff.

The test oracle canonicalizes both roots and requires byte-for-byte equivalent
semantic facts, coverage ownership, logical projections, reference
occurrences, and rendered HTML. Stable IDs/pages may differ only where the
identity rules require a new value.

## Acceptance sequence

The real composed crate has path dependencies on
`comrak_value_block_core`, `restart_composer_gate`, and `v3_runtime_slice`; it
does not copy their parser, capability, arena, or green types.

### Slice A: seam falsifier

1. Parse directly without `TreeMaterializer`: ordinary Paragraph; nested
   quote/list ownership; Setext with/without definitions; reference-only
   wrapper removal; closed/bare-EOF fence; whole/split-preface GFM table;
   escaped-pipe and padded-short cells; BOM/tab/LF/CRLF/lone-CR/non-BMP input.
2. Every line passes the exhaustive source-ledger receipt; wrapper removal
   preserves total byte/UTF-16 metrics.
3. Packed structure, normalized facts, logical projections/origins,
   references, and HTML equal the unlimited parser on those fixtures.
4. One local edit uses a real composer permit, adopts a balanced suffix, and
   retains a distant suffix leaf by exact ArenaId.
5. Cancellation/allocation failure before every typed action leaves the old
   root queryable and the candidate reclaimable.
6. The dependency path retains no `StructuralEvent`, `TreeMaterializer`,
   copied source leaf, aggregate logical String, or document-wide lookup
   directory.

### Slice B: architecture proof

- every-line restart and permitted convergence over all 1,322 block fixtures;
- 100,000-item tightness mutation bounded by changed pages/path depth;
- 10 MiB raw blocks with shared source/projection runs;
- oversized refillable table/reference scanners;
- 10,000 same-gap edit repacking;
- randomized edits and stable identity rules; and
- debug/release, strict Clippy, native/WASM, and ownership stress.

### Slice C: product gate

Worker scheduling, bridge bytes, Flutter adoption, physical-device
input-to-paint latency, memory peaks, shaping/accessibility, and honest
Unknown/source-visible behavior become acceptance gates only after Slice B.

## Stop conditions

Stop and redesign if the composed lane requires any of:

- retaining `StructuralEvent` history after the current bounded transition;
- implementing production as a downstream adapter over `StructuralEvent`;
- a document-wide mutable handle or `BlockId -> node` directory for normal
  parsing;
- querying committed green output to recover grammar state;
- source-position, parent, sibling, or suffix repair walks;
- copied source leaves or aggregate logical strings in structural storage;
- reconstructing source roles from line/column positions, content origins, or
  copied physical text after the parser decision;
- two separately committed structure/source/reference roots;
- synchronous unbounded transaction Drop or cancellation cleanup;
- cursor rebasing after earlier edits in the same base-root batch;
- unvalidated replayable capabilities; or
- a fallback Markdown predictor when exact work misses a deadline.

## Verdict

The architectural seam is now specific enough to implement. The selected
shape is not “adapt the old event sink to a new tree”; it is one exact parser
writing typed, capability-bound actions into one resumable persistent
transaction. The old materializer remains a differential oracle during the
transition and should disappear from the production dependency path once the
composed corpus gate passes.

The 2026-07-16 grammar-free writer implementation strengthens that topology:
the source ledger, projection composer, resumable packed builder, arena lease,
and identity allocator fit behind one cooperative authority boundary without a
raw run or manifest escape. It is deliberately not an architecture-selection
result. Selection still depends on connecting the real resumable parser,
persisting and adopting exact checkpoints, surviving the enumerated
retroactive Markdown transitions, and committing the remaining semantic roots
through the same boundary.
