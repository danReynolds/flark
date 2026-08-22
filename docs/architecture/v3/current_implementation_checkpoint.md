# Flark v3 current implementation checkpoint

**Status:** Active implementation checkpoint, 2026-08-02. The architecture is
selected and product-shaped large-document gates are green; package and launch
readiness are not claimed.

## Git checkpoints

- `d093521` records RFC 023 and the curated evidence chain.
- `1a991f0` is the first authoritative tracked baseline for the Dart engine,
  Rust parser/host, Flutter adapter, Worker/Wasm assets, and executable gates.
- `0f05668` records the engine lab, live-editor demo, and package guidance.
- `1f0d414` moves the endpoint's inline tests into a sibling module without a
  runtime or protocol change.
- `ae0d54a` isolates the endpoint error/event contract without changing its
  wire or ownership semantics.
- `5dd2b22` isolates recursive-Green authority, adoption, cancellation, and
  cleanup behind the `CandidateEndpoint` facade. A pre/post extraction A/B run
  produced the same 25 passing, 32 failing, and one ignored endpoint tests.
- `46fb82f` isolates viewport presentation preparation and credited VPB1
  streaming. The same full-module baseline remained unchanged after the move.
- `5f55fc7` isolates hot-inline preparation and credited HIO1 sidecar
  streaming. The full endpoint failure set remained unchanged after the move.
- `e37c44c` updates two obsolete intermediate scheduler assertions to the
  recursive-Green waiting phase; both tests then reach their unchanged terminal
  adoption/fallback, source, publication, query, receipt, and reclamation checks.
- `dd22eed` replaces a synthetic flat-publication lifecycle fixture with a real
  recursive-Green base, rejected update, replacement, and close proof. It does
  not weaken the recursive-Green delivery barrier.
- `9f111ad` isolates exact-candidate crop/build planning and credited candidate
  packet streaming behind the endpoint facade.
- `17c89b2` is the rebuilt native/Wasm checkpoint after bounded recursive-Green
  point, row, and inline-query work. Native and Wasm digests agree exactly.
- `2fa3b65` migrates the large-reference and 4,096-Paragraph fixtures to
  recursive-Green scheduling authority while preserving their bounded transfer,
  exact query, and next-edit continuity assertions.
- `5c58dce` moves cancellation ownership to recursive Green and proves that
  mid-parse and mid-stream replacement restore the acknowledged base and reclaim
  every retained root.
- `f5b91f1` migrates ATX, Setext, and thematic-break viewport fixtures from the
  displaced fixed-buffer schema to recursive-Green row geometry. All three
  focused release fixtures are green.

The disposable parser-research lab remains available locally but is ignored by
Git. Only findings and proof receipts referenced by RFC 023 are tracked. Cargo
`target/` directories under the lab are regenerable and have been removed.

## Current product-shaped evidence

The same named acceptance case is green on native Flutter and Chrome/Wasm:

`large RecursiveGreen headings keep nested references on one bounded client`

It uses a document larger than 512 KiB and verifies:

- distant ATX and Setext activation without whole-document Dart rendering;
- parser-authored heading kind, level, style, emphasis, strong reference text,
  destination, and title;
- marker-free active and passive presentation;
- exact source preservation through Setext deletion and insertion;
- no more than 96 materialized or mounted presentations; and
- one stable `EditableTextState` and platform input client.

The existing `distant winning definition edit atomically recertifies reference
family` Chrome case remains a green large-document Worker/Wasm control.

The current large-reference checkpoint additionally covers a 2,377,852-byte
document with 100,000 distinct definitions and early, middle, and final
reference uses in one visible tail. The packaged native runtime returns exact
recursive-Green point, row, and inline facts before and after a literal tail
edit. On the current workstation that edit took 6.2 ms, replacement publication
14.7 ms, the post-edit point query 2.3 ms, and the post-edit inline query 6.4 ms.
The 35-second cold build and 319.7 ms maximum heartbeat gap are diagnostic
receipts, not launch SLOs.

A separate 2,500-line fenced-code Flutter checkpoint stays on recursive Green
through its first middle-body edit, retains one `EditableTextState` and input
client, and exposes only a bounded marker-free body island. The focused native
Flutter case, native/Wasm digest parity (3/3 on each backend), and focused
Flutter Chrome marker-free checkpoint are green on the rebuilt bytes. This is
functional cross-platform evidence, not a fresh 100,000-reference Chrome timing
receipt.

The Chrome heading gate exposed and now covers a Web-specific ownership bug:
Wasm memory growth detached a pre-call JavaScript `DataView`. Structural range
and ordinal-window queries now reacquire `memoryData` after the Wasm call before
decoding receipts.

## Projected BlockQuote inline checkpoint

