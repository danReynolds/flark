# V3 composed runtime slice: integration contract

Date: 2026-07-15.

Status: **historical implementation contract; its primary-output decision was
superseded by the executable retention discriminator**.

> **Do not implement the full persistent event history as the selected output.**
> `v3_runtime_slice/RESULTS.md` proves its generic ownership, packed-page,
> projection, persistent-sequence, suffix-identity, and lifetime mechanisms,
> but a normal 10 MiB/100,000-line open paragraph or fence retains 200,001
> events and 7.25 MiB of arena payload (72.45 bytes per line). Preserve the
> green mechanisms and test transient parser events folded into compact
> finalized block stubs plus an O(open-depth) overlay. The contract below is
> retained as the exact record of the now-falsified event-tape hypothesis and
> its still-relevant adversarial requirements; it is not the current selection.

The isolated source, block, inline, and lifetime gates remain individually
promising, but no current executable composes the compact finalized-output
challenger across real edits. That next slice must meet the same identity,
restart, viewport, reference, repair, and lifetime gates without the retained
per-line mutation history.

The old `integrated_parser_slice` grammar, frontier, inline machine, and
convergence capability model are explicitly out of scope. They implement the
superseded Pulldown/custom subset. Only the proven Crop wrapper, grammar-free
scheduling ideas, and iterative page arena are candidates for extraction.

## Superseded decision tested by this contract

Use a **coverage-relative structural event tape** as the primary persistent
output, not the current one-final-`BlockFact`-per-page representation and not a
fully rewritten node record on every parser update.

An event page may contain `Open`, `Promote`, content-run append, end-position
write, list-repair, definition, finalize, and `Close` events for a stable
`BlockId`. A block may therefore open in one page and close many pages later.
Event pages are fixed-size transport/storage units and may split the event
stream in the middle of one physical source line. Every page has a persistent
leading **projection** checkpoint. Parser restart checkpoints are separate and
exist only at exact grammar-safe boundaries after a complete physical line.
A bounded viewport query starts from the projection checkpoint and folds only
requested pages.

Add only the indexes that materially improve queries or invalidation:

- a terminal block/inline-leaf index, written on promotion/finalization rather
  than on every source line;
- a reference definition occurrence/winner index;
- a page order index for exact occurrence ordering and source anchors;
- a lazy list-repair overlay; and
- the existing byte-bounded inline-fact cache.

This hybrid is preferable to a full persistent node table:

| Requirement | Event tape + focused indexes | Full node/property record as primary output |
|---|---|---|
| Long open paragraph/fence/list | Events span bounded pages; no old page mutation | Growing content/end state rewrites a map path on every line |
| Suffix reuse | Reuse old event pages when parser state and open `BlockId`s converge | Possible, but changed open ancestors still require record/child-sequence surgery |
| Viewport materialization | One page lookup, persistent open-stack checkpoint, bounded fold | Fast node lookup, but an ordered coverage traversal is still required |
| Random terminal lookup | Focused terminal index | Naturally good |
| Packed memory/WASM wire | Sequential page encoding | Per-node map objects are harder to pack |
| List/source-position repair | Lazy range overlay, no descendant rewrite | Tempts an eager subtree rewrite |
| Update complexity | Append plus one persistent page splice | Map update for every append/update plus ordered traversal structure |

The terminal index is deliberately not a second semantic tree. The event tape
is authoritative; the index is a derived lookup from stable IDs to a terminal
event/content cursor.

## Identity domains must not alias

The current gates overload `PageId` as an output page and an inline leaf. The
composed slice must use distinct, generation-safe types:

```rust
SourceRevision(u64)       // accepted text operation
ParseGeneration(u64)      // one cancellable parse attempt
SourceRootId(u64)         // scalar identity of one Crop root wrapper
CoverageId(u64)           // stable physical-line coverage unit
OutputPageId(u64)         // packed structural event page
BlockId(u64)              // structural open/finalize identity
InlineLeafId(BlockId)      // only inline-bearing finalized blocks
SymbolId(u64)             // normalized reference label
OccurrenceId(u64)         // one reference definition occurrence
OutputRootId { slot, gen } // remotely queryable persistent root
```

Page repacking must not invalidate an inline leaf. Editing a leaf may either
mint a new `BlockId` or retain its ID with an incremented content generation,
but the first slice should take the conservative rule: every block opened
after the restart boundary gets a new ID; unchanged suffix IDs are recovered
only by convergence and page reuse.

No content hash, pointer equality from Crop, absolute offset, or equal-length
edit may mint or recover any of these identities.

## Reuse and extraction inventory

