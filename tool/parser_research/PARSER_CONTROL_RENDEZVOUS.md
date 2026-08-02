# Parser control rendezvous

Status: **selected production contract; Setext and authenticated Table cursor
mechanisms green; reference restart and final parser/writer publication remain
HOLD**, 2026-07-18.

This contract defines the boundary between Markdown control, source access,
and canonical output. It exists to prevent the final parser implementation
choice from reintroducing any of the rejected architectures: a Dart predictor,
an enclosing-block reparse heuristic, a mutable donor AST, aggregate Paragraph
strings, or construct-specific fallback parsers.

## Working decision

Flark owns one incremental parser-control protocol. The control implementation
may port function-correspondent algorithms and generated scanners from pinned
Comrak, cmark-gfm, or Pulldown, but no donor owns the runtime source, document
tree, output identity, checkpoint, or reference map.

The runtime boundary is:

```text
parser control (grammar priority and scalar continuation)
  -> typed external-work request
actor (source/candidate authority) mints one bound work capability
  -> resumable recognizer/finalizer over a physical or logical cursor
parser control consumes one typed result
  -> canonical normalization plan
candidate writer atomically joins green/source/projection/reference/
checkpoint/host roots
  -> acknowledgement
parser control advances
```

The parser cannot advance past a request or output command until it consumes
the matching acknowledgement. The actor cannot choose which Markdown rule to
run, reinterpret a rejection, or manufacture a grammar result.

## Two input cursors, not one fake source range

The prototype initially described every external job as a sequential physical
byte source. That is sufficient for ATX, fence, HTML-terminator, Setext
underline, and current Table delimiter scans. It is not sufficient for
reference prefixes or the previous Paragraph line used as a Table header.

The production protocol has two source-authorized cursor kinds:

```text
PhysicalCursor
  exact source-root/revision/line identity
  physical byte and UTF-16 extent
  refillable monotone windows

LogicalProjectionCursor
  exact candidate-build/Paragraph consumer identity
  joined green barrier + composer high-water + source root/revision
  logical byte/UTF-16 extent
  each logical cut resolvable to authenticated physical/projection cuts
```

A logical Paragraph may omit quote/list prefixes, normalize line endings,
expand a partial tab, or cross non-contiguous physical runs. Therefore:

- `base_source_offset + logical_offset` is never valid provenance;
- a single physical `Range` cannot describe a general definition/header;
- external work returns logical cuts and projection-backed range capabilities,
  not caller-certified absolute ranges; and
- the writer, which already owns the projection root, is the only component
  that can resolve those cuts into canonical source/projection changes.

This keeps recognition and provenance in one authority chain. It also lets a
malformed definition/title lookahead replay from the committed logical cut
without retaining or rewinding an unbounded string.

### Selected candidate-fragment cursor

The executable storage model narrows this further: there is no separate,
cloneable Paragraph snapshot or projection tree. `SourceBoundProjectionComposer`
seals each run and the candidate writer immediately stores it as packed-green
Coverage. Program payload pages become strong children of those journalled
leaves. The unpublished candidate actor already owns the only build lease and
the current Crop source lease.

External-work admission therefore:

1. drains the composer and force-seals one exact green leaf barrier;
2. joins that cut to the composer high-water, source root/revision, provisional
   Paragraph Enter/origin, parser rendezvous, and writer epoch;
3. issues an actor-held Crop cursor at the authenticated fragment source cut;
   and
4. lends a non-cloneable sequential decoder while the candidate remains live.

One fuel unit decodes at most one Coverage/Program/source chunk. The decoder
yields typed Identity, Hidden/affinity, Atomic, Program, and Virtual pieces
together with opaque logical-to-physical cut capabilities. It never yields a
Paragraph `String` or asks a scanner to certify raw offsets. End-of-input is
selected by Paragraph logical-consumer ownership plus an explicit real/staged
terminator disposition; a lookahead or Setext line after the Paragraph cannot
leak into the cursor merely because it precedes the current event cut.

Reference destination/title slices and Table replay retain the exact source,
projection, and Program capabilities they cite until their writer transaction
commits or the whole unpublished candidate is cancelled. The session itself
does not clone an arena root: cancellation invalidates its build/session
identity and actor-held cursors together. Existing completed-manifest query
views are useful decoding evidence, but are not production authority for this
selected path.

## One scheduler envelope, typed work variants

