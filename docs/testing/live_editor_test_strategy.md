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

The product contract defines the expected result; tests encode examples and
can be wrong. Establish the expected visible behavior from the edit profile
before choosing an exact source spelling or copying any implementation output.
Review changes to expectations as behavior decisions. A common supported action
cannot become unsupported merely to make a failing test green. Shared data is
useful only when it makes several direct tests clearer.

## Test layers

| Layer | Use it for |
| --- | --- |
| Kernel | Markdown meaning, source transactions, graphemes, selection, history, failure behavior, and committed-model-versus-clean-parse equivalence |
| Controller | Logical-command delivery and every immutable live/source snapshot publication |
| Mounted surface | Actual painted text, styles, block presentation, caret and selection geometry, hit testing, scrolling, semantics, focus, and visible transients |
| Browser transport and canary | Packaged Wasm, real DOM-to-Flutter input, clipboard, focus, browser persistence, and the normal embedded-browser editing loop |
| Native canary | OS-owned key, pointer, clipboard, composition, focus, lifecycle, and scrolling routes |
| Performance | Measured frame latency, memory, parser work, opening, live/source transitions, resize, and sustained input on the production path |

Put an assertion at the lowest layer that can observe the failure. Add a higher
layer only when it proves something new.

## What requires actual paint

Controller state cannot prove that the correct frame appeared. Marker flashes,
lost styling, wrong block presentation, caret jumps, stale geometry, and focus
failures require mounted tests that inspect every actual paint produced during
the command.

For those cases, every painted frame must agree on:

- accepted source snapshot and canonical selection;
- visible text and source exposure;
- resolved inline styles and block presentation;
- displayed selection and caret source identity;
- geometry and accessibility semantics; and
- eventual equivalence with a clean rebuild.

A correct settled frame cannot excuse a wrong intermediate frame.

Check every accepted command immediately and every frame that actually paints.
An unpumped burst may produce one frame for several commands; that frame must
reflect the latest accepted snapshot. Require at least one post-input paint,
so an empty observation list cannot pass. Paint instrumentation must observe the
content and geometry used by the drawing path, not just echo controller state.

## Coverage design

Use exhaustive coverage when the finite dimension is small and high risk—for
example, deleting one grapheme in both directions across the supported inline
owners. Use pairwise coverage for larger combinations, then add named cases for
known interaction risks.

For supported actions, assert that the command was accepted as well as its
result. For unsupported actions, assert that rejection preserves source,
selection, typing intent, and history. Repeated-command helpers check each
individual command, not only the state after the loop. Tests of structural or
formatting commands assert the relevant styles and container membership; a
clean parse of a bad edit is still a bad edit. Replacement checks need explicit
expected text and mappings, rather than rebuilding a string from itself.

Always keep direct sequence regressions for:

- ordinary multiword typing, one character at a time, including spaces at block
  ends and before existing line breaks; check source order and caret after each
  character before testing Undo/Redo;
- deleting the final styled grapheme and immediately typing;
- repeated Return followed by typing;
- terminal-gap Backspace followed by typing;
- selection replacement followed by another command;
- delete/insert followed by Undo and Redo; and
- human-cadence versus true unpumped input.

Static rendered fixtures do not establish authoring support. For each supported
block, exercise creation from ordinary character input, body editing, exit,
deletion and Undo/Redo in the middle of an existing document as well as at EOF.
Assert that surrounding blocks keep their meaning on every published frame,
including intermediate delimiter entry. Explicitly deferred creation behavior
must remain visible in the readiness status.

Navigation coverage must cross rows, not only wrap within a paragraph: short,
empty, consecutive empty, code, heading and table rows in both directions,
with selection extension and the next key. Pointer controls need hover feedback
and read-only behavior alongside activation and painted-state checks. Select
small representative combinations; do not expand every axis into a framework.

Begin native/browser exploration with those ordinary writing paths. A large
generated invariant run can accept a correctly parsed source whose words were
inserted in the wrong order. Its result does not replace an explicit typing
oracle or an actual input session.

Parser and Core tests own semantic breadth. Mounted tests keep one smallest case
per distinct visible failure mechanism rather than replaying the full semantic
matrix through Flutter.

## Exploration versus regression

Property-based and generated histories are discovery tools. Run them in bounded
targeted or nightly jobs. When they find a bug, minimize it and add one readable
direct regression test.

Generated histories must include uninterrupted typing and editing. Keep
Undo/Redo round-trip probes separate or occasional; inserting them after every
mutation changes the history grouping being explored. State invariants remain
useful, but they do not establish that the requested edit was meaningful.