The first depth-one, single-Paragraph BlockQuote now composes inline authority
without asking Dart or Flutter to parse its projected text. A separately
demanded target-7 endpoint job derives strong, emphasis, and inline-code facts
in parser-owned projected coordinates, then the Dart presentation layer
geometrically composes those facts through the quote's disjoint physical-line
projection. The fixture deliberately carries one strong span across two marked
physical lines.

The product checkpoint proves:

- exact canonical quote source, including every `> `, emphasis delimiter, and
  code delimiter;
- marker-free displayed text with parser-certified styles across physical
  lines;
- one stable `EditableTextState` and platform input client through edit and
  recertification;
- no observer-visible quote, `**`, or backtick marker flash while the new
  projected facts are pending;
- native and real Chrome Worker/Wasm target-7 parity; and
- fail-closed behavior for unsupported projected constructs.

The focused native endpoint fixture passes on the normal 2 MiB Rust test stack.
That gate also exposed and removed a debug-stack-amplifying return-by-value
wrapper around the 91,856-byte inline projection state. The subsequent
recursive-container gate completed that stack-hygiene work: mutually exclusive
phase scratch is now heap-owned and the inline job itself is 4,768 bytes, with
an 8 KiB regression ceiling. Endpoint-owned recursive-Green clean-build and
adoption jobs are likewise boxed at their phase boundaries. Both the clean
CM321 endpoint and the large local-adoption endpoint now pass on the normal
Rust test stack.

This closes inline composition for the admitted quote slice. Nested and
multi-child BlockQuotes, authenticated local restart/convergence inside quotes,
and broader CommonMark container coverage remain open.

## Generic recursive-container checkpoint

The hardest pre-promotion container probe is now green through the production
parser, endpoint, publication, and independent host. The fixture embeds
CommonMark examples 321 and 325 in an 819,053-byte base document. Its local
edit replaces
CM325's outer-item `baz` Paragraph with `* βaz`, changing real recursive
structure rather than merely changing leaf text.

The paired gates prove:

- the base path changes from `Document/List/Item/Paragraph` to
  `Document/List/Item/List/Item/Paragraph`;
- independent host schema-11 facts change from outer-loose/inner-tight to
  outer-tight/inner-loose;
- the `β` edit rebases the suffix by three UTF-8 bytes but two UTF-16 code
  units, with exact row and editable ranges;
- adoption under patterned fuel reads less than 16 KiB of source and rebuilds
  fewer than 256 Green nodes;
- distant prefix and shifted-suffix arena pages retain identity;
- the incremental semantic digest equals an independent clean target build;
- the endpoint preempts the legacy parser, publishes an exact-base
  recursive-Green delta under unit fuel, and the independent host rebuilds
  branches rather than receiving transported branch pages; and
- clean CM321 point, row, inline, and viewport publication remains green on
  the same generic representation.

The 819,056-byte target point lookups in this proof use an explicit bounded
1,024-tree-node budget; the existing smaller/default-budget query gates remain
unchanged. This is not a new latency SLO. It records the actual admitted budget
for the structurally edited target so later query tuning cannot silently claim
the 256-node receipt.

This validates the serialized-Green container architecture and completes the
hardest-container gate. It does **not** convert examples 321 or 325 into official
v3 CommonMark coverage: they remain ledger-unclassified until the product-facing
semantic gaps and official ledger are reconciled explicitly.

## Production grammar-promotion checkpoint

The research line/finish controller is no longer merely a future donor. Its
mechanically promoted snapshot now drives the production refillable source,
fuelled direct controller, reference rendezvous, recursive-Green writer,
candidate endpoint, and independent host. The mutable proof tree, per-line
owned source strings, test renderer, and proof materializer remain outside the
production path.

The corpus gate separates two receipts that must not be conflated:

- structural grammar admission is 652/652, with zero unsupported or invalid
  production outcomes; and
- semantic HTML replay is 384 exact, 262 typed missing capabilities, and six
  known divergences.

The semantic gate exposed a representation regression rather than forty new
grammar failures: cached fenced-row geometry had displaced the fence's semantic
cuts, and the ledger renderer did not recognize the terminal-empty Item row.
Fenced close facts now retain a 33-byte grammar-owned semantic prefix followed
by the existing 24-byte `RGEO` trailer (57 bytes total, below the 64-byte cap),
while the query path remains compatible with the prior cached and legacy
schemas. The full production corpus receipt returned to its pinned baseline.
The public fenced-code live-edit gate now asserts the recursive-Green row,
editable span, physical ownership, and code path facts instead of requiring the
superseded flat structural query type.

A new public-runtime CM325 gate now performs the topology-changing edit on both
native and real Chrome Worker/Wasm. Dart observes the path change from a lazy
outer-item Paragraph to a second nested-list Item and the exact List facts
changing from outer-loose/inner-tight to outer-tight/inner-loose.

