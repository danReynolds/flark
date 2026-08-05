# Setext and fenced-code transaction gate

Status: **fenced-code plus fresh-build and retained-restart no-reference
Setext identity transactions integrated and green; reference-bearing
normalization remains open**, 2026-07-18. Ordinary
Paragraph/Quote/List/Item composition is executable. The retroactive audit has
selected the `LeafNormalizationGroup` contract in
`LEAF_NORMALIZATION_GROUP_GATE.md`; this gate now carries its Setext and raw
block receipts.

## Decision being tested

Fenced code remains an additive terminal kind over the existing exact source
ledger and projection composer. Setext is one exhaustive outcome of the active
Paragraph's opaque normalization group:

```text
NormalizeSetext
  consume the provisional Paragraph group and certified branch proof
  validate the fresh-build or retained-base group manifest
  retain the same BlockId and all surviving Inline coverage
  atomically install canonical Heading structure and reference/source changes
  return a Heading transaction at the same stack depth
```

The group, rather than a kind-neutral block binding, carries the private
provenance needed to restore an old canonical Paragraph, Heading,
reference-only gap, or Table to the provisional Paragraph state at a restart
sample. Fresh append and retained-base replacement share this semantic API;
storage chooses the bounded physical operation. No published manifest is
rewritten in place, and no document-wide `BlockId -> node` directory exists.

Closing Paragraph and opening a new Heading remains invalid: it changes
identity, leaves earlier coverage under the wrong wrapper, and emits two
structural blocks. A retained `Promote` event is also rejected because final
green is canonical rather than event-sourced.

## Setext branch truth

The donor branch has three semantically distinct outcomes:

1. A Paragraph with visible content and no leading definitions promotes to a
   Setext Heading. Existing Inline coverage remains unchanged.
2. A Paragraph with leading definitions and visible remaining content first
   finalizes the definition prefix, then promotes the surviving block while
   publishing the reference deltas in the same candidate.
3. A definitions-only Paragraph does **not** retire and retry ordinary block
   precedence. The Setext handler still consumes the branch; after definition
   finalization, the underline is appended as Paragraph content. In particular,
   `---` must not become a thematic break through fallback.

The third rule corrects an earlier misleading note in
`PARSER_SITE_SOURCE_LEDGER_PLAN.md`. CommonMark example 216 is a required
regression.

For the no-reference slice, the line recipe is:

```text
ResolveTerminator(CloseNone)
retained outer-container prefix claims
PromoteSetext(level, NoReferencePrefix)
Heading BLOCK_MARKER for the underline, logical None
Heading TERMINAL for its physical line ending, logical None
FinishLine
```

`NormalizeSetext` is a writer transaction, not a retained green event. It must:

1. flush preceding projection output and consume the exact group authority;
2. require provisional Paragraph control on the top active group;
3. validate the sealed old group manifest or the current fresh-build group;
4. consume every scanner-certified reference/source/projection partition;
5. path-copy only changed packed pages and persistent-tree depth;
6. install typed Setext Heading facts with the same primary `BlockId`;
7. update green, coverage, references, checkpoints, aggregates, and the active
   writer state atomically; and
8. return a new non-cloneable Heading transaction at the same stack depth.

Failure after any authority-changing phase poisons the candidate and leaves the
published root independently queryable. No partially promoted binding returns
to the parser.

### Selected fresh-build physical algorithm

The fresh writer records the provisional Paragraph `Enter` when it emits it;
it never searches backward by `BlockId` and never scans to the eventual
Paragraph `Exit`. Its authoritative coordinate is the build plus zero-based
event ordinal and source metric before the event. A partial-leaf byte offset is
only a current physical hint.

The canonical empty-facts Paragraph `Enter` is 10 encoded bytes and the current
Setext Heading form is 17. If those extra seven bytes still fit in the active
fixed-capacity 4 KiB leaf, the builder validates and replaces that one encoded
record in place. The leaf's structural/source monoid and Program-child array do
not change, so this path allocates no arena page. At the seven-byte capacity
cliff, the builder does not grow a second partial-page splitter: it force-seals
the unchanged leaf and enters the same sealed-page path used below.

