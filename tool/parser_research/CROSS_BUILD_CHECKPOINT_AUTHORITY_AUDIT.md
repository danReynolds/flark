# Cross-build checkpoint authority audit

Status: **same-build mechanisms are coherent; committed-session restart is
designed but not executable**, 2026-07-16.

This audit covers the current parser pause, source ledger continuation,
projection composer continuation, packed-green cut, projection reset,
checkpoint-boundary resolver, document identity allocator, and
normalization/adoption gates. It does not change production code.

## Verdict

The current pieces can form a correct cross-build checkpoint without retaining
source bytes. They cannot do so by serializing any existing same-build token or
by upgrading the current storage-boundary mechanism.

The missing authority is one manifest-derived, non-cloneable restart lease that
reauthorizes retained semantic identities and provisional normalization state
under a new candidate build. Raw `BlockId`s, byte offsets, adjacent Coverage
sides, and build-local binding stamps must remain observations rather than
constructors for that lease.

There is also an important two-view requirement:

1. a **canonical committed cut** locates the exact prefix/suffix boundary in
   final packed green; and
2. a **provisional resume recipe** restores the parser/writer state that existed
   at the sampled line boundary.

Those views are identical for an ordinary Paragraph and happen to remain
one-event-for-one-event for the current Setext rewrite. They are not identical
for reference-only normalization or whole/split Table outcomes. Final green may
contain no Paragraph wrapper or a multi-block forest even though restart must
restore one provisional Paragraph with its historical primary identity.
`SealedNormalizationManifest` must bind the two views; final green must never be
heuristically inverted.

## What the current seams actually prove

- `DirectLineBoundaryPause` is compact parser control. It contains open parser
  kinds, closed-child folds, the parser line cursor, and one deferred-source
  role. It contains no parser `NodeId`, source position, source byte, or
  semantic `BlockId`. Resume deliberately allocates fresh scratch node IDs.
- `CandidateSourceLineBoundaryContinuation` is exact same-build writer/source
  state. It contains build-scoped `BindingStamp`s, path logical metrics, prefix
  counters, and pending terminator/gap state, but no Crop cursor or source byte.
- `SourceProjectionComposerLineBoundaryContinuation` is exact same-build
  projection-prefix state after a fully drained structural flush. It remains
  tied to the same epoch and exact build-local green cut.
- `SerializedGreenLeafCut` is the correct capture-time logical observation:
  `(events_before, source_before)`. `leaves_before` is only a physical hint.
- `StoredRestartCheckpointMechanism` and
  `StoredConvergenceCheckpointMechanism` prove a manifest-bound physical line
  and adjacent Coverage side. They explicitly have neither parser-state nor
  exact-sequence-cut authority.
- `DocumentIdentityAllocator` mints only fresh build-scoped IDs. It has no path
  for reauthorizing a retained ID from a committed base manifest.
- `StoredProjectionResetCapability` can recover a prior run-attached reset but
  cannot recreate parser, semantic-path, provisional-group, or exact event-side
  state. It is no longer required by the selected composite restart path.
- Source lineage can map unchanged restart/prefix/convergence/tail regions to a
  current Crop root without copying source. It does not prove parser control or
  semantic identity by itself.

The isolation is sound. The gap is the committed join, not a need to replace
these mechanisms.

## Authority is not the same as stored data

Parser control frames, semantic IDs, logical metrics, and counters may be
encoded as ordinary immutable checkpoint data. They do not need to pretend to
be linear capabilities. Authority comes from the resolver that reads those
records through the sole committed manifest and emits one consumed lease after
all cross-checks pass.

This distinction matters because the parser pause is currently cloneable and a
`BlockId` is a small scalar. Neither property is unsafe if no API accepts a
copied value as resume authority. The production boundary should therefore be:

```text
cloneable/private checkpoint data
  + sole committed manifest owner
  + exact green sequence resolution
  + source-lineage proof
  + current LiveDocumentStore candidate epoch
  -> non-cloneable CandidateRestartLease
```

It must not be:

```text
byte cut + BlockId array + parser pause -> resume
```

## Required committed checkpoint record

The exact codec can remain private, but one sparse checkpoint entry needs the
following logical fields. Fields already inherited from the manifest need not
be duplicated in every record.

```text
StoredCompositeCheckpointEntry
  checkpoint schema and direct-control schema
  exact canonical events-before ordinal
  exact canonical source byte/UTF-16 metric
  physical line ordinal / parser line cursor
  restart or convergence role and boundary affinity

  direct parser control recipe
    open kinds and kind-specific control facts
    historical child folds
    current frame depth
    deferred terminator / blank-gap role

  source-ledger prefix recipe
    cumulative source/special metrics
    line and source-piece prefix counts
    open semantic path recipe
      retained BlockId
      provisional kind
      accumulated logical metric
    pending predecessor recipe, if any

  projection-prefix recipe
    next source metric and run generation/counts
    reset-certified exact canonical cut
    explicit no-pending-right-biased-Virtual proof

  active normalization reference, if any
    group manifest identity and group generation
    sampled checkpoint identity
    provisional primary BlockId and parent-path recipe
    canonical-cut <-> provisional-control mapping

  active raw-block fold/reference state required by the selected profile
```

