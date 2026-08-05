# Candidate writer vertical-slice plan

Status: **real-parser ordinary-container composition executable;
retroactive/raw grammar, checkpoint/adoption, and architecture selection
HOLD**, 2026-07-16.

This began as the smallest executable path that could prove the selected source,
projection, packed-green, cancellation, and publication mechanisms can be
contained by one authority boundary. It is not another adapter around the
research event stream, and it is not by itself the architecture-selection
gate: its first fixtures used grammar-free, already-final typed actions. A
second receipt below now connects the exact correspondent parser for the first
deliberately narrow grammar slice.

## One authority boundary

Production parser algorithms receive one candidate-owned typed action port:

```text
CandidateWriter
  LiveCandidateEpoch
  BoundSourceCursor + CandidateSourceLedger
  SourceBoundProjectionComposer
  parser/checkpoint-index builder
  ResumableSerializedGreenBuild + ArenaBuildTicket
  reference/fact roots
  one poison/commit state
```

The writer owns all generation, source, coverage, and arena identities. Parser
code cannot receive or replay a `ConsumedSourcePiece`, `CoverageId`, raw
`SourceProjectionRun`, source metric, arena owner, or manifest child root.
Those remain internal proof values.

The current actor-level composer-admission boolean is not this ownership
boundary. The composer currently escapes, its source seal is consumed into a
copyable receipt, and returned sealed runs can be dropped. The direct writer
must contain the composer and builder, then consume their non-cloneable
completion evidence at the sole manifest commit.

## Ordered resumable transitions

Each public parser action becomes a writer job. A job either completes in one
measured/preflighted kernel or returns `Pending`; no caller retries a partially
applied scalar command. At most one parser action, one unaccepted exact source
piece, one sealed projection run, and one encoded green event are pending.

### Open

1. Drain and sink any preceding projection envelope.
2. Consume a fresh internal block permit.
3. Offer the packed `Enter` and drive its bounded builder work.
4. Only after the `Enter` is ordered, open the matching ledger binding and
   return an opaque parser binding.

Document open is the same path with an empty projection prefix. An allocation
or codec failure at any step poisons the whole candidate.

### Consume exact source

The parser supplies only source capabilities already obtained from the
writer's active source/grammar transition plus typed owner/part/logical action:

1. the ledger validates and consumes the exact interval once;
2. the resulting private piece enters the sole composer immediately;
3. every ready run consumes one fresh internal coverage permit;
4. reset authority, when present, is joined inside the same composer state;
5. the sealed run is offered directly to the green builder; and
6. no raw run or piece returns to parser code.

If any downstream step fails after source consumption, the candidate cannot
continue or publish. It enters constant-time abort followed by fuelled arena
reclamation.

### Close and structural boundaries

Coverage already consumed under an open block must be serialized before its
`Exit`. Therefore close is not a bare ledger mutation:

1. request a structural flush at the current exact source boundary;
2. drain every bounded composer output and green-builder allocation;
3. offer the `Exit` with its final child aggregate/facts; and
4. consume the ledger/open binding only inside this ordered writer job.

The same flush happens before an intervening `Enter` or local structural
rewrite. Waiting until a later source piece exposes a changed owner key is too
late: metrics remain correct while the zero-width event order can be wrong.

### Four non-interchangeable flushes

The canonical composer needs four distinct internal transitions:

- `flush_before_structure`: preserve logical-envelope state, then order an
  `Enter`/`Exit` or rewrite;
- `flush_at_projection_reset`: consume a source/parser-safe reset request,
  keep an interior pending Virtual right-biased, and mark the final emitted
  run under a bounded finality signal;
- `finish_semantic_envelope`: apply only parser-authorized close-time and
  trailing-Virtual rules; and
- `finish_document`: consume exact EOF/source authority and permit no further
  input.

A post-hoc reset join with a mirrored pending-Virtual flag is mechanism-only.
Production reset policy, page count, pending packet, finality, and poison state
live in this one composer.

### Finish and publish

Document finish must consume, in one candidate-owned path:

- the non-cloneable exact `CandidateSourceSeal`;
- proof that every composer run was accepted by the sole green sink;
- the final Document `Exit` and structurally balanced green root;
- checkpoint, reference, fact, and Unknown-range roots for the same epoch; and
- the sole arena build owner.

Only then may the actor publish one composite manifest. A diagnostic receipt,
matching scalar source totals, or a green root alone cannot authorize commit.
Any failure leaves the previous manifest queryable and the candidate
reclaimable.

