# Composed block adoption contract

Status: **representation-neutral composer authority model GO; selected-storage
adoption seam HOLD**, 2026-07-16.

This contract connects the one exact block transition machine to whichever
compact structural representation wins the representation bakeoff.  It exists
to prevent a mechanically successful parser and a mechanically successful
persistent tree from being joined by a second, implicit grammar or by unsafe
checkpoint reuse.

## The checkpoint split is mandatory

A complete parser pause and a convergence key are different values.

```text
RestartState
  ControlContinuation       exact future control-transition state
  StableOpenBindings        current semantic IDs/path capabilities
  SemanticPrefixState       parser-owned composable semantic accumulators
  SourceCursor              current-revision physical-line/run cursor
  SchedulerCursor           fuel/allocation progress only
```

`ControlContinuation` comparison is a necessary control gate, not sufficient
authorization for a structural suffix. It is variant-local and contains only
fields that select future block-control branches. Stable bindings, edit
lineage, and a typed adoption proof for every changed semantic prefix remain
mandatory. `SemanticPrefixState` is still parser-owned: paragraph runs, table
preface/header descriptors, reference occurrences and finalizer cursors, list
folds, and similar values can change the exact semantic actions emitted by the
same control transition. It is never compared wholesale. A typed open-boundary
recipe composes it with the reused suffix or leaves the affected current range
explicitly unknown until an exact semantic finalizer completes.

The immutable published semantic tree is a third lifetime. The parser does not
read it to recover either continuation. A candidate builder may supply typed
persistent prefix roots while reconstructing parser scratch, but published
nodes are never hidden parser state.

The standalone `restart_composer_gate` makes this authority split executable.
Its control match returns an opaque witness with no attachment operation; only
the composer can construct an adoption permit after validating revision/edit
lineage, mapped boundary, immutable suffix-tail identity, live bindings,
aligned semantic paths, and a variant-specific recipe. The permit now retains
the exact semantic-root identity and generation plus both revisions, both
mapped boundaries, lineage, and suffix-tail identity. Every action is
permanently paired with its outer-to-inner stable binding and open depth.
The required production contract is that storage derives and supplies the same
stamp and capability path before the consuming, non-cloneable permit yields an
action; the public BlockId lookup escape hatch has been removed from the
representation-neutral model. The current selected-storage adapter does not
yet prove that derivation.

Its 14 debug/release tests cover changed list output, two equal-control
table-preface shapes, setext, reference-only detach/winner invalidation,
raw-run splicing, stale capabilities, mid-line rejection, old-output
non-restoration, every base-stamp substitution, and missing, wrong, or
reordered storage capability paths. A compile-fail gate rejects permit replay;
strict Clippy and WASM are green. The gate uses persistent source runs, paths,
folds, and exact tries with no complete semantic frame, growing `String`/`Vec`,
digest, or hash authority. It does not yet execute the complete set of
capability-bound recipes or suffix attachment under the shared top-level
transaction.

The focused `composed_adoption_storage_gate` closes a narrower mechanical step.
It consumes a Setext permit, checks caller-supplied stable and concrete values,
and runs the property portion through one immutable-base Enter+facts rewrite.
The successful witness retains BlockId, keeps a distant leaf ArenaId exact,
and leaves the old root queryable. Its wrong-first-leaf test fails without a
live-node delta, and no BlockId lookup is introduced.

It does **not** prove the selected-storage adoption seam. The storage stamp is
caller-supplied, root identity omits the `PageArena` instance identity, stable
capability IDs are not tied to physical Enter capabilities, the Setext content
recipe is discarded, no suffix is attached, and the output retains the base
source revision. The one-use permit is nevertheless consumed. The exact gaps
and required private base-proof/atomic-transaction replacement are recorded in
[`COMPOSED_STORAGE_AUTHORITY_AUDIT.md`](COMPOSED_STORAGE_AUTHORITY_AUDIT.md).
List/raw/table/reference/range recipes, suffix attachment, and late-failure
rollback remain unimplemented rather than hidden behind a BlockId adapter.

