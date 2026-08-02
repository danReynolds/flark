# Flark v3 definitive architecture

**Status:** Selected production architecture, updated 2026-08-01. Implementation is
active; launch readiness is not implied. Checkpoints A and B have verified
engineering evidence ready for user review. Checkpoint C has a
production-shaped role-reuse path, a marker-free selected-Paragraph editing
vertical, parser-certified fenced-code, ATX-heading, Setext-heading, and
indented-code verticals, parser-certified inline authority covering escaped
punctuation, hard line breaks, character references, direct links/images, and
grammar-revision-9 full/collapsed/shortcut reference links/images, a parser-certified atomic
thematic-break
vertical, a
top-level depth-one tight `BulletList` vertical with marker-free selected-item
editing, exact canonical source preservation, and a checkpoint-free
source-rope rank/select local-delta path, and the matching narrow top-level
depth-one tight `OrderedList` vertical, a depth-one single-Paragraph
`BlockQuote` vertical, plus a bounded current-revision
inline-facts cache, exact-clean packed block-page
splicing, and a green bounded top-level crop/splice vertical.
Ordinary Paragraph checkpoints can bracket local Paragraph, ATX-content,
Setext-transition, Paragraph↔thematic-break, and fenced-code-body edits of at
most 64 KiB. An edit within either admitted tight-list shape instead derives the
independent base and target predecessor/changed/successor line windows directly
from the persistent source rope and parses only those bounded windows.
Definition-free documents can now restart at BOF or run to EOF for local first-
or final-Paragraph edits. Definition-bearing documents use the
ordinary lane only strictly after the last definition-bearing leaf, with the
exact frozen definition count carried by every restart. Checkpoint collections
authenticate the exact top-level block count; “segmented” is only the derived
fact that this count exceeds one. It remains unpassed: edits to or restart
through definitions, definition-bearing BOF edits, unsupported or unanchored
regions, over-cap or lost-convergence crops, and broader block grammar;
virtualized multi-block layout over the bounded visible structural range
materializer; the 100 MiB product viewport; and the later grammar, floor-device,
accessibility, and launch gates are still open. The standalone
100,000-reference real Flutter Web/Worker/Wasm product gate is green: seven
zero-cadence platform deltas retain one marker-free `EditableText` and platform
input client, preserve exact source, and converge exactly at the final revision.
The latest measured preceding-build standalone Chrome receipt records a 4.2 ms
maximum synchronous callback and 7.6 ms total callback time across the seven
edits. The preceding-build combined
small-widget→100,000-reference-widget sequential reopen gate was also green
after correcting the Web module-loader cache lifetime; that Chrome run recorded
a 5.1 ms maximum synchronous callback and 8.8 ms total callback time. These are
regression-host receipts from bytes preceding grammar revision 6. Revisions 6
through 9 have not rerun that performance gate, and neither receipt is
floor-device `FrameTiming` or launch-SLO evidence. The revision-7 proof remains
recorded below; the revision-8 and current revision-9 addenda record only
functional, freshness, parity, ownership, and checkpoint evidence.

This is the normative, implementation-facing summary of Flark v3. It separates
decisions that are settled from evidence gates that are still open.
[RFC 023](../rfc/rfc_023_incremental_live_markdown_engine.md) retains the full
rationale and rejected alternatives. The
[implementation plan](implementation_plan.md) defines delivery order, and the
[proof ledger](../../../tool/parser_research/ARCHITECTURE_PROOF_LEDGER.md)
records executable evidence and reopen conditions.

## 1. Architecture decision

Flark v3 is a **Dart-first, large-document live Markdown engine** with an
optional Flutter adapter.

- The root `flark` package is Flutter-independent Dart. It owns the public
  document, edit, revision, query, and lifecycle API.
- `flark_flutter` depends on `flark`. It owns Flutter input, layout, painting,
  selection geometry, semantics, widgets, themes, and asset conveniences.
- Each document session has one authoritative Markdown grammar implementation.
  There is no Dart prediction parser and no presentation-side Markdown
  classifier.
- The grammar and persistent syntax state run off the calling Dart isolate: in
  a long-lived native isolate through FFI, or in a Web Worker through Wasm.
- The source, worker, parser, publication, and consumer states are persistent
  and revisioned. Ordinary edits do not rebuild a document-wide Dart string,
  AST, render plan, or `TextEditingValue`.
- Clean and incremental parsing use the same parser, result model,
  publication transaction, host store, and query API.

```text
flark_flutter (optional)
  input / viewport / paint / semantics
                  │ bounded intents and queries
                  ▼
flark (pure Dart)
  exact source / transactions / public session / revision adoption
                  │ schema-3 bounded byte protocol
          ┌───────┴────────┐
          ▼                ▼
long-lived native      Web Worker
Dart isolate + FFI     + Wasm
          └───────┬────────┘
                  ▼
Flark-owned persistent parser document
  source replica / SourceFacts / syntax / projection / semantics
                  │ credited atomic publication
                  ▼
independent host store and bounded Dart queries
```

`Dart-first` describes the product API and dependency direction. It does not
require the Markdown grammar to be implemented in Dart and does not tie the
engine to Flutter. A Dart CLI, server, web application, test, terminal UI, or
future renderer can use `flark` directly.

## 2. Product invariants

The implementation must preserve these invariants together:

1. Exact Markdown source is the canonical user document and commits
   immediately.
2. Typing, selection, caret, and IME work on the caller isolate stay bounded,
   even while authoritative presentation is behind.
3. Parser results normally arrive quickly enough to feel continuous, including
   incomplete bold, emphasis, links, and fences as they evolve.
4. The active Flutter island is a marker-free parser-certified projection with
   exact source/display maps. Newly authored syntax remains literal until
   certified; a mechanically maintained prior projection may paint
   provisionally without semantic authority, and parser failure falls back to
   editable literal source.
5. CommonMark, the selected GFM profile, and incomplete-input behavior have one
   grammar authority.
6. Local edits reuse persistent source, syntax, projection, semantic, and
   presentation state when exact convergence permits it.
7. A truly non-local edit may require non-local work. That work remains
   resumable, cancellable, memory-bounded, and off latency-sensitive callers.
8. A consumer observes one coherent revision or an explicitly stale
   paint-only revision, never mixed roots.
9. Clean parse/export and incrementally reached state converge exactly.
10. Whole-document materialization is an explicit cold or streaming operation,
    never a hidden cost in an ordinary edit or query.

Large-document support is therefore an architectural constraint, not a future
optimization. Small documents use the same model.

### 2.1 User review checkpoints

Three explicit reviews keep storage evidence, publication evidence, and editor
feel from being conflated:

The table preserves the wording of its recorded milestone receipts. In its
Checkpoint C row, “current rebuilt-byte” names the preceding pre-revision-6
artifact that produced the 4.2/7.6 ms figures; it does not describe the current
revision-9 bytes. The revision-7 through revision-9 addenda below the table are
the controlling current receipts.