## Current mechanism-composition slice

The first implementation deliberately proves mechanism composition before
real grammar:

1. empty document and one ordinary paragraph;
2. two paragraphs, proving coverage/Exit/Enter ordering;
3. nested quote/list ownership using already-final typed actions;
4. Identity, Hidden, Tab, NUL, LF, CRLF, lone-CR, and non-BMP source;
5. a dense transform stream that packs thousands of source pieces into
   bounded Program runs and direct arena pages;
6. suspension after every writer/builder phase;
7. cancellation plus representative injected failures at authority-changing
   source/green boundaries; exhaustive Program, page, branch, manifest, and
   allocator-boundary fault injection remains production hardening; and
8. decoded packed output with exact total byte/UTF-16 coverage and the expected
   structural event order.

The slice fails if its dependency path retains `StructuralEvent`,
`TreeMaterializer`, a source string/leaf copy, a document-wide block directory,
or a second source/projection cursor.

Passing this slice means the mechanisms can collapse behind one writer. It
does **not** establish a usable parser checkpoint, an incremental edit, a
retroactive Markdown transition, or the whole-editor data path.

### Executable receipt, 2026-07-16

`v3_runtime_slice::CandidateWriter` now forms the first grammar-free vertical
path. The live-document actor transfers its source ledger, projection
composer, resumable green builder, suspended arena ticket, and document
identity allocator into that one writer. Parser-shaped test code can obtain
only a writer-certified, non-cloneable source atom, an opaque binding, and a
typed logical action; it cannot receive a source piece, coverage/block ID,
projection run, source metric, green manifest, or arena ticket.

The exercised transition order is now explicit rather than inferred:

- a structural action flushes preceding projection output before `Enter` or
  `Exit`;
- a green offer must reach the distinct `ReadyForEvent` acknowledgement before
  the corresponding ledger open/close is applied;
- every test driver poll leaves the arena build suspended, including the
  intermediate builder phases;
- successful finish privately joins the retained non-cloneable source/composer
  seal, acknowledged green manifest, and sole suspended ticket before the
  grammar-free local commit; and
- cancellation begins abort in place and restores the same ticket if abort
  admission fails, rather than returning a second broad failure envelope.

Fifteen focused tests pass. They cover empty, one- and
two-paragraph, nested quote/list, exact Unicode, typed Hidden/Tab/NUL/all line
ending transformations, single-use and cross-candidate source atoms, premature
commit authority, and a failure injected after green `Exit` acknowledgement
but before ledger closure. That last transition poisons the candidate, admits
no further writer action, and remains fuelled-abort reclaimable. The dense
fixture consumes 5,000 certified source atoms, seals fewer than 100 projection
runs, allocates multiple Program pages, and never buffers more than one
projection-program page.

Verification from this directory:

- `cargo fmt -- --check`: passed;
- focused candidate-writer tests: 11/11 debug and 11/11 release;
- `cargo test --all-targets`: passed, including 85 library tests and every
  integration lane;
- `cargo test --release --all-targets`: passed;
- strict all-feature, all-target Clippy with warnings denied: passed; and
- pinned Rust 1.95 `wasm32-unknown-unknown` all-target checks: passed in debug
  and release.

This closes the current ownership and event-ordering **mechanism** falsifier.
It does not close the architecture-selection slice. Exhaustive allocation-fault
injection, external compile-fail/API-surface isolation, certified reset and
Virtual-finality composition, real checkpoint/adoption authority, retroactive
Setext/table/reference transitions, inline/reference/fact/Unknown roots, and a
production composite publication remain HOLD.

### First real-parser composition receipt, 2026-07-16

The `exact-parser` feature now adds one private `ExactBlockJob` that owns the
only protocol between `DirectValueBlockParser` and `CandidateWriter`. The
parser emits stack-shaped commands at the actual `add_child`, `add_line`, and
`finalize_borrowed` decision sites. It exposes no parser node ID or output
handle. The job acknowledges each command only after the corresponding writer
action has completed, and each outer poll performs at most one parser
transition, source poll, or writer poll.

The first supported grammar slice is CommonMark Document, Paragraph, ordinary
indent, and blank lines. It proves:

- read-only recognition and authoritative replay traverse the same immutable
  source range and must produce equal line receipts;
- LF, lone CR, and CRLF remain typed, with continuation newlines canonicalized
  and a closing paragraph newline retained as physical-only Terminal source;
- leading indent and blank gaps have exact surviving structural owners;
- source, projection, packed green, and the local build ticket join through
  the same writer completion path;
