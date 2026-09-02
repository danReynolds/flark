# Live-editor testing

## Goal

Tests should tell us whether Flark meets the [North Star](../../NORTH_STAR.md),
identify the layer that failed, and produce a small reproduction. They should
not create a second editor model or require a separate vocabulary to understand
product quality.

Every permanent case is an ordinary user scenario:

```text
starting state + user action -> expected visible and stored result
```

The test itself is the specification. Shared data is useful only when it makes
several direct tests clearer.

## Test layers

| Layer | Use it for |
| --- | --- |
| Engine/Core | Markdown meaning, source transactions, graphemes, selection, history, queues, failure behavior, and incremental-versus-clean equivalence |
| Controller | Logical-command delivery and every synchronous source, selection, generation, and presentation publication |
| Mounted surface | Actual painted text, styles, block presentation, caret and selection geometry, hit testing, scrolling, semantics, focus, and visible transients |
| Native canary | OS-owned key, pointer, clipboard, composition, focus, lifecycle, and scrolling routes |
| Performance | Measured frame latency, memory, parser work, opening, paging, resize, and sustained input on the production path |

Put an assertion at the lowest layer that can observe the failure. Add a higher
layer only when it proves something new.

## What requires actual paint

Controller state cannot prove that the correct frame appeared. Marker flashes,
lost styling, wrong block presentation, caret jumps, stale geometry, and focus
failures require mounted tests that inspect every actual paint produced during
the command.

For those cases, every painted frame must agree on:

- accepted source generation and canonical selection;
- visible text and source exposure;
- resolved inline styles and block presentation;
- displayed selection and caret source identity;
- geometry and accessibility semantics; and
- eventual equivalence with a clean rebuild.

A correct settled frame cannot excuse a wrong intermediate frame.

## Coverage design

Use exhaustive coverage when the finite dimension is small and high risk—for
example, deleting one grapheme in both directions across the supported inline
owners. Use pairwise coverage for larger combinations, then add named cases for
known interaction risks.

Always keep direct sequence regressions for:

- deleting the final styled grapheme and immediately typing;
- repeated Return followed by typing;
- terminal-gap Backspace followed by typing;
- selection replacement followed by another command;
- delete/insert followed by Undo and Redo; and
- human-cadence versus true unpumped input.

Parser and Core tests own semantic breadth. Mounted tests keep one smallest case
per distinct visible failure mechanism rather than replaying the full semantic
matrix through Flutter.

## Exploration versus regression

Property-based and generated histories are discovery tools. Run them in bounded
targeted or nightly jobs. When they find a bug, minimize it and add one readable
direct regression test.

Do not keep a permanent serialized journey language, universal driver, shadow
editor, or generated state model. Those systems are harder to trust than the
behavior they are meant to test.

## Shared test support

`LiveEditorTransitionProbe` is the one shared temporal helper. It opens the real
Rust-to-Core-to-controller path and records immutable synchronous publications.
Mounted support extends that observation through actual paint.

Tests wait on controller-owned completion futures. They do not sleep, poll
private counters, or infer parser completion from elapsed time.

A new helper is justified only when it:

- removes repeated assertions from at least three tests;
- exposes a production state that ordinary test APIs cannot observe; or
- owns one cross-cutting invariant such as canonical/display caret identity.

Narrow source checks may reject a known forbidden dependency or parallel state
slot, but private-name assertions are not substitutes for architecture proof.
Preserve architecture primarily with typed boundaries, behavior tests, static
analysis, and review.

## Native and performance scope

Native canaries stay small. They prove delivery through the real OS, not
Markdown semantic breadth. The macOS canary set covers character input,
Return/Backspace routing, arrow and pointer selection, clipboard/history,
scrolling, focus, and sustained editing.

Performance qualification is separate from functional correctness. It uses the
document presets and budgets in [DOGFOOD_MILESTONE.md](../../DOGFOOD_MILESTONE.md)
and records production-path measurements. A passing functional suite does not
imply acceptable p99 latency or memory.

## Pull-request expectations

Every editing bug fix includes:

1. the smallest direct regression at the owning layer;
2. an actual-paint regression when the failure was visible;
3. a native regression only when OS routing was necessary;
4. clean-parse equivalence for changed Markdown semantics; and
5. a focused performance measurement when the production hot path changed.

The fast local lane should stay under one minute warm. Native and full
performance qualification run before dogfood handoff, not on every small edit.

Complexity is itself a regression. If a test helper becomes a generic framework
or needs more explanation than the cases it supports, replace it with direct
tests.
