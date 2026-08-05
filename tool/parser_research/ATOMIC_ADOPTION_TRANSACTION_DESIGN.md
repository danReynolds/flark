# Atomic convergence-adoption transaction design

Status: **replacement design selected; restart selection and one-pass lineage
bundle implemented; leaf-group storage resolver and atomic mutation remain HOLD**,
2026-07-16.

> **Paragraph-action refinement, 2026-07-16.** The two-key storage authority
> and one-candidate transaction below remain selected. Its individual
> `PromoteSetext`/`PromoteTable`/`FinalizeParagraph` sketches are superseded by
> the exhaustive `LeafNormalizationGroup` contract in
> [`LEAF_NORMALIZATION_GROUP_GATE.md`](LEAF_NORMALIZATION_GROUP_GATE.md).
> A provisional Paragraph can normalize to zero, one, or multiple wrappers,
> so an Enter capability or one stable binding is not the complete authority.

This document replaces the authority model exercised by
`composed_adoption_storage_gate`. The existing crate remains useful evidence
that one packed `Enter` can be rewritten immutably without a `BlockId` lookup,
but its caller-echo stamp, cross-arena aliasing, partial Setext application,
old-revision manifest, and pre-allocation-only failure receipt make it the
wrong production seam.

The replacement keeps the selected high-level architecture: an exact parser,
one persistent serialized-green document, restart-and-converge, and immutable
suffix sharing. It changes who is allowed to authorize storage reuse.

## Decision

The composer must **not** issue an attachment permit.

It produces a non-`Clone`, complete semantic result named
`CompleteConvergenceRecipe`. The document store independently produces a
non-`Clone`, opaque `BaseAdoptionProof` from the actual arena, current output
manifest, active parse session, source store, retained edit lineage, old
restart checkpoint, selected convergence checkpoint, and storage-resolved open
path.

Neither value has an `attach`, `commit`, `authorize_storage`, or raw-root
operation. Only the document store has the private function that consumes both
values into the already-active candidate transaction:

```text
CompleteConvergenceRecipe     BaseAdoptionProof
 semantic truth only          storage/source truth only
            \                    /
             \                  /
              v                v
       CandidateWriter::bind_adoption       private
                        |
             full cross-validation
                        |
            resumable mutation program
                        |
       one current-revision composite manifest
```

This is a two-key protocol, but only storage publishes. The semantic key
cannot name or retain pages. The storage key cannot decide Markdown semantics.
There is no public API that accepts one key and returns an attachable suffix.

This is cleaner than moving a supposedly storage-authorizing token through the
representation-neutral composer. Parser composition and storage authority are
different responsibilities; combining them created the caller-echo and
cross-crate construction problems in the first gate.

## Implemented lineage receipt

`v3_runtime_slice::source` now contains the crate-private first half of the
storage key: `SourceStore::begin_lineage_adoption_bundle` freezes one exact
scalar lineage root and maps the restart boundary, retained prefix,
convergence boundary, and complete retained tail while validating each edit
record once. The consuming `LineageAdoptionBundleProof` is non-`Clone`; an
echoed `SourceSnapshotDescriptor`, pending job, changed region, broken chain,
or empty suffix cannot mint it.

The focused debug and release suites pass 8/8 and prove:

- two retained edits cost exactly two record validations and eight mapping
  attempts, rather than four independent history scans;
- equal-length edits in the retained prefix or tail fail closed with a typed
  changed region;
- only a mapped zero/Before boundary mints `EmptyRetainedPrefix`;
- the proof binds exact old and frozen-current revision/root/length
  descriptors and retains no Crop source root;
- a later source edit does not mutate the frozen historical proof, which makes
  the still-required live-epoch recheck explicit rather than implicit; and
- convergence affinity is cross-checked against the mapped tail edge. An
  insertion exactly at convergence with Before affinity is rejected because
  that boundary no longer equals the retained suffix start; the matching
  After-affinity mapping succeeds;
- replacements/deletions ending at a convergence cut stay before the retained
  tail, while the same operations beginning at that cut invalidate the tail;
  and
- corrupting the second lineage record cannot hide behind the constructor's
  first-record validation: both restart selection and adoption yield after the
  first record, then fail `BrokenChain` and cannot mint a proof.

This is not yet `BaseAdoptionProof`: it owns no arena page, checkpoint, open
path, or suffix cut. The storage-derived two-boundary resolver, restart
semantic recipe binding, resumable splice, current-revision manifest, and
latest-wins publication remain required before adoption is GO.

`SourceStore::begin_restart_selection` now supplies the other lineage moment.
Its non-`Clone` `RestartSelectionProof` maps one preferred restart with
Before affinity, the complete retained prefix, and an independent zero/Before
fallback in one frozen history pass. The focused debug and release suites pass
6/6: an insertion or deletion beginning exactly at the preferred boundary
remains after the parser
start; an equal-length prefix edit selects the proven zero fallback; an echoed
snapshot or out-of-range boundary fails construction; and each retained edit
record is validated once with at most three mappings. A deletion ending at the
restart invalidates the retained prefix, and a corrupt second record fails only
after the first fuel slice rather than being skipped. This proof selects where
parsing may begin, but deliberately has no suffix-attachment authority.

### Raw-offset entry points are mechanism-only HOLD

The current crate-private Stage-0 methods still take a copyable descriptor and
raw byte integers:

```rust
begin_restart_selection(from, preferred_restart: usize)
begin_lineage_adoption_bundle(
    from,
    old_restart: usize,
    old_convergence: usize,
    convergence_affinity,
)
```

They prove only scalar edit lineage. They check snapshot identity, range order,
and retained-region mapping, but they neither prove nor have enough historical
source state to prove that an integer is a UTF-8 scalar boundary, a physical
line boundary, or a checkpoint/cut in the named base manifest. For example, a
current multibyte source with no intervening edit can pass an in-range byte
inside one scalar straight through the Stage-0 mapping mechanism. Calling
these methods with composer-produced integers would therefore recreate a
caller-echo authority seam.

Production code must hide both methods behind storage-owned wrappers. The
recommended ownership is:

```text
CandidateWriter owns one OutputRootLease and LiveCandidateEpoch
  |
  +-- StoredRestartCheckpointCapability       non-Clone, non-owning
  |     base manifest + base source descriptor
  |     exact scalar/physical-line byte cut + prefix sequence cut
  |     grammar/profile + control/semantic restart state + open-path commitment
  |
  +-- CandidateStartBinding                   non-Clone, writer-owned
  |     consumed RestartSelectionProof + stored restart capability
  |     exact target source descriptor + actual parser start cursor
  |     live source/parse/build epoch
  |
  +-- StoredConvergenceCheckpointCapability   role-distinct, non-Clone, non-owning
  |     same base manifest/source + exact old scalar/line cut
  |     suffix sequence cut + grammar/profile + control/semantic suffix
  |
  +-- CandidateBoundaryCapability             non-Clone, writer-owned
        exact current scalar/line cut + parser state + live candidate epoch
```

The checkpoint capabilities contain private fields and are minted only by
source-directed descent through the actual base manifest. They retain no arena
owner themselves; the writer's one base-output lease supplies lifetime. The
restart wrapper derives the Stage-0 `from` and preferred integer from
`StoredRestartCheckpointCapability`, then transfers the consumed lineage proof
into `CandidateStartBinding`. The adoption wrapper derives all raw integers and
affinity from that binding plus the role-distinct convergence capabilities. It
must reproduce the exact selected restart rather than accept a second integer.

