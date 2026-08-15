# Parser donor bakeoff and decision update

Status: decision evidence, updated 2026-07-18. This supersedes the earlier
conclusion that Comrak had already earned primary-donor status and incorporates
the inline, packed-state, checkpoint-restart, integrated writer, retained
restart, Table, reference, source-session, and host-publication pressure tests.
It does not claim that the production parser-control seam has passed.

## Decision

Keep RFC 023's editor architecture and revise its parser implementation plan.

- The pinned CommonMark 0.31.2, selected GFM rules, and explicit Flark
  deviations are normative. No third-party implementation is the semantic
  authority.
- Flark owns one runtime parser: source input, continuations, checkpoints,
  reference symbols, facts, persistent output, work budgets, cancellation,
  and deltas.
- Stock Comrak, stock Pulldown, an enclosing-block adapter, and a clean-room
  grammar are rejected production seeds.
- Pulldown 0.13.4 is the **leading inline-algorithm donor**, not the selected
  donor for the whole core. Its delimiter, code-span, bracket, and reference
  algorithms survived extraction onto segmented source and value state without
  `Tree<Item>` or `TreeIndex`. Its selected block scanners also adapted to
  compact value state and its clean parser has substantially more cold-parse
  headroom. This is not permission to wrap or ship its stock first pass.
- Comrak/cmark-gfm remain independent differential peers and valid sources for
  localized algorithms, particularly GFM bare autolinks and generated scanner
  techniques. One runtime authority does not require one code-provenance
  lineage.
- Choose algorithms per seam. Pulldown leads the inline seam; Comrak/cmark-gfm
  may still lead individual block or GFM scanners. The next commitment gate is
  one integrated Flark-owned slice combining real segmented grammar, packed
  pages/stacks/facts, exact checkpoint restart/convergence, persistent output
  roots, and reference invalidation. Isolated successes do not prove that
  composition.

The resulting direction is an owned derivative, not a conventional downstream
fork. Every production syntax rule has exactly one implementation, regardless
of where that algorithm originated.

This is enough to continue commitment-gate research. It is not approval for
broad grammar accumulation or product integration, and it does not close RFC
023's physical-device Phase 0 exit criteria.

## 2026-07-18 integrated update

The integrated work changes the location of the decision, not the authority
model above.

The following production mechanisms now compose in one prototype family:

- persistent segmented source, exact line indexing, bounded actor byte
  sessions, and source-complete giant-line scanning;
- a source ledger, projection composer, streamed candidate writer, packed green
  output, persistent sequence splice, cancellation, and copied host
  publication;
- same-build and cross-build checkpoint selection, exact source/state
  convergence, retained Setext inversion, and canonical fragment replacement
  with variable output cardinality;
- a grammar-neutral streamed Paragraph-to-Table writer transaction, including
  65,537 columns and cancellation at every scanner/writer boundary; and
- a Dart source authority, versioned worker-replica journal, distinct source
  certification lineage, host staging, and Flutter certified-fingerprint
  attachment.

That is sufficient to mark the **incremental runtime architecture GO**.  It is
not sufficient to select the final parser-control implementation.  The exact
lane currently retains two deliberate red gates:

1. a split Setext edit converges at the correct donor checkpoint, but restart
   lacks a parent-authenticated canonical-fragment origin spanning the retained
   prefix; and
2. leading reference definitions stop at the parser-owned
   `ReferenceDefinitionCandidate` boundary because the direct parser lacks a
   resumable source-backed reference-prefix finalizer and the writer lacks a
   zero-wrapper reference-only outcome.

GFM Table control is the third comparison seam: its writer transaction is
green, while the current tree-backed Comrak Table implementation is rejected
because it owns an aggregate Paragraph string, `BlockTree` nodes, and a
table-sized alignment vector.

### Current incumbent and decision thresholds

The incumbent is now **a Flark-owned incremental runtime and control protocol
with algorithms selected per seam**.  The current Comrak-correspondent direct
transition is an algorithm donor inside that runtime, not an architectural
right to retain Comrak's document lifetime model.

