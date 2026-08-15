# Candidate parser state partition

Status: working architecture contract, 2026-07-16. This is narrower than RFC
023 and exists to prevent the next executable gate from accidentally proving
the wrong persistence model.

## Why this partition exists

An incremental parser can be semantically exact and still be unusable if its
checkpoint equality includes unrelated document state or if its reusable
facts retain revision-local absolute coordinates. Either mistake converts a
local edit into hidden suffix work.

The next block-core gate must therefore distinguish these authority/lifetime
domains:

1. `ControlContinuation`: only values that can affect how the next physical
   line is recognized;
2. `SemanticPrefixState`: unresolved/folded output values needed to finish the
   current prefix, including source/projection builders and pending gap state;
3. `StableOpenBindings`: root-scoped capabilities that connect the two value
   paths to concrete candidate storage without a BlockId lookup;
4. `SourceCursor` and source-lineage proof: exact revision/boundary authority;
5. `SchedulerCursor` plus the resumable arena build ticket: cooperative work
   and candidate ownership, never grammar equality;
6. immutable packed semantic/reference/restart roots; and
7. revision-local materialization used only while serving a query or delta.

Only the top-level convergence composer may combine items 1--5 into a complete
semantic recipe. Selected storage independently derives the real base/source/
path proof and is the only component that may publish item 6.

## Do not conflate three continuation roles

The executable gate deliberately serializes after every physical line to prove
that grammar continuation is not secretly being read from prior output. That
does **not** imply production should serialize or persist a complete checkpoint
after every line. There are three different roles:

1. **Live scheduling continuation** belongs to one worker job. It may hold the
   current source lease, reusable scanner scratch, an opaque pending-leaf
   builder handle, and write-only sink bindings. It survives a cooperative
   yield in worker memory and is destroyed on cancellation. It is not a
   reusable output fact and is never part of convergence equality.
2. **Canonical convergence input** is the disjoint tuple of exact control,
   semantic-prefix state, stable open bindings, source cursor, and legal
   scheduler boundary. Control equality alone never authorizes adoption.
   Semantic-prefix state may include persistent source-run expressions and a
   coalesced unresolved gap, but no copied aggregate leaf text, mutable output
   node, donor AST, or complete duplicate semantic frame.
3. **Persisted restart checkpoints** are sampled at useful sealed boundaries,
   normally closed block/page boundaries rather than every physical line.
   Pathological open constructs may retain an explicitly bounded streaming
   classifier record. Persisting a checkpoint must not repeatedly copy an
   ever-growing paragraph, code block, HTML block, or origin vector.

The every-line serialize/discard/write-only-sink test is therefore an
architecture falsifier: it proves rehydration from values and coverage is
possible. Its allocation pattern is not automatically the production storage
policy. The later integrated gate must separately prove that live yields and
persisted checkpoint sampling are bounded.

The proof-era falsifier passes all 1,322 CommonMark/GFM fixtures with exact blocks,
source ranges, logical leaves, origins, references, and HTML. Live scheduling
no longer serializes that state: it consumes/moves open frames and emits
append/drain/finalization deltas. Across 1 MiB paragraph, fence, HTML, list, and
table shapes it copies zero pending-prefix bytes and keeps 2–7 transient nodes.
Persisted JSON restart deliberately remains copy-based and is not the selected
runtime seam. The production restart value must use the disjoint state above
and packed persistent leases for giant open constructs.

The earlier lazy list-position overlay proved that donor repair chronology
could be represented without eager descendant scans, but the canonical-range
gate showed that it should not be persistent product state at all. Across all
1,322 fixtures and 3,841 nodes, ignoring 208 donor repair events preserves the
exact tree/kinds and normalized HTML while a total Flark coverage model has
zero parent-containment failures. Pristine donor ranges have eight. The
selected output therefore derives container hulls from canonical terminal,
marker, gap, ancestry, and subtree aggregates; repair stamps and historical
position snapshots are deleted. The batch adjudicator is semantic evidence
only. Packed local splice, suffix identity, and bounded aggregate updates
remain integration gates.

## Block continuation state

Only lossless values that can change the interpretation of the next line
belong in an exact block checkpoint:

- open quote/list-item container descriptors and only fields read by later
  transitions: list matching kind/delimiter/bullet, item indentation/padding,
  and effective `has_any_child`;