The protocol is generic at the ownership, scheduling, and checkpoint layer;
the grammar results remain typed.

```text
ParserWorkRequest
  lineage: parser instance + admission + source/candidate build
  boundary: exact control stage and open-path binding
  input:
    PhysicalLine(descriptor, start cut, end cut)
    LogicalProjection(paragraph capability, start cut, end cut)
  operation:
    RecognizeLine(rule family and parser-owned priority state)
    FinalizeReferencePrefix
    JoinTableHeaderAndDelimiter(last-line capability)

ParserWorkProgress
  NeedInput(window grant)
  OutputReady(typed bounded item)
  Complete(typed terminal result)
  Cancelled
```

Every work object is non-cloneable, actor-bound, resumable, and cancellable.
Every poll has independent source-read, transition, output-item, allocation,
and wire grants. Large outputs drain one item at a time; `Complete` never owns
a Table-sized vector or a vector of all reference definitions.

The variants share this envelope rather than sharing one universal scanner.
Generated recognizers and handwritten state machines remain independently
testable, while priority and mutation stay parser-owned.

### One request, one retained work capability, many acknowledged items

The lifecycle is deliberately asymmetric. Parser control owns the grammar
request, but the actor retains the non-cloneable work capability until that
request reaches a terminal result:

```text
parser: Quiescent -> RequestPending ------------------------------------+
                                                                    |   |
actor:                 WorkPolling -> OutputToken -> AwaitingAck ----+---+
                            ^                          |
                            +--------------------------+
                            |
                            +-> TerminalToken -> parser consumes terminal
                                                   -> GrammarResumes
```

`OutputToken` contains one typed bounded item and its request/work/sequence
binding. Minting it leaves the work incapable of polling. The candidate writer
consumes the token and returns an acknowledgement bound to the candidate root
and resulting aggregate root. Only that acknowledgement rearms the same work
capability for its next poll. Rejection, cancellation, stale lineage,
cross-work acknowledgement, double acknowledgement, or an aggregate mismatch
retires the hidden candidate and the work together.

This is not a chain of new parser requests. In particular, publishing one
reference occurrence must not replace the parser's pending request or drop the
reference DFA. Likewise, a Table row or cell does not become parser-owned
continuation state. The parser remains stopped at exactly one grammar stage
until the actor returns the terminal work token. This makes arbitrary output
cardinality compatible with bounded scratch while keeping the scheduler
envelope construct-neutral.

## Construct mapping

### Setext

The controller recognizes the physical underline in normal CommonMark
priority. Paragraph finalization supplies a projection-backed visible prefix.
The result feeds one canonical-fragment normalization:

- whole promotion preserves the old Paragraph identity as the Heading;
- split promotion mints a fresh Heading and moves the old Paragraph identity
  to the surviving suffix; and
- restart-crossing promotion requires a parent-authenticated old-manifest,
  source-cut, projection-cut, and provisional-Enter capability.

No caller supplies an ID or reconstructs old events.

The selected Setext path is now executable rather than merely specified. Its
44/44 focused suite includes fresh and retained restart, 10 MiB parent-bound
suffix splices, nested ownership, exact clean equality and stable identity,
stale/crossed authority, cancellation, EOF, and a following non-Paragraph
Open. The identity rule is not a Setext-only EOF repair: one private deferred
normalization may survive only until the next authenticated structural action,
and the writer resolves and acknowledges its whole outcome before a
non-Paragraph Open, parent/ancestor Close, or Finish crosses the owning parent.
Paragraph Open alone consumes the certified residual split outcome.

### References

`FinalizeReferencePrefix` consumes a `LogicalProjectionCursor` and emits one
ordered occurrence at a time:

```text
DefinitionOccurrence
  bounded normalized label
  logical/source-projection cuts for definition, label, destination, title
  deferred bounded clean-transform recipes
```

The exact chronology, normative divergences, publication roots, and stop
conditions are tracked in
[`REFERENCE_PREFIX_FINALIZER_GATE.md`](REFERENCE_PREFIX_FINALIZER_GATE.md).

Destination and title payloads remain source-backed even when enormous. A
terminal result distinguishes no definitions, reference-only, and visible
remainder. `ReferenceOnly` carries a parser-selected terminal mutation: the
ordinary close path removes the Paragraph wrapper, while an already recognized
Setext underline retains an empty Paragraph shell long enough for the Setext
transaction to preserve donor chronology. The work actor cannot choose between
those mutations from scanned bytes. Failed title lookahead commits only
through the last accepted definition and gives the ordinary Paragraph path a
fresh cursor at that cut;
it does not skip, copy, or retain the looked-ahead remainder.