| Checkpoint | Required demonstration and evidence | User decision |
| --- | --- | --- |
| **A — Responsiveness diagnosis** | **Evidence ready; review pending.** The release native/browser lab shows bounded foreground edits, caller heartbeat, latest-wins cancellation, convergence, and truthful close from small through 10 MiB fixtures. The public definition-free 100,000-Paragraph fixture is now incremental: its roughly 3.2 MB replacement completes in 20.994-26.977 ms native and 29.9-30.1 ms Chrome while foreground work stays below 8 ms. | Accept or revise the off-caller execution/observability model and confirm that the narrow incremental result feels live without mistaking it for broad-grammar completion. |
| **B — Persistent SourceFacts identity-reuse edit** | **Evidence ready; review pending.** The production-path proof covers clean equality, exact unchanged page identity, bounded changed-path work, false-lineage rejection, cancellation/fallback, zero-residency close, and native/Wasm parity for prefix, middle, tail, Unicode, and split-CRLF edits. | Approve the persistent storage/reuse model before treating it as promoted. |
| **C — Role-root delta and live editor integration** | **Partial vertical evidence; reviewable, not passed.** Authenticated exact-base SourceFacts reuse and packed block-page splice are complete. Ordinary Paragraph checkpoints bracket bounded interior Paragraph, ATX-content, Setext Paragraph↔H1↔H2, Paragraph↔thematic-break, and fenced-code-body crops in definition-free top-level documents. A 4,096-block thematic fixture keeps promotion and demotion inside one bounded parser crop; its packed splice deletes and replaces at most 64 records and retains exact next-revision authority. The corresponding Setext fixture keeps every local transition on `ParsingOrdinaryExact` -> `ExactBaseDelta`, transfers and replaces at most 64 records, preserves exact first/middle/last queries, and remains exact for the next revision; an over-4-KiB same-block Paragraph→Setext promotion deliberately takes the clean fallback rather than trusting a stale restart. Checkpoints also carry an exact frozen definition count strictly after the last definition-bearing leaf: a length-changing Paragraph edit behind 2,048 definitions and among 2,048 Paragraphs publishes `ExactBaseDelta`, transfers at most 64 records while retaining all References, preserves exact first/middle/last queries, and leaves the public runtime exact for a next revision. Separate 4,096-Paragraph definition-free boundary fixtures prove length-changing first- and final-block crops enter `ParsingOrdinaryExact`, publish `ExactBaseDelta` with at most 64 transferred and replacement records, preserve exact first/middle/last geometry, and remain exact for public revision 3. BOF reuses suffix checkpoints; EOF reuses prefix checkpoints and correctly mints zero fresh checkpoints beyond EOF. A bounded 128-leaf/2,048-fact-record Dart cache retains current-revision inline facts after the singleton host sidecar moves. Selected Paragraph, `FencedCode`, `IndentedCode`, ATX Heading, Setext Heading, atomic `ThematicBreak`, and inline `HardLineBreak` cross parser, publication, independent host, public Dart, native and Chrome parity, real Flutter, and the runnable demo. The current grammar-revision-9 Chrome checkpoint is green 3/3 for passive full/collapsed/shortcut reference links and a reference image alongside direct media, with exact cooked values, semantics, and a no-I/O image fallback; the focused later-definition direct-media recertification regression is green 1/1. Indented code publishes exact variant-7 structure first, then separately supplies its parser-authored physical-line projection through a demanded schema-3 query; managed Flutter hides certified indentation, preserves canonical source on Enter, and recertifies on the same input client. The existing BulletList slice publishes structured role variant 9, then separately supplies a demanded viewport-schema-5 selected path and canonical 28-byte item records. Its managed selected item hides the certified marker/prefix, preserves exact source including line endings, hands off between items, and exits the list when Enter is pressed on the terminal empty item. Current-byte native and Chrome managed gates, the focused Chrome engine-lab checkpoint, and the release demo for that schema-5 slice are green. The new checkpoint-free source-rope local-delta path uses predecessor/changed/successor windows in the base and target and has exact 20,000- and 100,000-item endpoint receipts: target local parse work is 295 transitions at both sizes, build is 18/21, stream is 20, and publication remains four records in two packets after examining 262,149 SourceFacts bytes. It matches the independent clean oracle, fixes the local-edit lifecycle restoration bug, survives two consecutive `ExactBaseDelta` edits after a 109-checkpoint/three-page underfilled persistent topology becomes the base, and closes to zero. The compact schema-6 selected-item projection and explicit geometry-then-inline demand path are green through rebuilt-Wasm/freshness 2/2, public-runtime semantic parity 1/1 on Chrome, and the managed compact BulletList batch 3/3 on native Flutter and 3/3 on Chrome. The first combined Chrome run had one transient timeout; the immediate isolated rerun and full rerun passed, so these receipts do not define a deterministic performance budget. The narrow OrderedList sibling admits only top-level depth-one tight one-physical-line items with `.`/`)` delimiters and 1–9 digit markers; it preserves zero padding, nonsequential ordinals, Unicode, CRLF, and a terminal empty item. Its 20,000-item local-delta receipt parses only three base and three target physical lines and matches the clean oracle. Structured role variant 10 is selected through a distinct viewport-schema-7/payload-kind-6 constant item projection, followed by a separate inline demand. Native, public-Dart, managed-Flutter, and focused-Chrome receipts cover paint-only marker hiding, exact source, and same-client `007)`→`008)` continuation. The standalone 100,000-reference real Flutter Web/Worker/Wasm product gate is green: seven zero-cadence platform deltas retain one marker-free `EditableText` and platform input client, preserve exact source, and converge exactly at the final revision. Its current rebuilt-byte standalone Chrome receipt records a 4.2 ms maximum synchronous callback and 7.6 ms total callback time; it is a regression-host receipt, not floor-device `FrameTiming` or launch-SLO evidence. The combined small-widget→100,000-reference-widget sequential reopen gate is green after the Web module-loader cache-lifetime correction; its Chrome receipt is 5.1 ms maximum synchronous callback and 8.8 ms total callback time, separate from the standalone receipt. | Keep C open. Review the current checkpoint, then extend restart/convergence to edits of or before definitions, definition-bearing BOF, unsupported or unanchored regions, containers, and broader grammar; add virtualized visible-set layout; and complete floor-device interaction gates. A new tail definition, typed unsupported tail, over-cap crop, or lost convergence continues to fail clean. This engine receipt does not claim broader loose, task, nested, multiline/lazy, mixed delimiter/type, container-wrapped, multi-block, or 10-digit-marker list forms; quotes, HTML, tables, complete reference-link interaction, active reference-media editing, incremental definition mutation, or authenticated restart/convergence for edits inside indented-code blocks. |

For the admitted tight-list wording in Checkpoint C, local incrementality no
longer means adding a second list-checkpoint index. Source-rope line rank/select
derives the base and target predecessor/changed/successor windows around the
edit; the exact parser validates those bounded windows and the existing
persistent block tree publishes their replacement as `ExactBaseDelta`.
Document size and total list-item count therefore do not define the local
parser payload. Unsupported list shapes still fail closed rather than
borrowing this authority.

Checkpoint A does not imply B, and B does not imply C. None is a launch gate;
the later viewport, behavior, accessibility, floor-device, and release reviews
remain mandatory. The implementation plan owns the full demo and approval
contract, while the proof ledger records current executable evidence.

The grammar-revision-6 `CharacterReference` slice is another admitted
Checkpoint C vertical, not a broader grammar claim. Rust is its sole
recognizer and publishes kind 9 with the exact source token and its
parser-cooked one- or two-scalar value. The existing schema-2 `IFO2`/`IFP2`
fact family and each 20-byte record remain unchanged. Dart performs only
mechanical validation, source/display projection, and edit planning from those
certified bytes. URI autolinks may contain nested kind-9 facts, so the same
parser-authored value cooks both their displayed label and destination; no
autolink-side entity scanner or Dart recognizer exists. Active and passive
projection remain marker-free, while partial replacement consumes the whole
source token and UTF-16 edit boundaries inside a surrogate pair are rejected.
The verified revision-6 milestone gates were the full Rust workspace, 60/60
focused Dart core cases including digest parity, 17/17 Flutter active/source
cases, the public-runtime entity-edit case on native and Chrome, and the 3/3
Chrome asset/reopen gate. At that milestone, direct links and images, broader
bracket hazards, and full CommonMark/GFM remained outside the admitted
vertical.
The 4.2/7.6 ms standalone and 5.1/8.8 ms combined timing figures in the
Checkpoint C row are preceding-build receipts; neither grammar revision 6 nor
grammar revision 7 has rerun that performance gate.

Grammar revision 7 admits direct inline links and images as projection kinds 10
and 11. Rust alone resolves bracket precedence and direct `(…)` tails, preserves
nested inline styles and link/image precedence, and cooks destination and
optional title values. The unchanged schema-2 20-byte fact lane carries exact
geometry. An atomically paired persistent `IPB5` value root carries
variable-width values and emits a bounded, self-framing `FLKIV001` companion
keyed by raw fact ordinal. Missing or orphan companions, corrupt cooked values,
reference, collapsed, shortcut, or incomplete link/image forms, and broader
unresolved bracket hazards fail the whole inline leaf closed. Dart only
validates and joins those two authenticated lanes. Flutter hides the complete
certified syntax, keeps active editing non-actionable, activates
parser-certified passive links, delegates passive images to an explicit builder
without implicit network or file I/O, suppresses links nested inside image alt
text, and preserves the outer action for an image nested inside a link.

## 3. Authority stack

Flark has one source truth and several successively stronger capabilities. The
distinction prevents transport acknowledgements and hashes from accidentally
becoming correctness authority.

### 3.1 Canonical source

The Dart document session owns the current exact user source, transaction
history, selection/composition ranges, and stable anchors. One edit intent
contains ordered, non-overlapping operations and advances exactly one revision
atomically.

The worker owns a persistent immutable Crop replica identified by its source
session, lease, revision, UTF-8/UTF-16 dimensions, and validated atomic
lineage. A seed or edit acknowledgement proves only that this exact replica
transition was installed in the current worker generation. It does not prove
source facts, parser completion, or publication eligibility.

### 3.2 Certified source facts

The worker derives one canonical `SourceFacts` index from the same immutable
source lease in bounded, resumable work. Session wire schema 3 transports:

- canonical global-prefix pages, currently capped at 64 checkpoints per page;
- an exact page receipt for flow control; and
- one terminal completion proof covering the full revision.

Page boundaries are canonical and independent of scanner poll fuel. Dart stages
the pages as a global overlay, but a page receipt can only advance or cancel
delivery. It cannot certify source. The exact terminal accepted receipt is the
sole authority that atomically promotes that same revision from provisional to
known.

The derived rolling-128 fingerprint is a convergence and corruption guard. It
is never exact source identity on its own. Certification additionally requires
the exact lease and revision, contiguous full-source coverage, dimensions,
profile, and clean completion.

Certification and structural commitment are deliberately separate authority
transitions. Exact certification installs the current source facts, but it
does not make that target the reusable incremental base. The reusable base
advances only after the independent host commits the matching structural
manifest. Until then, a superseding edit derives from the last
host-acknowledged base even when a newer UI revision has already certified.
Rust and Dart retain the same split between the active candidate proof and the
last structurally committed proof.

