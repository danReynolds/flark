# Source-bound candidate build authority

Status: **worker authority and atomic edit admission implemented; manifest writer still required before composed adoption**, 2026-07-16.

This note closes one narrow but essential authority gap between the Crop source
store, the resumable arena builder, serialized green, and the coordinator. It
does not introduce another source identity vocabulary or another persistent
index.

## Verdict

The existing primitives are sufficient:

- `SourceSnapshotLease` is the exact immutable source read authority;
- `SourceSnapshotDescriptor { revision, root, bytes }` is its data-only
  description;
- `ArenaBuildId` is the generation-safe candidate build epoch;
- `ArenaIdentity` scopes every persistent owner; and
- source/path capabilities, rather than scalar `BlockId`s, remain mutation
  authority.

The worker-owned seam now holds these values together. The remaining authority
gap is output construction and transfer: the serialized-green codec stores
source revision, root, and length, but its
public `SerializedGreenRootSpec` still lets a caller assert those scalars.
Likewise, `BlockId(pub u64)` and `CoverageId(pub u64)` are freely constructible,
and `Coordinator::attach_candidate` accepts an arbitrary `OwnedArenaRef`.
Those are appropriate proof-harness surfaces, not production authority.

This audit does **not** expose a reason to change the selected parser or packed
green architecture. It tracks three P0 integration requirements:

1. the source store, coordinator, arena, identity counters, and candidate are
   driven by one worker-owned `LiveDocumentStore` actor (**implemented**);
2. a candidate manifest must be constructed only by a source-bound writer and
   attached through a typed manifest owner; and
3. production parser writes must use build-scoped identity permits, never raw
   scalar IDs or a caller-supplied root spec.

The first slice also resolved two lifecycle gaps:

- parser lease issuance is private to `LiveDocumentStore` and limited to its
  one active candidate, while cloneable query snapshots cannot upgrade into
  parser/build authority;
- revision zero is admitted through the normal active generation-one parse for
  the exact initial root; generation zero remains bootstrap output, not a
  parsed document.

## Implementation receipt

The first worker-owned authority slice now exists in
`v3_runtime_slice::{live_document,source,coordinator,arena}`:

- `LiveDocumentStore` owns the source, coordinator, arena, candidate ticket,
  document-wide ID allocators, and fuelled abort handles;
- empty and nonempty revision-zero documents admit the normal generation-one
  parse;
- only one exact source cursor and generation-safe arena ticket can be issued
  for the active parse;
- build-scoped block/coverage permits are linear and IDs burn across abort;
- cancellation invalidates source access immediately, starts arena abort in
  constant time, and transfers journal owners under exact fuel; and
- `accept_edit` composes linear prepared source and coordinator transitions so
  every recoverable validation/allocation happens before candidate detachment,
  followed only by assignment-only publication inside one worker turn.

The adversarial suite covers cross-document/stale/recycled epochs, exact
Unicode/CRLF cursor bytes, wrong source descriptors, invalid/split-scalar
ranges, coordinator rejection after source preparation, repeated newest-plan
replacement, a real partial-build owner under zero/one abort fuel, and
separately fuelled arena reclaim. The complete runtime slice passes debug and
release all-target tests, formatting, strict Clippy, and a rustup-toolchain
`wasm32-unknown-unknown` check.

This closes source/build issuance and edit-clock atomicity. It does **not** yet
close the rest of this design: parser operations can still observe raw scalar
IDs in the mechanism harness, serialized-green clean construction still uses
the legacy nonresumable transaction, and no typed `CandidateManifestOwner`
can yet be attached through the production coordinator lane.

## Authority rule

No production constructor accepts any of the following as proof:

```text
SourceRevision
SourceRootId
source byte length
SourceSnapshotDescriptor
SerializedGreenRootSpec
BlockId
CoverageId
OwnedArenaRef
```

They are data. Authority is the possession and successful consumption of the
opaque values below, all minted inside the live document actor.

## Smallest production-shaped API

The names are illustrative. The visibility and ownership constraints are the
contract.