`RestartSelectionProof`, `LineageAdoptionBundleProof`, and the checkpoint/cut
capabilities are consumed by value at their authority transitions. Status
enums, `ProvenSourceMapping`, `ProvenRetainedPrefix`, descriptors, and raw
offsets remain diagnostic/data values and are never accepted by a bind or
attach API. Before and after every lineage poll, before parser start/build,
and at adoption bind/commit, `LiveDocumentStore` independently requires the
proof target, current `SourceStore` descriptor, `ParseToken`, queued generation,
base lease, grammar/profile, and `ArenaBuildId` to equal the writer's
`LiveCandidateEpoch`. A coherent frozen proof is intentionally not proof of
current liveness.

## Non-negotiable invariants

1. The immutable base is identified by a unique live-arena identity, local
   manifest address, manifest format/generation, and active parse base-output
   lease. Equal local `ArenaId { index, generation }` values in two arenas are
   not equal roots.
2. Old and current source identities come from `SourceStore` lineage, not a
   caller-supplied revision stamp. The proof binds both `SourceRevision` and
   `SourceRootId`, source lengths, the unchanged retained prefix before the
   restart point, old/current restart and convergence boundaries, and the
   complete unchanged retained tail.
3. The open capabilities are resolved by walking the base serialized-green
   root at the old boundary. They form one actual nested outer-to-inner path.
   Callers never supply a parallel `StableBinding[]` plus
   `GreenEnterCapability[]` pairing.
4. A complete semantic recipe is all-or-nothing. No action field may be
   ignored, and no permit may be consumed by a property-only substep.
5. Prefix construction, structural range replacement, source/projection
   replacement, property and close-fold rewrites, reference-root changes, and
   retained-suffix attachment happen in one arena-owned resumable build.
6. The candidate manifest names the exact current source revision and root,
   active parse generation, grammar revision/profile, semantic epoch, known
   range, serialized-green root, reference root, and restart root.
7. A candidate is invisible until the one manifest owner leaves the build
   journal and is accepted by the latest-wins coordinator. Failure changes no
   committed root.
8. `BlockId` may corroborate semantic identity inside an exact capability. It
   is never a locator. This design does not assume global `BlockId` uniqueness
   for storage authority and introduces no document-wide `BlockId` directory.
9. A pending semantic or source gap cannot be crossed by suffix adoption.
   Every `PendingGapRange` before the proposed suffix must either already be
   resolved by the parser or be consumed by an enumerated, complete resolution
   in the recipe. An `Unknown` marker is not convergence authority.
10. A candidate is bound to one live source/coordinator epoch. Source edits,
    coordinator admission, candidate polls, proof binding, and commit are
    serialized by the worker actor. Before every resumable proof/mutation
    slice and again at bind/commit, storage verifies the exact active
    `ParseToken`, current source descriptor, base-output lease, grammar/profile
    epoch, build ID, and absence of a newer queued generation. Any mismatch
    aborts the stale candidate; a snapshot proof cannot make an older revision
    live again.

## Root identity without bloating packed child edges

`PageArena` already mints a process-unique nonce and includes it in
`ArenaBuildId`, but public documents and green capabilities expose only local
eight-byte `ArenaId`s. The production boundary needs two identity levels:

```rust
/// Opaque, copyable identity; fields are private and only PageArena mints it.
pub struct ArenaIdentity(u64);

/// Public/root-scoped identity. It is not encoded on every child edge.
pub struct ArenaManifestId {
    arena: ArenaIdentity,
    local: ArenaId,
}

/// Local packed edges remain exactly index+generation (8 bytes).
struct LocalArenaId(ArenaId);
```

Minimal ownership hardening:

- add `PageArena::identity() -> ArenaIdentity`;
- bind `OwnedArenaRef` to `ArenaIdentity` and reject release/transfer to a
  different arena;
- store `ArenaIdentity` on `SerializedGreenDocument` and expose only
  `ArenaManifestId` at public root/capability boundaries;
- keep sequence child edges as local `ArenaId`, since a parent and child are
  necessarily in the same arena; and
- root-scope `GreenEnterCapability`, coverage/range capabilities, reference
  capabilities, and restart capabilities by `ArenaManifestId`.

This adds no bytes to persistent child edges. Root-scoped transient
capabilities become wider, which is appropriate: they cross component
boundaries and carry authority-relevant identity.

`ArenaBuildId` remains the exact candidate-session identity. A
`BaseAdoptionProof` stores the manifest ID plus a `LiveCandidateEpoch` that
owns the build ID. Every resume and poll verifies that build ID against
`ArenaBuildSession::id()` before reading or allocating.

## Source-lineage proof

The current `v3_runtime_slice::source` now has the right primitive to reuse:
completed `LineageMapJob::into_proof()` consumes the job into a non-`Clone`
`LineageMappingProof`. It binds source-store-derived `from` and `to`
`SourceSnapshotDescriptor`s (`revision`, `SourceRootId`, and byte length) plus
the exact unchanged range or boundary. Pending, changed, expired, and broken
lineage cannot upgrade into a proof.

There are necessarily two lineage moments because the convergence boundary is
not known when parsing restarts:

1. Candidate initialization runs a `RestartSelectionLineageJob`. It maps one
   provenance-selected restart boundary and complete retained prefix, plus the
   typed zero-boundary fallback, in one snapshot/pass. Its
   non-`Clone` `RestartSelectionProof` authorizes only the parser's starting
   cursor and retained-prefix eligibility; it cannot attach the prefix.
2. Once the parser proposes convergence, a
   `LineageAdoptionBundleJob` re-maps the restart boundary/prefix together with
   the convergence boundary/tail in one new immutable lineage snapshot and one
   pass over its edit records. Its consuming
   `LineageAdoptionBundleProof` is the only lineage value accepted by
   `BaseAdoptionProof`.

The bundle contains source-store-derived mappings over one exact pair of
snapshots for both immutable regions that the candidate retains:

```text
restart-prefix proof
  restart boundary:
    old restart boundary -> current restart boundary
    with Before affinity so an insertion at the boundary is parsed
  and, when the retained prefix is nonempty:
    [0, old restart boundary) -> [0, current restart boundary)

convergence-boundary proof
  old convergence boundary -> current convergence boundary
  with exact affinity

tail proof
  [old boundary, old source length)
    -> [current boundary, current source length)
```

The prefix and tail range proofs are the retained-region identities. A hash,
byte equality, an equal length, or the old `SourceTailId(u64)` is not
authority. An equal-length edit anywhere inside either retained region makes
the bundle job return `Changed { region, at_revision }` and adoption fails.

When both restart boundaries are zero there is no retained prefix range. The
store mints a typed `EmptyRetainedPrefix` only after the `Before`-affinity
restart-boundary proof maps `0 -> 0` over the same source descriptors. It does
not forge an empty `LineageMappingProof`, and it does not let a caller assert
that a nonzero prefix is empty.

The first gate does not retry an unbounded sequence of earlier checkpoints,
which could turn selection into `checkpoint_count * edit_history`. The
scheduler proposes one checkpoint at or before the earliest exact edited
provenance it can establish. The selection job maps that candidate and a zero
fallback in the same lineage pass. If the candidate prefix changed or its
mapping is ambiguous, selection returns the proven zero restart and reparses
from the beginning. A later optimization may map a bounded checkpoint batch in
one pass, but may not rescan lineage once per checkpoint.

The base-proof builder verifies:

- the bundle owns one `from` and one `to` descriptor for all four mappings;
- the bundle's `from()` equals the base manifest's source
  revision, source root, and source byte length;
- the bundle's `to()` equals the writer's immutable target descriptor, active
  `ParseToken`, coordinator active plan, and current
  `SourceStore` revision/root/length;
