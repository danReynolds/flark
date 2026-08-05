# GFM table direct-extension gate

Status: **canonical writer transaction GO; two-pass control selected; direct
grammar integration HOLD**, 2026-07-18.

This gate asks whether the streamed Paragraph-to-Table transaction can enter
the real actor-owned `ExactBlockJob` by a small extension of the existing
direct Comrak donor, without restoring an aggregate Paragraph string, a
materialized `BlockTree`, a table-sized `Vec`, or a second grammar authority.

The answer is **no for a surgical extension of the current direct API**.  The
writer/storage transaction is coherent and executable.  The missing work is a
larger source-backed GFM control seam plus persistent provisional projection
provenance.  Enabling the existing tree-backed GFM branch would invalidate the
architecture rather than integrate it.

## Executable receipts

The real v3 writer now proves all of the following:

- one scanner-derived Paragraph-to-Table header is streamed cell by cell under
  fuel one through the same packed-green builder, source ledger, projection
  composer, and candidate journal;
- the replacement may change projection-run cardinality, with distinct storage
  and composer acknowledgements joined exactly once;
- an inter-cell marker advances the normalization cursor only after its green
  acknowledgement;
- a 65,537-column delimiter row crosses the legacy `u16` compatibility cap as
  a `u32` fact and enters the real writer transaction without truncation or a
  table-sized writer allocation;
- the scanner stops before another byte after cancellation, and rebuilding the
  transaction then cancelling after every writer poll and every typed scanner
  input boundary retires the candidate generation under unit abort fuel;
- the generic reducer accepts one closed prefix Paragraph followed by one open
  Table in the same replacement; the old Paragraph ID survives on the closed
  preface and the Table/Header/Cells receive fresh IDs; and
- the current checkpoint scope retires only an unsampled unresolved Paragraph
  group.  A sampled group fails closed rather than being relabelled as a Table.

Focused tests:

```text
candidate_writer::tests::streamed_paragraph_table_normalization_rebinds_composer_green_and_ledger
candidate_writer::tests::streamed_table_column_count_crosses_legacy_u16_cap_without_truncating_writer_handoff
candidate_writer::tests::table_normalization_and_scanner_cancel_at_every_cooperative_boundary
serialized_green::tests::canonical_fragment_replacement_streams_a_retained_preface_before_an_open_table
candidate_writer::tests::table_fragment_checkpoint_scope_retires_only_an_unsampled_paragraph_group
```

These receipts validate the canonical transaction, not the real donor route.
The scanner-to-writer test must not be described as
`scanner -> ExactBlockJob -> writer`.

The currently compiled `StreamingTableRowJob` and raw writer handoff are also
not the selected two-pass authority yet:

- the scanner emits cells before it knows that a late tail/cell-limit check
  accepts the complete row;
- its research path intentionally crosses the donor `u16` cell ceiling;
- polling binds only a physical end, so same-length source can be substituted
  between polls;
- the writer accepts caller-authored cell cuts, alignment/count facts, and a
  reduced logical enum that cannot preserve general tab, NUL, CRLF/lone-CR, or
  projection-Program provenance; and
- the one-line fixture has no authenticated multiline `paragraph_offset` or
  retained literal preface cut.

Those APIs remain mechanism scaffolding. The production-shaped prototype must
put validation and replay behind one actor/writer-owned capability; wrapping
the raw scalar calls and renaming the test would not close the gate.

## Why the current direct donor cannot be switched to GFM

`DirectValueBlockParser::new` accepts only `SyntaxProfile::CommonMark`.
Capture, validation, durable encoding, reconstruction, and resume independently
enforce the same profile and a fixed supported open-kind set.

That is not merely a missing enum arm.  The existing GFM implementation in
`comrak_value_block_core/src/table.rs`:

- reads the complete accumulated `Paragraph` `LeafContent.logical` to recover
  the final header line and its preface split;
- uses origin-bearing `LeafContent` slices to materialize header cells;
- allocates an alignment `Vec` and one mutable `BlockTree` node per
  Table/Row/Cell;
- mutates/detaches the old Paragraph in the donor tree; and
- retains the alignment vector and table counters on the open Table node.