```rust
pub struct LiveDocumentStore {
    source: SourceStore,
    coordinator: Coordinator,
    arena: PageArena,
    identities: DocumentIdentityAllocator,
    candidate: Option<CandidateJob>,
}

// Private fields; neither Clone nor Copy.
struct BoundSourceCursor {
    descriptor: SourceSnapshotDescriptor,
    cursor: CropSourceCursor,
}

// Query identity, not an owner. All fields private.
#[derive(Clone, Copy, PartialEq, Eq)]
struct LiveCandidateEpoch {
    token: ParseToken,
    source: SourceSnapshotDescriptor,
    arena: ArenaIdentity,
    build: ArenaBuildId,
}

struct CandidateWriter {
    epoch: LiveCandidateEpoch,
    source: BoundSourceCursor,
    ticket: ArenaBuildTicket,
    ledger: CandidateSourceLedger,
    green: ResumableGreenBuilder,
    // reference/restart/large-fact builders join this same journal later
}

// Private fields; neither Clone nor Copy. This is the only value attachable
// to the coordinator in the production lane.
pub(crate) struct CandidateManifestOwner {
    owner: OwnedArenaRef,
    manifest: DocumentManifestId,
    epoch: LiveCandidateEpoch,
}
```

`LiveDocumentStore` owns or exclusively drives all five components on one
worker actor. Source edits and candidate polls cannot interleave inside one
actor turn. If this changes in the future, the design requires an atomic epoch
or seqlock; descriptor comparison by itself is not enough.

### Initial construction

`LiveDocumentStore::new` takes source text and creates:

- `SourceRevision(0)` with a nonzero freshly minted `SourceRootId`;
- a bootstrap/unparsed output at parse generation zero; and
- an active initial parse token at `ParseGeneration(1)` for the exact
  revision-zero source descriptor.

An empty revision-zero source is valid. It emits a Document Enter/Exit with
zero physical coverage and seals against `source.bytes == 0`. Zero must not be
used as a sentinel for source revision. `SourceRootId(0)`, fresh entity ID
zero, and exhausted counters remain invalid.

### Candidate admission

The only production source-lease issuance path is conceptually:

```rust
impl LiveDocumentStore {
    fn begin_candidate(&mut self, token: ParseToken) -> Result<(), BeginError>;
}
```

It performs, in one actor turn:

1. require `token` to identify the current active parse;
2. derive `{ revision, root, bytes }` from the current `SourceStore` root;
3. require token revision/root to equal that descriptor;
4. require the coordinator and arena to name the same `ArenaIdentity`;
5. call `PageArena::begin_build`;
6. consume the non-cloneable source wrapper into `BoundSourceCursor`; and
7. store one `CandidateWriter` containing the resulting source and build
   identities.

There is no public `begin_candidate(descriptor, spec, ...)`. A public
`SourceSnapshotDescriptor` may be freely copied or even forged because no
authority-bearing operation accepts it without re-deriving and comparing it
inside the actor.

`SourceStore::current_root` becomes `pub(crate)` in the production crate (with
a test-only observer if needed). If read-only consumers need source access,
they receive a separate, explicitly cloneable query snapshot type that cannot
be converted into a parser/build lease.

### Poll and live checks

Every candidate poll starts and ends with:

```text
candidate token == coordinator active token
candidate source descriptor == SourceStore current descriptor
candidate arena == PageArena identity == Coordinator arena identity
candidate build == suspended/resumed arena build generation
```

The same check runs:

- after restart selection and before `begin_build`;
- before and after a lineage/adoption proof poll;
- before binding an adoption proof;
- before and after each packed mutation slice;
- immediately before manifest allocation; and
- immediately before coordinator attachment/commit.

Because the actor is single-owner, source cannot change between a check and
the bounded work in the same slice. An accepted edit between slices makes the
old candidate stale and starts fuelled abort; it never downgrades stale output
to partial authority and never continues parsing the old root.

### Source-complete sealing

The writer, not the caller, derives the manifest source fields. A complete
clean build can seal only by consuming a private `CleanSourceSeal` minted when:

```text
the bound cursor reached exact EOF
the exhaustive claim ledger is empty
claimed physical bytes == source descriptor bytes
claimed physical UTF-16 == the cursor's checked UTF-16 count
green/composite source summaries equal the same totals
```

An incremental build can seal only by consuming the source-completion side of
`BaseAdoptionProof`: exact retained prefix, current parsed interval, and exact
mapped suffix must form one ordered, nonoverlapping partition of the writer's
current descriptor. A partial publication uses an equally private composite
seal containing certified regions plus an explicit `UnknownRange`. It does not
accept a naked caller-supplied `known_bytes` range.

Internally the seal derives the codec's root fields:

```rust
struct BoundManifestFields {
    source_revision: SourceRevision,
    source_root: SourceRootId,
    source_bytes: u64,
    source_utf16: u64,
    known: CertifiedAuthorityMask,
}
```

`SerializedGreenRootSpec` and `SerializedGreenDocument::build(spec, events)`
remain crate-private mechanism/test helpers. The production builder accepts a
borrow of `CandidateWriter` plus a consumed source seal. There is no conversion
from `SourceSnapshotDescriptor` or `BoundManifestFields` exposed to parser
code.