- the coordinator has no newer queued plan and its current parse generation
  still equals the writer's token generation;
- the bundle reproduces the initialization `RestartSelectionProof` exactly:
  same descriptors, restart affinity/mapping, and retained-prefix mapping or
  typed empty-prefix result;
- the storage-minted `CandidateStartBinding` ties that initialization proof to
  the selected checkpoint and actual candidate start cursor. Final proof
  construction borrows it and mints a one-use match; only successful
  `bind_adoption` consumes the writer-owned initialization proof;
- the restart boundary equals the selected checkpoint boundary and the
  candidate parser's actual start cursor;
- a nonempty prefix proof maps exactly `0..old_restart` to
  `0..current_restart`, while the empty-prefix variant is legal only at zero;
- the convergence-boundary and tail mappings name the same old/current
  convergence boundary;
- the mapped tail begins at those boundaries and ends at both snapshot ends;
- both boundaries are UTF-8 boundaries and physical-line convergence points;
  and
- the selected restart checkpoint belongs to the same base manifest and names
  the exact old restart boundary; and
- the selected convergence checkpoint belongs to that same manifest and names
  the exact old convergence boundary, grammar/profile, control continuation,
  and semantic suffix consumed by the composer.

No `AdoptionStamp` is accepted by any storage API.

### Snapshot and TOCTOU rule

`LineageMapJob` snapshots an immutable scalar lineage root, so a job can finish
correctly even after `SourceStore` has advanced. That proves history only up to
the job's frozen `to` descriptor; it does **not** prove that descriptor is
still live. The production wrapper therefore uses both snapshot consistency
and live-epoch checks:

- `LiveDocumentStore::accept_edit` is the only production edit entry point. It
  preflights both clocks and publishes the `SourceStore` and `Coordinator`
  changes as one actor state transition, so no candidate poll can observe a
  source revision that has not also invalidated its parse generation.
  Concretely, source prepares an immutable next root/transition without
  swapping it, coordinator preflights contiguous revisions/root and generation
  overflow without mutating, and the wrapper then commits both infallible
  state changes. Calling today's two mutating methods sequentially is not the
  production atomic boundary.
- `begin_candidate` first mints a `SourceParseEpoch` containing its
  `ParseToken`, exact target `SourceSnapshotDescriptor`, base-output lease, and
  grammar/profile epoch.
  The restart-selection job is bound to and polled under that epoch. After
  selection succeeds and `PageArena::begin_build` returns, storage extends it
  to `LiveCandidateEpoch { source: SourceParseEpoch, build: ArenaBuildId }`.
  The mandatory pre-build recheck makes the selection-to-build interval
  fail-closed; once the writer exists, its source and build identities are one
  epoch.
- `RestartSelectionLineageJob` and `LineageAdoptionBundleJob` each freeze an
  exact `from`/`to` descriptor pair at construction. They never consult a
  moving current root while polling.
- The source epoch is rechecked after restart selection and immediately before
  `begin_build`/base-manifest retain. A source edit in that cut point discards
  the selection proof without allocating a candidate.
- Before and after every fuelled proof poll, before `bind_adoption`, before
  every mutation poll, and immediately before arena/coordinator commit, the
  store compares the writer epoch and proof `to` descriptor with the current
  source/coordinator state. Because the actor is single-owner, source cannot
  mutate inside one poll slice.
- If an edit was accepted between any two slices, the queued/new generation
  makes `require_live_candidate` fail. Storage begins fuelled abort and
  promotes the newest plan. It does not merely decline suffix reuse and keep
  parsing the stale revision.
- `Coordinator::attach_candidate` and `Coordinator::commit` remain the final
  independent stale-generation guards after arena manifest construction.

If a future implementation permits `SourceStore` mutation from another
thread, this design is invalid without an atomic epoch/seqlock around source
and coordinator state. The selected design instead keeps these values inside
one worker actor.

### One-pass bundle and work bound

Do not implement the final proof as three or four independent
`LineageMapJob`s. They would each traverse the same retained edit history and
could snapshot different target revisions. Add a storage-facing bundle job:

```rust
struct LineageAdoptionBundleJob {
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    restart: BoundaryMapState,       // Before affinity
    prefix: RetainedPrefixMapState,  // Empty or 0..restart
    convergence: BoundaryMapState,
    tail: RangeMapState,
    // one immutable lineage root and one record cursor
}

struct LineageAdoptionBundleProof {
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    restart: ProvenBoundaryMapping,
    prefix: ProvenRetainedPrefix,
    convergence: ProvenBoundaryMapping,
    tail: ProvenRangeMapping,
}
```

One unit of bundle fuel validates one lineage record and applies that record
to the fixed set of at most four map states. Successful convergence therefore
visits `H` retained edit records once, with at most `4H` constant-time mapping
attempts. Candidate initialization separately visits at most `H` records for
its preferred restart/prefix plus zero-fallback selection, with at most `3H`
mapping attempts. The complete path is bounded by `2H`
record validations, rather than up to `4H` for four independent mappings
across initialization and convergence, and all counts appear in
candidate-only receipts. `H` remains bounded by lineage-ring retention and
every scan is fuelled.

The final bundle is started only after cheap source-provenance filtering,
exact control equality, and complete semantic composition have produced a
candidate recipe. It is not run at every parsed line. A changed tail declines
that recipe and parsing advances beyond the changed region before another
storage proof is attempted. Receipts count bundle attempts so adversarial
retries cannot hide repeated `H` work.

The lineage implementation still uses scalar-only history and retains no old
Crop root. The base manifest must therefore add `SourceRootId` and total source
bytes; those scalar values are sufficient to compare with the consumed proof.

### Eliminate the prototype's second source-identity vocabulary

`restart_composer_gate` currently uses its own `RevisionId`, `LineageId`,
`StableAnchor { PieceId, offset }`, and `SourceTailId`. Those were useful proof
types, but the v3 Crop-backed `SourceStore` cannot derive their `PieceId` or
tail scalar. Keeping them in production would recreate the exact parallel
authority this design is removing.

Production composer/parser contracts must use the same identity vocabulary as
the source store:

- shared `SourceRevision` and `SourceRootId` newtypes;
- byte boundary plus `BoundaryAffinity` at a physical-line boundary;
- source-run descriptors rooted in an immutable source snapshot or exact
  coverage capabilities; and
- the consumed `LineageMappingProof` for old-to-current identity.

Remove `LineageId` and `SourceTailId` from convergence authority. A small
grammar-free identity crate may own the shared newtypes if importing the v3
runtime into the composer would reverse dependencies. The recipe may carry
data-only source expectations, but it cannot mint an independent piece/tail
identity or convert between source models by equality or hashing.

## Opaque storage proof

The following is illustrative API shape. Its fields stay private to the
storage/adoption module:

```rust
pub struct BaseAdoptionProof {
    base_manifest: ArenaManifestId,
    live_epoch: LiveCandidateEpoch,
    lineage: LineageAdoptionBundleProof,
    start_match: CandidateStartMatch,
    prefix: RetainedPrefixCapability,
    old_convergence: GreenBoundaryCapability,
    current_convergence: CandidateBoundaryCapability,
    convergence_checkpoint: StoredConvergenceCheckpointCapability,
    open_path: ResolvedOpenPath,
    suffix: RetainedSuffixCapability,
    reference_base: ReferenceRootCapability,
    restart_base: RestartRootCapability,
    gap_epoch: CandidateGapEpoch,
}
```

Important properties:

- `BaseAdoptionProof` is neither `Clone` nor publicly constructible.
- The proof contains only the non-owning, arena-scoped `base_manifest`
  identity. `CandidateWriter` retains the actual `OutputRootLease` and
  `ArenaBuildOwner`; `LiveCandidateEpoch` records their expected identities
  for equality checks without transferring either ownership value into a
  proof that may be declined.
- `lineage` is one consumed bundle proof. `start_match` is minted only after
  that bundle exactly reproduces the writer-owned `RestartSelectionProof` and
  actual start cursor. The match is non-`Clone` and writer-scoped, but it does
  not consume the initialization proof during speculative composition.
- `bind_adoption` consumes `start_match` and the writer's initialization proof
  only after all semantic/storage preflight succeeds. Dropping or declining a
  `BaseAdoptionProof` leaves the writer able to test a later convergence
  boundary; successful bind makes replay impossible.
- `live_epoch` is revalidated at bind and every later mutation/commit slice.
  It is not considered valid merely because all lineage mappings share its
  historical target descriptor.
- `prefix` is resolved from the exact base manifest at the proven old restart
  boundary. Its nonempty variant carries the immutable sequence cut, boundary
  leaf, last fully retained page/subtree, summary, and restart open stack; its
  empty variant can be minted only from the typed zero-prefix lineage result.
  It is a non-owning capability scoped to the writer-retained base manifest.
- `convergence_checkpoint` is a non-owning, role-typed capability for the
  checkpoint at `old_convergence`. It cannot be constructed from or confused
  with the restart checkpoint. Its manifest, source-relative boundary,
  grammar/profile, control continuation, and semantic suffix are validated
  against the exact base before proof construction.
- `CandidateWriter`, not the proof, holds a retained base-manifest owner in the
  same arena build journal. The base therefore cannot disappear while the
  candidate is suspended, but declining one convergence candidate does not
  strand an owner inside a dropped proof. The writer releases its base owner
  before manifest commit after every retained child edge has been taken.
- `ResolvedOpenCapability` contains the exact manifest, leaf, byte offset,
  kind, source-open anchor, `Enter` capability, matching close/range
  capability, and depth. The open stack comes from one source seek; matching
  closes use balance summaries rather than scanning the suffix to EOF.
- `RetainedSuffixCapability` contains the immutable base sequence root,
  old/current source bounds, leaf cut, expected boundary leaf, first fully
  retained page, terminal page, and summary. Its constructor validates that
  the suffix is structurally attachable with the resolved open path.
- `ResolvedOpenPath` is an opaque chunked/fuelled path, not a required
  document-depth `Vec` allocation or recursive-drop chain.
- `gap_epoch` is minted from the current candidate writer. Bind rechecks the
  same epoch and current ledger, so a gap added after proof construction
  invalidates the proof. Gap ranges are never supplied in parallel by the
  caller.

There is deliberately no public getter that yields an `ArenaBuildOwner`, raw
suffix root, or attachable range. Semantic control/recipe composition should
run first from the parser's current state plus the stored convergence
checkpoint's data-only semantic suffix. The resulting recipe carries
expectations that storage later cross-checks against that exact role-typed
checkpoint capability; it does not require a `BaseAdoptionProof` view.
This avoids scanning lineage for control/semantic candidates the composer can
already reject.

## Storage-derived open path

The current serialized-green zipper already provides most of the mechanism:
`SerializedGreenDocument::seek` descends by source byte/UTF-16 summaries, and
`GreenStreamCursor::open_path` carries exact `GreenEnterCapability`s.

Add one storage-only two-boundary resolver:

```rust
fn resolve_adoption_regions(
    base: &SerializedGreenDocument,
    arena: &PageArena,
    start: &CandidateStartBinding,
    convergence: &StoredConvergenceCheckpointCapability,
    current: &CandidateBoundaryCapability,
) -> Result<ResolvedAdoptionRegions, AdoptionError>;
```

`old_restart` is derived from `start` and `old_convergence` from
`convergence`; accepting either again as a parallel scalar argument would
weaken the capability boundary.

It must:

1. seek both proven old source coordinates in the exact base manifest;
2. derive the nonempty retained-prefix sequence cut/summary at restart, or
   corroborate the typed empty-prefix variant at zero;
3. reconstruct the outer-to-inner structural stack from balanced
   `Enter`/`Exit` records;
4. bind every open `Enter` to its corresponding balanced range/close
   capability by summary-directed matching-close descent, without searching
   by ID or linearly walking the remaining document;
5. validate the restart checkpoint already bound by `start` against the base
   manifest, old restart boundary, grammar/profile/source identity, semantic
   open descriptors, restart path, and writer start cursor;
6. validate `convergence` against the same base manifest, exact old
   convergence boundary, grammar/profile, control continuation, and semantic
   suffix used by the composer; and
7. derive the immutable convergence suffix cut and summary from the second
   traversal.

`StoredRestartCheckpointCapability` and
`StoredConvergenceCheckpointCapability` are distinct storage-only wrappers
even if they share one packed checkpoint representation. This prevents a
restart capability from being accepted at the convergence parameter by type,
while the manifest and boundary checks prevent a sibling convergence
checkpoint from being substituted at runtime.

The recipe later pairs frames by exact depth and semantic expectation with
these resolved capabilities. `BlockId`, role, kind, and source-open anchor are
checked, but leaf+offset+manifest+depth locate the record.

Duplicate scalar `BlockId`s therefore cannot redirect a mutation. A duplicate
elsewhere in the document is irrelevant to authority. A repeated ID on the
resolved nested path is either accepted only when all depth/capability/source
expectations are distinct and match the parser's exact frames, or rejected as
`AmbiguousSemanticIdentity` if a semantic recipe tries to identify the frames
by the scalar alone. Parser-side ID-allocation uniqueness remains a useful
separate invariant, but adoption safety does not depend on it.

## Composer result: semantic proof, not storage authority

Rename `AdoptionPermit` to `CompleteConvergenceRecipe` and remove:

- `AdoptionStamp`;
- `StorageAdoptionContext`;
- `StorageAdoptionPlan`;
- `authorize_storage`;
- semantic-root scalar and generation from `CompositionContext`;
- arbitrary public `CapabilityId` from `StableBinding`; and
- every attachment/manifest/suffix-owner method from the composer crate.

The composer keeps the successful parts of the current gate:

- exact `ControlContinuation` comparison;
- physical-line-boundary requirement;
- aligned semantic prefix/suffix paths;
- list child-fold composition;
- raw-run composition;
- paragraph continuation/finalization;
- Setext and table promotion recipes; and
- reference definition/winner/consumer invalidation calculations.

Proposed result:

```rust
pub struct CompleteConvergenceRecipe {
    expectation: ConvergenceExpectation,
    frames: RecipeFrames,
    gap_resolutions: GapResolutions,
    suffix_semantics: SuffixSemanticExpectation,
}

pub struct ConvergenceExpectation {
    grammar: GrammarVersion,
    profile: ProfileId,
    old_revision: SourceRevision,
    current_revision: SourceRevision,
    old_boundary: SourceBoundaryExpectation,
    current_boundary: SourceBoundaryExpectation,
    opens: SemanticOpenPath,
}
```

These expectation values are semantic data, not an echoed storage stamp. They
can reject a mismatch, but they cannot authorize one. Constructors and fields
of `CompleteConvergenceRecipe` remain private to the composer; storage gets a
consuming iterator/getters over representation-neutral recipe values.

Each frame recipe must be complete. For example:

