# Structural representation bakeoff

Status: **candidate B direction selected; production implementation HOLD**,
2026-07-16.

The block grammar, source authority, scheduling model, and presentation
contract do not depend on a particular persistent tree encoding.  The current
`record_forest` is a mechanism witness, not a reason to preserve its physical
shape.  Before the compact output contract is frozen, this bakeoff must decide
which representation makes the already-selected authority/lifetime model
smallest and hardest to misuse.

## Decision

Select candidate B's source-ordered balanced-parentheses green stream as the
physical direction for the next composed gate. This selects the model, not the
current standalone `Arc` implementation as production-ready.

Candidate C is rejected as a representation. Its final composed codec stores
one local source run followed by at most one child document; it cannot encode
ancestor-owned continuation markers and gaps interleaved with an open
descendant Paragraph. Its large receipts used flat entries and did not encode
the required `Item -> Paragraph` workload. Closing that gap requires mixed
source/semantic tokens and nested source-order mutation, which converges on
candidate B rather than remaining a simpler hierarchy. Its shared packed-edge
arena mechanics remain reusable.

Candidate B wins directionally because its tested token order honestly carries
structure, source ownership, source order, properties, and folds in one
authority without a global `BlockId` directory or persistent absolute ranks.
Its standalone typed-plus-encoded tree has now been replaced by packed-only
shared-arena pages, and a depth-100 Enter rewrite proves one-base-root bounded
open-ancestor mutation with exact distant-page reuse. It remains on production
HOLD until logical-projection and large-fact schemas, generic token/range
mutation with boundary repacking, resumable ownership, and direct exact-parser
composition pass together.

## Candidates

### A. Normalized flat forest

- stable-ID keyed immutable block records;
- one preorder sequence;
- one total source-coverage partition; and
- sparse per-container direct-child folds, with zero/one/small child cases
  encoded inline and a persistent child sequence only when it is actually
  needed.

This is the conservative candidate.  Its structures have distinct query
responsibilities, but the total cost and atomic coordination of all of them
must be counted.  The existing 16.27-byte direct-child receipt covers one
100,000-child list only; it does **not** cover 100,000 Item-to-Paragraph
single-child containers and is not an honest whole-document estimate.

### B. Packed Euler / balanced-parentheses green stream

One persistent sequence stores `Enter` and `Exit` structure.  Its associative
summary tracks net depth, minimum relative depth, and the ordered closed-child
aggregate at that minimum.  A container-interior query therefore selects its
direct children while excluding nested descendants.

The strongest version is a serialized green tree: `Enter` carries the compact
immutable block facts and gap/marker/terminal coverage tokens are interleaved
in source order.  That version may replace the record table and semantic
coverage partition as well as preorder.  A weaker version that retains all
three old indexes must justify why its algebraic elegance is worth the extra
lookup machinery.

### C. Hierarchical packed green tree — rejected representation

Immutable block nodes own persistent, aggregate-bearing child sequences.
Zero/one/small children are inline; large sibling lists use the shared
persistent sequence primitive.  Source metrics live on subtrees, and coverage
tokens can be interleaved with block children while retaining an explicit
semantic owner for ancestor markers.

The shared arena proved bounded cross-page ownership without one proxy
allocation per ordinary block/edge. The composed representation nevertheless
failed the required nested Markdown source shape, globally exact nested ranges,
nested viewport traversal, and top-root nested mutation. Do not treat its
43.91-byte flat-entry receipt as a fallback representation receipt.

## Authority rule

The number of data structures is not itself the decision.  A derived index is
clean when all of the following are true:

1. it answers a different query from the grammar-owned semantic root;
2. it is deterministically rebuildable from that root;
3. it is built and adopted in the same top-level transaction and epoch;
4. stale or absent index state fails closed rather than changing semantics;
5. updating it is bounded by changed pages plus tree/open depth; and
6. it does not retain absolute positions, old source roots, or rebased ranks.

Any candidate that needs a second Markdown classifier, independently mutable
parent/child truth, or suffix-wide cursor rebasing is rejected regardless of
its benchmark result.

## Required operations

Every candidate must implement the same operations rather than benchmarking
its easiest lane:

- current byte and UTF-16 position to exact coverage owner and enclosing block
  path, including blank gaps and continuation-line container markers;
- bounded sequential viewport traversal without materializing the document;
- stable identity for every unchanged suffix block and packed page;
- insert, delete, promote, detach, and contiguous subtree reparent;
- replace one child contribution and propagate list/item output aggregates in
  `O(depth * log pages)` without a sibling/subtree scan;
- represent an unfinished open path without making presentation state part of
  parser convergence equality;
- locate every mutation boundary without an absolute rank that rebases after
  a prefix edit; and
- release or cancel every partial build iteratively under the shared arena and
  one top-level journal.

Parser-held or sampled restart cursors may be transient capabilities into the
current immutable root.  They are not a substitute for a required product
lookup, and they may not keep a retired semantic root alive.  If a candidate
claims no global `BlockId` directory, its tests must enumerate every real
caller and prove how that caller obtains a valid cursor.

## Workloads and receipts

The comparison reports retained payload, arena-slot/allocator overhead,
external maps, source descriptors, and peak temporary builder/journal memory.
At minimum it covers:

1. 100,000 top-level ordinary paragraphs;
2. one list containing 100,000 Items, each with one Paragraph;
3. nested List/Item/List ancestry with a tightness-changing inner edit;
4. a 10 MiB/100,000-line open paragraph and fenced block;
5. nested quote/list blank gaps and marker-only lines;
6. setext and table promotion, reference-only paragraph detach, and reparent;
7. 10,000 insertions into one source/structural gap; and
8. a prefix edit that proves exact distant suffix node/page identity.

For each workload report:

- retained bytes per semantic block and per source segment;
- live arena nodes and slots;
- maximum simultaneously owned streaming roots and journal bytes;
- nodes/pages visited and allocated by the edit;
- identities retained and identities intentionally replaced; and
- lookup depth for source, block, parent/enclosing, and viewport queries.

Payload-only or one-index-only numbers may accompany the receipt, but cannot
be used as the selection number.

## Decision rule

Select the smallest model that passes all operations with one semantic
authority and understandable ownership.  Prefer a modest measured memory cost
over a representation that saves bytes by introducing an implicit directory,
cursor repair protocol, or a second source of parent/child truth.

The discriminator is complete: serialized green is selected as the direction
and the tested hierarchical representation is rejected. Candidate B's
production implementation remains conditional on packed-only storage, complete
typed facts/source identities, bounded streaming queries, and batch mutation
passing the required workloads. A structure-only payload number may not be
used to waive any of those gates.

No third physical representation is opened after this discriminator. Candidate
B proceeds directly to shared-arena, disjoint restart/composer, and exact parser
integration. A failure there reopens this seam with concrete composition
evidence rather than another isolated data-structure idea.

If no candidate passes, preserve the authority/lifetime architecture and
redesign this representation seam.  Do not restore the event tape, donor
source-position repair chronology, or Dart prediction to make a candidate
look complete.
