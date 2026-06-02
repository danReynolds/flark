# Sovereign v2 Web Parser Strategy

Status date: 2026-05-08
Decision status: Comrak required on native and web; fallback parser removed

## Problem

Sovereign v2 relies on parser output for more than HTML rendering. Projection,
cursor masks, hidden marker ranges, ambiguity zones, render blocks, overlays,
and custom preview rendering all depend on source-position-preserving Markdown
structure. A web parser backend that only returns an HTML AST would make the web
surface compile but would not preserve the source-first editing contract.

## Current Behavior

The promoted v2 Flutter widgets require `SovereignNativeComrakParseBackend`
unless an app explicitly supplies its own `SovereignMarkdownParseBackend`.
On browser targets the default backend loads
`lib/assets/wasm/sovereign_comrak_bridge.wasm` and calls the same Rust Comrak
bridge ABI through Dart JS interop. On native targets it uses the bundled FFI
bridge.

If the packaged Comrak bridge cannot load, the default widgets surface that
failure directly. Sovereign no longer ships a Dart Markdown fallback parser or
an automatic source-only degradation path because divergent parser behavior
made live editing harder to reason about and harder to test.

## Parser Contract

The first-party web backend is Comrak compiled to WebAssembly with the same
versioned payload schema used by the native Rust bridge.

Reasons:

- one parser family across native and web;
- no divergent CommonMark/GFM interpretation;
- source ranges, hidden ranges, diagnostics, and unknown-field tolerance remain
  the same contract;
- upstream fixture conformance can run through the same v2 projection and
  render-plan gate.

## Rejected Defaults

Do not make an HTML-AST, rendered-output fallback, source-only fallback, or
secondary Markdown implementation the default editable backend. Any parser used
by the promoted widgets must preserve source positions, hidden marker ranges,
diagnostics, unknown-field tolerance, and render-plan input parity with the
Comrak bridge contract.

## Shipped Criteria

The first-party web backend is considered present when it has:

- the same `SovereignMarkdownParseBackend` interface as native;
- deterministic initialization and clear asset-loading diagnostics;
- Chrome tests for parser loading and promoted-surface rendering;
- package docs that explain the web asset model without requiring every app to
  invent its own backend.

CommonMark/GFM upstream fixture coverage through v2 projection and render-plan
generation remains part of the broader parser conformance lane. The current
feature coverage contract is tracked in
`docs/architecture/v2/markdown_test_matrix_2026-05-08.md`.