### 3.3 Parser and publication authority

Only a certified source capability may authorize structural publication. One
parser candidate owns all roots for its target revision and publishes them as
one manifest. Cancellation, stale authority, mismatch, allocation failure, or
worker failure publishes nothing partial.

The consumer-facing progression is therefore:

```text
exact Dart source
  -> exact provisional worker replica
  -> certified SourceFacts for that same replica
  -> exact parser candidate
  -> atomically installed host manifest
  -> current-revision bounded query result
```

## 4. Session and runtime model

The ordinary preview API is one long-lived, pure-Dart
`FlarkV3DocumentRuntime` facade. Final unversioned names may change at the
Checkpoint C promotion boundary, but these responsibilities may not be split
across public choreography:

- open and configure a document;
- apply transactions and maintain bounded inverse history;
- observe exact-source and certified/published revision changes;
- issue bounded source, structure, semantic, projection, diagnostic, and
  presentation queries;
- recover an in-protocol parser failure without changing source truth; and
- close with a truthful reclamation result.

Transport pumping, source-page certification, event receipts, publication
ACKs, endpoint generations, host revisions, isolate/Worker recovery, and drain
grants are implementation details.
The public facade schedules bounded turns automatically; a consumer does not
manually pump a driver.

The platform-neutral boundary below that facade is a byte endpoint. Native and
web adapters have the same bind, send, recover, poll/event, close, and failure
semantics. They own buffer transfer and platform startup only; they do not
interpret Markdown or create a second protocol state machine.

### 4.1 Native ownership

One long-lived Dart isolate owns the dynamic library, FFI calls, and endpoint
handle. Rust stores endpoints in a bounded generation-checked registry:

- the FFI handle is an opaque `{slot, generation}`, never a persistent pointer;
- each endpoint is externally serialized and `Send + !Sync`;
- stale handles cannot alias a reused slot;
- normal removal requires structural close and drain; and
- emergency destruction is containment, not normal lifecycle.

This is required because a Dart isolate is a logical serial owner but is not a
promise of permanent operating-system thread affinity.

### 4.2 Web ownership

Web uses an explicit external classic Worker owning one Wasm parser instance
and one worker-local endpoint slot. A separate main-context Wasm instance owns
the independent host/query store; parser and host authority are therefore not
collapsed merely because both compile from Rust. The core accepts explicit
runtime-neutral Worker and module URIs. `flark_flutter` may translate Flutter
asset packaging into that contract, but `flark` never imports Flutter asset
APIs.

The public runtime path is implemented and focused-browser verified under a
strict CSP. Exact transferred frames, stable scalar exports, proof-based close,
failure propagation, package asset identity, and bounded structural queries
use the same protocol and authority rules as native. A real browser receipt
replays a valid endpoint frame to induce a terminal Rust parser fault, observes
the public faulted state, invokes public `recover()`, replaces the endpoint,
streams an exact multi-page reseed, reaches current, queries the independent
host, and truthfully closes the real Worker. Exact root/Flutter publish archives
also boot this path from an external Chrome consumer. Physical JavaScript
Worker death and GC abandonment remain separate release gates.

## 5. Credited protocol, recovery, and close

The endpoint is a strict finite-state machine over bounded little-endian
frames. The current session schema is version 3. Every identity, revision,
generation, ordinal, source metric, and transported count is checked before
mutation or encoding; the v1 transport ceiling is `0xffffffff`.

Only one parser event is globally credited at a time across all event families.
The worker retains the event owner until an exact receipt is accepted. This
makes flow control, source lease ownership, publication ownership, and close
deterministic without unbounded Dart queues.

Two lifecycle rules are architectural, not incidental race fixes:

1. **Recovery drains retired frames.** Recovery advances to the exact next
   generation only from a quiet failed endpoint. A frame already queued from a
   retired generation is still structurally decoded and validated, then
   dropped without callback or receipt. The replacement handle cannot accept a
   receipt for the retired endpoint.
2. **Close is legal at any time.** A close request is idempotently latched. If
   an event has already reached the Dart driver, the driver processes and
   receipts it under the pre-close state, preserving its source/publication
   lease, and a later bounded `closeLatch` turn begins graceful close. If the
   credited event is still native-side or in transit, the native endpoint may
   latch close immediately; that latch invalidates the old non-close transition
   only after its exact receipt returns credit. At most one bounded post-close
   control command, normally the first drain grant, may wait for that credit.
   Commands deferred before the close latch are superseded and terminal
   `Failed`/`Closed` receipts never replay old work into another lifecycle.

The public runtime latches close intent synchronously before starting any
re-entrant shutdown work. Source mutation is rejected from that point, even if
the internal driver is still processing a pre-close event and has not exposed
its `closing` state yet.

The single deferred native command cell is a bound, not a general queue.
Ordinary source edits therefore emit no standalone `Supersede`. Dart revokes
host staging synchronously, edits coalesce while one source lease is live, and
only the next `SynchronizeSource` command crosses the endpoint after that
lease's credited event is receipted. Accepted source installation calls
`cancel_derived` before scanning the new source, so the stronger source
transition subsumes parser supersession. Any future caller of the retained
standalone `Supersede` wire command must independently prove command credit;
it cannot share an unseen-credit window with source synchronization.

Fault, supersession, close, and recovery may stop publication, but they cannot
silently change the exact Dart source. Completion is truthful only after the
native/worker endpoint is reclaimed; an unexpected worker exit is a failure,
not a successful close receipt.

## 6. Scheduling and liveness

Responsiveness follows from bounded ownership and scheduling, not from an
assumption that documents are small:

- the caller isolate performs bounded transaction, revision, query-adoption,
  and presentation scheduling work;
- native grammar work runs in its persistent isolate and web grammar work in
  its Worker;
- parser, `SourceFacts`, index, publication, query, and reclamation work consume
  explicit fuel and yield at bounded points;
- parse fuel measures aggregate accounted work, not document-shape-dependent
  parser phases: one exact-clean parse transition spends at most one 4 KiB
  quantum across discovery, explicitly charged line-state changes, and
  lexical/classifier work;
- the Web Worker begins each turn with a hard 32 KiB source grant, runs
  candidate ABI microgrants of 64 transitions, stops at a hard aggregate 4,096
  transitions or a four-millisecond target, and stops immediately on an event
  or zero progress. The hard grants, not the clock, bound an atomic overshoot;
- the session executor limits work per event-loop turn by both action count and
  elapsed time;
- newer revisions supersede obsolete work without starving the current one;
- supersession cancels provisional candidate authority without advancing the
  reusable SourceFacts base, so a zero-delay edit burst cannot strand Dart and
  Rust on different base revisions;
- active/visible queries may receive priority without changing grammar truth;
  and
- source, protocol, and derived roots are paged or persistent rather than sent
  as document-sized messages.

The expected common path is authoritative parser-to-presentation state before
the next paint. Exact-source fallback is the correctness backstop, not an excuse
for sustained visible lag. Floor-device input-to-paint, frame-tail, backlog,
and memory gates determine launch readiness.

## 7. Parser strategy

The production parser is Flark-owned and donor-correspondent:

- CommonMark plus the selected GFM/Flark profile is normative.
- Mature implementations such as Comrak, cmark-gfm, and Pulldown provide
  reviewed algorithms, provenance, fixtures, and differential oracles.
- Their mutable document trees and runtime state do not become Flark's session
  architecture.
- Parser control, persistent source, restart state, Green/projection output,
  semantics, and publication ownership form one coherent Flark document.

This avoids both long-term maintenance of a broad Comrak runtime fork and the
correctness risk of a Dart prediction layer. It is not a speculative rewrite of
Markdown from memory. The selected lexical donor is exactly Comrak `0.54.0`
(`172c2ee7d2c5c262a28be3e407aadf705daea2b7`), behind a small private facade.
Promoted controller/finalizer modules retain extraction hashes,
function-level provenance, differential tests, and explicit intake review when
that pin changes. The legacy bridge's Comrak `0.50` dependency is not the v3
parser baseline.

Production began with a **clean-but-resumable exact vertical** through the
final ownership and query seams. Its admitted structural subset now covers:

- empty documents with zero leaves;
- a total ordered partition of every nonempty blank-separated source into
  `Paragraph`, `Blank`, `DefinitionsOnly`, `FencedCode`, `IndentedCode`,
  `AtxHeading`, `SetextHeading`, `ThematicBreak`, `BulletList`, `OrderedList`,
  or typed `Unsupported` leaves;
- exact byte and UTF-16 leaf boundaries, including CRLF and Unicode; and
- both proven reference-definition terminal outcomes with leaf-local counts.

`BulletList` admission is deliberately narrow: one top-level, depth-one, tight
list with homogeneous `-`, `+`, or `*` markers, exactly one Paragraph in each
nonempty item, and an optional terminal empty item.