Do not keep a permanent serialized journey language, universal driver, shadow
editor, or generated state model. Those systems are harder to trust than the
behavior they are meant to test.

## Demonstrate fault detection

At M2a closeout, temporarily introduce representative faults such as lost
replacement text, incorrect style/container membership, or wrong caret/context.
When the M3a input/paint harness exists, also exercise dropped or duplicated
input and one stale painted frame. Verify that a corresponding test fails on
the intended behavioral assertion, restore the code, and record a short result
with the milestone evidence. An unrelated crash or compilation failure does
not demonstrate detection.

These are a few disposable checks of the tests we rely on. Do not introduce
production fault switches, a mutation framework, or a mutation-score target.

## Shared test support

Kernel tests issue a production command and inspect the returned immutable
snapshot immediately. Mounted support carries that same snapshot through the
first subsequent frame and records every actual paint.

There is no parser-completion future in the synchronous core. Tests do not
sleep, poll private counters, or call `settle()` across the frame under test.

A new helper is justified only when it:

- removes repeated assertions from at least three tests;
- exposes a production state that ordinary test APIs cannot observe; or
- owns one cross-cutting invariant such as canonical/display caret identity.

Narrow source checks may reject a known forbidden dependency or parallel state
slot, but private-name assertions are not substitutes for architecture proof.
Preserve architecture primarily with typed boundaries, behavior tests, static
analysis, and review.

## Native and performance scope

The immediate handoff is [D0-web](../architecture/v5/web_dogfood.md). Use the
embedded browser for routine dogfooding, with exact-source checks and direct
browser transport regressions for discovered input failures. Native canaries
and performance remain a separate D0-macOS gate; shared failures block both.

Native canaries stay small. They prove delivery through the real OS, not
Markdown semantic breadth. The macOS canary set covers character input,
Return/Backspace routing, arrow and pointer selection, clipboard/history,
scrolling, focus, and sustained editing.

Begin brief attended macOS and phone sessions as soon as M3a's paragraph editor
is writable. Combine known failure sequences with unscripted editing, and
repeat after changes to input, selection, or composition. Record observations
against the candidate and minimize failures at the layer that owns them.
Formal qualification before D0 does not replace feedback during construction.

Performance qualification is separate from functional correctness. It uses the
document presets and budgets in [DOGFOOD_MILESTONE.md](../../DOGFOOD_MILESTONE.md)
and records production-path measurements. A passing functional suite does not
imply acceptable p99 latency or memory.

## Pull-request expectations

Every editing bug fix includes:

1. the smallest direct regression at the owning layer, demonstrated failing
   on the intended behavioral assertion before the fix;
2. an actual-paint regression when the failure was visible;
3. a native regression only when OS routing was necessary;
4. clean-parse equivalence for changed Markdown semantics; and
5. a focused performance measurement when the production hot path changed.

The fast local lane should stay under one minute warm. Full native and
performance qualification run before dogfood handoff; brief real-input checks
begin in M3a and repeat when the relevant behavior changes. Report implementation,
local tests, CI, and device evidence separately. Required checks that did not run
remain outstanding, including skipped or environment-blocked checks.

## Repeated failure families

Font geometry needs a real-font browser lane. Ahem and other synthetic test
faces can make a broken font fallback appear monospaced. Compare actual caret
columns for unequal-width glyphs with test fonts disabled, include a proportional
negative control, and verify the release application's bundled asset loading.
Checking `TextStyle.fontFamily` alone does not prove the face was used. Language
pickers also need empty, undetected and unchanged-choice journeys alongside
successful changes; a harmless dismissal must not display an edit rejection.

Browser clipboard tests must also send the initiating keyboard shortcut through
the engine and focus/shortcut chain before checking the clipboard event. Direct
paste-event injection cannot reveal a swallowed Cmd/Ctrl+V or an unintended
permission-dependent clipboard API call. Verify browser default handling and
exactly one semantic mutation; retain separate native keyboard and platform
action checks.

When another variant escapes a purported family-level fix, pause feature
expansion in that area. The next correction must identify the general rule,
why the previous test missed it, and the owning layer where the behavior can be
implemented once. Consolidate the rule and demonstrate its neighboring cases
before adding more features. Another exception plus another isolated example
is insufficient closure. If the rule is unclear, resolve the behavior contract
or architectural boundary; do not silently reduce common supported behavior.

Complexity is itself a regression. If a test helper becomes a generic framework
or needs more explanation than the cases it supports, replace it with direct
tests.
