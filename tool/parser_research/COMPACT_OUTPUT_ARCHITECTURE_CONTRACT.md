# Compact output architecture contract

Status: **candidate contract under executable falsification**, 2026-07-15.

> **Representation seam reopened, 2026-07-16.** The `BlockOrder`,
> `BlockRecordTable`, and `CoveragePartition` names below state logical query
> and authority requirements and describe the first flat witness; they are not
> a selected physical layout. `STRUCTURAL_REPRESENTATION_BAKEOFF.md` compares
> that normalized forest with a packed Euler green stream and a hierarchical
> packed green tree. A candidate may combine the three logical views when it
> preserves their typed semantics, source-first queries, independent
> presentation/reference lifetimes, suffix identity, and transactional
> adoption. Do not carry the 84-byte proof record or a global BlockId directory
> into production merely because this document names them.

This document narrows the current Flark v3 direction. It is not a launch
decision and does not make the unfinished `record_forest` prototype
production-ready. It defines the smallest coherent model that the next
composition gate is allowed to prove.

The full persistent structural-event history is not the selected output. Its
ownership and persistent-sequence mechanisms remain useful, but the ordinary
10 MiB/100,000-line open-leaf discriminator retained 200,001 events and 7.25
MiB of arena payload. Exact parser events are therefore a bounded transient
builder protocol, not committed semantic history.

## One authority, several lifetimes

There is one exact block grammar and one exact inline grammar for each
revision. Different structures exist because their lifetimes and query costs
differ, not because they may disagree about Markdown.

```text
exact current source
  CoverageDirectory + Crop revision lease
             |
             v
one Flark-owned Comrak-correspondent block transition machine
  worker-local continuation             O(open depth + bounded scanner state)
  transient, backpressured events       O(current granted work)
             |
             v
compact persistent semantic state
  BlockOrder                            preorder stable BlockIds
  BlockRecordTable                      finalized immutable node facts
  CoveragePartition                    total non-overlapping source ownership
  Property/Reference indexes           sparse document aggregates
             |
             +--> exact bounded inline/reference service
             |
             v
revision-bound PresentationSnapshot     O(requested complete facts)
             |
             v
Flutter source-first view
  mounted host/layout lease             no semantic authority
  source selection/input lease          independent of parser completion
```

Dart never classifies Markdown to fill a parser deadline. A missing or
incomplete current-revision fact is `Unknown`, and its source remains visible.

## Persistent components

### CoverageDirectory

The coverage directory is the source-coordinate spine. It stores stable
physical coverage identities and byte/UTF-16/line aggregates in persistent
packed pages. It owns no old Crop root. Absolute offsets are current-root
queries, never persistent fact fields.

### BlockOrder

`BlockOrder` is a persistent preorder sequence of stable `BlockId`s. It is the
only normalized child ordering. The document root is a real stable block and
owns root-level editable gaps; no fake block ID or nullable semantic owner is
introduced for whitespace. A block record stores its parent, but never a
growing child vector. An unchanged suffix order page keeps exact identity after
a prefix splice.

### BlockRecordTable

A finalized record contains only immutable structural facts needed after its
close. Optional or rare data belongs in typed sparse property tables rather
than fixed-width empty slots. In particular:

- child content/order is not embedded in a parent record;
- source payload is represented by a source-backed run cursor;
- list tightness, task state, reference resolution, and similar derived facts
  are not copied into every descendant;
- chronological donor source-position repair metadata is absent; and
- an open frame shadows an older finalized record with the same stable ID.

The fixed 100-byte prototype record is an algorithm witness, not accepted
production packing. The gate must report retained bytes per ordinary block for
many-small-block input as well as the long-open-leaf case.

### CoveragePartition

Viewport discovery uses one total, ordered, non-overlapping source partition.
Each segment is one of:

- a terminal leaf owner;
- an explicit editable gap owner; or
- a container-marker segment whose owner is the innermost container.

Parent links recover enclosing containers. This is preferred to a second
overlapping interval tree if it survives blank lines inside nested quotes and
lists, continuous container rails, viewport boundaries, and compound block
subtargets. Table cells and inline ranges remain requested presentation facts;
they do not turn source coverage into a second structural tree.

### Sparse aggregates

Property and reference indexes are separately persistent because their updates
and validity differ from block finality:

- list looseness/tightness is a composable direct-child-range property;
- reference occurrences derive first-definition winners and dependency
  generations without entering block continuation equality.

Committed source ranges are Flark product coordinates derived from the total
coverage partition and preorder subtree aggregates. Parent containment is an
invariant. Comrak's chronological zero-column repair behavior is donor evidence,
not persistent semantic state: current corpus examples can leave an item ending
after its parent list. The canonical-range gate must enumerate every donor
source-position delta while preserving exact grammar, ancestry, rendered HTML,
and editor behavior. If a compatibility adapter proves unavoidable, it remains
at the parser seam and may not introduce repair chronology into ordinary block
records.

No aggregate may repair descendants eagerly, scan a subtree during adoption,
or use a wall-clock/global ordinal that rebases a reused suffix.

`BlockOrder` alone cannot supply a list property fold because preorder ranges
also contain nested descendants. The composed gate must choose one persistent
direct-child aggregate representation (for example, a sparse container-to-child
summary sequence) and prove local item replacement plus outer/nested property
updates in `O(depth * log pages)` without scanning siblings.

## Control continuation, semantic prefix, and presentation are distinct

The worker's `ControlContinuation` contains exactly the typed state that can
select the next block-control transition. It is parser-private and its retained
frame count is proportional to open depth. It is not a fingerprint stored in
the semantic forest. Persisted restart checkpoints are sampled separately and
may retain immutable source-backed cursors, not copied growing leaf prefixes.

`SemanticPrefixState` is also parser-owned but separately persistent and
composable. It contains source-backed paragraph/raw runs, table preface/header
descriptors, reference occurrences and finalizer cursors, child folds, and
other state that can change the semantic actions emitted by an otherwise equal
control transition. It is not compared wholesale and it is not recovered by
reading the published semantic root. The top-level composer validates a typed
adoption recipe for every changed prefix before attaching a suffix.

The semantic root has a separate grammar-free `OpenStructuralOverlay` containing
only product-visible open block identity, ancestry, kind, source anchors, and
target facts. It cannot authorize convergence. This prevents an opaque
`grammar_state: u64` from becoming a hidden second authority.

The active presentation output is a different object. A shallow paragraph or
table may require hundreds of exact hidden-marker, replacement, style,
ambiguity, run-edge, cell, task, fence, semantic-target, and capability facts.
Those facts are packed, page-capped, request-scoped, revision-scoped, and
published only when their requested leaf/range is complete. Partial fact pages
are not renderable.

A mounted Flutter host is different again. It may preserve element, focus,
input, and cached-layout identity while its semantic lease is withdrawn and it
source-paints. Host identity never certifies Markdown meaning.

## Partial publication

Every parser poll must describe the whole target source as an ordered
partition of:

```text
current finalized/certified facts | current UnknownRange | converged suffix
```

The parser may additionally publish one complete exact presentation snapshot
for an active/visible request. A structural block can be final while its inline
reference meaning or command capabilities remain unknown. Authority therefore
has explicit dimensions:

- block structure;
- inline projection;
- reference resolution;
- semantic targets; and
- edit capabilities.

Syntax, coverage, projection, targets, and capabilities for one snapshot adopt
atomically under the same source revision, parse generation, profile, and
request identity. Stale pages cannot be promoted into a newer root.

## Exact convergence and spanning containers

Suffix attachment requires:

1. exact operation-derived source alignment;
2. equality of all state that can select the next block-control transition;
3. equal stable bindings for open ancestors;
4. exact pending-leaf cursor identity or a boundary before that leaf;
5. a valid typed adoption recipe for every changed semantic-prefix value; and
6. immutable suffix pages that need no coordinate rebasing or old source root.

Semantic-prefix state is not automatically control continuation. This
distinction is decisive for a document-spanning list. A normal child edit or
insertion must not fail control convergence merely because prior output
differs, but control equality alone cannot authorize attachment. Normalized
`BlockOrder`, stable parent bindings, and composable property aggregates should
permit suffix reuse and at most one parent/property update.