| Current file/API | Disposition | Exact reason/change |
|---|---|---|
| `integrated_parser_slice/src/crop_source.rs`: `CropSnapshotLease::{from_text, identity, len_bytes, is_char_boundary, cursor_at, edit}`, `CropEditProvenance::{map_unchanged, map_descriptor}`, `CropSourceCursor` | **Extract** into the new crate | The wrapper, scalar root identity, operation-derived prefix/suffix lineage, and reusable chunk scratch are valid. Remove dependencies on the stale crate's `Anchor`, `BufferId`, `CertifiedSourceBoundary`, and leaf-capture types. Do not expose `materialize` on the runtime path. |
| `integrated_parser_slice/src/arena.rs`: `PageArena`, `ArenaId`, preflight allocation, `release_later`, `poll_reclaim` | **Extract with its tests** | It is grammar-free, generation-safe, and proves iterative bounded retirement. Use it for event/index pages; do not import the stale reverse-chain output format. |
| `integrated_parser_slice/src/scheduler.rs`: revision/generation/root-lease state machine | **Extract a small kernel, do not depend on the crate** | The three-clock and latest-one-queued model is useful. Arena-job/reverse-chain/forecast plumbing is larger than this slice needs. Preserve stale-token, at-most-three published roots, ack/release, and reclaim-backpressure tests. |
| `integrated_parser_slice/src/convergence.rs` | **Do not reuse** | It uses `PersistentSource` and proof-only logical/output root tokens; its own `REMAINING_INTEGRATION_GAP` says the roots are not real. Only the bounded no-old-root edit-record idea carries forward. |
| `comrak_value_block_core/src/checkpoint.rs`: `ResumableValueBlockParser`, `BlockCheckpoint`, `OpenOutputBindings`, `WriteOnlyBlockSink`, structural events | **Adapt the actual API** | This is the exact same-parser seam. Replace owned source-leaf/output strings, revision-local numeric handles, and literal-bearing metadata with stable coverage/block IDs and run cursors. Keep the sink write-only. |
| `comrak_value_block_core/src/parser.rs` + `table.rs` | **Reuse as the one block grammar** | No new integration grammar is allowed. The 55-function-correspondent ordering remains the sole block authority. |
| `comrak_value_block_core/src/source.rs`: `CoverageRange`, `OriginRun`, `OriginTransform` | **Adapt** | Coverage-relative origins are correct. `SourceDocument` and `CoverageLeaf { absolute_start, text: String }` are test-only and must not cross the runtime boundary. `OriginTransform` needs a source-backed/lazily transformed representation for large runs. |
| `comrak_value_block_core/src/tree.rs`: `BlockKind` | **Split metadata from payload before acceptance** | Production metadata must not own `CodeBlock.literal`, `HtmlBlock.literal`, or another growing `String`. The current one-megabyte close clone is a hard fail, even if pending `LeafContent` is move-based. |
| `relative_output_reuse_gate/src/lib.rs`: balanced `OutputTree` split/join/prefix-sum algorithms | **Extract/adapt** | The persistent sequence and byte/UTF-16 summaries are valid. Current `OutputPage`, `BlockFact`, symbol table, and property table are proof shapes only. Port the sequence to packed event pages in `PageArena`. |
| `relative_output_reuse_gate::OutputPage` | **Reject as the composed fact model** | It requires a fact's complete range to fit one page and has no handle update seam. It cannot represent an arbitrarily long list, fence, HTML block, or paragraph without an unbounded page. |
| `comrak_inline_fragment_gate`: `parse_inline_fragment`, `InlineFragmentRequest`, `InlineInputKind`, `InlineReferenceSnapshot`, packed facts/dependencies | **Reuse directly, calibrate tiers** | It is the definitive bounded inline service. Its current 8 KiB cap is prototype urgent-path evidence, not the final exact-product ceiling; over-cap work stays source-visible and moves to an exact background/pathological lane. |
| `lazy_inline_fact_cache_gate`: `FactCache`, scheduling/adoption logic, `LeafInlineContext`, reference dependency checks | **Extract the cache/controller, replace the synthetic document** | Cache validity remains `{InlineLeafId, content_generation, structural_context, reference presence dependencies}`. Real descriptors/content cursors come from terminal output, not `IndexedDocument`/uniform `LeafDirectory`. |
| `lazy_inline_fact_cache_gate::ReferenceSnapshot` | **Replace** | Its small `BTreeMap` and unknown-label behavior are test conveniences, not a collision-free persistent occurrence/winner index. |

## Required runtime interfaces

The signatures below define ownership and authorization, not final naming.

### 1. Crop source and edit lineage

```rust
struct SourceStore {
    revision: SourceRevision,
    root: Arc<CropSnapshotLease>,       // exactly the current root
    lineage: EditLineageRing,           // persistent scalar IDs/ranges only
}

struct EditRecord {
    from_revision: SourceRevision,
    to_revision: SourceRevision,
    from_root: SourceRootId,
    to_root: SourceRootId,
    old_edit: Range<usize>,
    new_edit: Range<usize>,
    unchanged_prefix: (Range<usize>, Range<usize>),
    unchanged_suffix: (Range<usize>, Range<usize>),
}

fn apply_edit(expected: SourceRevision, range: Range<usize>, replacement: &str)
    -> Result<AcceptedEdit, SourceError>;

fn map_old_range_to_current(
    committed: SourceRevision,
    old: Range<usize>,
    fuel: usize,
) -> MapStatus; // Proven(mapped), Changed, Pending, or HistoryExpired
```

The ring owns no historical Crop root. A mapping job snapshots an immutable
scalar root without cloning or eagerly scanning the retained history; its one
preflight lookup has a strict tree-depth bound, and every later record/tree
read is metered under poll fuel. Source edits may continue and overwrite every
live-ring slot without changing the job's scalar snapshot. History exhaustion
starts a clean parse; it never falls back to byte equality or hashing. An
active parser job may own one non-cloneable source lease. Accepting a later edit
cancels that job and makes its lease eligible for worker-side release.

