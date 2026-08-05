# Composed storage authority audit

Status: **historical audit; adoption authority HOLD; property-rewrite receipt
superseded**, 2026-07-16.

> The audited `composed_adoption_storage_gate` no longer compiles against the
> now-opaque packed manifest identity. Do not restore it as the production
> seam. Its original receipt remains evidence for immutable local page
> replacement only. The complete Paragraph transaction is now the sparse,
> fresh/retained `LeafNormalizationGroup` defined in
> [`LEAF_NORMALIZATION_GROUP_GATE.md`](LEAF_NORMALIZATION_GROUP_GATE.md).

This audit reviews `composed_adoption_storage_gate` against the actual
`restart_composer_gate` and packed `SerializedGreenDocument` APIs. The crate is
a useful mechanical join, but it does not yet prove that selected storage can
authorize or atomically publish an adopted suffix. Its current executable
result is narrower: one already-located Paragraph `Enter` can be rewritten to
a Heading through the immutable-base packed rewrite path without a `BlockId`
lookup.

## What the executable gate really proves

- `AdoptionPermit` is non-cloneable and is consumed before an action is
  exposed.
- A caller-supplied `GreenEnterCapability` is checked by packed storage against
  the selected manifest, base leaf, byte offset, BlockId, and old kind.
- The supported Heading facts pass the typed facts schema.
- A successful rewrite creates a new immutable root, leaves the old root
  queryable, and can retain an untouched distant leaf exactly.
- The tested wrong-first-leaf case fails without changing the committed root
  and leaves no live-node delta after reclaim.
- Unsupported composer actions are rejected by the adapter before
  `rewrite_enters` starts its transaction.

Those receipts are worth retaining. They prove the packed property-rewrite
mechanism and dependency direction. They do not prove source-lineage authority,
an exact physical open path, suffix attachment, or rollback after a partially
allocated multi-action mutation.

## P0: the storage stamp is a caller echo, not a storage proof

`StorageApplySpec` publicly accepts an `AdoptionStamp`. The tests obtain it
with `permit.stamp()` and pass it straight back to `authorize_storage`.
`apply_adoption` independently derives only the manifest-shaped root scalar and
generation. Lineage, old/current revisions, mapped boundaries, and suffix-tail
identity are therefore compared with values echoed by the caller rather than
facts derived from the source store and immutable storage root.

This does not close the authority seam described by `StorageAdoptionContext`.
The production API needs a private, storage-owned base proof produced by the
source/green boundary resolver. A public `AdoptionStamp` must not stand in for
that proof.

## P0: semantic-root identity aliases across arenas

`semantic_root_identity` encodes only `ArenaId.generation` and `ArenaId.index`.
`PageArena` has an arena nonce, but that nonce is absent from the semantic-root
mapping. Two fresh arenas with the same allocation shape can therefore issue
the same manifest scalar. A permit produced for the first arena can pass the
root check against a matching document and concrete capabilities in the
second.

The existing manifest test changes the numeric root value; it does not create
a second real arena/manifest with the same slot identity. The replacement root
capability must include document-session or arena identity as well as the
generation-safe manifest address. It should be opaque outside the component
that resolves the immutable base.

## P0: the adapter consumes only part of the Setext recipe

`PromoteSetext` contains `block`, `level`, and exact composed `content` runs.
The adapter matches `block` and `level` and discards the remaining fields with
`..`. It then changes only the existing `Enter` kind/facts. It neither applies
nor validates the composed content and does not attach the authorized suffix.

Despite that partial application, `apply_adoption` consumes the only one-use
permit and publishes a new document. No authority remains for a later atomic
suffix operation. A complete join must keep the non-cloneable storage plan
inside one transaction and consume every action field plus the mapped suffix
before publishing a manifest. Until then, this API is a property-rewrite
witness, not an adoption endpoint.

## P0: the result is not bound to the current source revision