The current serialized `BlockCheckpoint` is a correctness witness for pausing
and restoring scratch.  It is **not** a production convergence key.  Reusing
an old complete checkpoint after list looseness, paragraph content, or another
output accumulator changed would silently restore the old prefix into the new
revision.

Production resume must therefore reconstruct scratch from the split values.
No parser operation may query already-materialized semantic output to do so.
The candidate root provides typed aggregate values through the adoption
builder; the block machine continues to see only its private value state and a
write-only sink.

## Revision flow

1. Flutter commits source and selection immediately.  The worker admits the
   latest revision and cancels any superseded candidate.
2. Exact edit lineage maps a sampled old restart boundary into the current
   source.  Expired or changed lineage starts a clean parse; byte equality or a
   hash may reject work but cannot recover identity.
3. The parser resumes from `RestartState`.  Stable open bindings are validated
   against the old semantic root and the mapped source boundary.
4. Every parser poll streams bounded structural/output mutations directly into
   one candidate transaction.  Events are transient commands; they are not a
   committed event tape.
5. Only after a complete physical line may the worker test convergence against
   the correspondingly mapped old boundary.
6. After control convergence, the top-level composer selects and validates a
   typed adoption recipe for each changed open semantic prefix. Only then are
   unchanged structural/source pages attached by owned reference. Changed
   open-ancestor aggregates are recomputed from the new prefix and reused
   suffix without visiting every child.
7. Any exact output work that is not complete is represented by an explicit
   current-revision `UnknownRange` and authority mask.  A known suffix may be
   published after that range when its structural proof is independent.
8. Structure, coverage, aggregates, restart samples, and any presentation
   lease are validated and committed under one epoch/range manifest.  Failure
   or cancellation before manifest ownership transfer rolls back the entire
   candidate and leaves the old root queryable.

## Exact convergence predicate

At an old/current physical-line boundary, suffix attachment requires all of:

- operation-derived source alignment and complete unchanged physical-line
  identity from the boundary onward;
- equal syntax profile, grammar version, and variant-local
  `ControlContinuation` for every open frame;
- equal stable block bindings for the retained open path;
- an exact source-backed pending recognizer cursor where future grammar really
  reads prior input, or a boundary before that recognizer;
- a typed recipe proving that every changed semantic prefix has an exact
  associative/local adoption operation for that boundary; and
- suffix pages whose facts are coverage-relative and require no absolute
  coordinate repair or old Crop root.

The comparison occurs only at a physical-line grammar boundary.  A scheduler
pause in the middle of an oversized line carries resumable scanner state but
is never a convergence candidate.

## Variant-local continuation target

The current code audit sets this target; executable tests may reduce it but may
not replace it with a digest:

- global: profile/version, document-start state, current-frame selector, and
  the ordered open-frame path;
- List: matching type, delimiter, and bullet character;
- Item: effective content indentation and effective direct-child presence;
- indented code: kind only;
- fenced code: fence character, minimum closing length, and fence offset;
- HTML: typed block/terminator state;
- Paragraph: whether table promotion remains eligible plus the exact bounded
  table-header recognizer/source cursor still read by a future line;
- Table: column count and the autocompleted-cell count saturated at the
  grammar cap plus one;
- other leaf/container variants: only fields demonstrated by a future read.

Displayed ordered-list start, list tightness, `last_line_blank`, complete child
folds, raw literal projections, paragraph/reference payload, table row count,
and source-position repair history are output.  They cannot prevent otherwise
valid block convergence.

## Semantic-prefix adoption cases

### Spanning List and Item

The changed direct-child contribution is spliced into the persistent child
aggregate.  Its closed summary propagates through at most the open ancestry.
The unchanged sibling suffix keeps exact block/page identity even when final
list tightness changes.

### Fenced code and HTML

Raw content is a source-backed run sequence.  An edit changes only intersected
runs and aggregate projections.  Once fence/HTML continuation state and source
alignment converge, the remaining content runs may be shared; full raw content
is never part of transition equality.

### Paragraph, table-header handoff, and reference definitions

Only the bounded prior-input state still needed for possible table promotion
belongs to grammar continuation.  Paragraph content is a source-backed run
sequence.

