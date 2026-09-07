# Code editing follow-up

Fenced code now has visible selection, automatic syntax coloring, a language
picker and indentation commands. This follows the owner's report that selecting
code appeared broken and request for a more useful authoring experience.

## Implementation and boundaries

The selection already existed in the kernel. An opaque background on the code
text painted over its blue selection. Code keeps one block background, with
selection above it and foreground-only token spans above selection. Token
colors leave source positions and glyph metrics unchanged.

`package:flark/code.dart` provides theme-free tokens in pure Dart, using pinned
[`highlight` 0.7.0](https://pub.dev/packages/highlight). Only twelve grammars are
registered. Automatic detection examines the first 1,024 UTF-16 units; tokenizing
stops at an 8,192-unit block cap. The workbench's 4,096-unit leaf admission cap
is narrower. A 32-entry/65,536-unit token cache and 32-entry detection cache
bound retained data. Grammars initialize before the host opens input. Tokens
must rebuild the exact projected code; a decoration failure falls back to plain
text. Rust remains the only Markdown recognizer.

Untagged blocks infer a language without rewriting source. The contextual
toolbar exposes Automatic, Plain text and twelve explicit languages. A choice
edits only the first parser-owned fence info token, preserves body and metadata,
maps the selection and creates one Undo action. A metadata-bearing automatic
fence uses `auto`; unknown imported languages remain intact and uncolored.

Enter continues leading whitespace and quote/list prefixes. Opening braces,
brackets, parentheses and Python colons increase indentation; matching pairs
split around a new indented line. Recognized comments and strings suppress that
increase. Tab/Shift-Tab operate on code lines, preserving selection direction
and container prefixes. The stable step is two spaces, four for Python, or an
existing tab. This is a modest editing aid, without a formatter, language server
or closing-brace dedent. The shared commands remain available to a future
Fleury host. This does not implement or qualify that host.

## Reflection and corrections

Invisible selection was a testing gap, not an obscure Markdown edge case.
Selection geometry was insufficient evidence because a later paint obscured
it. The regression now samples an actual selected whitespace pixel and fails
against the old implementation. It also replaces the dragged range and checks
exact Undo. Chrome transport tests cover code copy/cut and backward selection.

The new code received two further corrections before handoff:

- The generated suite found that checking only the start endpoint let a
  backward selection cross out of code during indentation. A minimized direct
  case fails before the fix; both endpoints now validate against the code row.
- Normal browser dogfooding found that inferring the step from the smallest
  current indent made Tab followed by Shift-Tab remove too much whitespace.
  The simpler stable step fixes the cause without new editor state. Core and
  mounted regressions now start with already-indented text, and the browser
  round trip restores exact source and selection.

The useful testing unit is the editing sequence: select, see, copy or replace,
indent, reverse the indentation, type and Undo. Generated histories complement
those sequences; neither substitutes for inspecting the normal application's
actual result.

The preview itself also retained an older cached application after rebuilding.
`example/tool/serve_web.py` now serves the untouched normal web build under
content-derived asset paths and starts no new service worker. It retains the
same origin and saved drafts. Restart it after a rebuild. The final browser
visibly showed the new controls; HTTP-served artifact hashes are in the receipt.

## Evidence

Final local checks: **403 core, 116 Flutter host, 25 workbench and 4 real Chrome
transport tests** (548 total). The core run includes 1,000 generated histories,
seed 2028. All three analyzers and final formatting checks pass. Rust sources
and the parser Wasm are unchanged from the prior sealed candidate, whose 42
Rust tests and 1,322 transport comparisons are inherited evidence, not rerun
claims.

The normal `flutter build web --wasm` was rebuilt and exercised in the embedded
browser at 1280 × 720. Observations cover automatic JSON and tagged Dart/Python
colors, visible pointer selection, exact copy/cut/replacement and Undo, manual
plain text and reload persistence, brace-pair Enter followed by typing, Python
indentation, and the corrected selected-line Tab/Shift-Tab round trip. Existing
following prose stays outside code. Final cleanup and build hashes are in
[the candidate receipt](receipts/2026-09-06-code/candidate.json).

The local Dart timing diagnostic tokenizes approximately 4,150 units across
twenty changed-text cache misses after a cold sample. It separately reports
grammar startup and detection reuse. It is a diagnostic for decoration cost,
not Flutter frame, Wasm latency or device-budget qualification.

This is a dirty local D0-web candidate for exploratory dogfooding. D0-macOS,
AppKit/IME, floor devices, performance envelopes, Dune, Fleury and named-commit
CI/release evidence remain separate gates.
