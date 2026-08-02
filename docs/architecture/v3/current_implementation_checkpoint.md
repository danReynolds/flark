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

The Chrome heading gate exposed and now covers a Web-specific ownership bug:
Wasm memory growth detached a pre-call JavaScript `DataView`. Structural range
and ordinal-window queries now reacquire `memoryData` after the Wasm call before
decoding receipts.

## Maintainability boundary

Before `1f0d414`, `v3_candidate_endpoint.rs` was 22,908 lines. Roughly half was
an inline test module. It is now split into:

- `v3_candidate_endpoint.rs`: 11,603 production lines;
- `v3_candidate_endpoint_tests.rs`: 10,918 test lines.

The production unit is still too large. Its next extraction order is:

1. endpoint error/event contract;
2. recursive-Green authority/session ownership;
3. viewport preparation and streaming;
4. hot-inline preparation and streaming;
5. exact-candidate crop/build planning and packet streaming.

`CandidateEndpoint` remains the facade. Extractions must preserve wire types,
state transitions, cancellation, root ownership, and product behavior; they do
not authorize a parser or protocol redesign.

## Known non-green evidence

The complete inline endpoint unit-test module currently reports 18 passing and
39 failing tests. The failures include stale manifest-size/role expectations
and earlier crop/publication assumptions, but they have not yet been classified
case by case and therefore cannot be treated as harmless. The focused native
and Chrome product gates above are green, while this internal batch is an open
consolidation gate rather than release evidence.

CommonMark support also remains a fail-closed supported subset. The 652-fixture
ledger, not feature names or this checkpoint, controls conformance claims.

## Working rule

Architecture and integration stay with the primary agent. Delegated tasks must
name exact files and symbols, state one invariant and one requested artifact,
and return after the first patch or concrete blocker. Broad repository
rediscovery and unbounded review are not part of implementation work.