`OrderedList` admission is the corresponding narrow shape: one top-level,
depth-one, tight list with exactly one physical line and one Paragraph per
nonempty item, an optional terminal empty item, a homogeneous `.` or `)`
delimiter, and a 1–9 digit marker on every item. The exact mapping preserves
the list start, delimiter, each literal marker value, zero padding,
nonsequential ordinals, Unicode, CRLF, and an EOF terminal-empty item. Loose,
task, nested, multiline or lazy, mixed delimiter/type, container-wrapped,
multi-block, and 10-digit-marker shapes remain exact typed `Unsupported`
leaves instead of being approximated.

Admission occurs only after the selected exact block controller returns each
typed result. An unsupported shape remains an exact typed `Unsupported` leaf
and is exposed to the public structural query as source-backed `Unknown`.
Invalid definition-looking text that the normative parser leaves literal
remains Paragraph text. No regex, DFA, oversized-line special case,
enclosing-block guess, or legacy prediction parser decides grammar.

Even the initial Paragraph result requires the donor's complete competing
opener order: block quote, ATX, fence, HTML, Setext, thematic break, list,
indented code, and the selected GFM Table detector. A handler whose output is
not admitted may terminate in typed `Unsupported`, but its detector cannot be
removed or replaced by a Paragraph classifier. Until the source-backed Table
competitor is composed, every table candidate fails closed as `Unknown` rather
than being silently admitted as Paragraph.

The structural widening proves clean segmentation, publication, point query,
platform parity, and leaf handoff. Definition-free segmented top-level
documents now also have authenticated interior, BOF, and EOF restart, bounded
parser-only crop, and exact-base splice. An interior edit may lie in a
Paragraph, ATX content, a Paragraph↔Setext H1↔H2 transition, a
Paragraph↔thematic-break transition, or a closed fenced-code body; boundary
crops admit a first or final Paragraph edit and
authenticate the retained ordinary suffix or prefix. Every checkpoint collection
carries the exact top-level block count;
the prior loose segmented flag is no longer topology authority. All routes
remain capped at 64 KiB. Parser split/merge receipts prove that inserting or
deleting a blank boundary changes the exact block count and relevant ordinals
by +2 or -2 without losing topology. Edits to or through definitions,
definition-bearing BOF, any typed `Unsupported` leaf, missing anchors, an
over-cap crop, or an unclosed fence that consumes convergence authority take
the definitive exact-clean fallback. This does not prove restart through
containers or other block grammar. The hot-inline sibling publication certifies
strong, emphasis, code, strikethrough, accepted angle autolinks, ASCII escaped
punctuation, hard line breaks, character references, and direct plus
full/collapsed/shortcut reference links/images with parser-cooked destinations
and titles for one selected Paragraph, ATX-content, or Setext-content leaf at a
time, including a nonzero middle leaf and a small visible tail behind a large
definition prefix. The retained exact publication owns the reference winner
index and lends cloneable root-bound resolver capabilities to individual leaf
jobs; no leaf consumes the revision's reusable resolver authority. Setext inline content
excludes exactly the terminal content-line EOL while retaining earlier content
line endings as semantic softbreaks. Grammar coverage and simultaneous
visible-leaf materialization expand over the same authority rather than adding
another parser.

## 8. Persistent representation and publication

The worker document ultimately owns persistent indexed revisions of:

- segmented source and relative/composable `SourceFacts`;
- packed source-ordered Green pages;
- physical-to-logical projection runs;
- exact restart/checkpoint state;
- block and inline facts;
- reference occurrence, winner, dependency, and cooked-value indexes; and
- publication roots and reclamation ownership.

One self-contained manifest binds the certified source and every role root.
Records are encoded into canonically digested frames, and one credited `FPK3`
packet transfers one or more consecutive frames without materializing
per-frame Dart objects. The packet has a fixed 44-byte aggregate header,
24-byte directory entries, at most 256 frames, at most 64 KiB of aggregate
frame bodies, and a maximum raw size of 71,724 bytes. The offer independently
bounds each encoded frame to 5,140 bytes.

Dart performs constant-time packet-envelope checks before the synchronous
native or Web host call. Rust incrementally validates the frame directory,
bodies, record totals, and digests before returning one packet credit naming
the next frame ordinal. Commit authenticates the actual frame count and actual
encoded frame bytes. Native and Web adapters use one 71,724-byte bulk scratch
region synchronously for either packet admission or bounded query output; they
do not retain the caller's Dart view. The independent host installs nothing
until the complete candidate validates. Packet delivery is not installation
acknowledgement, so the worker retains its offered owner until the exact
installation receipt.

Reusable canonical pages exclude ephemeral publication, source-root, revision,
and worker-generation identity. Fresh role wrappers bind reused pages to the
target certified source, and a fresh manifest binds those roles to the target
publication. Direct immutable edges may be reused; retaining prior manifests as
recursive ancestry is forbidden.

The first structural sequence is a persistent measured tree whose pages contain
at most 64 semantic entries. Each entry carries composable byte, UTF-16,
reference-count, and lane-commitment measures. Green and Projection have
separate role wrappers and commitments but share the same immutable block root,
so publication does not duplicate physical structure. The independent host
validates that pairing before install. Exact-clean construction now uses the
bounded persistent packed-page cut/splice path rather than a separate flat
assembly route. The same splice accepts a top-level interior crop containing
Paragraph, ATX Heading, Setext Heading, fenced-code, and Blank replacement
leaves, including Paragraph↔thematic-break replacement, when ordinary
Paragraph checkpoints authenticate both boundaries, plus definition-free
first- and final-Paragraph crops from BOF or to EOF.
Interior checkpoints are definition-free by default; after an exact clean
parse they may instead freeze the exact reference-definition count, but only
when every retained definition lies before every usable restart. Broader
grammar and definition mutation restart/convergence remain open.

A point query supplies byte and UTF-16 coordinates plus boundary affinity.
Both coordinate systems must select the same leaf; affinity decides which
neighbor owns an exact boundary. The host returns exact absolute byte/UTF-16
leaf ranges and leaf-relative records. Its preflight budget and actual receipt
share the derived `3h + 1` maximum for a tree of height `h`, including role
wrappers and packed-page inspection.

Point and range queries are separate lanes. The point lane resolves one
caret/selection target. The structural range lane resolves consecutive
top-level blocks for a source interval: it performs one point seek, then walks
authenticated consecutive pages rather than repeating a root lookup for every
block. Native and Wasm share the fixed `FLKVR001` packet and an opaque
continuation bound to the exact structural ACK; same-source structural
republication invalidates that continuation as typed pending instead of
silently resuming against a different tree.

The public default range quantum is 4,096 encoded bytes, 24 blocks, 25 visited
pages, depth 16, and 320 tree nodes. The 25-page allowance is deliberate:
persistent splice history guarantees page capacity but not a lasting density
invariant, so the worst admitted 24-block window may occupy 24 sparse leaf
pages after its initial seek. A giant top-level block remains one structural
record; this lane bounds enumeration and transfer, not the size of one
structural block's source envelope.

For authenticated incrementality, `SourceFacts` pages contain relative
page-local facts and composable subtree summaries. Composition carries UTF-8,
UTF-16, line, split-CRLF, and rolling-hash boundary state. The current
implementation has that relative canonical page algebra, structurally
committed exact-base splice path, and fresh-wrapper publication; the absolute
clean-scan form remains only as a derived M1.1 transport projection.

SourceFacts storage-page alignment is deliberately not parser crop authority.
Before page widening, retained edit lineage derives a private exact parser edit
envelope. Adjacent edits compose through it with explicit boundary affinity;
a later distant edit drops the narrow envelope and uses the existing wider or
clean fallback. In a definition-free segmented top-level document, the parser
expands only that exact envelope to the preceding and following authenticated
ordinary Paragraph checkpoints and rejects a crop larger than 64 KiB. A clean
definition-bearing result now drops every ordinary checkpoint at or before its
last definition-bearing leaf and stamps every surviving checkpoint with the
exact total definition count. A crop strictly after that frozen prefix uses the
same bounded lane; a crop that accepts a new definition fails closed. At
production 4,096-UTF-16 SourceFacts spacing, the Paragraph endpoint proof parses
4,116 bytes / 168 lines in three parser transitions and publishes an
`ExactBaseDelta` replacing 8 of 16,386 records. A separate 671,794-byte fixture
with 4,096 Paragraphs around an ATX Heading and fenced-code block proves that an
interior ATX-content edit and then a fence-body edit both enter
`ParsingOrdinaryExact`, publish successive `ExactBaseDelta` revisions, and
remain exact on native and Chrome. Removing the closing fence loses
convergence and falls back to definitive exact-clean publication. A separate
4,096-Paragraph fixture lengthens the first block and proves the segmented BOF
route: `ParsingOrdinaryExact` publishes `ExactBaseDelta`, transfers at most 64
canonical records and at most 64 nonempty block-replacement records, preserves
exact first/middle/last geometry, reuses downstream restart checkpoints, and
retains exact-base authority through revision 3 on the shared public
native/Chrome path. Its EOF twin lengthens the final block through the same
`ParsingOrdinaryExact` -> `ExactBaseDelta` route, bounds both transferred and
nonempty block-replacement records to 64, preserves exact first/middle/last
geometry, retains authenticated prefix checkpoints, and correctly mints zero
fresh checkpoints beyond EOF before reaching public revision 3. The exact
authenticated top-level block count replaces a loose segmented-state flag.
Parser split and merge cases adjust that count and the relevant convergence
ordinal by exactly +2 or -2. A frozen definition prefix remains reusable for a
safe ordinary EOF crop. A new tail definition, typed `Unsupported` tail, or
over-cap crop fails clean to definitive parsing, as does lost convergence.
Definition-bearing BOF and general definition edits remain open.