The direct path deliberately has the opposite invariants:

- Paragraph scratch retains no aggregate logical bytes;
- `compact_direct_scratch` rejects any retained content and keeps exactly the
  open path;
- direct commands expose no `NodeId` or tree handle; and
- the pause schema currently rejects Table frames, `table_visited`, and table
  autocomplete state.

Removing the CommonMark check and adding `DirectBlockKind::Table` would
therefore either fail immediately or restore the output tree and aggregate
Paragraph payload that the direct path was created to eliminate.

## The two source windows are different authorities

A table opening compares two rows:

1. the current delimiter physical line after container-prefix handling; and
2. the final logical line of the already accepted provisional Paragraph.

The existing actor byte session is a sequential, current-physical-line view.
It correctly cannot rewind into the preceding Paragraph line.  The existing
streaming table scanner takes a borrowed contiguous slice and therefore is not
yet an actor-bound refillable source API.

Adapting that scanner to a sequential byte-source trait is small: its state is
already constant size, its cursor is monotonic, and line-ending bounds can come
from the actor descriptor.  That does not solve the second window.  The header
line may exclude quote/list prefixes and may include writer-owned transforms;
raw absolute source offsets alone are not its logical input.

The clean producer must therefore create, while each Paragraph line is
accepted, one cheap provisional last-line projection capability and record the
authenticated physical/projection cut before that line. A later delimiter
either consumes that capability into one paired header/delimiter work item or
replaces/discards it under fuel. It retains neither an aggregate Paragraph
string, speculative cell cuts, nor a candidate for every historical line.

Scanning header cells eagerly on every Paragraph line is unnecessary churn.
At delimiter priority, one actor-owned work capability can read the current
physical delimiter cursor and prior logical header cursor cooperatively.

The first design streamed cells into a hidden normalization transaction and
planned to discard it if a late count or syntax check failed. The current
writer/composer has no generic abortable fragment subtransaction, and adding
one would substantially enlarge the mutation protocol. More importantly, it
is unnecessary: the source and projection roots are immutable and already
authenticated.

The selected control candidate is **validate, then replay**:

1. Pass one scans the complete final logical header row and delimiter row with
   `O(1)` state and no candidate mutation. It classifies the result as
   `NotDelimiterCandidate`, `InvalidDelimiterCandidate`, or `TableReady`.
2. `TableReady` is a non-cloneable, source-bound capability. It binds the
   parser profile/options, paragraph owner, final-row cut, container context,
   source and projection roots/revisions, cell count, and writer epoch.
3. Consuming it internally mints fresh cursors over those same immutable roots.
   Pass two scans the validated row pair in lockstep and streams one header
   cell plus alignment at a time into the existing canonical writer. Callers
   cannot supply or reconstruct offsets, counts, or projection runs.

This shares the reference finalizer's request/work/terminal envelope without
forcing its per-output acknowledgement policy onto an all-or-nothing result.
References have independently committable monotonic occurrences; a Table row
can fail at its tail, so it must validate before its first writer mutation.

The terminal result must preserve the donor-observable retry distinction:

- `NotDelimiterCandidate` resumes Paragraph continuation without setting
  `table_visited`, so a later line can still open a Table;
- `InvalidDelimiterCandidate`, including a column-count mismatch, publishes no
  writer output and sets `table_visited`, preventing repeated delimiter
  attempts for that Paragraph; and
- `TableReady` authorizes the paired replay/stream transaction and installs
  `TableControl` only after its joined writer acknowledgement.

The actor cannot infer or collapse these dispositions from scanner failure;
they are parser-work results consumed at the exact GFM priority stage.

The donor's `u16` row-cell ceiling remains grammar-authoritative: 65,535 cells
are valid and the next cell rejects the whole row. The writer's wider `u32`
fact test proves transport/storage non-truncation only; it does not silently
extend the selected GFM profile.

## Smallest clean control contract

The donor grammar half needs only future-observable scalar state:

```text
ParagraphControl
  table_visited
  last_line_projection: writer capability | absent

TableControl
  columns: u32
  capped_autocompleted_cells: u32
```