The current scalar segment-tree witness still releases up to `2 * capacity -
1` nodes synchronously when its final fully-diverged snapshot drops. Shipping
therefore uses a strict, device-calibrated recent-lineage cap first; expiration
is a safe exact clean restart. If worker/device cancellation tails reject that
cap, lineage must reuse the existing fuelled arena sequence/reclaimer. It must
not grow a second bespoke reclamation subsystem.

### 2. Stable coverage and page order

A `CoverageId` identifies one complete physical line including its exact LF,
CRLF, or lone CR terminator. Reuse is legal only if edit lineage maps the full
old range to a current full physical-line range with the same boundaries. A
changed/split/joined line gets a new ID.

`CoverageDirectory` is a persistent sequence of IDs plus byte/UTF-16 metrics.
Absolute positions are query results from prefix sums. It contains no Crop
root, descriptor, weak lease, source slice, or absolute persistent fact.
Descriptors are packed in bounded SoA/varint pages; there is no `Arc`, `String`,
or heap object per physical line. A 1,000,000-line/100 MiB gate must report
descriptor bytes, page count, maximum page payload, and build/update work.
The acceptance target is a measured compact descriptor cost rather than an
unexamined object graph; any representation exceeding 24 retained bytes per
ordinary line must be justified against the existing roughly 15-byte/leaf
descriptor proof or redesigned.

`PageOrderIndex` supplies an exact order oracle for stable page/coverage IDs.
Reference occurrence trees may compare IDs through this oracle because edits
cannot reorder surviving pages. The implementation may use a standard order-
maintenance labeling scheme, but repeated insertion into one gap must be an
executable bounded/amortized gate. Relabeling changes neither `OutputPageId`
nor inline/cache identity.

The same oracle orders structural events. An event clock is never an absolute
document ordinal:

```rust
struct EventStamp {
    page: OutputPageId,
    local_event: u16,
}
```

This is essential for suffix reuse. If a changed prefix emits a different
number of events, every reused suffix stamp stays byte-for-byte unchanged.
Comparing two stamps asks the current root's page-order index and then compares
the page-local offsets. No suffix event number is rebased.

### 3. Source-backed pending logical content

The production checkpoint must not contain or compare a giant `String`.

```rust
enum LogicalRun {
    Source {
        coverage: CoverageId,
        local: Range<u32>,
        transform: OriginTransform,
    },
    OwnedTransform {
        payload: RunPayloadId,
        transform: OriginTransform,
    },
    Synthetic { payload: SmallPayload },
}

struct PendingContentCursor {
    arena_root: RunArenaRootId,
    run_count: u32,
    logical_bytes: u64,
    line_count: u32,
}
```

Run chunks are immutable, packed, and append-only. A parser checkpoint clones
a cursor, not payload bytes. Source runs resolve only through the parse job's
current `CoverageDirectory` and current Crop lease. Output/checkpoints own IDs
and local ranges, never a Crop descriptor or root.

Adjacent identity runs coalesce into a coverage-sequence slice rather than one
run per physical line. Prefix-stripped/transformed runs that cannot coalesce
are delta-encoded in fixed pages with no per-run allocation. The same
1,000,000-line/100 MiB gate reports run count, coalesced spans, packed bytes,
owned transformed bytes, and maximum run-page payload. Plain multiline input
must scale with coverage pages/spans, not one heap node per line; adversarial
blockquote/table transforms may scale with line count but must retain a packed,
explicit bytes-per-run bound.

`OwnedTransform` is charged and capped. Identity, code, raw HTML, fence-info,
and reference-definition value payloads must stay source-backed. No convenience
transformation may silently duplicate a 1 MiB or 10 MiB leaf.

#### Exact open-leaf convergence equality

For every semantic frame, checkpoint equality compares the exact
`PendingContentCursor`, including its generation-safe arena root and prefix
length. Root identity is preserved only by structural sharing authorized from
the old checkpoint and unchanged coverage lineage. A hash/digest is allowed as
a rejection accelerator but can never authorize equality.

Consequences are intentionally conservative:

- If an edit is outside an open leaf, its shared pending cursor can compare in
  O(1).
- If any changed run enters an open paragraph/fence/reference candidate, its
  cursor differs. Convergence is rejected without scanning the prefix and may
  not occur until that leaf closes.
- Editing bytes and later restoring equal text does not recover identity; the
  parse continues until a later exact state boundary.

This avoids both false convergence and an O(size-of-open-leaf) comparison. It
also permits the old Crop wrapper to die: pending runs refer to coverage IDs,
not source roots.

### 4. Parser restart records and convergence proof

Event-page boundaries and legal parser boundaries are different domains. The
page builder may seal as soon as its packed payload is full, including midway
through the events emitted for one physical line. It records a projection
stack root, but never pretends that the parser can restart there.

After a complete physical line, policy may create a sparse legal restart
record:

```rust
struct RestartRecord {
    source_boundary: CoverageBoundary,
    semantic: CanonicalBlockCheckpoint, // frames/folds + pending cursors
    bindings: OpenBindingState,          // stable BlockIds + coverage anchors
    projection: ProjectionStackRoot,     // persistent, content-free open stack
    output_cursor: EventTapeCursor,       // may point within a packed page
}
```