```text
PromoteSetext
  surviving block identity
  old Paragraph expectation
  new Heading kind + complete facts
  exact composed content SourceRuns
  source/projection replacement program
  close contribution / aggregate change

PromoteTable
  paragraph replacement or preface split
  table identity + complete table facts
  header/cell projection programs
  delimiter source ownership
  body child fold
  reference effects, if any

FinalizeParagraph
  Keep or reference-only wrapper removal/split
  all visible and hidden source/projection runs
  definition occurrences
  winner delta
  consumer invalidations
  resulting close contribution
```

There is no wildcard `..` match in storage translation. Exhaustive matching
must either translate every field or reject the whole recipe before commit.

### Pending gaps

The current candidate writer records every provisional or delayed range as:

```rust
struct PendingGapRange {
    range: CandidateRangeCapability,
    reason: PendingGapReason,
    required_authority: GapAuthorityMask,
}
```

`bind_adoption` rejects a recipe if any gap intersects the changed prefix,
open-ancestor recipe ranges, or adopted suffix and lacks one exact typed
resolution. A resolution consumes the gap capability and supplies all
structural, source/projection, property, aggregate, and reference changes.
The proof's `CandidateGapEpoch` must still equal the writer's ledger epoch at
bind; otherwise storage rejects it before consuming the recipe.

For the first implementation, the safest rule is stricter: the ledger must be
empty when adoption binds. Add typed reference-finalization resolution only
after the empty-ledger gate is green. An unresolved table header is always a
grammar uncertainty and can never be crossed by an `Unknown` range.

## One resumable candidate transaction

The candidate starts when the coordinator admits the parse, not when
convergence is discovered:

```rust
pub struct CandidateWriter {
    token: ParseToken,
    live_epoch: LiveCandidateEpoch,
    base_output: OutputRootLease,
    ticket: ArenaBuildTicket,
    base_manifest_owner: ArenaBuildOwner,
    restart_selection: RestartSelectionProof,
    build_state: CandidateBuildState,
    pending_gaps: PendingGapLedger,
    receipt: CandidateReceipt,
}
```

The parser streams exact current-revision prefix records into this writer.
`ArenaBuildOwner`s live in the arena-owned journal across suspension; a
short-lived `ArenaBuildSession` is borrowed only during a poll.

`LiveDocumentStore::begin_candidate` retains the coordinator-selected base
manifest into that journal and stores the owner on the writer. A declined
convergence recipe simply drops its data-only proof and continues parsing;
the writer keeps the one base owner for later convergence attempts or clean
EOF completion. Abort releases it through the normal fuelled journal, and
successful manifest construction releases it before the sole-owner commit.

Candidate initialization also derives the storage-owned
`RestartSelectionProof`. This is one resumable source-lineage pass before any
old prefix can enter the candidate plan. It proves the preferred restart
boundary with `Before` affinity and the complete nonempty retained prefix, or
selects the independently mapped typed zero fallback without a second history
scan. The writer binds that proof to its base checkpoint, live candidate epoch,
and actual current source cursor.

When convergence is proposed, the four-value
`LineageAdoptionBundleJob` proves restart, prefix, convergence, and tail again
against one frozen descriptor pair. Final proof construction borrows the
initialization proof and mints a writer-scoped `CandidateStartMatch` only after
the bundle reproduces it exactly. Successful bind consumes both; a declined
convergence leaves the initialization proof on the writer. No path trusts a
`ParsePlan` offset echoed by the caller.

`CurrentBoundaryCapability` is minted by `CandidateWriter::finish_line` after
the exhaustive source ledger closes a physical line. It is root/build/source
scoped and not publicly constructible; the parser does not pass a naked byte
offset back as authority.

The candidate is a splice over one immutable base, not an independently built
second full document:

```text
exact base prefix before restart
  + current transaction-owned records from restart to convergence
  + exact proven old suffix from convergence to EOF
```

Open ancestors may begin in the retained base prefix and close inside the
retained suffix. Their `Enter` facts and matching `Exit` close contributions
are enumerated recipe mutations against exact capabilities; they are not
recovered by reparsing or a `BlockId` lookup.

When convergence is proposed:

```rust
impl CandidateWriter {
    fn begin_base_proof(
        &mut self,
        store: &mut LiveDocumentStore,
        checkpoint: StoredCheckpointCapability,
        current: CurrentBoundaryCapability,
    ) -> Result<BaseProofJob, AdoptionError>;

    fn bind_adoption(
        &mut self,
        proof: BaseAdoptionProof,
        recipe: CompleteConvergenceRecipe,
    ) -> Result<AdoptionMutationJob, AdoptionError>;
}
```

Both methods are storage-internal in the production crate. Parser integration
sees a higher-level poll API and never receives either raw capability.

`AdoptionMutationJob` is a state machine, not one unbounded call:

```text
Preflight
  compare recipe expectations with source proof and resolved path
  validate every action variant and capability
  resolve all affected base leaves/ranges against one manifest
  prove no overlaps, gaps, duplicate targets, or stale children

BuildReplacementPrograms
  encode complete SourceProjectionRun replacements
  build typed large-fact/reference/restart pages

RewriteBoundaryPages
  decode/rewrite only intersected green leaves
  preserve total source byte/UTF-16 coverage
  validate balanced structure and close folds

ApplyBaseRanges
  apply sorted disjoint mutations against the one immutable base root
  replace the old restart-to-convergence interval with current parsed records
  rewrite exact open-ancestor Enter/Exit capabilities where folds changed
  retain the exact suffix range
  repack only a boundary leaf when the convergence cut is inside it

BuildCompositeManifest
  validate green/source/projection/reference/restart summaries
  encode exact current source root/revision and active parse generation
  allocate the manifest with every candidate component as a child

ReadyToCommit
```

Every phase accepts explicit fuel and may suspend. No destructor walks a large
journal.

### Exact suffix attachment

`persistent_sequence` already has the important immutable-base primitives:

- `retain_sequence_range_in_transaction`;
- `splice_owned_root_in_transaction`;
- `apply_disjoint_base_ranges_in_transaction`; and
- one-base leaf batch replacement.

The production adoption job uses a source/token range planner above these
leaf-level operations. It groups all changes by base leaf, re-encodes the
minimum boundary pages, applies disjoint base ranges right-to-left, and joins
the current prefix/replacements with the retained suffix owner.

The retained suffix is exact source and semantic input, not a promise that
every physical page after the cut is bit-identical. Open ancestors can have a
matching `Exit` in that tail whose close fold changes when the new prefix is
composed; the complete recipe rewrites those O(open-depth) exact close
capabilities. All closed suffix blocks and source/projection runs remain
unchanged, and no suffix content is reparsed.

If the convergence boundary is inside a packed leaf, that leaf must be split
and re-encoded because an `ArenaId` identifies the complete immutable page.
Every complete, unaffected suffix page and reusable suffix subtree after it
must retain exact identity. The success receipt must distinguish:

- boundary pages repacked;
- open-ancestor close/property pages rewritten;
- suffix leaf pages retained exactly;
- suffix branch roots retained exactly; and
- bytes/events semantically adopted.

Claiming the first partially intersected page is preserved would be false.

### Complete manifest

The current manifest has one green-root child and omits `SourceRootId`. Extend
it to a composite, versioned manifest:

```text
DocumentManifestV2
  ArenaManifestId                   transient/root scoped
  syntax profile
  grammar revision
  source revision
  source root ID
  source bytes + UTF-16
  parse generation
  semantic epoch
  known range / authority mask
  serialized-green summary + child ordinal
  reference-index summary + child ordinal
  restart-index summary + child ordinal
  projection-program registry summary/child ordinal when nonempty
```

Typed child ordinals and tags are validated on decode. A reference update
cannot commit beside an old green root, and a green suffix cannot commit beside
an old restart index. Large facts and projection programs use typed child
edges from packed pages or the manifest; they do not become copied aggregate
strings.