For a sealed opener, the builder first reduces its packed tail into exactly one
build-owned working prefix. It resolves the logical event coordinate through
`GreenSummary.tokens` and `metric`, decodes only the containing page, replaces
the exact Paragraph record, and repacks that page into one or at most two
pages. Existing projection Program payload pages are retained by authenticated
child IDs while the old root remains live; their payload bytes are never
copied. A resumable owned splice replaces the old page and installs its output
as the sole working prefix, after which the same reusable tail builder resumes.
There is no prefix chain, per-Paragraph segment tree, or permanent header
indirection.

Only after the green root is installed does the builder retype its active
validator frame. The source ledger then consumes the old Paragraph binding and
returns a Heading binding with the same `BlockId`, depth, path generation, and
logical metric. A failure in either half poisons the one arena journal, so no
mixed state can publish.

This algorithm deliberately reuses only the retained prototype's canonical
codec/page-fitting helpers. Its synchronous immutable-manifest transaction and
whole-Paragraph range scan are not part of the fresh path.

### Retained-restart deferred identity join

A restart may encounter a Setext underline after the primary Paragraph opener
has already been retained on the other side of the restart cut. The parser
cannot decide from the underline alone whether the current fragment is the
whole old Paragraph or whether a later Paragraph fragment must reopen the
retired identity. The writer therefore promotes storage with a fresh temporary
Heading identity while retaining one inseparable pair of private capabilities:

- the source-ledger identity recipe naming the retired Paragraph and temporary
  Heading under their authenticated parent; and
- the packed-green storage locator naming that exact promoted Heading Enter,
  updated if the active leaf is later sealed or repacked.

The next parser-selected structural action resolves the pair exactly once. A
Paragraph open consumes the residual recipe and reopens the retired Paragraph.
A non-Paragraph open chooses the whole-fragment result and rewrites the Heading
Enter back to the retired primary identity. The same whole-fragment rewrite
must run before an ancestor close or finish crosses the authenticated parent.
This last rule is essential: waiting until generic finish is too late after the
Document or another parent has already closed.

The generalized ordering invariant is:

```text
resolve deferred normalization
  -> acknowledge canonical green identity
  -> perform the structural Open, Close, or Finish that crosses its parent
```

The active-leaf case is a same-width identity rewrite. The sealed case locates
the exact event through the private storage capability, decodes one bounded
leaf, changes only the Heading `BlockId`, and installs it through the canonical
owned replacement path. Exits, folds, Program children, and source/projection
metrics remain unchanged. Capping the retained host prefix at the changed leaf
makes the rewritten identity visible without a second tree or a block-ID
search. Wrong-build, stale, replayed, or crossed capability pairs fail closed;
every phase yields the arena lease and cancellation retires the unpublished
journal.

### Checkpoint pairing constraint

The parser-only pause and a green leaf cut are each insufficient. An admitted
line-boundary checkpoint is one opaque join of parser control, writer open
bindings and active group/fence state, exact source/deferred-source state, a
fully drained composer reset, and the consumed green cut. Capture must occur
after `FinishLine` acknowledgement with no line-local work or escaped source
atom. The composer is drained before the leaf barrier, and the matching live
arena session consumes the cut before suspension.

The reset is a role on that exact composite cut, not a bit that must be
retroactively installed on the preceding Coverage run. A checkpoint-specific
drain rejects a pending right-biased `Virtual` before mutation; after the
parser, ledger, composer, and green metrics agree, the writer mints
`CheckpointProjectionResetAtCut` in the same entry with no duplicate
coordinate. This also represents source-zero and zero-metric structural sides
that a preceding-run marker cannot. Run-attached reset bits remain optional
storage/query acceleration, not restart authority.

The durable logical green coordinate is `(events_before, source_before)` under
the named build/generation. `leaves_before` and adjacent page IDs are cache or
corruption witnesses, not authority: a Setext rewrite is one event to one event
with zero source delta, even when a packed page becomes two. Consecutive blank
lines may advance only a pending gap and emit no new green event; checkpoint
admission must then reuse the current sealed logical cut or skip the sample,
never fabricate an empty semantic event.

