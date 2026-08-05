# Projection chunk edit-stability falsifier

Status: characterization result for the serialized-green feasibility slice,
2026-07-16.

## Question

The streaming projection chunker bounds clean-build work and memory, but does a
local edit in one huge dense projection envelope preserve reusable suffix
pages?

## Probe

`v3_runtime_slice/tests/projection_chunk_edit_stability.rs` builds a 200,000-byte
NUL-dense envelope. Every physical byte is one typed atomic NUL-to-U+FFFD
projection, so pieces cannot coalesce or split. It then models insertion of one
identical byte at source offset 1 and maps every unchanged old suffix range into
the new revision.

An old Program page is counted as eligible for exact identity reuse only when:

1. its whole physical range maps through the edit;
2. the new chunker has a page at that exact mapped range; and
3. the physical metric and encoded logical contribution are identical.

This is an upper bound on exact `ArenaId`/`CoverageId` reuse. Adoption cannot
reuse a page whose source-relative range or payload differs.

## Result

The clean envelope and edited envelope each span 196 Program pages. After the
one-byte local insert:

- zero mapped internal page boundaries realign; and
- zero old suffix Program pages are eligible for exact identity reuse.

Greedy maximum fill is deterministic, but its boundaries are relative to the
start of the envelope. One extra indivisible piece displaces one old piece from
each page into the next, cascading to the envelope end. Stable CoverageId
split/merge rules cannot recover identity when no physical chunk boundary
realigns.

This does not falsify the unified source-projection model. It falsifies the
stronger claim that page-bounded greedy fill alone gives local incremental reuse
inside an arbitrarily large transformed construct.

A comparison model places deterministic resets at surviving source-store
anchors no more than 8 KiB apart. It produces 220 pages and leaves 211 of them
eligible for exact reuse after the same edit (95.9%). This is not yet the
production source-bound composer, but it demonstrates that a stable reset can
bound the cascade without changing projection semantics.

## Smallest coherent remedies

### 1. Explicit envelope-size gate

Declare one transformed envelope to be the invalidation unit and cap its
physical bytes, piece count, or Program-page count. Above the cap, reject the
feature, use a typed degraded representation, or require a non-live fallback.

- Smallest implementation and proof surface.
- Honest only if product semantics permit the cap.
- Does not satisfy exact live editing for arbitrarily large table cells or
  similarly dense single constructs.

This is suitable as a temporary launch gate, not as the large-document end
state unless the limit is an explicit product constraint.

### 2. Deterministic source-anchor/reset chunking

Choose preferred reset boundaries from stable source capabilities, with minimum
and target payload sizes plus the existing hard 4 KiB maximum. After an edit,
the chunker greedily reaches the next surviving reset and resumes the old
partition. Resets must use source-store identities or parser-certified
boundaries; a content-only rolling hash is insufficient for uniform inputs such
as this NUL run.

- Retains the current flat Program-page/run model.
- Can bound churn to the distance to the next surviving reset.
- Requires the source-bound composer before it can be implemented correctly.
- Needs adversarial proofs for missing/sparse anchors and a mandatory hard-cap
  cut that never splits atomic transforms.

This is the smallest plausible production remedy if source storage exposes
stable anchors densely enough for giant single-line constructs.

### 3. Persistent projection mini-sequence

Represent a large Program as its own persistent sequence of bounded projection
pages. The source-bound adoption recipe can splice changed pages and certify an
unchanged suffix using source anchors and projection-state equality, without
turning every page into a separate top-level CoverageId.

- Cleanest general large-construct model and strongest suffix-reuse story.
- Adds another typed persistent root/edge role, traversal cursor, atomic adoption
  component, and corruption/retirement proof surface.
- Justified only if the source-anchor/reset approach cannot provide robust
  resynchronization or if top-level CoverageId fanout is itself too costly.

## Recommendation

Keep the current chunker as the bounded clean-build mechanism, but do not claim
large-envelope incremental locality from it. First integrate the non-forgeable
source-bound composer and prototype source-anchor/reset boundaries against this
same falsifier. Adopt a temporary explicit size gate while that work is open.
Promote Program to a persistent mini-sequence only if stable reset density or
reuse receipts fail on giant single-line/table/code fixtures.

The next executable step is now recorded in
`PROJECTION_SOURCE_RESET_RESULTS.md`. Actual `SourceStore` lineage plus bounded
local reset split/merge kept every tested edit local, including repeated prefix
growth, cross-reset deletion, dense atomic streams, CRLF boundary invalidation,
and byte/UTF-16-divergent Unicode. That result favors flat runs with embedded
reset capabilities; it does not remove the source-bound authority and
persistent-splice gates listed there.