- parser scratch compacts after `FinishLine` acknowledgement to the open path,
  retains no legacy `BlockEvent` or aggregate logical payload, and cannot
  advance with an unacknowledged command; and
- empty input and input ending in a newline do not invent a phantom physical
  line.

The integrated gate caught a real seam error that the block-shape oracle could
not see: the first direct command draft classified a paragraph's final newline
as logical inline content. The source-ledger contract requires `CloseNone`.
The command vocabulary and direct tests now encode that physical-only rule,
and the packed output verifies it.

Evidence is intentionally split:

- parser core: 6 direct tests, including supported-shape comparison with the
  exact legacy parser, 128 paragraph/blank cycles, explicit fail-closed syntax,
  and zero retained legacy events; focused debug/release and the complete
  parser-core debug/release matrices pass;
- integrated v3 job: 6 tests for empty, trailing newline/indent ownership,
  CRLF plus non-BMP continuation, blank close/gap/reopen, and the shared line
  ceiling, plus fail-closed unsupported grammar entering the existing fuelled
  candidate-abort lifecycle; and
- v3: all-feature/all-target debug and release pass with 95 library tests plus
  every integration lane; explicit `wasm32-unknown-unknown` debug and release
  all-target/all-feature checks pass through the rustup-managed toolchain; and
  strict all-feature/all-target Clippy passes with warnings denied.

This is decisive evidence for the direct parser/writer seam, not architecture
selection. The parser still materializes one temporary physical-line `String`
with an explicit 8 KiB ceiling; GFM, tabs/NUL, containers, raw blocks, Setext,
references, and tables fail closed. Giant-line refillability, fuelled deep-path
compaction, exhaustive failure-to-abort injection, real checkpoint capture, nonzero
restart, and suffix adoption remain HOLD.

### Hard-grammar ordering falsifier, 2026-07-16

The first quote/list audit rejects a tempting extension of the paragraph
slice: emitting every parser mutation immediately is not a valid stack port.
Two independent control facts make it fail:

- an existing quote/item prefix is recognized before the previous paragraph's
  pending newline, or a preceding blank gap, has its final disposition, while
  the source ledger correctly refuses a current-line claim until that
  predecessor is resolved; and
- the donor-derived `OpenNew` phase may construct a new block under the last
  matched ancestor before `CloseUnmatched` closes old deeper descendants. A
  handle-based event sink can name that ancestor, but an immediate stack sink
  would open the new block under the wrong writer top.

Nested marked blanks make early gap resolution invalid as well: the first
surviving outer marker does not prove that an inner quote or item survives.
The parser must finish the old-path containment decision before assigning the
gap. List close exposes a separate omission: `ClosedChildAggregate` is the
three-bit contribution to the parent, while the List's own derived `tight`
value is final output truth and needs a typed close-time fact. It must not be
published as provisional Enter data or recovered by rescanning every item.

The next executable slice therefore uses a **parser-owned, line-local
normalized intent transaction**. Exact decision sites record typed source cuts,
open facts, and finalization facts without replaying source text or exposing a
`NodeId`. After the line's containment decision is committed, a fuelled emit
cursor produces at most one writer command at a time in final stack/source
order. The canonical rule is owner-relative rather than one fixed phase list:
each retired frame closes at the earliest exact cut after its final physical or
logical source use. This places `a\n\nb`'s root gap after Paragraph `Exit`, while
placing a marked `> \n` gap before the retiring Quote `Exit`. New replacement
frames enter only after the incompatible old suffix has retired. The retained
intent state is bounded by current-line/open-path work and is discarded after
`FinishLine`; it is not a document event tape.

### Executable hard-grammar receipt, 2026-07-16

The first transaction slice is now executable for Document, Paragraph,
BlockQuote, List, and Item. Parser-private `NodeId`s name frames only while a
line recipe is collected; emission lowers them to ephemeral
`generations_from_top` selectors, and no parser or output identity crosses the
writer seam. List and Item opens carry typed normalized facts. List tightness
is required only at `Exit` as `GreenCloseFacts::List`, separate from the
three-bit closed-child contribution. Packed schema v7 encodes none/list-loose/
list-tight close facts with canonical one-byte tags and validates the closing
binding kind.

The executable transaction now enforces:

- one visible command and one acknowledgement-driven stack effect at a time;
- a total, contiguous physical source partition before any command is exposed;
- last-use merging of retained-prefix claims, pending gaps, and old-frame
  closes;