Suffix checkpoint pages are reusable only if checkpoint payloads are
source-relative: stable coverage/run capability plus local offset and exact
control/semantic continuation. They must not embed the old manifest's absolute
byte rank, `SourceRootId`, or `SourceRevision`. A query binds a retained
checkpoint to the new root-scoped manifest capability and derives its current
coordinate through sequence summaries. Any existing checkpoint record that
embeds old absolute source identity must be re-encoded or rejected; silently
retaining it would make the new current-revision manifest internally stale.

The same rule applies to reference occurrences, consumer capabilities, and
projection programs in a retained suffix: persistent payloads are relative to
stable coverage/source runs, while the composite manifest supplies the current
source identity. This is what allows a prefix insertion to retain large suffix
indexes without an O(document) coordinate repair pass.

Reference storage may index normalized labels to definition/consumer
capabilities. It may not become a document-wide `BlockId -> location`
directory. Consumer changes resolve through the exact old reference root and
candidate capabilities named in the complete recipe.

## Commit, rollback, and receipts

The final arena operation remains the existing strong primitive:

```rust
ArenaBuildSession::commit(manifest_owner) -> OwnedArenaRef
```

It succeeds only when the manifest is the sole remaining build owner. The
returned owner is wrapped in an arena-bound `SerializedGreenDocument` and
passed to `Coordinator::attach_candidate(token, ...)`. The coordinator again
checks that `token` is the exact active latest generation. A stale candidate
is scheduled for release and cannot publish.

Only after coordinator commit does the worker-current output change. The old
manifest remains independently owned and queryable until its normal remote/UI
lifetime ends.

Any failure before arena commit follows one path:

1. consume the live session with `begin_abort`, or let its constant-time
   `Drop` change the build to `Aborting`;
2. return a `CandidateAbort` containing only `ArenaBuildId` and candidate-only
   work/ownership receipts;
3. poll `PageArena::poll_build_abort(id, fuel)` until all build owners are
   scheduled; and
4. separately poll `PageArena::poll_reclaim(fuel)`.

No error path drains owners synchronously. No failed candidate receipt is
merged into committed-document metrics. A successful receipt is transferred
with the candidate manifest; an aborted receipt reports transient work and
eventual cleanup separately.

Required receipt fields include:

```text
restart-selection and adoption-bundle records validated
per-region lineage mapping attempts and first Changed revision
base sequence nodes visited
replacement/boundary pages allocated
reference/restart/projection pages allocated
exact prefix and suffix pages/subtrees reused
source bytes and events replaced/adopted
maximum owner-journal handles/bytes
allocations before failure
owners scheduled during abort
nodes/payload/edges reclaimed
old-root live-node delta (must remain zero)
```

## Call graph

```text
LiveDocumentStore::accept_edit
  -> atomically advance SourceStore + Coordinator actor state
  -> exact SourceTransition
  -> ParsePlan { token, base_output }

LiveDocumentStore::begin_candidate(active plan)
  -> verify SourceStore current revision/root == ParseToken
  -> mint SourceParseEpoch { token, target descriptor, base_output }
  -> RestartSelectionLineageJob snapshots one source descriptor pair
       -> map preferred restart Before-boundary + [0, old restart)
       -> map typed zero-boundary fallback in the same pass
       -> select preferred only if its entire prefix is unchanged; else zero
       -> one pass over retained edit records
  -> bind RestartSelectionProof to checkpoint + actual parser start cursor
  -> revalidate SourceParseEpoch; stale selection is discarded with no build
  -> PageArena::begin_build
  -> extend SourceParseEpoch with ArenaBuildId -> LiveCandidateEpoch
  -> resume once and retain coordinator-selected base manifest in build journal
  -> CandidateWriter { non-Clone ticket, base_output fixed by coordinator }

parser polls
  -> require exact live candidate epoch; abort if a newer edit is queued
  -> resume build briefly
  -> append exact current prefix records / source ledger
  -> suspend

parser reaches a physical-line convergence candidate
  -> select StoredCheckpointCapability from base manifest restart root
  -> cheap exact source-provenance/control checks
  -> Composer::compose(current semantic prefix, stored semantic suffix)
       -> reject and continue parsing, or
       -> non-Clone CompleteConvergenceRecipe
  -> CandidateWriter::begin_base_proof
       -> require exact live epoch before snapshotting lineage
       -> LineageAdoptionBundleJob snapshots one source descriptor pair
       -> map restart + optional prefix + convergence + tail in one pass
       -> consume into LineageAdoptionBundleProof
       -> reproduce writer's RestartSelectionProof/start binding
       -> mint non-Clone CandidateStartMatch without consuming writer proof
       -> serialized-green seeks at old restart and convergence boundaries
       -> resolve retained-prefix, nested open/range, and suffix capabilities
       -> verify live epoch against active source store/token/coordinator queue
       -> private BaseAdoptionProof

CandidateWriter::bind_adoption(proof, recipe)       private
  -> revalidate live epoch after any yielded proof/composer work
  -> neither input alone has an attach operation
  -> exhaustive cross-check + empty/resolved PendingGapRange ledger
  -> consume CandidateStartMatch + writer RestartSelectionProof only on success
  -> AdoptionMutationJob

AdoptionMutationJob::poll(fuel)
  -> revalidate live epoch before each slice; stale means fuelled abort
  -> one ArenaBuildSession slice
  -> prefix/range/projection/property/reference/restart mutations
  -> exact retained suffix
  -> current-revision composite manifest owner
  -> suspend or ReadyToCommit

CandidateWriter::commit
  -> revalidate live epoch and absence of a queued newer plan
  -> ArenaBuildSession::commit(sole manifest owner)
  -> Coordinator::attach_candidate(exact token)
  -> Coordinator::commit(exact token)
  -> revision-bound presentation delta
```

## Current APIs to reuse and minimal additions