The child-fold partition mechanism is now executable: effective
`has_any_child` (historical prefix or retained open-path child) is the only
closed-child-prefix fact observed by later block transitions, while the five
list-looseness fields form an exact associative range summary. The gate covers
all 33 reachable fold states, 22,737 exact sequence/split compositions, all
reachable-state associativity triples, every output-bit mutation on List and
Item frames, and historical-prefix deletion with a retained child. Output may
change while the typed suffix transition trace remains exact. This proves the
mechanism, not the full 100,000-item adoption gate. The composed gate must still
splice the direct-child property aggregate, reuse exact suffix pages/IDs, and
match a fresh parse after normal and tightness-changing edits.

## Scheduling and liveness

The UI applies exact source and selection immediately. Native uses a long-lived
parser isolate; web uses a Web Worker owning WASM. The worker uses latest-wins
admission and at most active, queued-latest, and acknowledged roots.

Parser work is either a preflighted bounded atomic kernel or an explicit
resumable state machine. Event generation, page sealing, index composition,
delta decode, adoption, and retirement all count against work/allocation
budgets. Dense output may yield between pages; it may not allocate an
unbounded event vector before a bounded sink sees it.

Persistent builders likewise stream one bounded sealed page into a shared
transaction rather than first collecting all encoded pages or owners. Builder
receipts include heap/input scratch and arena slot overhead, not only payload
bytes.

Ordinary active facts are prioritized and should reach the next refresh. A
miss never delays source/caret paint and never authorizes stale semantics. The
prediction-era immediate-inline/fence tests become either measured exact
urgent-path requirements or explicit product changes.

## Required discriminator suite

The compact output direction is not selected until one composed gate proves:

1. a 10 MiB/100,000-line open paragraph and fence retain compact coverage plus
   O(depth) open state, not per-line mutation history;
2. 100,000 ordinary small blocks report total record, order, coverage,
   property, reference, branch, manifest, arena-slot, input, and temporary
   builder bytes, including bytes per block;
3. a prefix/interior edit in a 100,000-item spanning list reuses unchanged
   suffix IDs and pages, with bounded parent/property work;
4. a tightness-changing edit does not hide an unbounded parent-close scan;
5. nested quote/list blank gaps and container-marker-only lines resolve through
   the total coverage partition and parent links;
6. setext/table promotion, reference-definition removal, detach, and reparent
   leave no duplicate/orphaned order or coverage records;
7. canonical half-open ranges provide total byte/UTF-16 coverage and strict
   parent containment across all donor fixtures; donor source-position deltas
   are enumerated and any compatibility projection stays parser-seam-only;
8. partial roots have total exact coverage, complete requested facts or one
   explicit unknown range, and no prematurely attached suffix;
9. forced allocation failure and cancellation at every component boundary
   release every consumed owner transactionally while the old root remains
   queryable; one top-level build journal spans the first component allocation
   through composite-manifest commit;
10. a prefix splice performs bounded local repacking, reuses exact distant
    suffix subtree/page identities, and current absolute byte/UTF-16 queries
    match a clean source oracle;
11. dense active facts are byte/record capped, leaf-complete, revision-atomic,
    and stale queries fail closed; and
12. the selected representation uses one arena, one persistent sequence
    primitive, one ownership-transaction implementation, one typed parser
    continuation authority, and one block transition authority; and
13. the composite manifest rejects a forest, presentation lease, or other
    component whose semantic epoch/range does not match the root being built;
14. 10,000 repeated insertions in one gap keep page count, height, payload,
    arena slots, and local fragmentation bounded while a distant suffix page
    retains identity; and
15. streaming construction and cancellation keep temporary memory bounded by
    one page plus tree/build-journal depth rather than document size.

After the grammar-free gate passes, the exact block machine must feed it
directly and reproduce clean parse output after every revision of the existing
transition corpus. Only that composed result can advance to inline/reference,
native/WASM scheduling, Flutter, and physical-device gates.

## Stop conditions

Reject or redesign the candidate if it requires any of:

- a second Markdown transition machine or a Dart prediction grammar;
- a suffix scan/hash/equal bytes to authorize convergence;
- old Crop ownership in output or checkpoints;
- absolute suffix fact rebasing;
- a growing literal, child vector, or per-line event history in a block record;
- historical output reads by the parser;
- subtree scans for repair or property propagation;
- unmetered recursive destruction;
- separate sequence/ownership implementations for each semantic index; or
- stale semantic facts carried by a UI host/layout lease.