A separate 4,096-block definition-free Setext fixture transitions one local
middle block Paragraph→H1→H2→Paragraph. Every phase enters
`ParsingOrdinaryExact`, publishes `ExactBaseDelta`, transfers and replaces at
most 64 records, preserves exact first/middle/last queries, and leaves exact
authority for the next revision. A same-block Paragraph→Setext promotion whose
prior content exceeds 4 KiB intentionally rejects the narrow restart and
finishes through exact-clean publication; this prevents reuse of a checkpoint
that predates the Setext underline. Setext uses structured block role variant 5.

A separate 4,096-block definition-free thematic fixture promotes and demotes
one middle Paragraph through one bounded crop. The crop discovers at most
16 KiB, remains within ordinary checkpoint authority, and its persistent block
splice deletes and replaces at most 64 records. The exact-clean and crop paths
publish `ThematicBreak` through structured role variant 6 with exact marker
kind/count, opening indent, BOM flag, marker envelope, and line-ending
geometry. Its visible and projected spans are both empty and its projection has
zero runs, so consumers render one atomic divider without placing marker bytes
in editable text.

Exact-clean `IndentedCode` publishes structured role variant 7. Its compact
Green summary carries the fixed four-column deindent recipe, BOF BOM fact,
physical line count, projected UTF-8 and UTF-16 lengths, and terminal EOL
width; its structural Projection covers the full physical source, exposes an
empty visible span, and records the line count. A selected exact leaf then
separately demands an at-most-8-KiB projection job. The job reuses the same
segmented lexical scan as the block controller, rather than recognizing
indentation again, and returns canonical 20-byte records for each physical
line through viewport schema 3. Those records distinguish hidden prefix,
source-backed content, physical EOL, and parser-certified internal blanks. A
synthetic final LF is projection metadata only when needed to complete the
parser recipe; it is not source or an editable caret position. Mirrored real
native and Chrome runtimes agree on the variant-7 summary and demanded
projection. Focused lifecycle evidence cancels in-flight derivation,
fuel-releases a completed projection root, and closes its retained publication
and runtime with zero resident nodes, live builds, or reserved external payload
bytes. This proves exact-clean structure and bounded selected-leaf projection;
authenticated restart/convergence for edits inside an indented-code block
remains unproven.

`BulletList` publishes structured role variant 9 with the marker,
item count, optional terminal-empty item, Paragraph count, and projected
lengths. The established selected-list vertical separately demands viewport
schema 5, whose canonical 28-byte item records carry each physical item source
range, certified hidden prefix, exact continuation/removal subspan,
source-backed content, and physical EOL. The payload also carries the exact
selected-item path, canonical source projection, and parser-authored editing
inputs for continuation, terminal-empty Enter exit, and column-zero Backspace.

Local tight-list edits require neither intra-list checkpoints nor list-wide
parsing.
The persistent source rope answers line rank/select queries for independent
base and target predecessor/changed/successor windows. A fuelled local parser
validates only those bounded windows, produces the corresponding compact list
summary replacement, and hands it to the existing exact block-tree splice. The
20,000- and 100,000-item endpoint receipts both require 295 target local-parse
transitions; build takes 18 and 21 transitions respectively, streaming takes
20 in both, and publication transfers a fixed four records in two packets
after examining 262,149 SourceFacts bytes. Exact clean-oracle parity, Unicode
and CRLF, cancellation/base restoration, two consecutive `ExactBaseDelta`
edits from an underfilled 109-checkpoint/three-page persistent topology, and
close-to-zero are covered.

The ordered-list lane shares that kernel without weakening its grammar
boundary. Its 20,000-item Unicode/CRLF middle-edit receipt discovers and parses
only three base and three target physical lines, matches an independent full
clean parse, survives cancellation and a second sequential edit, and keeps
window bytes below one-thousandth of the document. Nine-digit marker edits stay
local; a 10-digit marker, delimiter change, first/last-item boundary edit, or
stale authority takes a typed fail-closed path.

Selected-item materialization is being narrowed independently. Geometry is
demanded first, using the parser-selected item and one compact schema-6 item
record; inline facts are demanded only afterward for that exact content range.
This preserves one grammar authority while avoiding an item-count-sized
projection. The combined path is green: rebuilt-Wasm/freshness is 2/2,
public-runtime semantic parity is 1/1 on Chrome, and the managed compact
BulletList batch is 3/3 on native Flutter and 3/3 on Chrome. The first combined
Chrome run had one transient timeout; its immediate isolated rerun and the full
rerun both passed, so this receipt does not establish a deterministic
performance budget.

`OrderedList` publishes structured role variant 10. Its separately demanded
viewport schema 7 uses the distinct payload kind 6 and returns one constant
selected-item projection: 20 bytes of ordered-item metadata followed by the
canonical 28-byte item record, not a list-sized payload. The metadata carries
the selected ordinal, canonical EOL, exact opening-marker span, and literal
marker value. Geometry is installed first; inline facts are demanded only
afterward for that exact content range.

Reference-root reuse has an additional coordinate invariant. Canonical
reference records currently contain absolute byte and UTF-16 ranges, so an
exact unchanged suffix does not by itself make reuse safe after an earlier
length-changing edit. Reuse with a nonzero coordinate delta therefore requires
typed authority proving every retained definition lies in the exact unchanged
prefix: either the existing leading-remainder checkpoint or an ordinary
checkpoint strictly after the last definition-bearing leaf with the exact
frozen count. A 2,048-definition / 2,048-Paragraph endpoint fixture applies a
length-changing middle Paragraph edit, publishes `ExactBaseDelta`, transfers at
most 64 records, retains all 2,048 References, preserves exact first, edited
middle, and shifted-last Paragraph queries, and retains exact-base ordinary
authority for the next revision. The shared public Dart fixture then completes
a second edit at revision 3. This does not make definition edits incremental:
the dense 8,192-Paragraph late-definition regression still proves that a
length-changing edit before the last definition must fail closed to a fresh
`FullSnapshot`.

## 9. Dart query and Flutter adapter boundaries

The Dart engine exposes lightweight revision snapshots and bounded queries,
not a mandatory document-wide AST. Core coordinates are Flark-owned values;
UTF-16 interoperability is explicit alongside byte, scalar, line, and
projection metrics. One-shot full parse/export conveniences are honest
`O(document)` operations implemented over the same engine.

`flark_flutter` consumes current-revision query facts and owns:

- `TextInputClient`, `TextEditingDelta`, IME, and platform adaptation;
- a bounded marker-free active input island with exact source/display mapping;
- viewport layout, shaping, painting, hit testing, and selection geometry;
- frame scheduling, widget lifecycle, themes, and controllers; and
- accessibility materialization and Flutter asset conveniences.

The managed binding now queries by the current caret/selection point and adopts
the exact returned leaf range rather than guessing an enclosing block. Focused
receipts cover a Paragraph's exact source/display handoff and a
`DefinitionsOnly` leaf's empty projection with a collapsed caret. A real
three-Paragraph fixture now moves the same bounded input client among first,
middle, and tail leaves and adopts each leaf's marker-free parser-certified
projection. A bounded current-revision Dart cache retains decoded inline facts
for up to 128 leaves / 2,048 fact records after the singleton host sidecar moves.
Escaped punctuation hides only the certified backslash, carries no semantic
style, edits as one source atom, and remains marker-free across passive-to-active
handoff.
Grammar revision 5 adds `HardLineBreak` as inline projection kind 8. Rust is
the sole recognizer: neither Dart nor Flutter scans trailing spaces or
backslashes to infer the construct. For an admitted odd-backslash or
at-least-two-space form, the fact covers the complete marker plus physical
line ending, its content is the exact LF, CR, or CRLF bytes, and its closer is
collapsed. Marker-free presentation hides only the certified marker and maps
all three physical endings to one display newline while canonical source and
export retain the exact marker and ending. Replacing or deleting that displayed
newline expands to the complete marker-plus-ending source atom; insertion at
its certified boundary is authorized without splitting the atom. If an
unshielded candidate's next physical line begins with continuation indentation,
the Rust parser fails the whole inline leaf closed rather than letting another
layer guess. Terminal markers remain literal.
Grammar revision 6 adds `CharacterReference` as inline projection kind 9
without changing the schema-2 `IFO2`/`IFP2` family or its 20-byte fact record.
Rust alone recognizes and cooks the exact named, decimal, or hexadecimal
source token into one or two Unicode scalar values; invalid and unterminated
candidates remain literal, and code spans remain opaque. Dart validates the
certified source range and scalar payload, then mechanically derives
marker-free replacement text and source/display maps. It performs no entity
recognition. URI autolinks retain a nested kind-9 fact: this replaced the
rejected approach of independently decoding or rejecting entity-bearing URIs,
and mechanically derives both the cooked label and destination from the same
parser-authored value. Editing any portion of a cooked replacement consumes
the complete source token, insertions preserve untouched cooked scalar
prefixes/suffixes literally, and UTF-16 endpoints inside a surrogate pair are
rejected.
Grammar revision 7 adds `DirectLink` and `DirectImage` as inline projection
kinds 10 and 11 without widening the fixed fact record. The persistent inline
role becomes the explicit two-root `IPB5` bundle: child zero is the existing
fixed-width fact tree, and child one is the optional variable-width link-value
tree. The latter replays as `FLKIV001` with a 16-byte header,
fact-ordinal-keyed entries, exact destination/title source cuts, and
parser-cooked UTF-8 values. Both roots are authenticated and admitted together;
there is no independently inferred value order. Facts and values retain
separate 64 KiB ceilings, the combined selected-leaf query is capped at 128
KiB, and a default public viewport page is capped at 256 KiB.