Full Setext is one enumerated group outcome. It may reclassify certified
definition runs to the nearest surviving parent as Gap, remove them from the
terminal logical projection, retain visible coverage under the primary block,
and publish occurrences/winner changes. Definitions-only Setext instead keeps
the group unresolved as the same Paragraph and appends the underline as
content. Neither path accepts an arbitrary range or replacement graph.

## Typed Heading facts

```text
GreenHeadingOpenFacts
  level: 1..=6
  style: Atx | Setext
```

Paragraph and Heading both use the Inline logical channel. Therefore the
no-reference outcome reuses prior source/projection runs by identity even
though storage canonicalizes the enclosing balanced group.

## Fenced-code facts and source ownership

```text
GreenFencedCodeOpenFacts
  fence: Backtick | Tilde
  minimum_closing_length: u64
  fence_offset_columns: 0..=3

GreenFencedCodeCloseFacts
  closed: bool
  info: relative byte/UTF-16 logical slice
  literal: relative byte/UTF-16 logical slice
```

Fence length is not an eight-bit fact. Giant-line support forbids silently
capping a valid run at 255.

One FencedCode terminal owns one source-backed Literal stream. The writer
maintains a constant-size fold for first-line content end, first-line logical
end, total logical end, and accepted closer cuts. The parser supplies grammar
decisions; the writer supplies byte/UTF-16 metric truth. Info normalization is
a bounded derived service over the stored info slice, not a copied string in
the structural parser.

Physical opener/closer boundaries are already exact `BlockMarker`/`Terminal`
coverage cuts in the unified stream; duplicating them in close facts would add
a second coordinate authority. Only the logical info/literal slices belong in
the close payload.

| Physical source | Owner and part | Logical contribution |
| --- | --- | --- |
| outer quote/list prefixes | matching container, ContainerMarker | None |
| opener local indent plus complete fence run | FencedCode, BlockMarker | None |
| opener remainder/info | FencedCode, Content | raw source-backed |
| opener line ending | FencedCode, Content | canonical newline |
| body deindent | FencedCode, ContainerMarker | None |
| body remainder | FencedCode, Content | identity/typed atoms |
| body line ending | FencedCode, Content | canonical newline |
| accepted closer including allowed indent/trailing space | FencedCode, BlockMarker | None |
| closer line ending | FencedCode, Terminal | None |

The closer is an old-frame final-use witness. Outer retained prefixes and the
closer marker are emitted while FencedCode remains open; its `Exit` follows the
closer terminal. Exact physical cuts must be returned by the committed fence
handler. Advancing a donor cursor and reconstructing the marker later is not
accepted.

Raw replay must coalesce ordinary spans while preserving partial-prefix tabs,
unconsumed tabs, NUL replacement, LF/CR/CRLF, Unicode metrics, and cancellation.
It must not emit one parser command per scalar or copy a giant literal.

### Current fenced-code receipt

Fenced code now crosses the real direct parser, `ExactBlockJob`, source ledger,
projection composer, writer-owned constant-size logical fold, resumable packed
green builder, and sole local commit. The parser emits only `InfoEnd` and
`LiteralStart` semantic marks; no parser-facing byte/UTF-16 offsets or caller-
supplied close facts cross the seam. Every accepted logical source piece is
accounted at the ledger's single acceptance point, including canonical
terminators.

Executable tests cover Unicode info text with CRLF, exact byte/UTF-16 info and
literal slices, empty-info LF/lone-CR/CRLF, a 300-character fence, bare EOF,
nested Quote ownership, initial BOM, two sequential fences, tab/NUL failure,
fuelled abort, injected facts, reversed/missing marks, and an unclosed fold.
The sequential bare-EOF witness found and corrected a missing authoritative EOF
observation after an all-marker line; finalization now waits through one
resumable source-EOF confirmation. The correspondent parser also has a real
post-line pause/resume seam: it retains only O(open depth) control state, zero
source bytes, remaps fresh node IDs, and passes exact suffix-command tests for
terminators, blank floors, list folds, fences, BOM, Setext, and EOF. Its full
crate suite passes 99/99, including the 1,322-fixture differential corpus; the
pause scale receipt is 120 bytes at depth 2 and 1,656 bytes at depth 66. The
production checkpoint remains open because that parser half is not yet paired
with writer bindings, group provenance, source/composer cursors, and a committed
green cut.