Equal table-control state does not imply an equal semantic event stream. For
example, two two-column headers can have equal grammar continuation while one
has a multi-line preface that must become a sibling paragraph during table
promotion. That preface splice is a typed output-prefix adoption, and suffix
attachment is forbidden until the composer certifies it. It is not a reason to
put paragraph bytes in grammar equality, but it is why grammar equality alone
must never expose an `authorize reuse` API.

Reference-definition recognition and leading-definition removal are exact
semantic-prefix finalization. A changed paragraph may control-converge before
that finalizer completes.  In that case the paragraph/output-reference range
is current but `Unknown`; a certified structural suffix may still attach.
The finalizer then emits ordered definition occurrences, the surviving
paragraph projection or detach, and a local structural/property splice under
a newer semantic-root generation.  It cannot retroactively authorize the
already-attached block suffix.

In serialized green, “detach” means remove the Paragraph semantic wrapper,
not delete its entire balanced source range. Definition bytes remain in the
one physical coverage stream, become source-only/reference-definition runs
owned by the still-open parent, and commit with occurrences/winner changes.
Visible remainder, if any, keeps a split Paragraph wrapper. This distinction
is mandatory because deleting the balanced range would corrupt total source
metrics or force a second coverage authority.

Table promotion is different: the delimiter line changes the block transition
itself.  The table-header recognizer must therefore be complete before the
boundary can converge; `Unknown` cannot be used to attach a suffix across an
unresolved grammar decision.

### Setext promotion, detach, and reparent

These are typed structural replacements of a known contiguous range.  Stable
identity is retained only where the exact parser binding says the semantic
block survived.  No generic range repair walks or guesses identity from equal
text.

## Stable identity rule

- A block opened after the restart boundary receives a fresh ID unless the
  parser performs an explicitly typed promotion that preserves identity.
- Open ancestors restored from the old restart sample retain their stable IDs
  while their immutable node/version may change.
- Unchanged suffix blocks recover identity only by the exact convergence and
  page-attachment proof.
- Coverage IDs survive only when edit lineage maps their complete physical
  source unit unchanged.
- Page IDs, block IDs, coverage IDs, source revisions, parse generations, and
  presentation generations are distinct types and never inferred from one
  another.

## Required composed tests

The first executable composition is not accepted until it covers:

1. every-line restart and clean-parse equality over the full CommonMark/GFM
   block corpus with canonical Flark ranges;
2. an edit in a 100,000-item spanning list where transition state converges,
   tightness changes, and distant suffix IDs/pages remain identical;
3. early edits in 10 MiB fenced-code and HTML blocks with bounded raw-run
   replacement and suffix sharing;
4. a giant paragraph edit that converges structurally while reference output
   is unknown, then exact finalization produces the same blocks, occurrences,
   ranges, and HTML as a clean parse;
5. table delimiter and setext edits that demonstrate convergence is withheld
   until the grammar-changing decision is known, including equal-column
   headers with versus without a promoted preface paragraph;
6. reference-only paragraph detach, duplicate first-winner replacement, and
   consumer invalidation without a global block reparse;
7. nested quote/list markers and blank gaps with exact byte/UTF-16 owner and
   enclosing-path queries before, inside, and after the unknown range;
8. cancellation or forced allocation failure at every parser, page, aggregate,
   restart-sample, and manifest boundary with zero leaked candidate owners;
9. a prefix splice with no suffix coordinate rebasing or dependency on the old
   source root; and
10. latest-wins publication with no stale semantic or presentation lease
    accepted by the UI.

## Stop conditions

Stop and redesign the seam if composition requires:

- comparing or restoring a complete old pause checkpoint as if it were the
  transition key;
- treating equal control continuation as sufficient without a typed semantic-
  prefix adoption recipe;
- reading the committed semantic tree from the block grammar;
- retaining transient parser events as historical output;
- parsing an unknown region with a second classifier to meet a UI deadline;
- treating a table/setext grammar decision as output-only;
- scanning siblings/subtrees to repair a reused suffix; or
- committing components in separate ownership transactions and validating the
  epoch only afterward.