Definition recognition and inline lookup both use the single
[`ReferenceLabelService` contract](REFERENCE_LABEL_NORMATIVE_GATE.md). Donor
label limits or normalization helpers are differential evidence, not a second
semantic authority.

The candidate writer applies only the typed reference-only mutation selected
by parser control. Source coverage and ordered occurrences remain in the candidate. A
first-definition winner/dependency aggregate is derived from candidate-owned
persistent occurrence state; it is not block-control continuation. The first
initial-build proof stored a separate winner root. The selected restart shape
below instead makes the first element of each per-label sequence authoritative,
avoiding a second mutable winner index.

The restart/re-winner shape is selected but **PROVISIONAL** pending its focused
receipt. Initial builds keep one global source-ordered occurrence sequence and
an exact-label directory whose leaves own per-label persistent occurrence
sequences; element zero is the winner, so a separate mutable winner map is not
required. A committed checkpoint retains the same directory shape truncated
to occurrences fully writer-acknowledged before the active Paragraph. Each
child sequence length is therefore an authenticated prefix rank, not a
caller-provided coordinate.

For one authenticated contiguous restart-to-convergence replacement, the
writer reads old changed occurrences forward and deletes each label at its
fixed checkpoint rank, then consumes new occurrences in reverse and inserts
each at that same rank. Repeated labels preserve source order; removing rank
zero promotes the untouched suffix occurrence; suffix shifts remain implicit
in the persistent sequence. Arbitrary move/reorder is explicitly outside this
operation. Published occurrence descriptors retain the exact interned label
and persistent cooked destination/title byte-blob roots. Exact
source-revision ranges and group-local projection cuts are transaction
witnesses only. Before terminal Paragraph mutation, an authenticated
random-access projection cursor replays each accepted value range through the
pinned streaming cleaner into the blob writer; after the occurrence/index
roots join the candidate manifest, the old projection can retire. Unchanged
suffix occurrences reuse their cooked blob roots by identity. Crop does not
expose stable leaf identities, and finite scalar lineage is not a durable
replacement. Source navigation is therefore a separate stable-anchor or
lazy-coordinate-index gate. Restart cannot retain its current fixture-only
numeric labels or heap `Vec` of changed occurrences: the production writer
adopts the committed exact interner, streams changed occurrences into a
persistent replacement spool, and reverse-traverses that spool for the
fixed-rank per-label insertions. Inline consumers reach winners through a
bounded committed exact-label query, never a caller-authored label ID.

This is not green until tests prove insertion before the winner, winner
deletion, relabel/destination edits, duplicate order, large-suffix identity and
no enumeration, bounded path copying, fuel-one, cancellation/fault, crossed
checkpoint rejection, and initial/checkpoint memory constants. Passing that
gate still leaves a separate integration HOLD: the parser-owned prefix
finalizer must feed the real `CandidateWriter`, and one unpublished manifest
must join the global occurrence sequence, label directory, checkpoint-prefix
root, exact interner, cooked-value blob roots, source lineage,
green/checkpoint, and host roots.

### GFM Table

While accepting a Paragraph line, the writer retains exactly one cheap
last-line projection capability plus the authenticated prefix cut. It replaces
the prior capability on the next line; historical lines and speculative cell
vectors do not accumulate.

When the parser reaches the GFM delimiter priority stage it requests a
single work item over the current physical delimiter cursor and the retained
logical header cursor. Unlike reference occurrences, a Table header is one
all-or-nothing normalization: a late cell-count or syntax failure invalidates
every earlier cell. The work therefore uses two passes over immutable,
authenticated input instead of mutating a hidden writer transaction that
would need rollback:

1. **Validate.** Scan the complete final logical header row and physical
   delimiter row cooperatively with constant retained state and no output
   mutation. The parser receives only `NotCandidate`, `CandidateRejected`, or
   a non-forgeable `TableReady` capability.
2. **Replay and stream.** `TableReady` internally mints fresh cursors bound to
   the same paragraph/projection/source roots and replays the already-validated
   rows. It emits one paired header-cell/alignment token at a time into the
   canonical writer. The caller cannot reconstruct the authenticated final-row
   cut, cell count, or projection provenance.