The public semantic-parity suite no longer asks the endpoint for the superseded
flat structural projection. All 24 cases now consume recursive-Green point,
row, path, and separately demanded sidecar facts directly, and pass on both the
native endpoint and rebuilt real Chrome Worker/Wasm. This cutover exposed and
fixed three production gaps rather than hiding them behind a compatibility
projection:

- ordinary Paragraph and Heading inline sidecars now rejoin through the
  current-revision cache by exact ACK, owner frame, and geometry;
- an over-8-KiB inline demand is rejected during the bounded recursive-row
  preflight instead of being scheduled and failing later; and
- an EOF checkpoint is reusable only when its parser position remains a
  physical line start in the target revision, preserving contiguous geometry
  when text is appended to the final Paragraph.

The first marker-free product atom also consumes this authority directly. A
top-level `Document -> ThematicBreak` path collapses to an empty active input
island, paints the divider, preserves affinity and exact canonical source, and
deletes atomically with Backspace or Delete. Nested thematic breaks deliberately
remain literal until container chrome can compose them, and active IME
composition suppresses the atomic handoff. The managed Flutter checkpoint is
green 2/2 without changing the `EditableTextState` or platform input client.

This closes grammar promotion and the public recursive-authority cutover. It
does **not** close the official Dart CommonMark ledger, broader product grammar,
or launch readiness. General restart/convergence and atomic incremental
publication remain the next architectural wave.

## Maintainability boundary

Before `1f0d414`, `v3_candidate_endpoint.rs` was 22,908 lines. Roughly half was
an inline test module. That commit first split it into:

- `v3_candidate_endpoint.rs`: 11,603 production lines;
- `v3_candidate_endpoint_tests.rs`: 10,918 test lines.

The production endpoint is now split into these cohesive units:

- `v3_candidate_endpoint.rs`: 7,994 facade/orchestration lines;
- `v3_candidate_endpoint_candidate.rs`: 1,136 exact-candidate crop/build and
  credited packet-streaming lines;
- `v3_candidate_endpoint_contract.rs`: 351 error/event-contract lines;
- `v3_candidate_endpoint_hot_inline.rs`: 542 hot-inline preparation/streaming
  lines;
- `v3_candidate_endpoint_recursive_green.rs`: 764 authority/session lines;
- `v3_candidate_endpoint_viewport.rs`: 1,268 viewport preparation/streaming
  lines; and
- `v3_candidate_endpoint_tests.rs`: 10,492 test lines.

The planned extraction order is complete:

1. completed — endpoint error/event contract;
2. completed — recursive-Green authority/session ownership;
3. completed — viewport preparation and streaming;
4. completed — hot-inline preparation and streaming;
5. completed — exact-candidate crop/build planning and packet streaming.

`CandidateEndpoint` remains the facade. Extractions must preserve wire types,
state transitions, cancellation, root ownership, and product behavior; they do
not authorize a parser or protocol redesign.

## Known non-green evidence

The complete endpoint unit-test module currently reports 36 passing, 21 failing,
and one intentionally ignored large-scale case in release mode. The sequence
from 28/29/1 through 31/26/1, 33/24/1, and 36/21/1 introduced no new failure
names. Each improvement replaced displaced route or transport assumptions while
preserving terminal publication, query, cancellation, continuity, and
reclamation assertions.

The remaining failures are now classified. Most still assert the older
ordinary-crop phase, block-page topology, fixed viewport schema, or legacy
list/quote sidecar ownership. Two production gaps remain explicit rather than
being relabeled as fixture drift: a marker-only terminal list item has no
marker-free editable presentation row, and local edits inside a long single
Paragraph can publish a `FullSnapshot` instead of an `ExactBaseDelta`. The
focused native and Chrome product gates above are green, while this internal
batch remains an open consolidation gate rather than release evidence.

CommonMark support also remains a fail-closed supported subset. The 652-fixture
ledger, not feature names or this checkpoint, controls conformance claims.

## Working rule

Architecture and integration stay with the primary agent. Delegated tasks must
name exact files and symbols, state one invariant and one requested artifact,
and return after the first patch or concrete blocker. Broad repository
rediscovery and unbounded review are not part of implementation work.

Before a delegated implementation wave, the primary agent records the dirty
paths and assigns one owner to every dirty file. A shared file moves serially
through that owner; another agent reports the needed hunk instead of editing it
concurrently. Only one agent runs Cargo in the worktree at a time. Use
`./scripts/run_native_focus.sh <package> <exact-test-filter> [cargo flags...]`
for the requested focused receipt; the full gate remains checkpoint work. The
handoff names the changed files, exact command, log path, exit status, and first
remaining failure.