For simpler suffix splicing, creating a scheduled restart record may seal a
partially filled event page after the line. It may not create a semantic
checkpoint for every transport page or in the middle of `process_line`.
Convergence occurs only at these records. If its cursor is within an old page,
the splice may copy/split that one bounded page; all later old pages must retain
identity.

Canonical semantic frames and projection stacks live in persistent
generation-safe stack/frame stores. Each restart record holds O(1) roots and
cursor scalars. It must not clone `open_depth` frames. Updating a frame creates
only the paths actually changed by the grammar; unchanged prefix frames share.
A deep-nesting gate records unique frame nodes, stack nodes, path copies, and
bytes across many event pages/restart records to reject hidden
O(open-depth * boundaries) retention.

Checkpoint equality first accepts shared frame roots, otherwise compares
canonical frame values under explicit fuel. Pending-content cursors inside a
frame remain O(1) exact identity checks and are never traversed. A digest may
reject unequal stacks early but may not authorize convergence.

`CanonicalBlockCheckpoint` is the same state consumed by the exact
`ValueBlockParser`; it is not a second transition machine. Runtime bindings and
positions remain excluded from semantic equality, but convergence requires
them independently.

An old suffix may be adopted only when all of these are proven:

1. current boundary maps exactly to an old legal restart boundary through every
   retained edit record;
2. the complete old boundary-to-EOF range maps to the current boundary-to-EOF
   range as unchanged operation lineage;
3. canonical block frames/folds/profile are exactly equal;
4. every pending content cursor is exactly equal by shared identity/prefix;
5. open `BlockId`, parent relation, structural context, and coverage-relative
   runtime anchor state are exactly equal; and
6. the leading projection state required by the old suffix page is exactly
   equal.

No source bytes, pending logical bytes, or suffix output facts may be hashed or
scanned to make this proof. If an output ancestor was reopened with a new
`BlockId`, semantic equality alone is insufficient: old suffix events may
refer to the old ID, so convergence must wait until the differing binding is
closed.

#### Source-backed physical-line input is part of this gate

The exact core currently takes `&str` and slices it throughout `process_line`.
That interface cannot be the composed runtime API: materializing a 10 MiB line
before entering the exact parser would make the separate oversized-scanner
work irrelevant.

The one exact parser must instead consume a source-backed abstraction along
these lines:

```rust
trait PhysicalLineView {
    fn len_bytes(&self) -> u64;
    fn terminator(&self) -> LineTerminator;
    fn byte_at(&mut self, offset: u64) -> SourcePoll<u8>;
    fn cursor_at(&self, offset: u64) -> PhysicalLineCursor;
    fn bounded_prefix(&mut self, maximum: usize) -> SourcePoll<BoundedBytes>;
    fn append_source_run(&self, range: Range<u64>, into: &mut PendingRunBuilder);
}
```

The parser may use a contiguous fast path for an ordinary small line, but both
paths invoke the same transition functions. Bounded scanners return exact
single-authority results to those transitions; the grammar must not rescan or
reimplement their decisions. An oversized classifier is fuelled over the line
cursor and source-run builder.

Reference-definition parsing is included. It must consume a logical-run cursor
or exact streaming facade results, not call `reference_definitions` on one
aggregate `String`. Until this path passes, oversized physical-line support is
not integrated with the selected block core.

### 5. Packed structural event pages

At minimum the tape needs these operations:

```rust
Open            { block, parent, kind_tag, start_anchor }
Promote         { block, metadata }
AppendRuns      { block, run_range }
DrainRunPrefix  { block, logical_bytes }
WriteEnd        { block, end_anchor, write_stamp: EventStamp }
RepairListEnds  { list, subtree_interval, repair_stamp: EventStamp }
Definition      { occurrence, symbol, value_cursor, origin_anchor }
Finalize        { block, terminal_record }
Close           { block }
```

`kind_tag`/`metadata` may own only bounded metadata. `CodeBlock.literal`,
`CodeBlock.info`, `HtmlBlock.literal`, and definition URL/title strings are
forbidden as growing owned fields. A terminal/definition record points at an
immutable logical-run cursor and coverage anchors.

Each packed page owns:

- a new `OutputPageId`;
- ordered coverage IDs and byte/UTF-16 coverage totals;
- a leading persistent projection-stack root;
- bounded event bytes and page-local event offsets;
- optionally, a scalar link to a separate legal post-line restart record; and
- constant-size tree summaries.

It owns no source bytes for identity runs and no Crop object of any kind.
Splitting/joining the page sequence must preserve unchanged leaf `ArenaId`s and
large unchanged subtrees. Tree association is not semantic identity.

All component roots (event sequence, checkpoints/run chunks, projection stack,
terminal/reference/order/repair indexes) are retained from one acyclic output-
root manifest through real `PageArena` child edges. An `ArenaId` encoded in a
payload is not by itself an ownership edge. The lifetime gate must reject a
manifest that leaves a referenced component reachable only through an
untracked scalar ID.