Alignment data affects canonical output, not future block branching.  It can
remain in the writer-owned header sequence/Table facts instead of a donor
`Vec`.  Body-row continuation needs the column count and the capped missing-cell
count only; this agrees with the existing `BlockTransitionCheckpoint` grammar
projection.

The direct parser requires an external-work rendezvous, analogous to the
source-backed ATX admission but legal at an open Paragraph/Table boundary:

```text
donor pauses at the Table opener stage
  -> actor validates current stripped delimiter + previous logical header under fuel
  -> actor returns one source-bound TableReady capability
  -> actor replays validated rows and streams paired cell/alignment tokens
  -> joined writer ACK consumes the capability
  -> donor consumes one typed terminal certification
  -> donor emits one typed Table decision command
```

The decision command must have a replace-top stack effect for a whole Table and
the same final Table top for a split-preface outcome.  Row/Cell fan-out is
streamed by the writer from the scanner capabilities; it must not be recreated
as transient donor `BlockTree` children.  Current delimiter/body source claims
remain exact writer commands and are acknowledged before `FinishLine`.

Pause/restart must add the two scalar control variants above plus an opaque
reference to the writer-owned last-line candidate in the composite checkpoint.
The donor pause alone cannot persist or reconstruct that candidate.

## Authenticated cursor/join mechanism checkpoint

`v3_runtime_slice/src/table_projection_cursor_gate.rs` now exercises the
smallest private validate-to-writer join without changing the shared candidate
writer while its whole-normalization work is in flight.  Its in-memory provider
is a mechanism fixture, not the selected production storage shape.

The checkpoint binds one non-cloneable `TableReady` to all of:

- `LiveCandidateEpoch`, including source root/revision and arena build;
- syntax profile, grammar revision, and semantic epoch;
- provisional Paragraph owner and generation;
- an opaque Paragraph-consumer barrier joining packed-green leaf state,
  composer high-water, source cursor, and projection generation;
- exact header/delimiter cut identities plus delimiter terminator/container
  ownership;
- projection root and Program generation;
- certified cell count, writer epoch, and `table_visited` state.

The private join rejects crossed source, candidate epoch, Paragraph, barrier,
projection, delimiter ownership, or writer epoch before starting a writer
transaction.  On success it replays the whole retained header and delimiter,
including inter-cell pipes, trim, and terminators—not only materialized cell
content.  Each typed Identity/Hidden/Tab/NUL/CRLF/lone-CR/Program piece retains
its projection provenance and a non-overlapping physical-cut capability.
Identity spans may split at a cell partition; an Atomic or Program span must be
decoded to a finer piece or remain whole, so replay cannot duplicate physical
source ownership to satisfy a logical cut.

The focused mechanism gate is four of four green.  It covers whole-row typed
projection replay under fuel one, zero-read cancellation, rejection and every
crossed binding before visible mutation, same-length source replacement, and
the 65,535/65,536 boundary.  Receipt command:

```text
cd tool/parser_research/v3_runtime_slice
cargo test --offline --features 'exact-parser host-mirror-probe' \
  --lib table_projection_cursor_gate -- --nocapture
```

The full isolated scanner receipt is green: seven differential tests, five
downstream tests, and four two-pass Table tests, including body rows, fuel-one
cancellation, and the 65,535/65,536 boundary.  The Table/reference/Setext/list
priority matrix is also four of four green.

### Exact remaining production seam

A cloneable green snapshot or copied row is not required.  The unpublished
candidate build already remains actor-owned until commit/cancellation.  The
production seam should therefore be one non-cloneable `LiveDocumentStore`
session which:

1. forces a barrier joining the builder leaf, composer projection high-water,
   source root/revision, Paragraph consumer owner, and exact physical
   `source_before`;
2. holds the builder decoder and a Crop source cursor inside the actor while
   `TableReady` carries only a non-cloneable session seal;
3. sequentially decodes Coverage descriptors and Program child pages, advances
   Crop by each authenticated physical metric, and yields typed logical pieces
   plus physical-cut capabilities under fuel; and
4. binds the delimiter as a second actor-owned source subrange with staged
   terminator/container ownership.

