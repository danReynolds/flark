# Flutter editor runtime boundaries

This is the maintainability contract for the v4 Flutter editor. It applies the
[North Star](../../../NORTH_STAR.md) to code ownership without introducing a
second product model.

## Dependency direction

```text
FlarkEditorController (public facade and UI coordination)
  |-- FlarkEditorRuntimeState (time, lifecycle, async lineage)
  |-- FlarkPlatformInputBridge (Flutter input connection and shadow)
  |-- FlarkSurfaceProjector (pure source-to-visible publication)
  |-- FlarkViewportInstallationPlan (pure viewport adoption decision)
  `-- flark_core (source, selection, history, Markdown, edit authority)
```

The boundary components never import the controller. `flark_core` never
imports Flutter. Markdown recognition and permission to retain rendered
semantics remain in Rust/Core.

## One owner for each kind of truth

| Concern | Owner | Must not own |
| --- | --- | --- |
| Source, selection, history, Markdown semantics, semantic edit receipts | Core | Flutter input connections or widgets |
| Edit generations, lifecycle, serialized edit tail, parser/page single-flight, publication barriers | Editor runtime | Markdown rules or rendered rows |
| Connection/window epochs, serialized platform shadow, delta/value validation and classification | Platform input bridge | Markdown rules, viewports, or history |
| Visible rows, marker hiding, styles, source/display mapping, selection projection | Surface projector | Documents, timers, queues, or callbacks |
| Whether a viewport result can atomically replace or certify the current surface | Viewport installation plan | Mutation or asynchronous work |
| Public commands, Flutter callbacks, and composing the owners above | Controller | Parallel copies of owner state |

## Rules that prevent the bug classes we have seen

1. An asynchronous result carries an edit-generation stamp. A stale result
   cannot publish or clear a newer edit's barrier.
2. One accepted edit enters one serialized runtime tail. Parser and page work
   are independently single-flight.
3. A platform callback is validated against one serialized shadow before any
   member of its delta batch is applied. A bad batch applies nothing.
4. One surface publication is projected from one immutable captured state.
   Projection cannot read a document or mutate controller state while rows are
   being built.
5. A certified empty viewport is a valid current semantic result; row-cache
   replacement and certification are separate decisions.
6. Optimistic source mapping may preserve semantics only where a parser-authored
   edit receipt allows it. It fails closed for structural uncertainty.

## Controller reduction policy

The controller is still larger than the intended facade. Remaining work should
be extracted by authority, not by file length:

- semantic input transaction orchestration should become one bounded lane with
  explicit inputs and outcomes;
- viewport query/restore orchestration should become one coordinator after its
  current state transitions are fully pinned; and
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