### Typed manifest transfer

Arena commit wraps the sole returned owner immediately:

```text
ArenaBuildSession::commit(manifest_owner)
    -> OwnedArenaRef
    -> CandidateManifestOwner { owner, manifest, epoch }
```

Only that private conversion can create `CandidateManifestOwner`.
`Coordinator::attach_candidate` in the production lane consumes this typed
value and independently verifies:

- its manifest is in the coordinator's arena;
- decoded manifest source revision/root equals both its epoch and the
  coordinator's current source;
- decoded parse generation equals the active token;
- grammar/profile/schema generations are current; and
- its arena owner is the exact decoded typed manifest root.

On any rejection the complete typed owner is returned to the caller for
fuelled retirement. The generic coordinator proof harness may retain an
`OwnedArenaRef` overload under test configuration, but runtime code cannot
call it.

## Fresh identity minting

`ArenaBuildId` is reused as the transient build epoch. It already contains
arena nonce, build-slot index, and generation, and its ticket is linear. A
second build-epoch system would add vocabulary without adding authority.

Stable scalar IDs come from two document-wide monotonic counters, not counters
that restart at one for each build:

```rust
struct DocumentIdentityAllocator {
    next_block: NonZeroU64,
    next_coverage: NonZeroU64,
}

// Linear, private fields, no Clone/Copy.
struct FreshBlockPermit {
    build: ArenaBuildId,
    id: BlockId,
}

struct FreshCoveragePermit {
    build: ArenaBuildId,
    id: CoverageId,
}
```

Minting happens inside a candidate poll while the actor has both the current
allocator and writer. The permit is tagged with the writer's `ArenaBuildId`.
`emit_enter` consumes one `FreshBlockPermit` and returns a build-scoped open
binding. Finishing a source claim consumes one `FreshCoveragePermit`. A permit
from a stale, cancelled, recycled, or cross-arena build fails before storage
mutation.

The allocator advances before returning a permit and never rolls back:

- cancellation burns minted and reserved IDs;
- late validation/allocation failure burns them;
- an unused reservation is burned; and
- counter exhaustion fails closed before mutation and never wraps to zero.

Burning IDs is intentional. Reuse after cancellation creates an ABA hazard if
a delayed capability or diagnostic delta survives. Two monotonic scalars are
not a locator or document-wide identity directory and add constant retained
state.

`BlockId` and `CoverageId` keep compact `u64` encodings, but their tuple fields
become private in the production crate. Public code gets value accessors, not
constructors. Test/oracle constructors are explicitly test-only.

New IDs are disjoint from every ID ever minted by the live document. Exact
suffix adoption copies old IDs from resolved, nonoverlapping base capabilities;
it does not remint them. Paragraph-to-Heading promotion similarly preserves an
ID only through an exact open capability. Parser code cannot request a scalar
ID to be preserved.

Storage correctness still must not rely on scalar uniqueness. Every mutation
uses manifest + leaf + offset/range capabilities, and `BlockId` only
corroborates the resolved record. Thus a deliberately corrupted base with a
duplicate scalar ID cannot redirect a write. By-construction minting prevents
duplicates in valid output without introducing a `BlockId -> location` map.

If arena manifests ever become durable across process restarts, add a document
namespace and allocator high-water marks to the durable envelope. The current
arena is process-local, so identity stability is an editor-session contract.

## Edit admission and source consistency

Wrapping only the build path was insufficient: the prior caller could mutate
`SourceStore` and then fail while advancing `Coordinator`, leaving their source
clocks split. `LiveDocumentStore::accept_edit` is now the production entry
point and implements this contract:

1. validate source range/boundaries and compute the next immutable Crop root
   without publishing it;
2. preflight source revision/root identity and coordinator revision/generation
   overflow without mutation;
3. invalidate the active candidate;
4. publish source and coordinator changes as one infallible actor transition;
5. keep only the newest queued parse; and
6. release the stale parser source lease immediately on the worker, while its
   arena journal continues fuelled reclamation.

The global `SourceRootId` mint now has a fallible production path. Direct
proof-harness construction retains its fail-fast helper, while live-document
construction/edit preparation reports exhaustion before publication. A
prepared persistent lineage update is allocated before detaching the candidate;
the source and coordinator commit methods consume opaque prepared values and
perform no recoverable work after their invariant checks.

## Cancellation contract

Cancelling a source-bound candidate has four independent effects:

1. invalidate the live candidate epoch so no further permit or seal can be
   minted;
2. stop parser/source reads and release or defer the bound Crop snapshot on the
   worker;