Event emission is itself resumable. The production sink/parser surface is
pollable (`emit -> Accepted | PageFull` and `push_line_slice(fuel) -> Pending |
Complete`), not the correctness prototype's infallible `emit(())`. A bounded
`EventPageBuilder` can seal/adopt a full page, preserve exact line-continuation
state, and yield without retaining a line-sized `Vec<StructuralEvent>`. The
parser must not build an unbounded event journal and flush it after
`process_line`.

One dense or oversized line can emit many pages of opens/promotions. Urgent
line-byte, open-depth, and event-density thresholds are scheduling limits, not
grammar limits: crossing one moves the exact job to the fuelled/pathological
lane while source stays visible. Consecutive homogeneous container opens may
use compact run encoding, but every implied `BlockId` and parent relation must
remain recoverable. A giant `> ` prefix and delimiter-dense line are mandatory
tests for temporary event bytes, pages yielded, cancellation points, and exact
clean projection.

#### Persistent-sequence APIs that must be made executable

Adapting the `Arc<OutputPage>` proof to `PageArena` is an open implementation
gate, not assumed plumbing. The minimum surface is:

```rust
EventPageBuilder::{push_event, seal_page}
OutputSequence::{from_pages, split_at_cursor, splice, locate_page,
                 locate_byte_utf16, page_rank, page_order}
OutputRootManifest::{build, retain_component, release_later}
```

Every operation returns counters for page payload bytes copied/adopted, leaf
and branch arena nodes allocated, child references added/transferred, old nodes
visited, relabeled order entries, and exact reused `ArenaId`s. Port the 65,536-
page prefix insertion, 1,000 mixed splices, stale-handle, WASM build, and
fuelled-reclamation tests before the parser attaches.

#### Viewport materialization contract

The leading projection checkpoint of every event page is an O(1) root into a
persistent open-stack store. Its frames carry the bounded facts needed to paint
source immediately: `BlockId`, structural kind/context, relevant list/property
IDs, code/raw-block classification, and coverage-relative start/current
anchors. They carry no accumulated literal or inline payload.

Querying a page whose enclosing fence/list/quote opened 100,000 pages earlier
must not replay to the `Open` event. The worker loads that page's projection
root, walks only current open depth, then folds the requested page events. Query
work is proportional to open depth plus returned pages/events, never distance
to the opener. Deep open depth is metered and tested separately; duplicating
the full stack into every page is forbidden.

### 6. Lazy list source-end repair

The current correctness materializer scans a complete list subtree on
`RepairListSourcePositions`. That is not admissible for a large list.

The leading candidate is a lazy range overlay:

- Each terminal block stores `raw_end`, `end_write_stamp`, and a
  `zero_end_fallback` frozen at close time from the final-child aggregate.
- Closing a list emits one ordered repair stamp over its closed preorder
  interval. A persistent range-max tree records the greatest repair stamp in
  O(log blocks) without visiting descendants.
- Querying a node finds the greatest containing repair stamp. If raw end
  column is zero and `repair_stamp` orders after `end_write_stamp`, it returns
  the frozen fallback; otherwise the raw end wins.
- A later `WriteEnd` therefore overrides an earlier repair. A later enclosing
  list repair can apply the same frozen fallback again. Descendant mutations
  after the repair cannot change the stored fallback.

This model is coherent with the donor's postorder rule, including nested list
repairs, but remains a hypothesis until it matches the eager oracle over all
1,322 fixtures. Examples 255 and 257 are mandatory focused cases because a
blank line closes the list before differently indented following content; a
document-final blanket repair produced the wrong timing in earlier work.

Reject this overlay if the full trace reveals a fallback that depends on a
descendant write occurring after the fallback was frozen. Do not repair that
failure by scanning or rewriting the subtree.

### 7. Reference occurrence, winner, and dependency indexes

`ReferenceIndexSnapshot` must provide the existing
`InlineReferenceSnapshot` interface and additionally own:

- collision-free normalized-label interning to stable `SymbolId`;
- every definition as an `OccurrenceId` with stable coverage anchor and
  source-order comparison through `PageOrderIndex`;
- a persistent per-symbol ordered occurrence set;
- a first-definition-wins value pointer;
- `presence_generation`, incremented only when the symbol crosses
  undefined/defined; and
- a value generation/delta for repaint without inline reparse.

Inserting/removing a definition touches only definitions in changed pages,
affected symbol trees, and persistent map paths. Removing a winner must find
the next occurrence through the per-symbol tree, never scan the document.
Repeated prefix and same-gap insertions are required to falsify hidden suffix
relabel/rewrite work.

Interned labels and order entries have explicit lifetime accounting. A symbol
mapping may be removed only after its last definition occurrence, cached
dependency, active candidate, committed root, acknowledged root, and offered
root are retired. Reintroduction then mints a new generation-safe `SymbolId`;
IDs are never silently reused. Tombstones retain at most scalar generation
state, not normalized strings or values. Coverage/page order entries follow the
same oldest-live-root epoch rule. A unique-label type/delete storm must plateau
after reclamation rather than leak session history.

The order-maintenance implementation reports labels relabeled per edit,
maximum label bytes, index nodes copied, and amortized/worst observed work in
the 10,000 same-gap history. “Variable labels usually stay short” is not an
acceptance argument; an unbounded linear label or suffix relabel is a failed
gate.

There is deliberately no eager document-wide inline-consumer index. The
dependency index is bounded to cached facts:

- each cache entry owns its exact symbol presence dependencies;
- an optional reverse `SymbolId -> cached InlineLeafId`s index is bounded by
  the cache's entry/byte cap; and
- uncached leaves own no inline facts or dependency objects and parse against
  the current reference snapshot only when requested.

### 8. Lazy exact inline adoption

Replace the synthetic document adapter with:

```rust
struct InlineLeafVersion {
    id: InlineLeafId,
    content_generation: u32,
    context: LeafInlineContext,
}
```

The terminal index supplies logical runs, origins, and exact context. Preparing
one job materializes at most the bounded inline leaf into a temporary string
and calls the real Comrak inline service. Adoption requires all of:

- current source revision;
- current parse generation/root lease;
- exact `InlineLeafVersion`, including structural context;
- current viewport epoch; and
- current reference presence generations.

The current 8 KiB Comrak input ceiling is a prototype urgent-path value, not a
final product limit. Keep three separately calibrated policies:

- an urgent atomic threshold that must fit the liveness budget;
- a larger worker/background exact threshold with independent fact/payload
  ceilings; and
- a pathological-leaf lane that is resumable, isolated, or deliberately
  deferred while source remains visible.

Crossing an urgent threshold never invokes a predictive parser and never means
“unsupported forever.” Launch policy must state how an exact result is
eventually produced for a valid large leaf and what resource ceiling protects
the process. Exercise ordinary 8 KiB, research 64 KiB, dense 1 MiB, output-
explosion, and cancellation cases before selecting thresholds. Value-only
reference changes retain structural inline facts and emit a symbol repaint
delta.

### 9. Latest-wins coordinator, deltas, and retirement

The coordinator retains three clocks and no more than these physical states:

- current source root;
- current committed output root;
- one active candidate or its latest queued replacement;
- one UI-acknowledged output root; and
- at most one offered output root during atomic handoff.

```rust
struct RuntimeDelta {
    base_output: OutputRootLease,
    target_source_revision: SourceRevision,
    parse_generation: ParseGeneration,
    offered_output: OutputRootLease,
    page_splices: Vec<PageSplice>,
    changed_symbols: Vec<SymbolId>,
    invalidated_inline_leaves: Vec<InlineLeafId>,
}
```

Every queried page/fact batch repeats the root lease, source revision, and parse
generation. Flutter adopts it only when its exact source revision is current.
A stale delta or inline completion is discarded without mutating presentation.

Cancellation is checked between ordinary physical lines, source-cursor slices,
page allocations, lineage steps, and reference-index operations. Real Comrak
inline work is atomic only within the configured urgent/background threshold;
the threshold is recorded in every receipt. Oversized physical-line
classification is a separate streaming gate, and bounded normal-line work may
not be presented as evidence for it.

Output/index pages live in the extracted `PageArena`. Cancellation and old-root
release enqueue generation-safe roots and reclaim at most caller-supplied fuel.
No recursive `Arc` destructor is accepted as the physical retirement proof.

## Adversarial multi-revision histories

Each history compares every committed result with a clean exact parse of the
same bytes. Intermediate stale generations must never publish.

1. **Unicode prefix churn before 65,536 pages.** Insert an astral-scalar prefix,
   replace it before the first job completes, undo it, then insert a different-
   length CRLF prefix. The last revision alone commits. A large suffix subtree
   and every unchanged suffix page keep exact arena identity; absolute byte and
   UTF-16 queries both match clean output.
2. **Skipped structural revisions.** Insert an opening fence near the top,
   delete a closer in a later revision, add a closer lower down, and edit again
   while parsing. Exact lineage crosses every skipped source revision; old job,
   sealed-root, page, and inline completions are rejected.
3. **Boundary-changing grammar.** Edits create/remove setext underlines,
   thematic breaks, table delimiters, lazy blockquote continuation, multiline
   reference definitions, list-blank tightness changes, fenced blocks, and HTML
   termination. Place the triggering pair on opposite output pages. No page
   boundary may alter the grammar result or authorize early convergence.
4. **Same bytes, different inline context.** Move `[x] text` through ordinary
   paragraph, first direct list-item paragraph, later list-item paragraph,
   heading, and table cell while an old inline job is in flight. Task facts
   exist only in the certified first-item context.
5. **Reference winner churn.** Start with duplicate definitions and visible plus
   never-parsed consumers. Insert a new first definition, change only its URL,
   remove winners one by one, cross undefined/defined, then restore. Winner,
   value repaint, presence generation, cached invalidation, and uncached-zero-
   dependency behavior must all be exact.
6. **Repeated same-gap definitions/pages.** Perform at least 10,000 alternating
   insert/delete operations at the same prefix/interior gap. Order labels,
   occurrence sets, and output allocation must not grow work proportional to
   the unchanged suffix.
7. **Giant open pending leaf.** Use a 10 MiB multiline paragraph under the
   research-large-inline build only as source-visible content. Edit its first,
   middle, and last lines. Pending comparisons read/hash/copy zero logical
   prefix bytes; a changed run prevents convergence until the paragraph closes.
8. **Code and HTML literal ownership.** Parse 1 MiB and 10 MiB multiline fenced
   code and raw HTML blocks using individually ordinary-sized physical lines.
   Close/finalize must not clone the aggregate literal into `BlockKind`, an
   event, a checkpoint, or a terminal record. Cancel after 1 MiB and prove
   bounded root release.
