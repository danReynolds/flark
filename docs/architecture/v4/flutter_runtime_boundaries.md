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
  |-- FlarkEditorCommandExecutor (typed native invocation, command ordering,
  |                               private command tickets)
  |-- FlarkEditorParseDriver (bounded native parser progression, streamed-head
  |                           probing, edit-publication certification,
  |                           edit/adoption generation barriers)
  |-- FlarkEditorSourceEditPlanner (pending-presentation evolution and
  |                                 publication certification decision)
  |-- FlarkEditorSemanticReceiptAdopter (generation-safe semantic receipt
  |                                      publication and viewport adoption)
  |-- FlarkEditorViewportAdopter (atomic queried-viewport publication,
  |                              navigation, and certified retirement)
  |-- FlarkEditorInputMutationPlanner (exact source-splice validation,
  |                                    continuation rewrite, bounded input
  |                                    result, and certification facts)
  |-- FlarkPlatformInputBridge (Flutter input connection, shadow, and one
  |                              normalized callback observation)
  |-- FlarkInputTransactionState (callback and bounded successor lineage)
  |-- FlarkEditorInputState (one bounded Flutter platform-input window)
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
| Typed native edit, semantic-action, history, and composition-cancel invocation; private ticket identity; history boundary ordering | `FlarkEditorCommandExecutor` in `flark` | Receipt-to-platform adoption, Flutter state, or an extensible command registry |
| Bounded native parser progression, streamed-head certification probes, edit-publication proof, edit/adoption barriers, and generation-bound parse publications | `FlarkEditorParseDriver` in `flark` | Timers, Flutter state, viewport installation, or outward notification |
| Parser-authorized pending-presentation evolution and the optimistic-versus-certified publication decision for one exact source splice | `FlarkEditorSourceEditPlanner` in `flark` | Native source mutation, Flutter input types, timers, callbacks, or outward publication |
| Generation-safe source publication, structural transition adoption, refresh anchoring, and bounded viewport mutation for one committed semantic receipt | `FlarkEditorSemanticReceiptAdopter` in `flark` | Flutter input installation, successor callbacks, parser waiting, or outward publication |
| Exact literal mutation validation, parser-authored inline-continuation rewrite, bounded scalar-safe input result, canonical selection, structural-certification classification, and composition-active fact | `FlarkEditorInputMutationPlanner` in `flark` | Platform callback normalization, hidden-projection admission, history adoption, native commands, or outward publication |
| Connection/window epochs, serialized platform shadow, atomic validation, and normalization of delta/full-value callbacks against current or provisional input into one immutable observation | Platform input bridge | Markdown rules, viewports, history, or source mutation |
| Callback scope, logical successor classification, bounded provisional/late lineage, paired platform actions, capture/deferral transitions, fallback insertion, composition base, and reconciliation accounting | Input transaction state | Markdown decisions, source mutation, rendered presentation, or execution of returned effects |
| Current bounded platform value, global origin, canonical selection mirrors, active row, and oversized-selection state | Flutter input state consuming portable input-window plans | Native source, Markdown rules, rendered rows, or command ordering |
| Bounded input facts, visible rows, marker hiding, styles, source/display mapping, selection projection, legal-caret normalization, hidden-only mutation classification, and visible-grapheme deletion mapping | Editor snapshot and surface projector in `flark` | Documents, timers, queues, callbacks, or Flutter types |
| Evolution and certified retirement of parser-authorized pending rows, structural transition ownership, and successor caret boundaries | Pending-presentation evolution in `flark` | Markdown inference, Flutter types, mutation callbacks, or asynchronous work |
| Conversion between portable input facts and Flutter text types | Flutter text adaptation | Source mutation, Markdown rules, or retained editor state |
| Bounded viewport, rows, visible source, certification, and optimistic coordinate mapping | `FlarkEditorViewportState` in `flark` | Native queries, input restoration, publication, or Flutter types |
| Native viewport queries, continuation lifetime, stale-result rejection, ordered page path, and retained refresh origin | `FlarkEditorViewportPager` in `flark` | Input restoration, publication, Flutter types, or mutable render state |
| Atomic queried-viewport receipt adoption across navigation, bounded source/rows, source publication generation, and certified pending-presentation retirement | `FlarkEditorViewportAdopter` in `flark` | Flutter input restoration, notification, timers, layout, or paint |
| Public commands, Flutter callbacks, receipt-to-platform adoption, lifecycle, and composing the owners above | Controller | Parallel copies of owner state or direct native command invocation |