- current leaf kind and the pending paragraph/setext/table-header handoff;
- fenced-code character, run length, indentation, and closed/open state;
- HTML block type and the exact terminator state needed by that type;
- enabled syntax-profile/version identity; and
- any bounded scanner state whose next transition depends on already-consumed
  bytes of an oversized physical line.

The state is a value serialization. It contains no pointers into scratch,
mutable AST/tape nodes, a donor parser arena, or a retired Crop revision.

Line-ending/origin builders, visible paragraph runs, reference-definition
folds, close-time list aggregates, and `PendingGapRange` belong to
`SemanticPrefixState`, not `ControlContinuation`. Concrete Enter/range handles
belong to `StableOpenBindings` or the candidate build, and source revision/
boundary values belong to `SourceCursor`. Keeping those values disjoint is
what lets the composer prove all required equality without making published
green storage a hidden grammar checkpoint.

Ordered-list displayed start, eventual list tightness, open-frame
`last_line_blank`, code/HTML output projections, and the five accumulated
child-looseness bits are output state. The child bits form an exact associative
33-state range summary; changing any of them does not change the later block
transition trace. Keeping these fields out of transition equality is required
for convergence inside a document-spanning list.

### Blank-gap ownership is bounded future-dependent output

An initially matched blank line cannot always be assigned immediately. In a
continuing list/quote its gap remains owned by the deepest surviving container;
when the following nonblank line exits that container, the same physical gap
must be lifted to an ancestor or the document root. Emitting it immediately
and repairing positions later would restore the donor mutation history this
architecture rejects.

The parser therefore retains one coalesced `PendingGapRange` containing exact
source boundaries/metrics and the provisional deepest matched binding. More
blank lines extend that range in O(1). The next nonblank line resolves it to
the deepest surviving open binding, or EOF resolves it after final closure.
The resulting `SourceProjectionRun` has logical contribution `None`. This
pending value is restart-serializable semantic-prefix state; it is not a
grammar guess and does not retain source text. Initial convergence may occur
only after it is resolved, unless the composer later gains a typed
`ResolvePendingGap` recipe proved against the old suffix.

## Reference definitions are output, not continuation state

Comrak 0.54's `resolve_reference_link_definitions` removes every recognized
leading definition from a paragraph. `parse_reference_inline` returns the
consumed length even when a normalized label is already present; its lookup of
the current `refmap` controls only whether a new winner value is inserted.
Consequently, first-definition-wins state does not influence later
block/container transitions for the selected CommonMark/GFM profile.

The owned engine should emit every definition occurrence into a persistent
ordered occurrence index. A separate aggregate derives each symbol's first
winner. Block suffix convergence therefore does **not** compare a global
reference snapshot.

Reference consumers use the bounded inline service and retain:

- stable normalized-label symbol ID;
- symbol `presence_generation` and whether it was resolved; and
- the normalized label as a collision/corruption guard.

A winner URL/title-only change updates the symbol value without reparsing
consumers. A defined/undefined transition changes inline grammar and schedules
only dependent leaves. Removing a winner while another duplicate remains a
winner is a value change, not a presence transition. None of these cases is a
reason to reject otherwise-valid block suffix convergence.

Pending bytes for a not-yet-finalized multi-line reference definition remain
ordinary local paragraph/leaf state until the exact reference facade accepts
or rejects them.

## Inline-derived block annotations are output, not continuation

Comrak recognizes a strict GFM task marker only after inline parsing: the first
parsed child must remain Text, then its generated task scanner consumes the
decoded `[ ] ` / `[x] ` prefix and annotates the containing Item/List. This
ordering matters for escaped openers, resolved shortcut references, and entity-
decoded whitespace.

The block spine therefore emits only a structural certificate that a leaf is
the **first Paragraph directly under Item under List**. Tightness is unrelated;
a loose item's first paragraph remains eligible, while later paragraphs do
not. The bounded inline service returns an exact task-symbol fact, checked
state, and hidden-prefix projection. A renderer/list aggregate may promote
that fact to checkbox or list-level presentation.

Task recognition cannot affect how the next physical line is parsed for the
selected profile, so neither TaskItem nor `list.is_task_list` belongs in block
checkpoint equality. This keeps the phase split exact without making the block
core duplicate Comrak's decoded-inline precedence.