| Current API/evidence | Reuse | Exact addition needed |
|---|---|---|
| `PageArena::begin_build`, `resume_build`, `ArenaBuildSession`, `suspend`, `begin_abort`, `poll_build_abort`, `commit` | Keep as the ownership/lifetime backbone. `ArenaBuildId` already binds the unique arena nonce, build slot, and generation. | Expose opaque `ArenaIdentity`; bind public root owners/manifests/capabilities to it. Add candidate receipt plumbing, not a second transaction system. |
| `ArenaBuildOwner` and arena-owned `OwnerJournal` | Keep linear owners across yields and constant-time cancellation transition. | Migrate persistent sequence and serialized-green builders from legacy `ArenaBuildTransaction` to `ArenaBuildSession`/`ArenaBuildOwner`. |
| `LineageMapJob::poll` and new consuming `into_proof` / `LineageMappingProof` | Reuse its immutable scalar snapshot, record validation, map functions, fuel semantics, and unforgeable proof pattern. | Add one-pass `RestartSelectionLineageJob` (preferred restart/prefix plus zero fallback) and four-value `LineageAdoptionBundleJob`; each scans one snapshot once. The final bundle owns one descriptor pair and one consuming proof. Add typed zero-prefix, region-specific `Changed`, and receipt counters. |
| `Coordinator::require_current_active` semantics plus `SourceStore` descriptors | Reuse exact token/generation/queued-plan rejection as the final publication guard. | Put production edits and candidate work behind one `LiveDocumentStore` actor; add prepared-source-edit and nonmutating coordinator-preflight APIs so source/coordinator advance as one infallible state transition. Add `SourceParseEpoch`, `LiveCandidateEpoch`, and `require_live_candidate` checks around every yielded proof/mutation slice and at bind/commit. |
| `SerializedGreenDocument::seek`, `GreenStreamCursor::open_path`, `GreenEnterCapability` | Reuse source-summary descent and exact Enter coordinates. | Add arena-bound manifest identity, matching close/balanced-range capability, storage-only two-boundary resolver for retained prefix/open path/suffix, source-root metadata, and root-scoped capability validation. |
| `rewrite_enters` | Retain its one-base validation and packed leaf rewrite logic as a lower-level property-rewrite test. | Do not use it as adoption endpoint. Generalize to a resumable multi-range planner that also rewrites source/projection/structure/Exit aggregates and advances source identity. |
| `replace_leaf_batch_in_transaction`, `apply_disjoint_base_ranges_in_transaction`, `retain_sequence_range_in_transaction`, `splice_owned_root_in_transaction` | Reuse balancing and immutable suffix sharing. | Port them to resumable build owners; add token/source-range-to-leaf planning and boundary repacking. Do not expose absolute persistent ranks. |
| `SerializedGreenBuildReceipt` | Reuse allocation/reuse counters. | Stage as candidate-only receipt; add suffix identity, reference/restart/projection, late-failure, abort, and reclaim receipts. |
| `Coordinator` latest-one admission, attach, promotion, commit | Keep publication semantics and stale generation rejection. | Bind `OutputRootLease` to `ArenaManifestId` rather than a bare local `ArenaId`; verify manifest source root/revision and parse generation on attach. |
| `restart_composer_gate` control/semantic algorithms and action variants | Keep the representation-neutral semantic composition. | Replace permit/stamp/storage authorization with private-constructed `CompleteConvergenceRecipe`; make every variant complete; remove `CapabilityId` as storage authority. |
| `composed_adoption_storage_gate` Setext test | Keep as a narrow lower-level receipt until superseded. | Do not extend the adapter. Build the new atomic gate beside it, then retire/rename the old crate so it cannot be mistaken for production proof. |

The largest mechanical change is the sequence/builder migration. Avoid a
generic trait that supports both transaction models indefinitely. Port the
selected packed path to `ArenaBuildSession` and retain small synchronous test
helpers that begin/resume/commit internally. Keeping legacy and resumable
production builders side by side would create two ownership semantics at the
most sensitive boundary.

## Cross-crate dependency and construction discipline

Recommended dependency direction:

```text
restart_composer_gate
  representation-neutral grammar/control/semantic recipe types
             ^
             |
atomic_adoption_store (or v3 runtime adoption module)
  owns BaseAdoptionProof, candidate writer, green/reference/restart storage
             ^
             |
parser integration / worker coordinator
```

The composer does not import arena or serialized-green types. Storage imports
the composer result type.

Private constructors remain enforceable across crates:

- `CompleteConvergenceRecipe` fields and constructor are private to the
  composer. It exposes a consuming representation-neutral iterator and
  read-only expectations.
- `BaseAdoptionProof` fields and constructor are private to storage. It is
  never passed into the composer.
- Storage's `bind_adoption` is the only function that can see both concrete
  types. It is private or `pub(crate)` and returns an opaque mutation job, not
  a suffix owner.
- The parser integration calls one orchestration method; it cannot construct a
  proof by repeating scalars or pair public stable/physical paths.

Rust privacy protects construction discipline; storage validation still checks
all data because privacy is not a substitute for corruption/staleness checks.

## Adversarial executable test matrix

### Authority and identity

1. **Real cross-arena collision.** Build two fresh `PageArena`s with identical
   allocation shapes so their local manifest `ArenaId`s are equal. A proof or
   document from arena A used with arena B must fail on `ArenaIdentity` or
   `ArenaBuildId` before decoding or allocating. Both roots remain queryable.
2. **Wrong build session.** Suspend candidate A and try to bind its proof while
   candidate B is resumed in the same arena. Reject `CrossBuildProof`; neither
   journal transfers an owner.
3. **Wrong base manifest generation/address.** Reclaim and reuse a local slot,
   then replay an old checkpoint/root capability. Reject before mutation.
4. **No caller echo.** Compile-fail tests prove callers cannot construct
   `BaseAdoptionProof`, `CompleteConvergenceRecipe`, or call private bind/attach
   primitives. There is no public `AdoptionStamp` round trip.

### Source lineage and tail

5. **Wrong old revision/root.** Select a checkpoint from a different published
   manifest while using the active base output. The consumed bundle proof's
   `from` descriptor differs and proof construction fails.
6. **Restart Before affinity.** Insert bytes exactly at a nonzero old restart
   boundary. The restart maps to the position before the insertion, the
   retained prefix ends there unchanged, and the parser consumes the inserted
   bytes. An After-affinity start is rejected.
7. **Changed retained prefix.** Make both length-changing and equal-length
   edits inside `[0, old_restart)`. Restart selection rejects that checkpoint
   and returns its independently proven zero fallback from the same history
   scan; it never retains the changed prefix merely because the boundary maps.
8. **Typed zero prefix only.** At old/current restart zero, a mapped `0 -> 0`
   Before-affinity boundary mints `EmptyRetainedPrefix`. Attempting the empty
   variant at either nonzero boundary, or fabricating an empty range proof,
   fails.
9. **Equal-length edit inside tail.** Change bytes after the proposed boundary
   without changing length. Tail mapping returns `Changed`; no proof is minted.
10. **One snapshot and one pass.** With `H` retained edit records, the final
    bundle validates exactly `H` records while mapping restart, prefix,
    convergence, and tail to one `from`/`to` descriptor pair. Instrumentation
    proves no hidden per-value lineage scan. Initialization plus convergence
    validates at most `2H` records.
11. **Advance during restart selection.** Yield partway through the one-pass
    initialization job, or finish it but pause before `begin_build`, then
    accept a new edit. `SourceParseEpoch` revalidation discards the proof and
    no arena build/base owner is created.
12. **Advance between restart selection and convergence.** Accept a new edit
    after `RestartSelectionProof` starts the parser, including while semantic
    composition yields or after it returns a recipe but before the final bundle
    is constructed. `require_live_candidate` observes the queued/new
    generation, aborts the old build, and promotes the new plan. The old start
    proof/recipe cannot be paired with the new source descriptor.
13. **Advance during bundle polling.** Poll part of a bundle under one-unit
    fuel, accept an edit, then poll again. The immutable job could still prove
    its historical snapshot, but the orchestration rejects its stale live
    epoch before another slice/bind and begins fuelled abort.
14. **Advance after proof before bind.** Finish and consume a valid bundle,
    then accept an edit before `bind_adoption`. Exact source descriptor,
    active token, generation, and queued-plan checks reject it without
    attaching any base range.
15. **Advance after bind/allocation.** Allocate at least one replacement page,
    accept a newer edit, and resume. The next mutation-slice check aborts; all
    candidate owners reclaim and the old root remains queryable. If the edit
    arrives after manifest construction, coordinator attach/commit rejects the
    stale token independently.
16. **Wrong convergence affinity.** A deletion exactly at the boundary maps
   differently for Before/After. Only the parser-declared physical-line
   affinity can prove convergence.
17. **Expired/broken lineage.** Ring overwrite and deliberately broken chain
   fail closed and cleanly fall back to further parsing/EOF.

### Path and recipe

18. **Nonnested path.** Create sibling Paragraph capabilities with plausible
    IDs and feed semantic frames as parent/child. Storage's zipper-derived path
    cannot match; reject before allocation.