Keep the direct-Comrak-derived control seam only if all of the following become
true in the integrated candidate:

- the direct transition no longer constructs or retains `ValueBlockParser`,
  `SourceDocument`, `BlockTree`, aggregate Paragraph strings, or reference
  maps;
- Setext, Table, and reference finalization use the same actor-bound,
  resumable, checkpoint-serializable control protocol instead of per-construct
  driver classifiers;
- the complete maintained Comrak/local seam remains below roughly 2.5k changed
  lines and stays concentrated in a small, upgrade-rehearsable file set; and
- a donor upgrade rehearsal plus CommonMark/GFM differential corpus can be
  completed without reconstructing Flark's output/storage contracts inside the
  fork.

Select a fully Flark-owned, spec-derived block/control parser instead if any of
these occurs:

- separating the direct transition from Comrak's lifetime representations
  requires a second broad compatibility layer;
- Table, reference, and restart each need independent external-control hooks
  rather than one generic parser-work rendezvous;
- the maintained seam exceeds the threshold or spreads into additional mutable
  AST/content files; or
- a clean spec-derived state machine reaches the same writer/checkpoint port
  with materially fewer authority wrappers and equal differential evidence.

This is not a choice between “fork everything” and “write all Markdown from
memory.”  Normative CommonMark/GFM fixtures remain the authority, donor code is
ported with provenance where it wins, and the production runtime has exactly
one implementation of each syntax decision.

## Assumptions that failed

### “Pulldown has materially weaker CommonMark semantics”

The earlier 649/670 GFM score mixed an old GFM core corpus with the selected
CommonMark 0.31.2 authority and counted excluded footnotes. Pulldown 0.13.4
passes all 652 canonical CommonMark 0.31.2 examples in the current harness.
With Flark's table, strikethrough, task-list, autolink, and tagfilter flags, the
remaining selected-profile gaps are concentrated in GFM bare autolinks and
tagfilter rendering. Tables, strikethrough, and task lists are not a broad
semantic deficit.