The remaining API mismatch is the isolated `StreamingTableRowJob` facade:
pass one currently accepts `Arc<[u8]>` rows.  Its state machine is already
monotonic and constant-size, so production must adapt it to consume the actor
session's sequential byte stream directly.  Materializing a `String`, cloning
arena pages, or handing scalar cuts back to the grammar adapter would be a
regression, not an implementation shortcut.

## Missing prefix-retain operation

The generic reducer already accepts:

```text
closed Paragraph(old ID) + open Table(fresh ID)
```

The producer cannot yet derive that forest without replaying the old preface
projection.  A clean split transaction needs one typed operation that retains
the old provisional Paragraph's exact prefix up to an authenticated
source/projection cut, rewraps it with the same Paragraph ID, and replaces only
the last-line suffix with streamed Table events.  The persistent sequence may
reuse old pages/runs; callers must not reconstruct Green events or copy a
second projection tape.

This operation is also the right abstraction to challenge against split
Setext/convergence cases.  A caller-provided byte offset plus `Identity` run is
not sufficient authority.

## Why implementation stops here

The source-backed direct path currently recognizes only root-level ATX and
terminally rejects a non-match.  A refillable GFM delimiter line must run in
the donor's real priority order after open-container continuation and before
ordinary Paragraph continuation.  It must work inside quotes/lists and must
not skip Setext, thematic-break, list, or code-block precedence.

Consequently the honest extension includes all of:

1. a refillable prioritized line-control entry point beyond root ATX;
2. provisional last-line candidate persistence and composite checkpoint
   authority;
3. GFM Paragraph/Table control variants and pause codecs;
4. typed external-scan admission/result and replace-top command semantics;
5. two-pass streamed body-row writer commands, since late width rejection must
   not leave partial row output;
6. the authenticated old-prefix retain operation; and
7. differential GFM chronology, nested-container, restart, cancellation, and
   oversized-line receipts.

That is a parser-control milestone, not a local Table adapter.  Implementing
only the existing short-line/tree-backed branch would create a second runtime
model and make the large-document claim false.

## Architecture consequence

The canonical architecture is strengthened: typed grammar decisions can reduce
one provisional normalization group atomically into canonical green, source,
projection, identity, and open-ledger state without a prediction parser.  The
remaining uncertainty has moved upward to the donor-control boundary.

The next comparison should implement the same small control contract in two
ways:

- a deeper direct-Comrak refactor that bypasses `table.rs` output mutation in
  direct mode; and
- a Flark-owned GFM Table control job derived from the specification and kept
  differential against Comrak.

Whichever version can consume actor-bound source and writer capabilities
without aggregate strings, `BlockTree` output ownership, or duplicated grammar
wins.  The existing streamed writer transaction is shared by both and should
not be rewritten.

The current best path is a Flark-owned donor-correspondent controller: retain
the donor's priority, row-limit, escaping, trimming, and `table_visited`
semantics as differential obligations, while making replay authority and
writer ownership native to Flark's runtime. A deeper direct-Comrak refactor
remains a falsification peer, not the default merely because Comrak supplied
the original algorithm.

## Two-pass acceptance additions

The control prototype is not green until it proves:

- pass-one and pass-two cell cuts are differential-equal for escaped pipes,
  backslashes, trimming, tabs, NUL, CRLF, and nested quote/list projections;
- a multiline Paragraph replays only its authenticated final header row while
  preserving the preceding literal preface;
- a successful reference-looking preface stays literal and publishes no
  reference occurrence, while a rejected count mismatch leaves the Paragraph
  intact so ordinary reference finalization can later install it;
- 65,535 cells succeed and 65,536 reject without a partial writer root;
- body rows validate completely before emitting, then pad/trim only according
  to the certified column count while retaining physical coverage of ignored
  tails;
- cancellation/staleness before replay publishes nothing, and cancellation
  after mutation retires the entire unpublished candidate;
- pass one resumes under fuel without rescanning from its start on every poll;
  and
- `TableReady` cannot be cloned, replayed, crossed between sources, or
  reconstructed from Dart/raw offsets.