- marker floors that keep marked blank suffixes on their exact container even
  when that container retires on the following line;
- parent-owned leading source before a new Paragraph `Enter`;
- no replacement source targeting a retired frame; and
- scratch compaction only after `FinishLine`, with the emitted path and any
  staged gap floor remapped together.

Fifteen focused direct tests pass in debug and release. They cover
`> a\n- b`, `> a\n> - b`, list siblings, lazy quote continuation, nested
quote/list continuation owners, marked blank continuation/EOF/retirement,
tight and loose lists, CRLF, exact line partitions, leading parent gaps, and 64
one-line nested quotes. The complete correspondent-parser all-target suite is
green. The exact adapter maps all current kinds, coverage parts, ancestor
selectors, typed opens, and close facts into `CandidateWriter`; its nine
focused release tests pass, the exact-feature v3 suite passes 100 unit tests
plus every integration/doc lane, and strict all-target Clippy passes with
warnings denied.

The gate remains intentionally open. The current recipe uses synchronous
`VecDeque`/`HashSet` sealing and O(depth) compaction, retains an 8 KiB proof
line ceiling, and supports no Setext, fenced/raw blocks, references, or tables.
Those are proof scaffolds. The direction fails if the next constructs require a
persistent handle directory, generic mutation graph, Markdown re-scan, source
range inference from final positions, document history, or non-fuelled
deep-path work.

## Architecture-selection slice

After the basic writer is green, the actual exact parser and storage must form
one vertical slice. The parser's real state defines the checkpoint schema; a
storage-only checkpoint designed first would merely guess at that contract.

1. Extend the now-executable real parser/writer path beyond ordinary
   Document/Paragraph while keeping `StructuralEvent`, materialization, copied
   source leaves, and aggregate logical strings off the dependency path. Make
   giant-line and dense fan-out scanners refillable rather than relying on the
   current 8 KiB proof ceiling.
2. At writer-aligned line transitions, persist the exact event-side cut plus
   parser control, semantic-prefix, open-binding, projection/reset, source,
   epoch, and immutable-base state needed to resume.
3. Seed a nonzero actor-owned restart, prove collision-safe convergence, and
   atomically attach one immutable suffix while preserving a distant leaf by
   exact `ArenaId`.
4. Exercise the representation-stressing operations before selection: Setext
   promotion, reference-only wrapper finalization or list tightness, GFM table
   reparent/rewrite, and a defined/undefined reference transition.
5. Attach one real bounded inline/reference service root to the same composite
   manifest, then run the same semantic/incremental slice on native and Wasm.
6. Drive one revisioned host edit through latest-wins worker admission, compact
   output adoption, and viewport presentation without a document-sized Dart
   graph or payload.

Clean and incremental parsing continue through the same writer. A fresh parse
is merely a candidate with no retained prefix/suffix capabilities.

Only this real-parser slice can select the whole architecture. Full corpus,
fuzz, scale, device, and UX matrices remain production/launch hardening after
selection; they cannot rescue a failed authority or representation seam.

## Mechanism milestone evidence

The current grammar-free milestone is complete only when:

- empty, one-paragraph, two-paragraph, nested, transformed, and Unicode source
  fixtures decode to exact source metrics and structural event order;
- every writer/builder phase can suspend, and injected failures after green
  acknowledgement but before ledger mutation poison rather than continue;
- the non-cloneable source/composer completion and sole green manifest can
  succeed only through one writer-owned join;
- allocator receipts cover actual capacity and every exercised growth
  boundary; and
- debug, release, strict Clippy, native, and Wasm checks pass.

## Architecture-selection evidence

The architecture-selection slice is complete only when:

- the composed tests pass in debug, release, strict Clippy, native, and Wasm;
- allocator receipts cover actual capacity and every growth boundary;
- no failure path can return consumed authority or publish a partial epoch;
- module-boundary and external compile-fail/API-surface tests prove grammar
  code cannot bypass the writer through raw composer, run, ID, metric, or
  green-builder constructors (`pub(crate)` is not sufficient if grammar code
  shares that visibility; proof-harness access must be test/feature isolated);
- a local edit retains a distant suffix leaf by exact `ArenaId`;
- clean and incremental decoded output are semantically equal; and
- the production-facing parser port is smaller than this proof protocol: one
  typed writer, not a graph of manually synchronized capabilities.

If these mechanisms cannot collapse behind that writer, or retroactive grammar
requires a general mutable node directory/event history, stop and reopen the
parser/output design rather than adding another adapter.