Pulldown documents the remaining GFM autolink/tagfilter work in
[issue 518](https://github.com/pulldown-cmark/pulldown-cmark/issues/518).
Tagfilter is a small output/classification rule for Flark's literal raw-HTML
policy. Bare autolink recognition is the material grammar addition.

### “Pulldown's compact indexed tree is already the right runtime shape”

It is not. Instrumented stock `FirstPass` receipts found:

- roughly 2,000,001 nodes and 192 MB of reserved node capacity for a 2 MB,
  million-`a\n` paragraph;
- roughly 336,000–432,000 nodes and 24.2 MB capacity for about 1 MB of
  token-dense inline input; and
- an 11.2 MB giant paragraph consumed in one approximately 4.6 ms call after
  an edit, without a sub-leaf cancellation point.

The first pass eagerly records inline candidates and soft breaks. Wrapping it
is a killed direction. A qualifying core keeps compact block/leaf structure and
runs inline recognition lazily and resumably.

A separate public-API process probe makes the resource result externally
reproducible without trusting the private node counter. On this host, the 2 MB
million-line paragraph reached 99,811,328 bytes maximum RSS and emitted
2,000,001 events; the 1.008 MB dense-inline shape reached 24,297,472 bytes RSS
and spent about 148 ms in event resolution. The 10 MiB plain line stayed sparse
but its constructor still ran as one 4.37 ms, non-cancellable call. The probe is
`src/bin/pulldown_stock_memory.rs`; capacity and RSS answer different questions,
and both violate the intended dense-line/urgent-work shape.

### “Comrak can be adapted surgically because its block/inline split is good”

The split is valuable; the surgical conclusion is false. The
[Comrak-derived seam](comrak_derived_core/RESULTS.md) removed arena nodes,
copied leaf strings, and AST conversion, and proved real sub-line yielding and
retroactive setext promotion. It nevertheless reached 2,117 core lines for 14
mapped functions spanning 718 upstream lines while still omitting exact list
tightness, HTML, tables, references, inline parsing, rope input, and integrated
persistent output. That roughly threefold expansion is direct maintenance-risk
evidence.

### “One grammar authority implies one primary code donor”

Authority is a runtime property, not a provenance property. A Flark block
machine, inline machine, and GFM scanner can use algorithms from different
permissively licensed implementations while still making every decision once
and emitting one fact stream. The dangerous design is two callable parsers or
two grammar-sensitive consumers, not a recorded function-level provenance
ledger.

### “A candidate's own work and memory counters prove the SLA”

They do not. The [donor-neutral Gate A harness](gate_a_harness/README.md) now
separates test-only batch views from the production
`begin_edit -> poll -> commit` path, but final acceptance also requires code
audit plus external allocator, RSS, wall-time, and transfer instrumentation.

### “Extracting Pulldown's inline algorithms proves its runtime shape”

It does not. The extracted inline machine proved that selected algorithms do
not intrinsically need Pulldown's mutable tree, but its first retained tape and
fact representation still failed the memory gate. On a 10 MiB unmatched
delimiter-dense leaf it retained 5,242,880 tokens and 100,663,296 bytes of
token capacity; the isolated process reached about 129 MB RSS. This is an
algorithm-seam pass and a retained-representation fail.

### “Packed state plus restart can be proved independently and then assumed to compose”

The independent mechanisms are encouraging, not additive proof. A toy packed
grammar kept the worst 10 MiB adversary below the 96 MiB experimental ceiling,
and a separate checkpoint prototype reparsed one 4 KiB page after a balanced
10 MiB middle edit before exact state/source convergence. Neither implements
Markdown, and neither combines real inline semantics, production source
indexes, persistent output roots, or Gate A/B adapters. Their composition is
now the risk to test.

## Symmetric slice receipts

| Property | Comrak-derived seam | Pulldown-derived seam |
| --- | --- | --- |
| Scope | Quotes, lists, paragraphs, fences, setext | Quotes, lists, paragraphs, fences, setext |
| Core size | 2,117 physical lines | 1,839 physical lines; about 1,639 nonblank/non-comment |
| Exactness evidence | 17 selected Comrak structural fixtures | Four selected Pulldown parity fixtures, a composite fixture, and 250/250 clean-vs-resumed edits |
| 10 MB leaf | 2,442 polls; maximum 4,096 bytes/poll; about 38–39 ms | One semantic chunk; maximum 4,096 bytes/poll; 848 bytes transient state |
| Million `a\n` paragraph | 197.8 MB if the research line trace is retained; 5.8 MB RSS when drained | One chunk, two checkpoints, parser metadata below 32 KiB |
| Edit locality | Checkpoint-resume mechanism only | 250/250 convergence; maximum 4,120 reparsed bytes; suffix IDs reused |
| Decisive gap | Exact containment/tightness and source-backed inline handoff may keep expanding | Complete block semantics and integration with the extracted inline algorithms remain unproved |

The Pulldown slice's flat `String`, eager `Vec` suffix clone/shift, simplified
grammar, and self-reported memory are explicit gaps. Its favorable row proves a
representation can coalesce a million soft breaks; it does not prove the
shipping document model.

## Inline, representation, and restart receipts

The final bounded inline experiment produced a deliberately split result:

- [`pulldown_inline_gate/`](pulldown_inline_gate/) matches Pulldown 0.13.4 on
  5,000 generated emphasis and 5,000 generated code-span cases, exercises
  selected link/reference cases, and preserves clean-versus-resumed equality at
  fuel 1/2/7/31. It has no general mutable tree and every scan/resolve/output
  phase yields. Its rich `Vec` tape/facts nevertheless fail the dense-memory
  contract.
- [`packed_inline_state/`](packed_inline_state/) uses fixed 4 KiB packed pages,
  one-to-two-byte dense records, packed stacks, and fuelled finalization. Its
  worst 10 MiB all-open row accounts for 86,705,329 bytes with 78,495,744 bytes
  external RSS, below the 96 MiB falsification ceiling. It uses a toy grammar,
  parses from the start, and the slowest 10 MiB case takes about 1.18 seconds.
- [`checkpoint_restart_state/`](checkpoint_restart_state/) proves genuine
  restart and collision-safe exact convergence. A balanced 10 MiB middle edit
  scans 4,096 bytes and attaches the unchanged suffix; changing persistent open
  state correctly forces a 10 MiB scan. Its state and facts are unpacked, its
  page roots attach linearly, and source editing/reclamation remain outside the
  demonstrated budget.

The result narrows rather than removes uncertainty: compact resumable state and
true suffix convergence are mechanically plausible, while the production risk
is integrating them with the real grammar and one persistent source/fact model.

Cold full parsing is not the decision criterion, but it is useful headroom on
the same 100 KB operation:

| Shape | Comrak p50/p95 | Pulldown p50/p95 | markdown-rs p50/p95 |
| --- | ---: | ---: | ---: |
| Typical blocks, 102,540 bytes | 4,408/4,509 us | 974/1,056 us | 127,672/130,920 us |
| Giant inline, 102,410 bytes | 3,421/3,485 us | 832/841 us | 130,962/132,097 us |

## Proposed parser shape

```text
Persistent source rope
  -> resumable block machine
     -> compact block facts + source-segment leaf views
        -> lazy resumable inline machine for changed/visible leaves
           -> reference-symbol dependencies + direct source facts
              -> persistent paged syntax sequence + compact range deltas
```

Important boundaries:

- A leaf view is a segment sequence, not a flattened string. It preserves
  stripped container prefixes, virtual tab spaces, line breaks, table-cell
  context, and exact logical-to-source mapping.
- The block machine owns block classification and reference-definition
  extraction once. The inline machine cannot rescan block grammar.
- The inline machine owns delimiter, bracket, code-span, autolink, and
  reference-use recognition once. Dart and projection code consume facts only.
- Plain source runs and soft breaks are coalesced into bounded pages; neither a
  byte nor a newline implies a syntax object.
- Global structure changes use persistent sequence/range splices. Global
  reference-value changes use symbol indirection. Defined/undefined or
  first-definition precedence changes can alter link recognition/nesting and
  therefore enqueue dependent leaves through the fuelled inline machine; they
  do not masquerade as value-only updates or eagerly rewrite every Dart fact.
- Clean and incremental modes invoke the same grammar implementation. Clean
  parsing is a scheduling mode, not a second parser.

Using Comrak-derived block code and stock Pulldown inline parsing is explicitly
rejected. Pulldown's inline passes depend on its first-pass tree and container
context, so that composition would retain both donor representations. A mixed
lineage is acceptable only after extracting algorithms onto the Flark segment,
state, symbol, and fact contracts.

## Donor-neutral authority and gate corrections

The executable Gate A contract currently selects 181 CommonMark block examples
and eight GFM table examples, more than 400 intermediate edit revisions, giant
line and construct cases, million-line memory, stable-order histories, exact
source facts, and direct deltas. Gate B adds 398 normative inline/reference
fixtures, 11 histories and 687 scalar-safe revisions, segmented source maps,
4 KiB scan/resolution/emission fuel, 10 MiB dense-leaf resources, and compact
reference invalidation. Comrak and Pulldown differ on nine exact Gate A
serializations and on 225 incomplete typing revisions. That disagreement is
evidence, not a vote: normative fixtures govern pinned cases and a candidate
must match its own clean parse at every revision.

A red-team pass also hardened the gate to require:

- semantic parent equality, acyclic ancestry, and source containment;
- 64 KiB maximum coverage pages so one mutable whole-document record cannot
  fake locality;
- stable fact identity as well as coverage identity outside the blast radius;
- independent replay of coverage and fact deltas to the committed snapshot;
- a global HTML-comment activation fixture; and
- external rather than self-reported resource/liveness receipts.

Gate B's red-team lanes additionally require unresolved reference dependencies
to survive while a definition is absent. Changing a winning definition's value
updates one symbol without consumer churn. Removing or restoring the winner
fuelfully re-resolves 5,000 distinct dependent leaves, while the committed delta
adopts one output-sequence root and one dependency-generation root instead of
synchronously enumerating 5,000 Dart objects.

The harness still does not claim that either final parser-control
implementation passes.  The integrated source/state/output model now exists;
full selected grammar adapters, reference/inline composition, external
resource receipts, and physical-device evidence remain work.

## Integrated commitment gate

The runtime portion of this gate is now substantially exercised by the v3
prototype.  Do not accumulate the remaining grammar profile or begin product
cutover yet.  First close the authenticated Setext-fragment,
reference-finalizer/root, and Table-control seams in the same
production-shaped candidate with this exact scope:

1. Use the same persistent source pages, stable anchors, checkpoint roots,
   packed fact pages, and output/delta roots for both block and inline work. No
   flat-string or batch-snapshot adapter may hide the production path.
2. Integrate a narrow but real block-to-segment handoff with Pulldown-derived
   emphasis/strong, code-span, inline-link, and reference-link algorithms.
   Shared label normalization, table-pipe ownership, virtual bytes, and
   stripped block prefixes must each have one owner.
3. Restart before a local edit and attach a suffix only after collision-safe
   exact parser-state and immutable-source-tail convergence. Facts spanning the
   boundary must remain exact.
4. Use fixed/paged compact lexical, stack, fact, checkpoint, dependency, and
   sequence representations. Enforce the 4 KiB work contract inside scan,
   resolve, output, allocation, hashing, index composition, suffix adoption,
   and old-revision reclamation.
5. Pass the relevant Gate A and Gate B slices, including 10 MiB dense leaves,
   million soft breaks, cancellation/supersession, clean equality, value-only
   reference changes, and 5,000-leaf defined/undefined fanout.
6. Measure isolated external RSS, allocator traffic, wall time, and native/WASM
   behavior. The 96 MiB research threshold is a kill ceiling, not the product's
   complete memory budget.
7. Record exact donor functions, modifications, license notices, and code-size
   expansion. Algorithm choice remains per seam; no whole-donor assumption is
   carried into the candidate.

### Go

Fund broader parser implementation if the integrated candidate passes without
a second grammar consumer, general mutable tree, whole-leaf uninterruptible
work, global fact enumeration, or hidden unbounded allocation/copy/reclamation.
Pulldown remains the default inline donor only for seams it actually wins.

### Revise

Keep the Flark contracts but substitute the smallest better algorithm by seam
if one Pulldown mechanism fails—for example a generated Comrak/cmark scanner or
the cmark-gfm autolink routine. Differentially prove the substitution. If the
architecture works but the 10 MiB ceiling is too tight for realistic floor
devices, revise the product memory/SLA explicitly rather than hiding it in a
prototype counter.

### Stop

Reopen the product/SLA decision if exact inline behavior requires a general
whole-document tree, unbounded synchronous leaf work, duplicated block/inline
classification, or an unmaintainable second broad semantic port. Do not fall
back to the narrow Comrak adapter or stock Pulldown by default.

## Maintenance model

- Pin the syntax profile separately from donor versions.
- Keep a function-level provenance and local-modification ledger.
- Preserve all applicable MIT/BSD notices in source and distributions.
- Track upstream semantic, security, and pathological fixes in both major
  peers, porting only relevant changes through normative and differential
  tests.
- Require clean/incremental differential fuzzing, native/WASM parity, external
  memory/time receipts, and a representative donor-update rehearsal before
  cutover.
- Keep v2/Comrak only as a migration fallback and test peer until v3 earns the
  package's inherited behavioral suite; never run it as a simultaneous live
  grammar authority.