19. **Duplicate IDs cannot redirect.** Put the same scalar `BlockId` on a
    distant block and the target open block. The leaf+offset+manifest+depth
    capability mutates only the resolved target. A scalar-only recipe is
    rejected. Also test repeated IDs on the open path with distinct source
    anchors.
20. **Reordered/missing/extra frame.** Every mismatch between recipe frames and
    resolved path fails; no prefix action is partially consumed.
21. **Every action field matters.** Mutate Setext content, table preface,
    reference definitions/winner, raw runs, list child fold, and close
    contribution independently. Omitting or altering any field makes the
    result differ from a clean parse or fail validation.
22. **Pending gap.** Leave a reference-finalization or table-recognition
    `PendingGapRange`. Binding fails. Then supply the enumerated full typed
    resolution (when implemented) and prove that the gap capability is
    consumed exactly once.

### Atomic mutation and sharing

23. **Declined proof owns no base lease.** Construct a valid
    `BaseAdoptionProof`, make final recipe/path preflight decline (or explicitly
    discard the speculative proof), and drop it. The writer still owns exactly
    its original base manifest owner and restart-selection proof and can
    continue parsing to a later convergence point. No owner is stranded,
    released twice, or transferred through the proof. Mint two speculative
    start matches, bind one successfully, and prove the second fails after the
    writer's start proof is consumed.
24. **Late failure after allocations.** Use a multi-leaf, multi-action recipe;
    let the first replacement and reference pages allocate, then fail a later
    close-fold/projection-child validation. Receipt must show nonzero transient
    allocations. Old root remains byte-for-byte queryable. Fuelled abort and
    reclaim return live-node, payload, edge, and owned-reference metrics to the
    exact pre-candidate baseline.
25. **Failure after manifest allocation.** Inject stale coordinator generation
    after the composite manifest is built. Coordinator rejects the candidate;
    releasing it reclaims every candidate-only component while preserving the
    old published root.
26. **Exact retained-region page identity.** Edit inside a large document with
    substantial proven prefix and suffix. Restart/convergence boundary leaves
    and O(open-depth) ancestor property/close pages may change; complete
    unaffected prefix/suffix leaves and at least one large reusable subtree on
    each available side retain exact `ArenaId`s. Receipt distinguishes
    repacked/rewritten pages from exact reuse. A zero-prefix case reports no
    fabricated prefix page.
27. **Old and new roots concurrently queryable.** New root reports current
    revision/root and new Setext/table/reference semantics; old root reports
    its old source identity and semantics. Both return the same unchanged far
    suffix page IDs.
28. **Complete reclaim.** Release new root, poll to completion, verify old root
    still works. Release old root, poll to completion, and verify zero live
    nodes/bytes/edges and zero pending releases. Repeat after cancellation at
    every mutation phase.
29. **Sparse journal/fuel receipts.** Suspend after many transferred/released
    owners. Abort with fuel `0`, small fuel, and remainder. Each unit schedules
    at most one live owner; sparse holes cause no hidden scan.

### Product-equivalence gates

30. For every CommonMark/GFM adoption case, compare the incrementally adopted
    document against a clean exact parse for structure, facts, physical
    coverage, logical projection, definitions/winners/consumers, restart
    checkpoints, and rendered output.
31. Fuzz edits before, at, and inside proposed convergence tails, including
    Unicode, tabs, tables, reference-only paragraphs, nested lists, raw HTML,
    and long fenced code. Incremental output must equal clean output or decline
    adoption and continue parsing.
32. Run the same gates with forced one-unit fuel, cancellation at every yield,
    release-mode overflow/corruption checks, strict Clippy, Miri where useful,
    and WASM compilation.

## Implementation sequence

The replacement should be built in dependency order; no later slice can paper
over a failed earlier authority gate.

1. **Arena-root identity gate.** Add `ArenaIdentity`, arena-bound root owners
   and manifest capabilities, and the real two-arena collision test. Keep local
   child edges unchanged.
2. **Manifest/source gate.** Add source root/length and composite typed child
   schema. Make coordinator attach verify current source identity/generation.
3. **Resumable packed mutation gate.** Port persistent sequence and
   serialized-green builders to `ArenaBuildSession`/`ArenaBuildOwner`; prove
   yield, abort, late failure, and exact suffix sharing.
4. **Bundled lineage and boundary/path proof gate.** Add the one-pass preferred
   restart/prefix plus zero-fallback selection job, four-value adoption bundle,
   live-candidate epoch checks, and storage-derived green open/range/suffix
   capabilities. Consume
   them into private `BaseAdoptionProof`. Prove retained-prefix, wrong-tail,
   every source-advance cut point, bounded record visits, nonnested-path,
   duplicate-ID, and stale-root rejection.
5. **Composer refactor gate.** Replace `AdoptionPermit` with
   `CompleteConvergenceRecipe`; remove storage authority and make Setext,
   list/raw/table/reference recipes exhaustive. Prove it cannot attach alone.
6. **Atomic two-key gate.** Consume both keys into the one candidate job. Cover
   all structural/source-projection/property/reference/restart mutations and
   current-revision manifest commit; prove neither key alone exposes reuse.
7. **Direct parser gate.** Replace proof-era events with parser-site typed
   writes and an exhaustive source ledger, then run clean-parse differential,
   cancellation, memory, large-document, WASM, Flutter revision-binding, and
   physical-device liveness gates.

Do not extend `composed_adoption_storage_gate` action by action. Its public
shape makes partial success easy and atomic success awkward. Keep its narrow
Enter-rewrite receipt while building the replacement, then remove it once the
atomic two-key gate proves strictly more.

## Rejected alternatives

### Keep `AdoptionPermit` and make `StorageAdoptionContext` private

Rejected. It still assigns attachment authority to a representation-neutral
component that cannot derive physical storage truth. Cross-crate adapters
would continue pairing two worlds and could consume only part of the permit.

### Let storage trust a composer stamp after checking the manifest scalar

Rejected. Repeating revisions, boundaries, tail IDs, or local arena slot IDs
does not prove their source. The exact bug is already present in the focused
gate.

### Search the document by `BlockId` when applying each action

Rejected. It adds a global directory or document walk, makes duplicate IDs an
authority ambiguity, and separates mutation location from the convergence
boundary that proved it. The zipper already has the correct coordinate path.

### Commit property changes first and attach the suffix later

Rejected. This consumed the Setext permit while dropping content in the first
gate. Intermediate immutable roots are still semantically partial roots if
published. One manifest must represent the complete current revision.

### Preserve the first suffix page even when the cut is inside it

Rejected. Page identity covers the entire encoded payload. Repack the boundary
page and make exact reuse claims only for complete retained pages/subtrees.

### Support unresolved gaps by marking them `Unknown`

Rejected as default convergence authority. `Unknown` can be a presentation
state, but it does not prove that suffix structure/reference semantics are
independent. Require an empty gap ledger first, then add narrow typed gap
resolutions with explicit dependency proofs.

## Resulting judgment

The failed composed gate does not invalidate packed serialized green or
restart-and-converge. It demonstrates that the authority boundary was assigned
to the wrong layer.

The coherent replacement is:

- composer proves complete semantic compatibility;
- storage proves actual base/source/path/suffix identity;
- neither proof can attach by itself;
- one resumable candidate consumes both and applies the whole recipe; and
- one arena-bound, current-revision composite manifest is the only published
  result.

This design has more prerequisite work than extending `rewrite_enters`, but it
removes rather than accumulates architectural seams: no caller-echo stamp, no
dual stable/physical capability list, no partial permit consumption, no
cross-arena root alias, no `BlockId` directory, and no second commit for the
suffix. Those are the properties required for the live editor's exactness and
large-document lifetime model.