9. **List repair ordering.** Run CommonMark examples 255/257, nested lists,
   inner-repair -> later-end-write -> outer-repair, and multiple lists closing
   within one page. Compare every effective source end with the eager exact
   materializer while range-update work remains logarithmic.
10. **Reference/structure edit inside a document-spanning list.** The list's
    stable open `BlockId` survives restart; changed child events splice before
    reused suffix events. If the ID or binding state differs, convergence is
    rejected rather than rewriting suffix parent IDs.
11. **Root handoff under a stalled UI acknowledgement.** Run at least 1,000
    edits while holding the acknowledged root. Published roots never exceed
    three, queued parse state remains latest-only, reclamation stays fuelled,
    and the final current root becomes queryable after acknowledgement.
12. **CRLF/lone-CR/scalar edges and randomized edits.** Reuse coverage IDs only
    for full mapped physical lines; edits at every Unicode scalar boundary and
    around CRLF must match a clean line scanner and full parser.
13. **Deep persistent checkpoints and distant viewport.** Keep a deeply nested
    quote/list/container stack open across many event pages and sparse restart
    records. Unique frame/stack nodes grow with actual transitions, not
    `depth * pages`; querying the last page replays zero earlier pages. Include
    early-equal and deep-difference convergence comparisons under fuel.
14. **One-million-line/100 MiB retention.** Parse/describe plain short lines and
    a prefix-stripped adversarial form. Report packed coverage/run/checkpoint
    bytes and object counts. Plain identity content coalesces across coverage
    spans; neither shape allocates one heap object per line/run.
15. **Single-line event and scanner pressure.** Feed a very large plain line, a
    1 MiB `> ` container-prefix line, delimiter density, a fence/HTML opener,
    and an oversized reference-definition candidate through
    `PhysicalLineView`. No full line or aggregate reference string is
    materialized; event pages stream/yield; exact output matches clean parsing.
16. **Symbol/order reclamation storm.** Type and delete many unique normalized
    labels and repeatedly insert at one order gap while old roots are
    acknowledged/released. After cache/root reclamation, retained label/value
    bytes plateau and no stale `SymbolId` or order entry can resolve.

## Mandatory receipts

### Correctness

- Incremental vs clean equality after every committed history revision for
  source leaves, structural event projection, block kind/metadata/parentage,
  source ranges, logical content, origins, line offsets, definition occurrences,
  winners, inline facts, projection facts, and normalized HTML.
- The complete 1,322 CommonMark/GFM corpus must pass clean, every-legal-line
  restart, and varied sparse-restart/event-page-packing modes. Focused task-list
  and list-repair cases remain separate because the corpus does not exhaust the
  selected profile.
- Every attempted convergence emits a reasoned receipt for each required proof
  component; no boolean “same hash” shortcut is allowed.

### Copy, scan, and identity

For each revision report separately:

```text
source_bytes_inspected
source_chunk_bytes_copied
line_scratch_bytes_copied
pending_logical_bytes_copied
pending_logical_bytes_hashed
pending_runs_compared
coverage_descriptor_bytes/pages
logical_run_records/coalesced_spans/packed_bytes
canonical_frame_nodes_allocated/reused
projection_stack_nodes_allocated/reused
restart_records
block_kind_owned_payload_bytes
temporary_line_bytes
temporary_event_bytes
event_pages_yielded
event_payload_bytes_copied
output_pages_allocated/reused
output_index_nodes_allocated/visited
order_entries_relabelled/max_label_bytes
reference_occurrences_visited/allocated
reference_index_nodes_allocated/visited
interned_label_value_bytes/tombstone_bytes
inline_source_bytes_materialized
retained_strong_crop_leases
retained_weak_crop_leases
```

Hard assertions:

- checkpoint creation and checkpoint comparison copy/hash zero pending logical
  bytes and inspect zero pending runs when cursor identities differ or match;
- event-page creation may occur mid-line, but only legal post-line restart
  records can participate in convergence; projection pages never masquerade as
  parser checkpoints;
- sparse restart/projection roots structurally share frames/stacks and do not
  retain `open_depth * boundary_count` copied frames;
- 1/10 MiB code and HTML close operations add zero aggregate literal bytes to
  block metadata and copy no full literal at close;
- the `PhysicalLineView` 10 MiB and oversized-reference cases materialize no
  full physical/logical line, and temporary event storage stays bounded while
  pages stream;
- a proven unchanged suffix allocates zero suffix event pages, facts,
  occurrences, or terminal records, and preserves exact page IDs plus at least
  one large unchanged subtree root;
- changing the number of prefix events rewrites zero suffix `EventStamp`s;
- persistent committed output, checkpoints, inline cache, and deltas retain
  zero strong **and zero weak** Crop leases;
- after cancelling the only old-root job, a `Weak<CropSnapshotLease>` for its
  wrapper fails to upgrade while current output remains queryable;
- runtime code contains no whole-document `CropSnapshotLease::materialize`
  call; and
- reference winner removal visits changed occurrences plus logarithmic index
  paths, not all definitions or consumers; and
- unique deleted symbols/order entries reclaim normalized/value/label payloads
  after the oldest live root/cache epoch retires.