The record owns no aggregate source string, source fragment, parser tree, or
green page mirror. Pending terminator/gap state should be encoded as a typed
source-range/owner recipe. Any unspent build-local debug coverage permit is
burned; a later build mints fresh coverage only after the recipe is revalidated
against the current source.

### Why the canonical cut must be finalized, not merely copied

A green cut captured while a Paragraph is provisional may be changed by later
normalization. Setext preserves the event count, but reference-only
normalization can remove a wrapper and Table normalization can insert a forest.
The group transaction must therefore translate every admitted provisional
sample to an exact cut in the final canonical sequence and persist that mapping
in the sealed group/checkpoint footprint. Copying the capture-time
`events_before` into the final manifest is valid only when the normalizer proves
the ordinal was preserved.

The final checkpoint entry consequently carries both:

- a canonical event/source cut for retained-prefix and suffix splice; and
- a group-relative provisional sample for control restoration.

### Projection reset is a role on the exact cut

The composite checkpoint does not need to backpatch the last Coverage record.
A checkpoint-specific drain plus the exact green cut already proves the full
projection reset condition: no open envelope, pending source piece, pending
run, or unacknowledged output crosses the cut, and composer, green, source,
build, and epoch metrics agree. The writer therefore mints a private
`CheckpointProjectionResetAtCut` role inside the same entry. It carries no
second byte/event coordinate.

This is stronger than the existing run-attached reset bit. An exact event-side
cut distinguishes zero-metric structural boundaries, source zero, and repeated
blank-line samples that legitimately share one green cut but have different
parser/source recipes. The checkpoint index is already required; reset lookup
is therefore direct rather than a backward page scan. The run bit may remain a
temporary standalone-storage optimization, but it is not restart authority.

Two constraints are non-negotiable. First, checkpoint draining must reject a
pending right-biased `Virtual` before it changes storage affinity; ordinary
finish would attach a terminal Virtual to the previous run while uninterrupted
input may attach it to the next. Second, any later normalization must translate
the sampled provisional cut to an exact canonical cut and may not coalesce a
projection run across that footprint. Those are explicit checkpoint-schema
invariants, not caller conventions.

When no green event was emitted, the builder may reuse only the still-linear
cut that it proves remains current. Multiple physical restart samples may
reference that same cut; the checkpoint index orders them by physical restart
position/sample identity rather than deduplicating them by green coordinates.

## Resolution and resume data flow

### Capture and commit in the old build

1. `ExactBlockJob` reaches acknowledged `FinishLine` with no active line,
   command, source atom, or writer action.
2. The parser produces its compact direct-control state.
3. The writer uses a checkpoint-specific composer drain that rejects a pending
   right-biased `Virtual` before mutation. A successful drain proves that no
   projection state crosses the candidate cut; it does not mark or search for
   a preceding Coverage run.
4. The source ledger exposes its quiescent prefix/path/deferred recipe.
5. The green builder force-seals an exact leaf barrier and returns the linear
   `(events_before, source_before)` cut.
6. One writer-owned join checks parser open kinds/deferred role against the
   ledger path, the ledger's accepted-projection metric against composer and
   green, active group/fence state, profile/schema/build, and the exact cut.
   That join mints `CheckpointProjectionResetAtCut` inside the same composite
   entry; the cut remains the sole coordinate authority.
7. The checkpoint builder stores private data under the candidate manifest;
   same-build execution may resume only from the still-linear continuation.
8. If a later group normalization changes canonical structure before the
   sample, that transaction records the canonical-cut translation before the
   manifest can commit.

### Resolve in a later build

1. The edit offset may select the nearest earlier index entry, but is query
   input only. The entry itself is reached from the committed checkpoint root.
2. The resolver revalidates source/profile/grammar/parse/semantic generations
   and the checkpoint/control schemas.
3. It descends `GreenSummary.tokens` to the stored event ordinal, checks the
   exact prefix source metric, decodes only the containing page, and
   reconstructs the canonical open path at that exact sequence side. A generic
   source-coordinate `seek` is insufficient because zero-metric structural
   events may share the coordinate.
4. It validates retained parent identities against canonical green. If a
   provisional group is active, it validates the terminal identity and resume
   recipe through the sealed normalization manifest instead of requiring that
   terminal to exist in canonical green.