While a source edit invalidates exact structure and viewport authority, Flutter
retains the last exact bounded passive pixels, render objects, and geometry and
keeps the one active `EditableText`/input client on its mechanically updated
projection. This is stable paint, not semantic authority: stale hit testing,
link actions, and accessibility semantics fail closed until exact authority
returns. The product checkpoint also latches its compact ready shell after
first readiness, so transient parser work cannot expand diagnostics or resize
the editor. This policy removes marker and layout flicker without turning a
stale projection into a second Markdown parser.
ATX and Setext Headings use the same generic Heading Dart contract and cross
native, Chrome, managed Flutter, and the runnable demo with parser-authored
heading typography and hidden certified markers. Setext's projection excludes
only the terminal content-line EOL, preserves internal line endings as
softbreaks, and hides the certified underline. Indented code uses the separately
demanded schema-3 physical-line projection to hide parser-certified
four-column prefixes while retaining residual indentation, literal
Markdown-looking content, exact line endings, and internal blank lines. The
real managed checkpoint maps Enter to one canonical line ending plus the
four-space continuation prefix, then adopts the exact current-revision
variant-7 result without replacing the `EditableTextState` or platform input
client. A thematic break uses exact
boundary affinity to collapse the active island before or after the atom,
paints one parser-certified divider over an empty `EditableText` projection,
and handles Backspace or Delete as whole-source-atom deletion without replacing
the `EditableTextState` or platform input client.

The narrow BulletList adapter currently uses the established schema-5
selected-item record rather than classifying list syntax in Dart. It paints the
certified marker in the gutter while keeping that marker and prefix out of
editable text, maps the display selection to exact source, hands the same
bounded island between items, inserts the parser-authored canonical
continuation on Enter, exits from a terminal empty item, and removes the exact
authorized prefix on column-zero Backspace. The canonical document retains
every marker, indentation byte, Unicode scalar, and CRLF even though the
selected item is marker-free. Its compact successor explicitly demands
schema-6 geometry before selected-content inline facts; that combined adapter
path is green through rebuilt-Wasm/freshness 2/2, Chrome semantic parity 1/1,
and managed compact batches 3/3 on native Flutter and 3/3 on Chrome.

The narrow OrderedList adapter consumes the distinct schema-7/payload-kind-6
selected-item projection and retains the same geometry-then-inline ordering.
The marker is paint-only: the gutter paints the parser-certified literal marker
while the same input client's editable projection omits it, and the canonical
source retains it byte for byte. In the focused zero-padded case, editing
`007) alpha` shows `alpha` in the input island with `007)` in the gutter; Enter
produces canonical CRLF source containing `007) al` followed by `008) pha`,
then paints `008)` without replacing the `EditableTextState` or platform input
client. The ordered vertical is green through native parser/endpoint and host
receipts, the public Dart decoder/demand path, managed Flutter, and the focused
Chrome engine-lab gate.

The Dart visible-block materializer consumes at most one bounded range quantum
per `advance`, retains an opaque continuation, and hard-caps a window at 256
blocks. The Flutter coordinator translates a layout source range into that
pure-Dart demand and performs at most one quantum per frame. A real Chrome
checkpoint over the 4,096-reference fixture reaches an exact visible range
before and after a marker-free edit. This proves bounded structural
materialization, not a virtualized whole-viewport editor: height indexing,
layout, shaping, and multi-block paint remain Flutter work.

A mixed 4,096-Paragraph Flutter checkpoint now moves one stable
`EditableTextState` and platform input client from marker-free ATX content to
the literal fenced-code body, edits and recertifies both, and remains green on
native and Chrome. It proves the selected-island product behavior over the
mixed structural slice; the Rust endpoint receipt, not the widget test alone,
proves bounded incremental parser work.

Flutter types never cross into `flark`. The adapter does not own source
certification, parser synchronization, canonical selection/source authority,
or Markdown classification. Parser-safe boundaries are not assumed to be
shaping-safe; bidi, ligatures, joining scripts, wrapping, and accessibility
retain device acceptance gates.

## 10. Rejected production shapes

Implementation must not reintroduce:

- Dart-side prediction or presentation-side Markdown classification;
- enclosing-block reparsing presented as generally exact incrementality;
- a mutable document-wide AST shared across revisions;
- document-sized bridge payloads or ordinary-edit materialization;
- synchronous enumeration of unchanged suffixes or semantic fanout;
- mixed-revision publication or active stale semantic actions;
- per-block editable widgets as document storage/input;
- main-thread web parsing for the large-document path; or
- a Flutter dependency in the core engine.

## 11. What is proven and what remains

The following decisions have executable support today:

- the Dart-only `flark` / dependent `flark_flutter` package direction;
- persistent Rust source/runtime ownership and bounded source replication;
- schema-3 cross-language `SourceFacts` pages, terminal proof, and exact
  provisional-to-known promotion semantics, with structural host commit as the
  separate reusable-base promotion boundary;
- one-credit endpoint FSM, deferred close, and retired-generation frame drop;
- generation-checked native registry and FFI ABI;
- a long-lived native isolate with detached main-port cleanup, native emergency
  finalizer tokens, truthful disposal completion, recovery, immediate and
  credited close, terminal deferred-command invalidation, and large provisional
  source certification;
- one-command ordinary-edit pacing in which rapid edits coalesce behind the
  live source lease and a real native receipt proves only the latest source is
  synchronized and certified. Zero-delay browser supersession is covered
  without allowing an uncommitted certification to become the next delta base;
- a production `flark-parser` exact-clean boundary that consumes only the
  pinned Comrak `0.54.0` lexical facade, preserves the complete root-opener
  order, uses one segmented lexical/reference path at every source size,
  accounts one shape-independent 4 KiB aggregate quantum across source and
  lexical work, and returns typed `Unsupported` leaves whose public form is
  source-backed `Unknown` rather than guessing. It now totally partitions
  blank-separated input into exact
  byte/UTF-16 `Paragraph`, `Blank`, `DefinitionsOnly`, `FencedCode`,
  `IndentedCode`, `AtxHeading`, `SetextHeading`, `ThematicBreak`, `BulletList`,
  and `Unsupported` leaves.
  Thematic breaks publish exact atomic marker facts and an empty projection
  through structured role variant 6. Differential receipts
  cover 20,000 lines and 10,000 reference prefixes; giant, ordinary-line,
  newline-dense,
  CRLF/Unicode, fuel-partition, 10 MiB, and
  early-cancellation/supersession witnesses retain bounded state;
- an actual exact-clean parser-to-host ownership vertical: ordered multi-page
  SourceFacts plus Green, Projection, References, and CleanEof roles stream as
  a self-contained unique-postorder DAG into a fresh host arena, undergo
  canonical validation, atomically replace the root, and answer bounded
  per-record queries without document-scale revalidation;
- that ownership vertical crossing both credited production endpoints into
  independent native and main-context-Wasm host stores, including source
  certification, exact parser work, time-sliced candidate polling, multi-role
  frame/packet publication, causal install acknowledgement, edit-derived
  replacement, latest-revision supersession, and truthful close;
- a packed measured structural tree with at most 64 semantic entries per page,
  paired Green/Projection wrappers sharing one root, exact byte/UTF-16
  affinity-aware lookup under the derived `3h + 1` bound, exact-clean packed
  block-page splicing, and an anchored segmented top-level interior restart
  whose ordinary Paragraph checkpoints bracket Paragraph, ATX-content,
  Setext-transition, Paragraph↔thematic-break, and fenced-code-body edits of at
  most 64 KiB while the exact edit envelope remains independent of SourceFacts
  page alignment. Admitted BulletList edits instead use checkpoint-free
  source-rope rank/select to derive bounded predecessor/changed/successor
  windows in the base and target, then publish through the same exact block
  splice without scanning the whole list.
  Checkpoints strictly
  after the last definition-bearing leaf also carry the exact frozen
  definition count;
