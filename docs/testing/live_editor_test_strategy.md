# Live-editor test strategy

**Status:** direct v4 test architecture

**Date:** 2026-08-14

## Decision

Flark uses ordinary tests plus one small temporal probe. It does not use a
serialized scenario language, universal driver interface, or duplicated
headless/mounted corpus.

The rule is simple: put every assertion at the lowest layer that can observe
the failure, and add a higher-layer test only when it contributes new evidence.

## The four evidence lanes

| Lane | Proves | Main location |
|---|---|---|
| Engine/Core | GFM semantics, incremental-versus-clean equivalence, transactions, anchors, history, queues, UTF and failure behavior | Rust tests and `flark_core/test` |
| Controller transitions | Source, selection, projection, and every synchronous state exposed during one logical action | `live_editor_transition_test.dart` |
| Mounted surface | Actual paint, layout, geometry, hit testing, scrolling, semantics, and visual transients | focused Flutter widget tests and `live_editor_transition_surface_test.dart` |
| Native canaries | A few facts that require an OS: key routing, pointer selection, clipboard shortcuts, and scrolling | `macos_native_canary_test.dart` |

Performance and physical-device qualification are separate evidence. A green
functional test does not prove p99 latency, memory, IME behavior, touch UX, or
thermal behavior.

The GFM/CommonMark JSON corpus remains. It is parser input data with an
independent semantic oracle, not a live-editor journey framework.

## Temporal transition probe

`LiveEditorTransitionProbe` opens the real Rust -> Core -> controller path and
records immutable synchronous samples. It never performs an asynchronous
source read from a listener, because that could associate a later source with
an earlier publication.

For one action the observable phases are:

1. state before the host callback;
2. every synchronous controller publication;
3. state at callback return;
4. the first mounted frame after the action, when paint is relevant;
5. mutation-settled state;
6. presentation-settled state;
7. terminal comparison with a clean rebuild, when semantic convergence is the
   property under test.

The deterministic barriers are controller futures. Tests do not poll
`pendingEdits`, sleep, or guess when parsing has caught up.

Every sample automatically asserts mechanical invariants: valid projected
ranges and selection, the platform input selection representing the canonical
selection, exact runs matching their represented source, no resync or fault,
and bounded state. Tests then add the smallest user-visible invariant, such as
“an unrelated strong block is never exposed as `**source**`.”

The mounted recorder observes `FlarkSurfacePaintObservation` from the production
render object. Its receipt includes revision, visible source range, rendered
rows, row geometry, selection rectangles, caret rectangle, scroll offset, and
content/visual hashes. Each painted caret also reports the source offset it
represents; the recorder automatically requires that offset to equal the
controller's canonical extent in the same frame. Controller publication is not
accepted as proof that a frame painted.

## What stays an ordinary test

Most coverage belongs in direct parameterized tests:

- parser construct matrices and incremental-versus-clean properties;
- Return/Backspace intent resolution for paragraphs, lists, quotes, headings,
  code blocks, tables, and boundaries;
- full-value/delta/selector arbitration and duplicate platform entrances;
- selection replacement, paste, undo/redo, composition, graphemes, caps,
  faults, and multiple sessions;
- projection algebra and certification authority;
- layout mapping, gestures, navigation, virtualization, accessibility, and
  read-only behavior.

A transition test is admitted only for a transient or cross-layer invariant
that those final-state tests cannot prove. A mounted transition is added only
when actual paint or geometry matters. This prevents every semantic case from
being duplicated through Flutter.

## Regression examples

The initial transition family protects failures found in dogfooding:

- punctuation such as `*`, `[`, `` ` ``, `~`, `>`, and escapes must not relay
  unrelated certified blocks to raw Markdown;
- Return or Backspace followed immediately by typing must preserve one exact
  source/selection lineage;
- an incremental result must converge to the same semantic presentation as a
  clean rebuild;
- a source newline used as an editor-owned blank row must occupy one visual
  line, and Backspace must remove it.

The syntax family is compact and data-driven. It is not one bespoke test per
punctuation mark.

## Native canaries

Native canaries deliberately do not replay the semantic suite. The macOS pack
reuses one app process and checks five routing boundaries:

1. real character input, including syntax punctuation, reaches the intended
   source position without a raw-projection flash;
2. real Return and Backspace route once and preserve exact source/selection;
3. pointer selection plus cut/undo uses the real AppKit paths;
4. wheel scrolling changes scroll position without changing selection.
5. sustained human-cadence editing across wrapped Markdown and internal layout
   fragments never rehomes the visible caret.

Before sending native input, the actuator requires the exact dogfood PID to be
frontmost and accessibility-focused, waits for requested window geometry to
converge, and verifies the expected source selection again. A canary is invalid
if any of those preconditions drift; it must not type into an assumed target.

iOS and Android later get similarly small canaries for touch, clipboard/menu,
lifecycle, and accessibility routing. Real IME, autocorrect, dictation,
selection handles, VoiceOver/TalkBack, and device performance remain physical
qualification.

## Goldens

Goldens are sparse. Keep them for a stable visual contract that cannot be
expressed more precisely as text, style, geometry, and semantic facts. Good
examples are one dense mixed-Markdown page, selection/caret decoration, and a
read-only parity surface. Do not create a screenshot for every edit case or
transient frame.

## Budgets

- transition-probe support: target <= 450 lines; redesign at 600;
- fixed controller transitions: <= 15 focused tests;
- mounted transition tests: <= 8;
- native canaries: <= 6 per platform;
- goldens: <= 8 stable contracts;
- focused transition process: <= 6 seconds warm on the benchmark Mac;
- warm PR functional gate: target < 60 seconds, hard review at 90 seconds;
- pre-dogfood functional plus native canaries: < 3 minutes;
- generated/property exploration: nightly or targeted, < 5 minutes per lane.

One-time phase-sensitive regressions may be repeated 100 times before landing.
That repetition is a stability receipt, not routine PR work.

## Admission rule

Add a test only if it owns a distinct failure mechanism or evidence boundary.
When a bug is found:

1. reproduce it at the lowest observable layer;
2. add a temporal controller test only if an intermediate publication was
   wrong;
3. add a mounted test only if an actual frame or geometry was wrong;
4. add a native canary only if OS routing was necessary to reproduce it;
5. add a performance case only if measurement identifies a budget regression.

Delete helpers that become generic test languages, duplicate assertion logic,
or require broad fixture migration. Complexity is itself a regression.

References: [RFC 028](../architecture/rfc/rfc_028_source_authoritative_edit_transactions.md),
[v4 build plan](../architecture/v4/build_plan.md), and
[physical IME matrix runbook](ime_device_matrix_runbook.md).
