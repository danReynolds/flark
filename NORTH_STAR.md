# Flark North Star

## Product promise

Flark is a jankless, performant live Markdown editor whose visible surface
always shows the rendered result a user reasonably expects while exact Markdown
source remains canonical.

Typing must never reveal unrelated markers, lose input, move the caret, or
publish a visibly inconsistent intermediate state. The same promise must hold
under rapid input and scrolling throughout the declared live-rendered envelope.
Documents outside that envelope open in an explicit source mode; they do not
silently take the live-rendered path. The no-jank promise belongs to the
live-rendered envelope; source-mode latency and maximum size are qualified
separately rather than implied to be unbounded.

## Product principles

1. **Rendered-result fidelity.** Show the best current rendered result. Expose
   exact source only where the current edit genuinely cannot yet be rendered.
2. **Temporal coherence.** Never paint torn transitions, marker flashes, stale
   styling, incorrect block shells, or caret jumps.
3. **Intent-preserving editing.** Typing, deletion, replacement, selection,
   structural commands, and history act on the visible meaning users edit.
4. **Continuous interaction.** After every accepted command the editor remains
   focused, correctly targeted, writable, and ready for the next command.
5. **Responsive within a measured work envelope.** Live editing remains
   native-responsive as document size, viewport movement, and input cadence
   scale to receipt-defined byte and shape limits. Ineligible documents remain
   exact and writable in source mode without requiring a full render model.

## UX features

| Feature | What good looks like |
| --- | --- |
| Continuously rendered presentation | Every frame has current text, styles, block presentation, selection, caret, geometry, and semantics. |
| Inline semantic editing | Typing, deletion, and replacement operate on visible graphemes without exposing markers or changing unrelated formatting. |
| Caret, selection, and navigation | Pointer, keyboard, range, and focus transitions preserve the intended visible target and accept the next command. |
| Structural editing | Return, Backspace, list, quote, table, and block actions perform the expected split, continuation, exit, merge, or lift. |
| History and platform input | Undo, redo, paste, composition, clipboard, and equivalent platform routes preserve user intent and coherent history. |
| Continuous rapid input | Human cadence and unpumped bursts accept the same commands without dropped input, stale state, or bad intermediate paint. |
| Explicit scale boundary | Live editing stays within measured latency, memory, and work bounds; documents outside the byte-and-shape envelope enter a disclosed source mode. |

These feature groups organize product coverage, not the codebase. A test may
cover several features, and tests live at the layer that can observe the bug.

## Product invariants

- Exact source, canonical selection, platform input, displayed selection, and
  painted caret describe the same accepted generation.
- Complete Markdown constructs stay rendered while focused and edited inside
  the live tier; literal or incomplete syntax remains visible authoring text.
- Rapid input is as correct as slow input. A correct final frame cannot excuse
  a wrong intermediate frame.
- Unaffected text retains its styling and block presentation during an edit.
- Crossing the declared size boundary changes mode explicitly and never changes
  source bytes or silently performs unbounded foreground work.
- Read-only and editable surfaces render equivalent Markdown consistently.

## Architecture guardrails

- Exact source and selection are canonical; rendered content is a
  revision-bound projection, not a second document model.
- Rust owns Markdown recognition, delimiter dependencies, block structure, and
  any proof that rendered presentation may be retained during an edit.
- Source, selection, presentation, geometry, and action authority publish as
  one coherent snapshot. Stale asynchronous results cannot overwrite it.
- Foreground parsing, mapping, layout, and paint work are bounded by the live
  tier. Larger documents use a source-only path that does not build a complete
  render model.
- Performance is part of correctness and is measured on the production path.

## How we prove it

Tests start from user-visible scenarios and assert at the lowest useful layer:

- Core tests cover Markdown meaning, edit results, history, and clean-parse
  equivalence.
- Controller tests cover command delivery and every synchronous publication.
- Mounted tests cover actual paint, style, geometry, focus, selection, and
  visible transients.
- Native and performance tests cover OS routing and measured scale behavior.

A dogfood-visible marker flash, lost style, caret jump, dead input state, or
wrong block shell requires an actual-paint regression test.

## Current bar

Dogfood readiness is stated per platform under
[DOGFOOD_MILESTONE.md](DOGFOOD_MILESTONE.md). D0-web is the immediate owner
handoff: its supported browser scenarios must pass with no open browser B0/B1.
D0-macOS additionally requires the native canaries and measured performance
profile to pass. A pass on one platform does not close another platform's gates.

Safe fallback is not enough: a common editing path that repeatedly exposes raw
source still fails this North Star even when no data is lost.

## Active documents

- [Rendered editing behavior](docs/architecture/v5/edit_profile_v1.md)
- [Testing strategy](docs/testing/live_editor_test_strategy.md)
- [Dogfood milestone](DOGFOOD_MILESTONE.md)

Architecture RFCs record why the implementation is shaped as it is. They are
not additional product principles or required test taxonomies.