- separate authenticated point and structural-range query lanes. The range
  lane uses one seek plus a consecutive page walk, the fixed `FLKVR001` packet,
  and an opaque structural-ACK-bound continuation. Its default
  4,096-byte/24-block/25-page/depth-16/320-node quantum is consumed one
  `advance` at a time by Dart, with a hard 256-block window cap, and at most
  once per Flutter frame. The 4,096-reference Chrome checkpoint reaches exact
  range state before and after a marker-free edit;
- the production-spacing Paragraph endpoint crop examining 4,116 bytes / 168
  lines in three parser transitions and replacing 8 of 16,386 records through
  `ExactBaseDelta`; the public roughly 3.2 MB 100,000-Paragraph replacement
  completes in 20.994-26.977 ms native and 29.9-30.1 ms Chrome with foreground
  work below 8 ms;
- the 671,794-byte, 4,096-Paragraph mixed public fixture reaching exact
  native/Chrome revisions after both an interior ATX-content edit and an
  adjacent fence-body edit, with the endpoint proving
  `ParsingOrdinaryExact` -> `ExactBaseDelta` for each and clean fallback after
  removal of the fence closer destroys convergence;
- the 2,048-definition / 2,048-Paragraph reference-frozen fixture publishing a
  length-changing middle edit through `ExactBaseDelta` with at most 64
  transferred records, all References retained, exact first/middle/last
  queries, and a public next-revision edit reaching exact current structure;
- the 4,096-Paragraph segmented BOF fixture publishing a length-changing first
  block through `ParsingOrdinaryExact` -> `ExactBaseDelta`, transferring at
  most 64 canonical and block-replacement records, preserving exact
  first/middle/last geometry, reusing suffix checkpoints, and reaching exact
  public revision 3;
- the 4,096-Paragraph segmented EOF twin publishing a length-changing final
  block through the same delta route, bounding transfer and replacement to 64
  records, retaining prefix checkpoints, minting zero correctly fresh EOF
  checkpoints, preserving exact first/middle/last geometry, and reaching exact
  public revision 3; plus parser-level blank-boundary split/merge topology
  receipts proving exact +2/-2 block-count and ordinal changes;
- a bounded current-revision inline cache that reattaches parser-certified facts
  after the independent host's singleton sidecar moves, while invalidating
  atomically on edit or authority replacement;
- an ATX Heading vertical with exact level, opener, marker-free content,
  optional accepted closer, indentation/BOM validation, and CRLF-preserving
  source geometry through parser, publication, independent host, public Dart,
  native and Chrome parity, real Flutter, and the runnable demo;
- a Setext Heading vertical using structured role variant 5 and the generic
  public Heading API, with exact H1/H2 underline geometry, marker-free multiline
  content that excludes only its terminal EOL while retaining internal
  softbreaks, native/Chrome parity, real Flutter, and the runnable demo. Its
  4,096-block Paragraph↔H1↔H2 receipt stays within 64 transferred/replacement
  records per local revision, while an over-4-KiB same-block promotion falls
  back cleanly;
- a ThematicBreak vertical using structured role variant 6, with exact
  marker/count/indent/BOM/envelope/EOL facts and a zero-run empty projection
  through independent-host queries and shared native/Chrome public semantic
  parity. Its 4,096-block Paragraph↔thematic-break crop stays bounded and the
  managed Flutter adapter preserves affinity, paints one semantic divider, and
  deletes the whole canonical atom with Backspace or Delete on the same input
  client;
- an IndentedCode exact-clean and selected-leaf vertical using structured role
  variant 7, with exact four-column/BOM/line-count/projected-length/terminal-EOL
  summary facts followed by a separately demanded schema-3 payload of canonical
  20-byte physical-line records. Real native and Chrome runtimes agree on the
  source-backed marker-free projection; the real managed Flutter checkpoint
  preserves code typography, maps Enter to canonical indentation, and
  recertifies on the same `EditableTextState` and platform input client.
  In-flight derivation cancellation, fuelled root release, and zero-residency
  close are covered independently;
- a narrow top-level depth-one tight BulletList structured and selected-item
  vertical using structured role variant 9 and the established separately
  demanded viewport-schema-5 payload of canonical 28-byte item records. The
  managed item projection is marker-free while the canonical source remains
  exact, including Unicode and CRLF; item handoff, canonical Enter
  continuation, terminal-empty Enter exit, and exact column-zero Backspace
  prefix removal use parser-authored inputs. Loose, task, nested, mixed-marker,
  and multi-block BulletList forms remain fail-closed. Its new local
  delta lane uses source-rope line rank/select rather than list-wide parsing:
  the 20,000- and 100,000-item fixtures both spend 295 target local-parse
  transitions, build in 18/21, stream in 20, and transfer four records in two
  packets after 262,149 SourceFacts bytes. The result matches a clean oracle,
  survives two consecutive `ExactBaseDelta` edits after a
  109-checkpoint/three-page underfilled topology becomes the base, restores
  lifecycle authority correctly, and closes to zero. The existing schema-5
  current-byte native and Chrome managed gates, focused Chrome engine-lab
  checkpoint, and visual release-demo inspection remain green. Compact
  schema-6 selected-item geometry followed by a separate inline demand is also
  green through rebuilt-Wasm/freshness 2/2, Chrome semantic parity 1/1, and the
  managed compact batch 3/3 on both native Flutter and Chrome;
- a narrow top-level depth-one tight OrderedList vertical using structured role
  variant 10 and one separately demanded viewport-schema-7, payload-kind-6
  selected-item projection. It preserves `.`/`)` delimiters, 1–9 digit literal
  marker values, zero padding, nonsequential ordinals, Unicode, CRLF, and a
  terminal empty item. Geometry is demanded before inline facts. Native,
  public-Dart, managed-Flutter, and focused-Chrome receipts cover paint-only
  marker hiding, exact source preservation, and same-client `007)`→`008)`
  continuation. Its 20,000-item local-delta receipt remains bounded to three
  base and three target physical lines and matches the clean oracle. Loose,
  task, nested, multiline/lazy, mixed delimiter/type, container-wrapped,
  multi-block, and 10-digit-marker forms remain fail-closed;
- exact Flutter leaf handoff, including the definitions-only empty projection
  and collapsed-caret case, without replacing source or parser authority;
- a pure-Dart document-owning `FlarkV3DocumentRuntime.open` facade with automatic
  bounded execution, binding derivation, one-shot exact-structure readiness,
  semantic source/certified/structure revision status, synchronous bounded
  structural queries, small apply/undo results, exact-source range and cold
  export access, recovery, and truthful close. A schema-3 in-protocol `Failed`
  event settles initial readiness with a typed recoverable failure. The normal
  barrel exports no host/session/certification/transport choreography and also
  compiles and executes on JavaScript without importing Flutter.

The grammar-revision-6 milestone receipts were: the full Rust workspace
all-targets suite; the focused 60/60 Dart core gate including digest parity; the
17/17 Flutter active/source gate; the native/Chrome public-runtime
character-reference edit; and the 3/3 Chrome asset/reopen gate. Their rebuilt
Worker/Wasm asset version was
`6716b6530eb4b989-7d0c661b2b6c0cfe-bba3dc0f34f51964`, with Wasm SHA-256
`6716b6530eb4b989315123b631420c27e733422ae45865daf4f68204cedc2cd4`.
Those historical receipts cover character-reference cooking, fixed-record
engine/host validation, nested URI-autolink cooking, atomic edit planning, and
surrogate-safe active editing. Prior broad receipts also include the
671,794-byte mixed public fixture on native and Chrome, the 4,096-Paragraph
marker-free same-client Flutter checkpoint on both platforms, and the
preceding-build standalone 100,000-reference Chrome product receipt above.

The grammar-revision-7 proof remains the historical receipt for the dual-root
`IPB5`/`FLKIV001` publication and query path, strict Dart joining, marker-free
direct-link/image projection, passive direct-media presentation, and the
stable-paint pending policy. The virtualized-surface focused suite is green
10/10; the Chrome live
checkpoint is green 2/2 for passive and active direct link/image presentation,
whole-label replacement, insertion at the final visible link-label boundary,
image-alt replacement, exact hidden destination/title preservation, the
existing live autolink edit, one stable input client, and the first-frame
flicker regression. The nonzero-value Dart wire-codec gate is green 6/6, and
focused native and Web direct-media runtime gates are green 1/1 each. The
packaging/freshness gate is green 12/12. The Rust workspace all-targets release
build and Wasm rebuild are green. Root and Flutter Wasm bytes and buildinfo are
identical at asset version
`a868f652dbdd5e5d-5f412bffe731e227-bba3dc0f34f51964`, with Wasm SHA-256
`a868f652dbdd5e5d22431e4e5d5401ea5c46855e5b02a905077ade9a1adb55f7`.
These are current functional, freshness, and parity receipts, not new
performance measurements. The prior active-sidecar failure was a cross-language
Begin-layout defect hidden by zero-value fixtures: Rust encoded link-value
entry count, encoded bytes, then storage-page count, while Dart decoded the
last two fields in the opposite order. Dart now matches the Rust writer and
reader, and a byte-exact nonzero fixture prevents recurrence. The 100 MiB
viewport, reference-link and broader grammar, floor-device, accessibility, and
launch gates below remain explicit.

