# Flutter editor runtime boundaries

This is the maintainability contract for the v4 Flutter editor. It applies the
[North Star](../../../NORTH_STAR.md) to code ownership without introducing a
second product model.

The final package and application-state destination is defined by
[Portable editor architecture](portable_editor_architecture.md). This document
governs the current Flutter implementation while it is cut over to that
portable kernel.

## Dependency direction

```text
FlarkEditorController (Flutter facade and adapter coordination)
  |-- FlarkEditorCoordinator (pure Dart command lifetimes, async lineage,
  |                           publication permission, pending presentation)
  |-- FlarkPlatformInputBridge (Flutter input connection and shadow)
  |-- FlarkInputTransactionState (callback and bounded successor lineage)
  |-- Flutter text adaptation (TextEditingValue <-> portable UTF-16 facts)
  |-- FlarkEditorSnapshot / FlarkSurfaceProjector (`flark` immutable bounded
  |                                                 visual publication)
  |-- FlarkViewportInstallationPlan (`flark` viewport adoption decision)
  |-- FlarkEditorViewportPager (`flark` native queries, stale-result cleanup,
  |                             page path, and refresh origin)
  `-- flark (source, selection, history, Markdown, edit authority)
```

The boundary components never import the controller. `flark` never imports
Flutter. Markdown recognition and permission to retain rendered semantics
remain in Rust and headless Dart.

The intended end state is a small public controller over one application-level
edit coordinator. That coordinator admits typed commands and asynchronous Core
receipts, then publishes one immutable editor snapshot to the platform adapter
and renderer. The current boundary work is converging on that shape; it is not
permission to preserve the controller's existing method graph.

## One owner for each kind of truth

| Concern | Owner | Must not own |
| --- | --- | --- |
| Source, selection, history, Markdown semantics, structural-command routing capabilities, semantic edit receipts, pending-presentation adoption policy | `flark` and Rust | Flutter input connections or widgets |
| Identity-checked command lifetimes, edit generations, serialized edit tail, parser/page single-flight, publication barriers, current pending presentation | `FlarkEditorCoordinator` in `flark` | Flutter types, Markdown rules, or rendered rows |
| Connection/window epochs, serialized platform shadow, delta/value validation and classification | Platform input bridge | Markdown rules, viewports, or history |
| Callback scope, logical successor classification, bounded provisional lineage, paired platform actions, composition base, reconciliation accounting | Input transaction state | Markdown decisions, source mutation, or rendered presentation |
| Bounded input facts, visible rows, marker hiding, styles, source/display mapping, selection projection | Editor snapshot and surface projector in `flark` | Documents, timers, queues, callbacks, or Flutter types |
| Evolution and certified retirement of parser-authorized pending rows, structural transition ownership, and successor caret boundaries | Pending-presentation evolution in `flark` | Markdown inference, Flutter types, mutation callbacks, or asynchronous work |
| Conversion between portable input facts and Flutter text types | Flutter text adaptation | Source mutation, Markdown rules, or retained editor state |
| Bounded viewport, rows, visible source, certification, and optimistic coordinate mapping | `FlarkEditorViewportState` in `flark` | Native queries, input restoration, publication, or Flutter types |
| Native viewport queries, continuation lifetime, stale-result rejection, ordered page path, and retained refresh origin | `FlarkEditorViewportPager` in `flark` | Input restoration, publication, Flutter types, or mutable render state |
| Public commands, Flutter callbacks, and composing the owners above | Controller | Parallel copies of owner state |

## Rules that prevent the bug classes we have seen

1. An asynchronous result carries an identity-checked command ticket and edit
   generation. A stale result cannot publish source, adopt presentation, or
   clear a newer edit's barrier.
2. One accepted edit has one command lifetime and enters one serialized edit
   tail. Completion is exact-once; history lifetimes exclude later commands.
   Parser and page work are independently single-flight.
3. A platform callback is validated against one serialized shadow before any
   member of its delta batch is applied. A bad batch applies nothing.
4. Callback and platform-mutation scopes cannot nest. A Return or Backspace
   text observation consumes at most one companion action/selector callback.
5. One `FlarkEditorSnapshot` is projected and published through one function.
   Projection cannot read a document or mutate controller state while rows are
   being built; the renderer reads no mutable visual truth from the controller.
6. A certified empty viewport is a valid current semantic result; row-cache
   replacement and certification are separate decisions.
7. Optimistic source mapping may preserve semantics only where a parser-authored
   edit receipt allows it. It fails closed for structural uncertainty.
8. Viewport page index is derived from one ordered anchor path. Moving forward,
   backward, or adopting a refresh replaces that path atomically.
9. Flutter does not switch over Markdown transition kinds to decide structural
   chaining, row retirement, or parser-certification requirements. Core reduces
   its typed receipt into one pending-presentation adoption outcome.
10. A viewport query returns an unapplied generation-bound receipt. Page history
    advances only during synchronous adoption; a zero continuation produces no
    asynchronous cleanup handoff.
11. Rust publishes structural-command routing capabilities with each certified
    row. Flutter may check caret geometry against those capabilities, but it
    does not reconstruct eligibility from Markdown row kinds or containers.

## Controller reduction policy

The controller is still larger than the intended facade. Remaining work should
be extracted by authority, not by file length:

- structural-command admission now consumes parser-authored capabilities, and
  Flutter input successor classification, reservation, and retirement have one
  tested state owner. Receipt-driven source/render promotion remains in the
  facade until it has a narrow outcome that does not call back into controller
  state;
- viewport query and page-path orchestration now return one typed Core result;
  the remaining restoration handoff should move only when its portable
  selection outcome can replace, rather than call back into, controller state;
- command-specific Markdown behavior must move only through new Core receipts,
  never into a Flutter helper.

An extraction is a win only when it removes mutable ownership from the
controller, has no reverse dependency, adds a direct invariant test, preserves
the full behavior suite, and does not regress the production-path performance
gate. Moving methods into `part` files or sharing controller fields does not
qualify.

## Review checklist

- Does every new mutable field have exactly one named owner?
- Can an old async completion affect a newer edit or interaction?
- Can presentation code observe state changing halfway through a publication?
- Did any Flutter code infer Markdown meaning or edit authority?
- Is the new boundary directly testable without opening a native document?
- Did the full native-backed editor suite and relevant performance lane stay
  green?