3. begin the existing arena-journal abort and reclaim it under explicit fuel;
4. retain advanced identity counters, burning every cancelled ID.

No destructor scans green pages, parser open depth, identity reservations, or
lineage history. The current committed manifest remains independently owned
and queryable.

One risk remains empirical rather than architectural: releasing the last
strong reference to a fully divergent, very large old Crop root is not fuelled
by `PageArena`. A whole-document replacement can therefore produce a stale
root whose final drop has document-scale work. It occurs on the Rust worker,
not Flutter's UI isolate, but it can still delay the newest worker parse. The
launch gate must measure full-replacement cancellation for 10 MiB and 100 MiB
roots on native and Wasm. If the tail exceeds the liveness budget, defer old
snapshot drops to worker idle/recycler turns (accepting a bounded temporary
memory increase) or change the source backend/reclamation strategy. Do not
pretend arena fuel also meters Crop destruction.

## Adversarial test matrix

### Source and manifest binding

1. A compile-fail test cannot construct `BoundSourceCursor`,
   `LiveCandidateEpoch`, `CleanSourceSeal`, or `CandidateManifestOwner`.
2. A forged/copyable `SourceSnapshotDescriptor` is rejected as an argument to
   every authority-bearing API because no such overload exists.
3. Nonempty and empty revision-zero documents complete a generation-one clean
   parse and commit exact root/byte/UTF-16 fields.
4. Same revision with another `SourceRootId`, same root with another revision,
   and correct pair with a wrong length all fail before manifest publication.
5. Corrupting encoded source root/length/summary makes manifest decode fail;
   it cannot be repaired from the caller's descriptor.
6. Accept an edit at every cut point: before lease issuance, after lease
   issuance, after arena build start, after source seal, after manifest
   allocation, before attach, and between attach/commit. The stale candidate
   never becomes current and the old committed output stays queryable.
7. Empty, ASCII, astral UTF-8, LF, CRLF, lone CR, and NUL-to-U+FFFD fixtures
   prove exact physical byte/UTF-16 totals and typed logical transforms.

### Arena and epoch isolation

8. Build in two arenas whose first local IDs collide. Cross-arena permit,
   owner, manifest, query capability, attach, and release attempts all return
   the original linear authority.
9. Abort a build, recycle its build slot, and prove its old permit and source
   seal fail against the new generation.
10. Suspend/resume with one-unit fuel and prove no operation can use a stale or
    replayed ticket.

### Entity identities

11. Compile-fail tests cannot construct raw production `BlockId`,
    `CoverageId`, or clone a fresh permit.
12. Replaying one consumed permit cannot emit a second Enter/coverage claim.
13. A permit minted for build A fails against build B before allocation.
14. Abort after minting IDs; the next candidate receives strictly newer IDs.
15. Adopt a distant suffix and prove every eligible suffix Block/Coverage ID
    is byte-for-byte retained while all changed identities are fresh.
16. Put duplicate scalar `BlockId`s in an adversarial base and prove an exact
    leaf/offset capability changes only its resolved record.
17. Set each identity counter to its terminal value and prove overflow leaves
    source, candidate journal, and current manifest unchanged.

### Cancellation and lifetime

18. Cancel/fail before and after every yield and allocation. Polling abort with
    fuel one performs at most one recorded ownership transition, eventually
    reaches zero owners, and never changes the current output.
19. A cancelled writer's source `Weak` observer eventually stops upgrading;
    scalar lineage jobs retain no Crop root.
20. Identity high-water marks never move backwards across cancellation,
    failure, supersession, or clean fallback.
21. A 10 MiB and 100 MiB whole-root replacement reports stale-source drop
    latency, newest-parse start delay, peak overlap memory, and native/Wasm
    behavior. This is a physical-device/worker gate, not inferred from arena
    receipts.

## Stop conditions

The authority gate fails if any production path:

- accepts a caller-supplied root spec, source descriptor, raw entity ID, or
  arbitrary arena owner as build/commit authority;
- treats descriptor equality, a source hash, or byte equality as a live-source
  proof;
- permits more than one active parser lease or public repeated issuance;
- starts IDs from one for each candidate or rolls them back on abort;
- lets a scalar `BlockId` locate or authorize mutation;
- attaches a manifest without independently checking source, parse, grammar,
  schema, build, and arena epochs;
- permits revision zero only through a sentinel exception instead of a normal
  exact descriptor; or
- claims cancellation is fuelled while synchronously dropping unmeasured
  document-scale source/parser state.

Passing this gate establishes provenance and lifetime authority. It does not
by itself prove source-ledger completeness, projection chunking, atomic suffix
adoption, or direct parser-to-green composition; those consume this seam.