The private authenticated-cursor mechanism is 4/4 green. It covers typed
projection replay under fuel one, zero-read cancellation, crossed binding and
same-length source replacement rejection, and the 65,535/65,536 column
boundary. The isolated scanner adds seven differential, five downstream, and
four two-pass tests; the Table/reference/Setext/list priority matrix adds four.
No cloneable green snapshot is needed: the actor retains the only unpublished
packed-green/Program/Crop cursors and `TableReady` carries a non-cloneable
session seal. This closes the cursor design, not the real parser-priority,
prefix-retain, body-row, writer, or manifest integration.

The successful result is whole Table or retained Paragraph preface plus Table.
Future control retains only column count and capped autocomplete state. The
terminal distinguishes a non-candidate, which remains retryable, from a
malformed delimiter candidate, which sets the parser's `table_visited`
disqualification; the actor does not infer that state. Body rows use the same
validate-then-replay rule because a very wide or malformed tail can reject a
row after many otherwise valid cells.

This remains linear in the affected row, not the document, and keeps scratch
`O(1)`. The second scan is preferable to a table-sized vector or a generic
abortable fragment transaction. Cancellation before replay is free; after
canonical mutation begins it cancels the entire unpublished candidate or
yields until that normalization reaches a quiescent boundary.

Table delimiter priority preempts ordinary Paragraph/reference finalization.
If a retained header has a visible preface such as a reference-definition-like
line, a successful Table split preserves that preface as literal Paragraph
projection. It emits no reference occurrence and changes no reference winner
sequence or exact-label directory. This priority and mutation are
parser-selected; the Table actor never
invokes reference-label semantics or infers a definition from the preface.

The general rendezvous policy is therefore:

- independently committable, monotonic outputs such as ordered reference
  occurrences use output-token/ack/rearm; and
- all-or-nothing transforms over replayable immutable input such as Table rows
  use validate-then-replay before the first writer mutation.

Both policies share request lineage, source-bound cursors, terminal parser
resume, cancellation, and writer-authenticated publication. They are not two
grammar authorities.

## Canonical normalization is a transaction, not parser mutation

Setext, Table, and reference-prefix removal all reduce a provisional Paragraph
range. They use one transaction family:

```text
NormalizationOrigin
  base manifest and candidate build
  provisional Paragraph Enter
  authenticated source and projection cuts
  old coverage/run/event summaries

NormalizationPlan
  retained old prefix, if any
  streamed replacement events and projection runs
  identity recipe
  occurrence/fact deltas
  expected physical/logical aggregates

NormalizationCommit
  green acknowledgement
  source-ledger acknowledgement
  projection acknowledgement
  reference occurrence/directory/checkpoint-prefix acknowledgements,
  when applicable
```

Publication is legal only after the actor joins every required acknowledgement
under the same lineage. Cancellation or any mismatch retires the whole hidden
candidate; the current source and structural roots do not change.

Some restart-spanning normalizations cannot choose their final identity at the
first typed reduction. In that case the writer may retain one private deferred
pair joining the source-ledger recipe to the exact packed-green occurrence, but
it may not let that pair drift past its authenticated parent. The next
structural action is itself the rendezvous:

```text
Paragraph Open        -> consume the certified residual outcome
non-Paragraph Open    -> choose whole outcome, then open
parent/ancestor Close -> choose whole outcome, then close
Finish                -> choose whole outcome before sealing
```

For every whole outcome, the writer first acknowledges the canonical identity
rewrite and only then performs the Open, Close, or Finish. This is a general
before-parent-crossing invariant, not a Setext-specific finish hook. It keeps
the ledger parent stamp live while storage consumes its exact locator, and it
makes stale/crossed tokens or cancellation fail inside the unpublished journal
instead of after authority has been lost.

## Checkpoint boundary

Persisted parser control contains only future-observable scalar state. Before
work is minted, this can include the compact candidate hint and projection
anchor needed to deterministically raise the same typed request after resume.
A composite checkpoint separately retains:

- parser control continuation;
- stable open writer bindings;
- semantic-prefix roots, including pending projection/candidate state;
- source cursor and source-root lineage;
- reference global-occurrence, exact-label directory/per-label sequence, and
  authenticated checkpoint-prefix roots;
- normalization origin, if a sampled boundary legally owns one; and
- packed green/source/projection candidate roots.