The bounded active input window, literal input-mutation planning, native command
invocation, native parse progression, ordinary source-edit presentation
planning, and committed semantic-receipt publication now each have one tested
owner. Edit completion also delegates its parser-certification and phase-proof
loop to that same driver. The remaining failed ownership check is host
reconciliation: the controller still executes the transaction owner's typed
successor effects and combines them with parser timer admission,
lifecycle/publication routing, and history/composition host adoption.

## Rules that prevent the bug classes we have seen

1. An asynchronous result carries an identity-checked command ticket and edit
   generation. A stale result cannot publish source, adopt presentation, or
   clear a newer edit's barrier.
2. One accepted edit has one command lifetime and enters one serialized edit
   tail. Completion is exact-once; history lifetimes exclude later commands.
   Parser and page work are independently single-flight.
3. A platform callback is validated against one serialized shadow before any
   member of its delta batch is applied. A bad batch applies nothing. Delta and
   full-value callback shapes converge to one observation before edit policy.
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
  Flutter input successor classification, reservation, pending/late capture,
  deferral, fallback insertion, metric accounting, and retirement have one
  tested state owner. The controller no longer mutates successor collections,
  provisional tails, or certification completers; it executes the owner's
  typed shadow, resync, and promotion effects. Native command admission,
  invocation, ticket identity, and ordering have one portable executor.
  Task-action, history/composition adoption, and successor effect execution
  remain in the facade until each has a narrow outcome that does not call back
  into controller state;
- Flutter delta and full-value callbacks now normalize through one immutable
  platform observation for ordinary edits, provisional semantic successors,
  late receipt races, and certification-deferred commands. The controller no
  longer validates or extracts raw delta mutations itself;
- viewport query and page-path orchestration now return one typed Core result;
  the remaining restoration handoff should move only when its portable
  selection outcome can replace, rather than call back into, controller state;
- bounded parser pumping, streamed-head probes, edit-publication certification,
  phase-safe viewport proof, and edit/adoption barriers now return typed
  generation-bound Core publications. Flutter retains timer admission and
  installs the resulting viewport; those are the next seams to judge
  independently rather than folding lifecycle into the driver;
- ordinary source edits now pass bounded host-neutral input facts to one
  portable planner. The planner evolves parser-authorized continuity,
  structural edit cells, paragraph gaps, and caret boundaries, then returns
  the one publication requirement Flutter must honor. Receipt adoption and
  preferred platform-window routing remain in the facade until they can move
  as a complete state transition;
- committed semantic receipts now advance source publication, pending
  presentation, refresh anchoring, and the bounded viewport through one
  generation-safe portable adopter. The existing pure input-window planner
  also computes the receipt's bounded result window; Flutter only installs
  that value and reconciles platform-specific successors;
- ordinary literal input now delegates exact splice validation,
  parser-authored inline-continuation rewrite, bounded scalar-safe output,
  canonical selection, structural-certification classification, and an
  explicit composition-active fact to the pure input-mutation planner. Flutter
  retains hidden-projection admission, history/composition adoption, native
  command execution, and platform publication;
- canonical selection restoration now delegates same-row, cross-row,
  collapsed, and capacity-exceeding placement to the same portable input-window
  planner. An oversized history selection keeps its exact Core endpoints while
  Flutter exposes only a bounded collapsed surrogate; the controller no longer
  rebuilds selection windows with a parallel branch tree;
- projected selection normalization, hidden-only mutation classification, and
  Backspace/Delete mapping across hidden delimiters now live with the portable
  surface topology that defines those caret stops. Flutter consumes a typed
  boundary-or-source-range result and no longer reimplements projection gaps;
- queried viewport receipts now cross one portable atomic adopter before
  Flutter restores input. Page navigation, bounded source/rows, source paint
  generation, refresh origin, task-check retirement, and structural/gap
  retirement can no longer advance as separate controller branches;
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
- Is the new boundary directly tested at its narrowest truthful layer, without
  requiring a Flutter widget when it owns no Flutter behavior?
- Did the full native-backed editor suite and relevant performance lane stay
  green?