This closes the normal-sized fenced-code composition question. Oversized
whole-line scheduling, 1/10 MiB integrated literal workloads, restart on the
first line after a checkpoint, and floor-device latency remain broader parser
and product gates.

### Current Setext receipt

No-reference Setext now crosses the real direct parser, `ExactBlockJob`, source
ledger, projection composer, `CandidateWriter`, and packed-green builder. The
writer records one private linear capability for the provisional Paragraph
`Enter`; the parser can request only the typed Paragraph-to-Setext outcome, and
the ledger retypes only after packed storage acknowledges the canonical
Heading. Direct Setext Heading creation is rejected. The returned writer
receipt independently matches block identity and exact Heading facts before
the parser accepts the transition.

The active-page path rewrites the canonical 10-byte Paragraph `Enter` to the
17-byte Setext Heading form in place with zero arena allocations. At the exact
4,096-byte capacity cliff, the unchanged page is sealed and one bounded job
repackages it into two pages with payloads of 4,091 and 92 bytes. That witness
took 11 polls and four arena allocations: the sealed page, two replacements,
and one AVL branch. A sealed middle-page promotion preserves the exact prefix,
suffix, and Program payload page IDs. First, middle, last, wrong-generation,
wrong-coordinate, replay, and cancellation-after-every-poll witnesses all pass.
Fixed Setext repack scratch is 10,240 bytes; the already-reusable splice scratch
is 10,688 bytes.

End-to-end receipts cover LF, lone CR, CRLF, bare-EOF underlines, Unicode,
sequential normalization groups, nested Quote/List ownership, exact
byte/UTF-16 totals, typed Setext level/style facts, physical-only
underline/terminator coverage, and exact logical projection of the visible
heading text. An injected failure after green acknowledgement but before ledger
retyping poisons and fuel-cancels the unpublished candidate; it cannot finish
or publish a mixed state.

The retained-restart gate now proves both deferred outcomes on the real actor,
source ledger, packed-green journal, checkpoint index, adoption splice, and
host-prefix machinery. A later Paragraph consumes the residual recipe; EOF,
an ATX Heading sibling, and the first ancestor close choose the whole result
and restore the retired Paragraph identity on the canonical Setext Heading.
Focused tests cover exact EOF identity, non-Paragraph ordering, stale polling,
unit-fuel cancellation, crossed storage/ledger authority, clean-parse semantic
equality, distant suffix reuse, and parent-selected publication. The temporary
replacement identity never appears in the published whole-fragment trace.

The remaining Setext risk is no longer the no-reference identity transaction.
It is composition with reference-prefix finalization and the broader 10 MiB
open-group convergence/latency gate shared with the other normalization
outcomes.

## Decisive witnesses

- `alpha\n===\n` and `alpha\n---\n`: stable BlockId, Heading facts, marker
  ownership, no retained Paragraph Enter.
- `> alpha\n> ===\n`: retained Quote marker, same nested terminal identity.
- multiline Paragraph promotion and every fuel split around promotion.
- CommonMark example 216 and definition-plus-visible-content Setext.
- closed and bare-EOF backtick/tilde fences, including a run longer than 255.
- quoted/list-nested closer ordering, shorter near-miss closer, trailing space,
  and CR/LF/CRLF.
- body indentation, partial tabs, NUL, non-BMP text, 1 MiB and 10 MiB literals.
- injected failure before and after group normalization, typed active-state
  replacement, raw-fold finalization, and close acknowledgement.
- clean versus checkpoint-resumed equality with a promotion or closer on the
  first line after restart.

## Reject conditions

Reject or redesign this direction if either construct requires:

- a document-wide mutable block directory;
- a generic parser-facing kind/range mutation API;
- rescanning Markdown inside the writer;
- copying or retaining aggregate Paragraph/code strings;
- rewriting a published root in place;
- source-position repair after the decision site;
- non-fuelled work proportional to terminal length or document size; or
- a second raw/reference parser with independent semantic authority.
