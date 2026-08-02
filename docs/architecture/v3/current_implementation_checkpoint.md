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
- `e37c44c` updates two obsolete intermediate scheduler assertions to the
  recursive-Green waiting phase; both tests then reach their unchanged terminal
  adoption/fallback, source, publication, query, receipt, and reclamation checks.
- `17c89b2` is the rebuilt native/Wasm checkpoint after bounded recursive-Green
  point, row, and inline-query work. Native and Wasm digests agree exactly.

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

## Maintainability boundary

Before `1f0d414`, `v3_candidate_endpoint.rs` was 22,908 lines. Roughly half was
an inline test module. That commit first split it into:

- `v3_candidate_endpoint.rs`: 11,603 production lines;
- `v3_candidate_endpoint_tests.rs`: 10,918 test lines.

The production unit is still too large. Its current split is:

- `v3_candidate_endpoint.rs`: 9,645 facade/orchestration lines;
- `v3_candidate_endpoint_contract.rs`: 351 error/event-contract lines;
- `v3_candidate_endpoint_recursive_green.rs`: 764 authority/session lines;
- `v3_candidate_endpoint_viewport.rs`: 1,268 viewport preparation/streaming
  lines; and
- `v3_candidate_endpoint_tests.rs`: 10,707 test lines.

The remaining extraction order is:

1. completed — endpoint error/event contract;
2. completed — recursive-Green authority/session ownership;
3. completed — viewport preparation and streaming;
4. hot-inline preparation and streaming;
5. exact-candidate crop/build planning and packet streaming.

`CandidateEndpoint` remains the facade. Extractions must preserve wire types,
state transitions, cancellation, root ownership, and product behavior; they do
not authorize a parser or protocol redesign.

## Known non-green evidence

The complete endpoint unit-test module currently reports 27 passing, 30 failing,
and one intentionally ignored large-scale case in release mode. The exact same
25/32/1 counts and failure names occurred at `17c89b2`, after `5dd2b22`, and
after `46fb82f`, proving that both extractions added no drift. Two tests now
pass after only their obsolete intermediate phase assertions were updated; all
of their terminal product and lifecycle assertions remain unchanged. The
remaining failures include intermediate-phase
expectations from the older crop pipeline, older viewport limits, and earlier
restart/publication assumptions. They have not yet been classified case by case
and therefore cannot be treated as harmless. The focused native and Chrome
product gates above are green, while this internal batch remains an open
consolidation gate rather than release evidence.

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