`SerializedGreenDocument::rewrite_enters` preserves the base manifest's
`source_revision` and `known_bytes`; it advances only parse generation and
semantic epoch. The focused test composes old revision 10 and current revision
11 against a packed base tagged `SourceRevision(1)`, yet reports the returned
document as a successful adoption.

The complete transaction must publish the exact current source revision,
mapped coverage, known range, and suffix identity. At minimum, a same-source
property-only path must reject a permit whose current revision is not the
packed root's source revision.

## P1: the physical open path is caller assembled

`ConcreteOpenBinding` contains two public values: a `StableBinding` and a
`GreenEnterCapability`. The stable capability ID is an arbitrary public `u64`
and the focused test hard-codes it independently of packed storage. The adapter
checks equality with the binding retained by the permit, but it never derives
or validates that capability ID from the concrete Enter, never validates
`opened_at`, and never proves that multiple Enter capabilities form one nested
outer-to-inner path.

Packed storage validates each leaf coordinate, BlockId, and kind, which is
necessary but not sufficient. Its builder also does not enforce global BlockId
uniqueness, so same-ID Enter records make a caller-assembled path ambiguous.
The source-boundary resolver must return the concrete open stack and mint or
resolve its root-scoped capability identities. The adapter must consume that
opaque path proof rather than accept parallel public structs.

## P1: rollback after partial mutation is not exercised

The wrong-capability test corrupts the first and only target. Packed storage
rejects it while locating the base leaf, before allocating a replacement page.
The unchanged live-node count is real, but it does not test rollback after an
earlier action allocated pages and a later action failed.

Add a multi-action late-failure test that observes a nonzero transient
allocation receipt, polls reclaim, proves the old root byte-for-byte queryable,
and returns to the exact pre-call live-node/storage metrics. A successful test
must also release both old and new roots and prove eventual zero retained
nodes.

## P1: validation and receipts are not wholly pre-transactional

The adapter translates every supported action before calling
`rewrite_enters`, so unsupported actions do fail before the packed transaction.
Actual leaf, offset, BlockId, and old-kind validation occurs inside
`rewrite_enters` after its transaction has opened. This is safe only if
rollback and receipt semantics are explicit. Today a failed call can update
the caller's receipt before returning an error.

Either preflight all concrete targets against the immutable base before
opening the build, or document and test transactional rollback plus a
candidate-only receipt that is committed to the caller only with the new root.

## Required replacement contract

The selected architecture remains coherent if the join is shaped as follows:

1. A source/green boundary resolver constructs a private `BaseAdoptionProof`
   containing document-session or arena identity, manifest identity,
   source/edit lineage, old and current revisions, both mapped boundaries,
   suffix-tail identity, and the exact concrete open stack.
2. Stable binding capability IDs are issued or resolved by that component and
   are checked against the physical Enter capabilities. Callers do not pair
   public stable and physical values.
3. `AdoptionPermit::authorize_storage` consumes the permit and the private base
   proof into one non-cloneable transaction plan. The plan cannot publish a
   partial action.
4. One immutable-base, cancellable build applies every typed prefix recipe,
   source/coverage range mutation, property rewrite, reference invalidation,
   and exact retained suffix. All action fields are consumed or the build
   fails closed.
5. Commit validates structural/source summaries and publishes a manifest bound
   to the current source revision, grammar/profile, parse generation, semantic
   epoch, known range, and suffix lineage. Cancellation or any validation
   failure publishes nothing.
6. Tests must reject a real cross-arena collision, wrong real source revision,
   wrong mapped tail, non-nested concrete paths, duplicate-ID redirection, and
   late failure after allocation. The success witness must prove exact content
   and coverage, current-revision metadata, suffix page reuse, simultaneous old
   and new root queries, and complete reclamation after both roots are retired.

This changes no selected high-level architecture. It prevents a narrow packed
rewrite receipt from being mistaken for the central convergence/adoption proof
that the architecture still needs.
