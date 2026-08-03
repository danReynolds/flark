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
wrapper around the 91,856-byte inline projection state. The state object's size
remains a follow-up native/Wasm stack-hygiene item: a later cleanup should make
it a small handle over heap-backed or phase-specific state, without reopening
this protocol or presentation architecture.

This closes inline composition for the admitted quote slice. Nested and
multi-child BlockQuotes, authenticated local restart/convergence inside quotes,
and broader CommonMark container coverage remain open.

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