Inline cache identity must nevertheless include this structural certificate.
Leaf bytes alone are insufficient: unchanged `[x] text` bytes can move between
an ordinary paragraph, a later list-item paragraph, and the certified first
paragraph. The cache version therefore includes the block-owned inline input
context; a context change withdraws the old visible facts before scheduling a
lazy reparse.

## Canonical product ranges are not donor mutation history

Comrak's list source-position repair has observable ordering, but it is donor
AST bookkeeping rather than Markdown continuation or a coherent editor range
contract. In fixture 257 an item extends beyond its parent list. Across the
complete corpus there are eight such donor containment failures and zero under
the canonical Flark model.

Committed ranges therefore come from a total non-overlapping coverage
partition and current final ancestry:

- terminal and container-marker facts own their exact syntax;
- explicit gaps own every otherwise-unclaimed byte;
- the real document root owns root-level gaps and the full source range; and
- a container range is the hull aggregate of its own markers and preorder
  subtree, guaranteeing parent containment.

`RepairListSourcePositions`, `PositionStamp`, `end_stamp`, repair scopes, and
prior descendant position snapshots are absent from production records and
deltas. Donor source positions remain differential evidence only. V2 behaviors
that used block endpoints as gap/focus proxies must migrate to explicit gap
adjacency, exit-separator, selection, and mounted-host facts rather than
mechanically adopting different endpoints.

## Crop revisions are source leases, not persistent fact anchors

Crop supplies an immutable source revision and cheap edited successors. Its
public API does not expose a stable subtree identity suitable for persistent
fact anchors. A descriptor containing `{root_id, absolute_start,
absolute_end}` is useful while reading one revision, but must not be embedded
unchanged in a reusable output suffix.

After a prefix-length edit, revision-local absolute facts would require one of
two invalid outcomes:

- rewrite every reused suffix fact to the new root/offsets; or
- retain old Crop roots indefinitely so the old coordinates remain readable.

Persistent block, origin, and inline facts must instead be relative to
immutable coverage/output leaves. Ordered index nodes carry subtree source
lengths and fact counts. A local splice rebuilds only changed leaves and
`O(log pages)` index paths; a reused suffix page keeps the same identity and
payload. Absolute offsets are derived by prefix sums only when answering a
current-revision query or encoding a visible/changed delta.

Logical inline ranges are leaf-relative. Compound physical origins use
leaf-relative origin runs or stable coverage-leaf references. Container
ancestry/tightness should use structural events and aggregate properties rather
than absolute descendant rewrites.

## Exact convergence contract

A candidate structural suffix is reusable only when all of the following are
proved:

1. a source-store-derived, non-constructible lineage proof binds the exact old
   and current source root IDs/revisions and maps the complete retained suffix
   without crossing changed bytes;
2. the remaining logical/source suffix identity is exact for all coalesced
   edits, not merely equal in length or hash;
3. the complete block continuation values are equal;
4. semantic-prefix values compose into complete typed recipes for every open
   frame; unresolved pending gaps are either resolved before the boundary or
   covered by an explicit resolution recipe;
5. stable bindings resolve to one storage-derived nested physical path on the
   exact immutable base root; and
6. one resumable storage transaction consumes the complete recipe, attaches
   the retained suffix, publishes the current source revision and all composite
   roots, and can reuse old pages without rebasing facts or retaining an old
   Crop root.

Reference symbol values and the validity of separately cached inline leaves
are deliberately outside item 3. They have their own occurrence/dependency
indexes and revision-safe adoption rules.

## Required executable receipts

The next integrated candidate is incomplete until it demonstrates:

- a prefix insertion followed by structural convergence reuses suffix output
  pages by identity with work bounded by changed pages plus tree depth;
- the reused suffix retains zero strong or weak dependency on the old Crop
  root after retirement;
- a reference-value edit converges structurally and changes one symbol without
  reparsing consumers;
- undefined/defined transitions reparse only scheduled dependent inline
  leaves, prioritizing visible leaves and staying cancellable;
- list-tightness or container aggregate changes do not rewrite every
  descendant fact; and
- current absolute canonical byte/UTF-16 ranges reconstructed from relative
  facts satisfy total coverage and parent containment after insertions,
  deletions, CRLF edits, and Unicode edits; every deliberate donor range delta
  is enumerated rather than silently normalized.
