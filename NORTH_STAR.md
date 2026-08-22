# Flark North Star

## The product promise

Flark is a jankless, performant live Markdown editor whose every visible frame
shows the best current rendered result a user would want to see.

Exact Markdown source remains canonical, but parser or scheduling uncertainty
must not leak into the interface as avoidable marker flashes, stale styling,
caret jumps, block-shell changes, or partially updated rows.

This is a product requirement, not an aspirational slogan. Architecture,
features, tests, and release claims are evaluated against it.

## What “best current rendered result” means

For every visible frame, Flark must show:

1. current source and current selection—never stale content or coordinates;
2. parser-certified rendered semantics everywhere they remain valid;
3. the smallest parser-authored exact island where an edit has genuinely made
   semantics uncertain; and
4. one coherent presentation snapshot, including text, style, block identity,
   geometry, selection, caret, hit behavior, and accessibility semantics.

An incomplete construct may need to remain visible as literal source. That is
correct only inside the smallest affected dependency island. Exposing unrelated
delimiters or demoting an entire row because a smaller authenticated island was
available violates the North Star.

Failing closed protects correctness. It does not, by itself, satisfy the
product. A common editing path that repeatedly falls back to avoidable raw
source remains a product gap even when no data is lost.

## User-visible invariants

- Typing, deletion, replacement, paste, selection, and composition respond in
  the same frame budget expected of a native editor.
- Ordinary prose editing does not flash `#`, `**`, `_`, link syntax, or other
  unrelated Markdown markers.
- Unaffected rendered text keeps its style and block presentation while an
  affected island is pending.
- Source, platform input, canonical selection, displayed selection, and painted
  caret preserve one identity through every edit generation.
- Rapid input is as correct as human-cadence input; correctness may not depend
  on Flutter coalescing bad intermediate publications.
- Scrolling, paging, large documents, and offscreen parsing do not change the
  semantics or responsiveness of the visible editing surface.
- Structural controls and accessibility actions are exposed only when their
  current-revision authority is proven and the action will work.
- Read-only and editable surfaces consume the same parser-owned presentation
  facts and render equivalent Markdown consistently.

## Architectural consequences

### Source is canonical

The exact source transaction, canonical source selection, and revision are the
authoritative editing state. Rendered text is a revision-bound projection, not
a second document model.

### Rust owns Markdown meaning

The parser/runtime owns Markdown recognition, delimiter dependencies, block
structure, projection facts, and the proof that a pending edit may retain some
presentation. Dart and Flutter may validate and transform a typed proof, but
must not infer Markdown safety from marker scans, character allowlists, or
stale rendered structure.

### Authority is granular and explicit

Parser-authored edit cells or dependency islands describe the exact affected
range, admitted edit class, retained outside presentation, block-shell
authority, revision, and chaining rules. A receipt belongs to one source
lineage and row; it cannot migrate to another row or silently broaden itself.

### Publication is atomic

The controller publishes source, generation, viewport, presentation, mapping,
selection, and action authority as one coherent snapshot. Asynchronous parser,
page, history, or semantic-action results cannot overwrite a newer optimistic
generation or pair old facts with new source.

### Work is bounded

Input windows, parser pumping, viewport queries, ABI payloads, projection
metadata, layout, and paint work have explicit caps. Optional continuity
metadata may be dropped without evicting the baseline rendered facts needed by
later rows. Large-document behavior is achieved through paging and
virtualization, not unbounded work hidden behind an asynchronous API.

### Performance is part of correctness

The normal paint path must not allocate test receipts or perform diagnostic
style reconstruction when no observer is installed. Functional correctness,
frame latency, memory, native input, and physical-device behavior are separate
evidence gates; none may be inferred from another.

## Testing the North Star

Tests must observe the layer where a failure is visible:

- Rust and Core differential tests prove that every emitted authority record
  agrees with a clean parse, including the block shell and the complete set of
  facts outside the affected island.
- Controller transition tests prove every synchronous source, selection,
  generation, viewport, and authority publication—not only the settled state.
- Mounted tests inspect every actual paint for text, resolved styles, block
  identity, visible source, source generation, canonical/display caret
  identity, geometry, semantics, and clean-rebuild convergence.
- Mounted cadence lanes include per-edit frames, human cadence, and true
  unpumped bursts. A final correct frame cannot excuse an earlier bad frame.
- Native canaries prove only OS-owned behavior such as key routing, focus,
  pointer input, clipboard, scrolling, and platform text input.
- Performance profiles and physical-device qualification remain mandatory for
  latency, memory, IME, touch, accessibility, lifecycle, and thermal claims.

A dogfood-visible marker flash, lost style, caret jump, stale action, or wrong
block shell requires an actual-paint regression. A controller-only or
final-state assertion is insufficient.

## Decision rules

When extending live editing:

1. Start from a user-visible editing scenario and its expected rendered frame.
2. Identify the parser-owned dependency island and prove the retained outside
   facts against a clean parse.
3. Extend the typed authority contract; do not add Markdown heuristics to the
   host or renderer.
4. Keep matcher domains disjoint, bounded, revision-bound, and fail-closed.
5. Add the lowest-layer semantic proof and the smallest actual-paint acceptance
   case that can catch the product regression.
6. Measure production-path cost and ensure diagnostic instrumentation is absent
   when disabled.
7. State the remaining unsupported edits explicitly. Do not relabel them as
   acceptable merely because exact-source fallback is safe.

Complexity is itself a regression. New abstractions must remove duplicated
authority logic, clarify ownership, or enable a parser-proven capability. A
generic scenario language, renderer-side Markdown policy, or another parallel
continuity mechanism requires exceptional evidence.

## Current posture

The active v4 architecture is pointed in the right direction:

- source and selection remain canonical;
- Markdown and retention authority remain parser-owned;
- the ABI is typed, bounded, and exact-minor negotiated;
- ordinary facts are protected from optional continuity metadata;
- edit-cell receipts retain only explicitly authorized presentation; and
- mounted tests now inspect actual intermediate frames.

The redesign is not complete. The current edit-cell and literal-envelope
vocabulary covers a bounded set of common edits, while broader syntax-shaped
changes still fall back to exact source. Two continuity representations remain
in the implementation, and the controller/runtime files still carry more
orchestration and policy than the long-term modular shape should.

Therefore the current state is suitable for focused engineering dogfooding and
architecture review, not the D0 handoff defined in
[DOGFOOD_MILESTONE.md](DOGFOOD_MILESTONE.md), universal janklessness, or release
qualification. The next architectural milestone is parser-owned
dependency-island authority broad
enough to replace common whole-row fallback, followed by consolidation of the
parallel continuity paths and extraction of controller/runtime responsibilities
without moving Markdown policy into Dart or Flutter.

## Related contracts

- [D0 macOS dogfood-ready milestone](DOGFOOD_MILESTONE.md)
- [Continuously rendered Markdown RFC](docs/architecture/rfc/rfc_027_continuously_rendered_markdown.md)
- [Live projection contract](docs/architecture/v4/contracts/live_projection_v2.md)
- [Source-authoritative edit transactions](docs/architecture/rfc/rfc_028_source_authoritative_edit_transactions.md)
- [Large-document architecture](docs/architecture/rfc/rfc_029_large_document_architecture.md)
- [Live-editor test strategy](docs/testing/live_editor_test_strategy.md)
- [Performance evidence contract](docs/architecture/v4/contracts/performance_evidence_v1.md)