The composer half of this boundary is now 22/22 green. Its opaque continuation
retains no source or heap payload, fits a 224-byte scalar envelope, and derives
the next composer generation as the sealed-run count plus one rather than
serializing a redundant word. That closes composer-state size and replay, but
not the composite checkpoint: parser pause, writer bindings, source lineage,
normalization state, semantic roots, exact green cut/tail, and host publication
must still be consumed together.

Live scheduler cursors and refill buffers are worker-local and are not
convergence equality. The first production slice may decline to checkpoint an
active request and restart it from the last quiescent composite checkpoint,
because no active-work output is public. It may not, however, forbid
checkpoints for the arbitrarily long Paragraph merely because that Paragraph
could later raise a reference or Table request: the compact parser hint and
writer projection anchor must cross ordinary line-boundary checkpoints.

If later recovery requirements demand checkpoints between streamed output
items, the checkpoint must be taken only after an item acknowledgement and
must include the work's scalar continuation plus the exact candidate aggregate
roots named by that acknowledgement. A checkpoint may never capture an
unacknowledged output token. Opaque capability IDs are useful only when the
composite manifest proves the corresponding persistent root and cut; an ID
alone cannot authorize resume.

## Backend consequence

The long-lived production module should be described as a **Flark-owned,
donor-correspondent block controller**. Removing `ValueBlockParser`,
`SourceDocument`, `BlockTree`, aggregate `LeafContent.logical`, and the donor
reference map from the current direct path already requires owning the open
stack, transitions, output protocol, and checkpoints. Keeping that code inside
a broad Comrak fork would not reduce the architecture; it would only make the
ownership rewrite collide with upstream files.

The maintainable donor boundary is therefore expected to contain only:

- generated scanners or small lexical helpers that remain independently
  bounded;
- bounded inline parsing/materialization where it wins;
- provenance-pinned, differentially tested algorithm ports; and
- stock Comrak/cmark-gfm clean parsing as an oracle in CI.

This backend ownership is selected for RFC 023. The executable gates below may
still reopen it if the remaining reference/Table/inline joins require a second
grammar, caller-authored provenance, or a donor lifetime tree. A future small
direct-Comrak transition may still contribute a localized algorithm if it
satisfies this protocol without donor lifetime structures; it is not an
alternative runtime state model. The current evidence does not show a surgical
Comrak path that replaces the selected controller.

## Commitment gates

Production implementation remains on hold until the still-open parts of these
gates are executable. The status prefix distinguishes settled mechanisms from
the final integration:

1. **SETEXT MECHANISM GO; FINAL MANIFEST HOLD.** The full parent-selected
   Setext route consumes an authenticated
   restart fragment and passes identity, cancellation, stale-capability, and
   exact clean-parse comparisons in the 44/44 focused lane, including the
   generalized normalization-before-Open/Close/Finish boundary. It must still
   publish through the final multi-root manifest.
2. **REFERENCE HOLD.** Reference finalization must pass donor differentials for
   indentation,
   multiline forms, duplicate definitions, false candidates, EOF edges,
   Unicode label limits, giant destination/title values, one-byte refills,
   cancellation, and nested logical projections. Its restart/re-winner index
   must pass the provisional per-label-sequence/prefix-rank gate, and the real
   finalizer/writer join must publish candidate roots before terminal resume.
3. **TABLE CURSOR GO; DIRECT CONTROL/WRITER HOLD.** Table control uses the same
   scheduler envelope and authenticated logical-projection capability. The
   4/4 private cursor plus isolated scanner/priority suites pass without a
   cloneable snapshot or aggregate vector; whole/split/nested/restart control,
   prefix retain, body continuation, and real writer publication remain.
4. **CONTROLLER EXTRACTION HOLD.** The extracted controller must no longer
   construct or retain donor lifetime
   types, exposes one traceable command port, and keeps the maintained donor
   seam concentrated below the agreed maintenance ceiling.
5. **ATOMIC PUBLICATION HOLD.** One composite candidate must publish green,
   source, projection, reference,
   checkpoint, and host-delta roots atomically; no gate is inferred from a
   similarly shaped standalone proof.

If the remaining parts pass, production integration can proceed without a dual
parser. If they require per-construct fallback drivers, caller-supplied
provenance, or a broad mutable-AST compatibility layer, stop and reopen the
selected controller design.