### Bounds and cancellation

- Page/index allocation is proportional to changed pages plus logarithmic tree
  paths. Use structural counters rather than a wall-time claim.
- Lineage mapping is fuelled per edit record. History exhaustion selects a
  clean parse.
- Normal block work yields at physical-line/page-allocation boundaries;
  oversized source and dense event emission yield within their own fuelled
  cursor/page state machines. Every atomic inline call reports its selected
  urgent/background threshold.
- `PageArena::poll_reclaim(fuel)` performs at most `fuel` reference transitions
  and releases at most `fuel * 4 KiB` page payload bytes.
- Published output roots are `<= 3`; inline cache bytes and entries never
  exceed configured caps; queued inline and block work is latest-window/latest-
  revision only.
- Record native and raw-WASM p50/p99 slice times and retained bytes, but do not
  turn workstation timings into a product SLA. Flutter/device gates remain a
  later acceptance layer.

## Sequenced implementation plan

1. **Extract infrastructure only.** Create the new crate with identity types,
   Crop-only source/lineage, `PageArena`, and a reduced three-clock
   coordinator. Port exact source, stale-generation, root-cap, and iterative
   reclaim tests. No parser dependency yet.
2. **Build packed event persistence.** Adapt the proven balanced split/join and
   byte/UTF-16 prefix sums to `PageArena`; implement coverage/page order,
   persistent structurally shared projection stacks, terminal index, composite
   root ownership, and suffix identity receipts. Port the exact sequence APIs
   and arena counters listed above. Do not add Markdown classification.
3. **Eliminate payload-bearing block metadata.** Refactor the exact block core
   so code/HTML/paragraph content lives in source-backed run cursors and
   append/finalize deltas. Replace `&str`/aggregate-reference input with the
   common `PhysicalLineView` and logical-run/scanner facade without adding a
   second transition path. Make the 1/10 MiB aggregate and single-line copy
   gates green before composing edits.
4. **Attach the one exact block machine.** Feed stable source-backed physical
   lines to `ResumableValueBlockParser`; stream its existing write-only event
   output to stable `BlockId` pages, and store persistent canonical frame roots
   only at legal post-line restart boundaries. Re-run all 1,322 clean/sparse-
   restart projections plus deep nesting and dense-event lines.
5. **Implement real restart/convergence.** Store runtime checkpoints at page
   boundaries, walk multi-edit lineage under fuel, require semantic + pending +
   binding + projection equality, and splice old suffix pages by arena identity.
   Run histories 1-3, 7-10, and 12.
6. **Implement exact reference indexes and lazy repair.** Add collision-free
   symbols, occurrence sets/order oracle, winner/presence generations, and the
   list-repair range overlay. Run full corpus plus histories 5, 6, and 9. A
   subtree scan or all-definition update is a failed gate.
7. **Replace the synthetic inline adapter.** Connect terminal run cursors and
   structural contexts to the existing Comrak service/cache. Preserve source-
   visible fail-closed behavior, implement/calibrate urgent/background/
   pathological exact lanes, and run histories 4-5 plus native/WASM cache
   receipts.
8. **Compose coordinator/delta/retirement.** Publish revision-tagged page
   handles, reject stale work, ack/release roots, and reclaim through the arena.
   Run histories 2 and 11 with exact source/output weak-lifetime assertions.
9. **Only then update RFC 023.** The RFC may call the architecture selected
   only if the composed receipt is green. Flutter transport/layout/IME/device
   gates remain explicit launch blockers rather than being inferred from Rust.

## Stop conditions

Stop and reassess the representation if any of these is required:

- a second Markdown transition machine for restart or page building;
- hashing/scanning a pending leaf or remaining source suffix to authorize
  convergence;
- a persistent Crop descriptor/root sidecar in output;
- mutating/rebasing every suffix fact after a prefix edit;
- absolute event ordinals that require suffix stamp rebasing;
- cloning a growing code/HTML/paragraph payload into metadata;
- materializing an oversized physical line or reference-definition candidate
  merely to enter the exact core;
- copying a complete canonical/projection frame stack into every event page or
  restart record;
- reading historical materialized output from the parser;
- scanning a list subtree to repair source ends;
- scanning all reference definitions or consumers after a local edit; or
- retaining deleted symbol strings/order labels after every referencing root
  and cache entry is reclaimed; or
- recursive/unmetered final-root destruction in the worker runtime.

If the exact block core cannot satisfy the payload/event/checkpoint contract
without restoring output reads or a second grammar, run this same composed
slice against MD4C before committing further. A parser swap alone does not
solve event persistence, source identity, reference ordering, inline cache,
transport, or Flutter layout.

## Recommendation

Proceed with this slice. The isolated gates are compatible at the conceptual
level, but **not directly wire-compatible today**. The most serious gaps are
the mutable handle-addressed block output versus immutable pages, full pending
content in checkpoints, literal-bearing `BlockKind`, and the absence of an
exact persistent reference-order index. The event-tape model plus focused
indexes gives each of those a concrete, falsifiable resolution while preserving
the two properties the product needs: definitive Markdown semantics and work
proportional to the changed/visible region.

Do not call the architecture accepted until the composed histories prove both
exact suffix identity and zero retired Crop ownership in the same run.