### Grammar revision 8 evidence addendum

Revision 8 is a narrow strict-GFM bare-autolink addendum to revision 7.
It admits only exact lowercase `http://`, `https://`, and `ftp://` URI
prefixes; boundary-gated lowercase `www.` with a dotted domain; and an ASCII
`[A-Za-z0-9.+_-]+` email local part followed by a dotted domain. URI and
`www.` recognition precede email recognition, and terminal trimming matches
GFM examples 621–631. Markerless exact-source facts select `exactContent`,
`httpPrefixedExactContent`, or `mailtoExactContent`; they do not store a
separately cooked bare-autolink value.

The whole-leaf stage is resumable and fuelled. It uses a source-relative range
cursor with 256-byte source reads, charges token bytes before synchronous
classification, and caps a candidate token at 8 KiB. Code, angle autolinks,
direct links/images, and bracket context shield candidates. An overlong token,
unknown or unresolved bracket context, overlap, or invalid state makes the
whole leaf unsupported with no partial bare-autolink facts. Explicit
`mailto:`/`xmpp:`, uppercase URI/`www.` prefixes, relaxed autolinks,
reference/collapsed/shortcut links, and full CommonMark/GFM remain outside this
admission.

The same-ordinal activation path is now idempotent when the exact ordinal is
already active and no activation is pending. This prevents a redundant handoff
from replacing the certified projected value with canonical source when
authority and caret intent have not changed. The current receipts are Rust
exact-clean 46/46, promotion audit 2/2, and engine 251/251.
`cargo test -p flark-parser` is green: 309 non-doc tests and one compile-fail
doctest passed, three manual scaling receipts were ignored by design, and zero
tests failed. The remaining gates are packaging 12/12, freshness 2/2, Dart
inline facts/projection 68/68, native sidecar end-to-end 7/7, Web Chrome
sidecar end-to-end 3/3, Flutter presentation/surface 24/24, the example Chrome
checkpoint 3/3, and the focused exact bare-classifier large-paragraph gate 1/1.

Root and Flutter assets are byte-identical at version
`dfcce276df7954a9-714e23750091d226-bba3dc0f34f51964`. The Wasm is
3,506,644 bytes with SHA-256
`dfcce276df7954a97a11f3faef4f93217adddba0d4b620db5e4942a8a2e4c930`;
the Worker is 33,195 bytes with SHA-256
`bba3dc0f34f51964fe55bf67363b75fdc68a1387ce28f1771529c44ad7493a60`.
These are functional and identity receipts, not full-grammar, release,
floor-device, or new timing evidence.

### Grammar revision 9 evidence addendum

Revision 9 makes full, collapsed, and shortcut reference links and images
parser-certified. Inline fact kinds 12 and 13 remain additive within the
existing `IFO2`/`IFP2` and `FLKIN002`/`FLKIP002` record family and reuse the
authenticated `FLKIV001` companion-value root. The grammar revision advances,
rather than those schemas, because use-site semantics now depend on the
document-global first winning normalized reference definition.

One exact reference root owns a fuelled, resumable winner index. Exact-base
requests retain and reuse that index, while each edited leaf receives a
root-bound resolver and remains locally bounded. Resolved fact geometry is
leaf-relative; destination/title cuts are document-absolute and their cooked
values come from the parser-owned winning definition. Dart decodes these facts
but does not infer reference syntax or search definitions.

The resolver capability is cloneable but carries no arena-page ownership. The
retained exact publication remains the sole winner-index owner and lends each
hot-inline or viewport-leaf job a cheap root-bound clone. Consuming a job-local
resolver therefore cannot remove reference authority from later leaves in the
same revision.

Undefined and malformed reference uses stay literal under CommonMark tail
replay. Missing resolver authority, an over-cap reference tail, or an existing
winner whose cooked payload cannot fit the bounded companion lane revokes the
whole-leaf bracket certificate and fails closed. This keeps the parser
definitive without turning global reference semantics into document-wide work
for each local edit.

The current real-Chrome checkpoint is green 3/3 on asset version
`c8d79f20ac3ffce4-76c8745528303a41-bba3dc0f34f51964`. Its Worker/Wasm case
materializes marker-free full, collapsed, and shortcut reference links plus a
reference image beside direct media; it checks cooked destinations/titles,
passive semantics, the labelled no-I/O image fallback, and three direct-media
label/alt recertifications on one input client. A focused Chrome regression is
separately green 1/1 for a length-changing direct-link label edit before later
reference definitions. Neither receipt proves general definition mutation,
active reference-media editing, complete reference-link interaction, or new
performance timing.

These are implementation receipts, not completion claims. Native startup
timeout reclamation is deterministic, and fresh admission at 2,048 below a
4,096-slot resident ceiling reserves one simultaneous create-before-revoke
recovery for every admitted endpoint. The broader GC/exit/capacity stress
matrix remains active. Attachment is available only through the unstable
`FlarkV3DocumentRuntimeAdapter` SPI, while the ordinary open path derives its
document and binding authority. Exact publish archives verify their buildinfo
identity, contain no absolute checkout path, and resolve into an isolated hosted
cache without path overrides. External Dart and JavaScript consumers, a
relocated macOS arm64 `dart build cli` bundle, a Flutter Web build, and a real
Chrome Worker/Wasm runtime are green. The Linux AOT branch exists but still
requires a Linux CI receipt. The temporary 8
KiB special/reference-line path has been removed, and source-backed cooked
reference destination/title records are integrated. The exact GFM Table
detector remains an M1.2 grammar gate rather than an M1.1 infrastructure gate.
Native and Web now share a Rust-authored bounded point query with typed gaps and
fixed scratch; the block-result taxonomy is exercised end to end but remains
preview-only until Checkpoint C. Selected per-leaf Paragraph inline facts and
marker-free handoff are complete for strong, emphasis, code, strikethrough,
accepted angle autolinks, escaped punctuation, hard line breaks, and character
references, plus direct and full/collapsed/shortcut reference links/images
through the authenticated companion-value lane, including bounded
current-revision cache reuse after the host sidecar moves. Undefined,
malformed, or incomplete uses remain literal when definitive; missing resolver
authority, over-cap tails, and unrepresentable winners fail the whole leaf
closed. This admitted passive reference-media presentation does not imply
general definition mutation, active reference-media editing, complete
reference-link interaction, or full CommonMark/GFM.
Fenced-code breadth is complete for the current top-level closed/unclosed
vertical, and ATX and Setext Headings are complete through their current
top-level verticals from parser to demo. ThematicBreak is complete through its
current top-level atomic vertical from parser to demo. IndentedCode is complete
through its current exact-clean structure and demanded selected-island
projection vertical from parser to demo; restart/convergence for edits inside
indented-code blocks is not proven. BulletList is complete through its current
top-level tight-list structure, established schema-5 selected-item projection,
and checkpoint-free source-rope local-delta vertical. List-local parser work no
longer scales with surrounding document or total item count for the proven
shape. The compact schema-6 geometry-then-inline selected-item integration is
green. OrderedList is complete only through its narrow top-level depth-one
tight one-physical-line-per-item vertical, distinct
schema-7/payload-kind-6 constant selected-item projection,
geometry-then-inline demand, and 20,000-item bounded local-delta receipt.
Broader loose/task/nested/multiline/mixed/container/multi-block list forms,
including ordered markers longer than nine digits, and virtualized multi-block
layout over the bounded visible-range materializer, restart/convergence beyond
the definition-free
and reference-frozen Paragraph-anchored interior and definition-free boundary
at-most-64-KiB subsets, and remaining block/inline grammar are not complete.
Edits to or before definitions, definition-bearing BOF, typed unsupported
leaves, missing ordinary Paragraph anchors, new tail definitions, over-cap
crops, and edits such as fence closer removal that lose convergence continue
through definitive exact-clean fallback. The
reference-frozen engine receipt does not imply reference-link UI support.
Production 100 MiB support still requires the later multi-level role directory,
virtualized layout/height indexes, and an integrated product-scale viewport
gate. Full CommonMark/GFM, authenticated suffix
restart for broader grammar, complete parser-to-paint integration,
physical-Worker/GC hardening, floor-device scale/UX, and launch gates follow.

## 12. Reopen conditions

Reopen the parser or package-topology decision only if implementation proves
that the selected design requires one of the following:

- a second live Markdown grammar authority;
- a flattened Paragraph or document snapshot on ordinary edits;
- document-sized transaction state or bridge payloads;
- non-atomic publication of revision roots;
- unbounded work between cancellation points;
- unacceptable persistent-memory growth under the 1/10/100 MiB gates; or
- inability to meet live parser-to-paint and frame-tail targets on floor native
  and web devices.

Absent one of those results, production work should refine this design rather
than reopen the Comrak-fork, enclosing-block, small-document, or dual-parser
bakeoff.