5. Source lineage maps the stored source boundary to the current source and
   proves the required unchanged retained regions.
6. `LiveDocumentStore`, while owning the sole current base-output lease and
   new candidate epoch, consumes the resolved record and mints one
   `CandidateRestartLease`.
7. The lease remints Crop cursors, new-build binding/path generations, parser
   scratch node IDs, composer generations, validator state, and active group
   handles. Retained semantic `BlockId`s do not change.
8. `ExactBlockJob::resume_from_checkpoint` installs parser and writer state as
   one candidate-start transition. No partially restored path becomes visible
   on failure.

Convergence uses a role-distinct entry and the same exact control/path/group
comparison. Only after lineage, semantic-prefix, reference/fact, projection,
and canonical sequence cuts all agree may storage attach a retained suffix.

## Smallest missing executable seam

The smallest useful new authority is not a generic checkpoint API. It is a
Setext-sized, manifest-derived retained-path/group lease:

```text
ResolvedRetainedParagraphRestart
  canonical exact-sequence cut
  retained parent path from committed green
  provisional Paragraph primary BlockId from the sealed group manifest
  provisional open-path logical metrics
  direct parser control recipe
  mapped source/deferred recipe
  projection-prefix/reset recipe

LiveDocumentStore + current base-output lease + new candidate epoch
  -> CandidateRestartLease

CandidateRestartLease
  -> ExactBlockJob::resume_from_checkpoint
     (fresh parser scratch + restored CandidateWriter path/group)
```

The writer-side consuming operation should install the whole path in one call
and return parser-visible `CandidateWriterBinding`s outer-to-inner. It should
not call ordinary `candidate_open_binding` repeatedly: that API consumes
`FreshBlockPermit`s and would silently replace stable semantic identity. The
restored `BindingStamp`s use the new build ID and fresh path-generation stamps;
only their manifest-authorized `BlockId`, kind, depth, and logical prefix are
retained.

The first executable receipt should remain deliberately Setext-only:

1. sample immediately before a Setext underline inside a very large Paragraph;
2. commit the old revision as canonical Heading with the same primary ID;
3. edit the underline valid -> invalid -> valid;
4. resolve the old checkpoint through its committed manifest and source
   lineage;
5. restore parent path plus provisional Paragraph under a new build while
   preserving the primary `BlockId`;
6. prove parser commands, source/projection metrics, final canonical output,
   and a distant retained leaf match a clean parse; and
7. reject wrong manifest, event ordinal, source metric, source revision,
   profile/grammar, group generation, parent path, candidate epoch, and stale
   source advancement.

The receipt must additionally show:

- old and new `BindingStamp` build/path generations differ;
- the document allocator never moves backwards and fresh IDs remain disjoint;
- no checkpoint-owned source bytes or aggregate Paragraph text;
- failure at every allocation/join phase publishes nothing; and
- work is bounded by checkpoint interval plus changed pages and sequence
  height.

That is the smallest test that proves retained IDs, a restored semantic path,
active normalization, parser control, and real storage authority meet in one
place. A path-only unit test would leave the dangerous caller-composition gap
intact.

## Durability boundary

“Durable” currently has two possible meanings and they should be named
separately.

### Committed-session / cross-build durability

This is feasible without source bytes. The committed arena manifest remains
live, `LiveDocumentStore` retains document identity clocks, and `SourceStore`
retains enough edit lineage to map unchanged regions. If lineage expires, the
correct fallback is an exact zero/full restart, not a guessed checkpoint.

### Process-persistent durability

This is not currently supported and should not be implied by the same term.
Arena IDs and owners are process-local, the direct pause has no stable external
codec, and the identity allocator envelope has no document namespace or
persisted block/coverage high-water marks. Supporting save/reopen checkpoints
would require at least:

- a disk-stable checkpoint/green schema and migration policy;
- a persistent document identity namespace;
- persisted allocator high-water marks restored above every retained ID;
- persisted source roots or another exact source-version binding; and
- revalidation that does not depend on process-local arena capabilities.

None of that is required for fast incremental reparsing inside one editor
session. It should be a separate product/storage decision.

## Architectural judgment

The proposed join remains clean: source bytes stay under one source authority;
parser control stays parser-owned; semantic identity is retained only through a
committed manifest; canonical green stays canonical; provisional normalization
history exists only in a sparse group manifest when a sampled checkpoint needs
it; and every build-local handle is reminted under the new epoch.

The architecture would become a patchwork if it tried to derive provisional
state from final green, accepted a caller-supplied ID/path, or added a special
Setext-only restart coordinate. The two-view checkpoint plus one generic
manifest-derived restart lease avoids those traps and extends to the already
enumerated reference/table outcomes without changing the authority model.
