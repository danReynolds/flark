# D0-web: browser dogfood qualification

The next handoff is the complete Flutter web workbench in Codex's embedded
browser. The packaged Rust parser Wasm and Dart/Flutter web application already
exist; this milestone expands their qualification. D0-macOS remains a separate
gate under [DOGFOOD_MILESTONE.md](../../../DOGFOOD_MILESTONE.md).

## Acceptance

- Current core, host and workbench functional/actual-paint suites pass. A real
  browser transport regression covers failures caused by DOM input delivery.
- On the normal web build, verify ordinary multiword typing, whitespace,
  punctuation, inline formatting, deleting the final styled character then
  typing, structural Return/Backspace, selection replacement, clipboard,
  Undo/Redo, pointer placement, scrolling and document switching. Include
  double-click word selection, selection reached from both sides of hidden
  delimiters, word deletion toward whitespace, document-edge shortcuts and
  actual keyboard copy/cut/paste followed immediately by typing.
- Type through multiple bounded-input contexts, verify the entire exported
  source against an independently constructed expectation, and continue typing
  immediately after selection, history and source/rendered transitions.
- Exercise the Tour, both dense presets and the giant-line source preset;
  source remains exact and writable when the byte or shape budget is exceeded.
  Check source paging and rejection above the writable ceiling without losing
  the existing document. The current 32 KiB/shape settings remain candidates.
- Saved drafts survive reload and document switching. Inspect, resize and
  focus recovery preserve the next writable position. Browser-local storage
  is the workbench's persistence contract, with Copy Markdown as its export.
- A sustained browser session has no corruption, duplicate/dropped delivered
  input, dead input, crashes or editing-loop hangs. Record browser/viewport,
  source/build identity, scenarios, observations and any remaining roughness.
- Rebuild the normal application and open that exact candidate for owner use.
  No known B0/B1 in the supported browser editing loop may remain open.

Browser automation delivery is checked independently from editor acceptance.
An actuator timeout or target replacement is not evidence that a key reached
the app. Do not claim native clipboard or IME qualification from synthetic DOM
input; pair transport regressions with interactive browser canaries.

## Evidence boundaries

This is readiness for exploratory owner dogfooding on the observed embedded
browser. It does not certify every browser, mobile keyboard/IME, accessibility
technology, crash-proof persistence, native macOS behavior or a universal
latency envelope. Formal browser performance budgets need a browser-specific
production-path receipt; native raster measurements do not transfer to web.
Visible browser stalls still block this milestone even without a numeric
cross-browser performance claim.

The native gate retains AppKit clipboard/composition/queued-key canaries,
foreground timing, memory and process-lifecycle qualification. Findings shared
by both hosts are fixed in the owning shared layer and rechecked on web.

## Execution order

1. Resolve browser input/clipboard failures with a minimal transport regression.
2. Exercise the normal workbench's complete common editing loop and boundaries.
3. Run sustained editing, reload, focus, scrolling and responsive-layout checks.
4. Review findings and exact candidate evidence; hand off D0-web when green.

Status (2026-09-06 automatic outdent): **ready for exploratory owner dogfooding**.
The [code editing review](code_closer_review_2026_09_06.md) and
[candidate receipt](receipts/2026-09-06-closer/candidate.json) cover visible code
selection, syntax coloring, language choice, indentation and typed-closer outdent. Normal browser
sequences include already-indented Tab/Shift-Tab round trips, cut/Undo, brace
pair splitting and language persistence across reload. The earlier navigation,
hover and fence-creation corrections remain covered by the full suites.
Native qualification retains its separate evidence boundary.
