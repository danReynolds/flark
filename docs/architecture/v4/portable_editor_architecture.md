# Portable editor architecture

**Status:** active implementation contract, 2026-08-29.

This document replaces the v4 assumption that the public `flark` package is
Flutter-specific. It narrows the architecture program to one outcome: a
portable Dart editor kernel with Flutter as an adapter, while Rust remains the
sole Markdown and source authority.

## Destination

```text
flark-engine / flark-parser / flark-runtime     Rust
  canonical source, Markdown, certification, edit recipes
                       |
                  flark-abi
                       |
                       v
flark                                           pure Dart
  document/session API, selection and history policy,
  editor coordination, immutable bounded editor snapshot
                       |
                       v
flark_flutter                                   Flutter
  TextInput adaptation, widgets, layout, paint, gestures,
  semantics, accessibility, platform lifecycle
```

The former `flark_core` package is now the public pure-Dart `flark` package.
The former Flutter `flark` package is now `flark_flutter`; package names and
responsibilities therefore agree during the remaining architecture work.

Rust remains callable by another language through the host-neutral ABI. We do
not invent Swift, Kotlin, or Web SDK abstractions before a real second-language
consumer. Host-independent Markdown behavior must nevertheless remain below
the ABI; pure Dart must not become a second Markdown implementation.

## One owner for each truth

| Truth | Final owner |
| --- | --- |
| Exact source, revisions, Markdown meaning, certification, semantic edit recipes | Rust runtime |
| Canonical Dart selection/history policy and editor command sequencing | `flark` |
| Pending work lineage and permission to publish a bounded state | `flark` editor coordinator |
| Current bounded source/presentation/input/navigation state exposed to a host | immutable `FlarkEditorSnapshot` |
| Flutter connection epochs and normalization of `TextEditingValue`/delta callbacks | `flark_flutter` input adapter |
| Layout, paint, hit testing, gestures, semantics, accessibility | `flark_flutter` surface |

The snapshot is a revision-bound publication, not another document model. It
contains only bounded host-facing state and cannot mutate the native document.

## Update cycle

```text
host observation -> typed command -> coordinator
coordinator -> bounded Core/native work -> typed receipt
receipt + current generation -> next immutable snapshot
snapshot -> input adapter and renderer
```

There is one outward snapshot publication point. An asynchronous completion
cannot mutate public state directly; it returns a generation-stamped receipt
to the coordinator. Flutter does not infer Markdown meaning or decide whether
stale presentation remains valid.

This is editor-specific infrastructure. There is no generic event bus, plugin
container, service locator, or reducer framework.

## Peer reference shape

- [CodeMirror 6](https://codemirror.net/docs/guide/) separates immutable state
  and transactions from an imperative, viewport-rendered view.
- [ProseMirror](https://prosemirror.net/docs/guide/) separates model, state,
  transforms, and view around one transaction path.
- [Lexical](https://github.com/facebook/lexical/blob/main/AGENTS.md) keeps an
  immutable committed editor state in a framework-agnostic core with separate
  headless and framework integration packages.

Flark adopts the state/update/view separation, not a peer's canonical rich-text
tree or plugin surface. Exact Markdown source and bounded parser-certified
projection remain Flark's differentiators.

## Milestones and review gates

### M0 — allocation and baseline

- Freeze this authority map and record current structural metrics.
- Preserve the existing native-backed Core and Flutter suites as the
  equivalence harness.
- Reject any next step that requires a reverse dependency or a second state
  path.

Review gate: the destination names every current correctness authority and is
compatible with non-Flutter Dart without weakening the North Star.

Baseline at the start of M0:

| Measure | Value |
| --- | ---: |
| Flutter production Dart under `lib/src` | 14,905 lines |
| Headless production Dart under `lib/src` | 14,455 lines |
| Flutter controller | 7,587 lines (50.9% of Flutter production source) |
| Approximate controller method declarations | 220 |
| Controller notification call sites | 49 |
| Core + Flutter Dart test source | 27,367 lines across 42 files |

The baseline is not a quota. It makes regressions and method-only file splits
visible during milestone review.

### M1 — package truth and immutable publication

- Rename the pure-Dart package to `flark` and the Flutter package to
  `flark_flutter` atomically.
- Promote the existing immutable surface publication into one complete bounded
  `FlarkEditorSnapshot` consumed by the Flutter surface.
- Keep one production behavior path; compatibility barrels may re-export but
  may not contain behavior.

Review gate: dependency analysis proves `flark` imports no Flutter library;
the renderer consumes a captured snapshot; Core and Flutter suites pass.

### M2 — portable editor coordination

- Introduce one editor-specific coordinator in `flark`.
- Move command sequencing, generation-stamped receipt adoption, publication
  permission, and host-neutral pending presentation into it by complete
  responsibility.
- Give every admitted edit one identity-checked command lifetime; stale
  commands cannot publish source or adopt pending presentation, and history
  lifetimes are exclusive.
- Migrate literal edits, semantic edits, history, selection, and viewport
  commands onto the same command/receipt cycle.

Portable viewport page history, optimistic range mapping, and atomic viewport
adoption decisions live in `flark`; Flutter retains only the query-to-input and
query-to-render handoff until those effects have narrow typed outcomes.

Review gate: each migration removes state and branches from the Flutter
controller, adds direct coordinator tests, and preserves observable behavior.
A helper that calls back into the controller does not qualify.

### M3 — thin Flutter adapter

- Move the immutable snapshot, surface rows, and deterministic source/display
  projector into `flark`; represent the bounded platform input facts with
  host-neutral UTF-16 value types.
- Make the Flutter controller an API/notification adapter over the portable
  coordinator.
- Confine `TextEditingValue`, delta/full-value callback quirks, connection
  epochs, composition routing, widgets, and rendering to `flark_flutter`.
- Make the render surface depend on `FlarkEditorSnapshot` plus typed actions,
  never on the controller.
- Delete the old orchestration and transitional aliases.

Review gate: the controller owns no parser scheduling, Markdown transition,
history, viewport algorithm, pending-presentation policy, or scattered
publication state. File reduction is evidence of moved authority, not a target
by itself.

Current checkpoint: snapshot and deterministic projection ownership is now in
`flark`, with one explicit Flutter text conversion boundary. The complete
native-backed Dart and Flutter suites pass, including the 1 MiB and 5 MiB paint
scenarios. Parser-authorized pending-presentation evolution also operates on
framework-neutral Core rows in `flark`; Flutter no longer splices projection
continuity, decides when certified parser facts retire that continuity, or
splices edit-cell result shells. Bounded viewport, row, visible-source,
certification, and optimistic-coordinate state now advance through one tested
portable state machine. Native viewport querying, continuation cleanup,
generation-safe result adoption, and ordered page paths now run through one
tested portable pager; Flutter synchronously installs its typed result and
restores platform input. Rust now publishes the row-level boundary modes that
may enter structural edit intents, so Flutter no longer derives semantic
command eligibility from Markdown kinds or containers. Flutter-specific
successor classification and bounded lineage lifecycle have one tested input
transaction owner. Bounded platform value, global UTF-16 origin, canonical
selection mirrors, active row, oversized selection, and restoration now
advance through one directly tested Flutter input-state owner, while pure
`flark` plans the capacity-bounded scalar-aligned window, parser-row
activation, neutral-line ownership, and pending boundary restoration shared by
any future host adapter. The controller-facade work is therefore smaller but
not complete. Typed source edits, semantic edits/actions, history, and
composition cancellation now enter one portable command executor that owns
native invocation, ordering, and private coordinator tickets; Flutter retains
receipt-to-input/viewport adoption rather than native command authority.

### M4 — large-module audit and deletion

- Audit the remaining large production modules by reasons-to-change, mutable
  ownership, fan-in/fan-out, and direct testability.
- Split only independently owned layout, paint, hit-testing, semantics,
  transport, or codec responsibilities.
- Delete forwarding layers and obsolete tests/docs created by the old path.

Review gate: every retained large module is a cohesive deep module with a
narrow interface. No split shares a controller/state bag.

Current audit:

| Module | Verdict | Reason |
| --- | --- | --- |
| Flutter controller | Further boundary work required | Bounded input-window state and native command execution have moved, but the controller still combines platform callback admission, receipt reconciliation, parser scheduling, and lifecycle/publication routing. |
| Pure command executor | Retain | It is a closed typed native-command lane with private coordinator tickets and direct lifetime/ordering tests. It owns no host adoption state, callback registry, or generic dispatch. |
| Pure input-window planner | Retain | It owns capacity, scalar-aligned cuts, local-to-canonical selection equivalence, and restoration from parser/pending surface geometry without a frontend dependency. |
| Flutter input state | Retain | It owns one `TextEditingValue` window and its canonical mirrors, adapting immutable plans through named transitions. It imports no controller and its oversized-selection invariants have direct tests. |
| Rust runtime document | Retain while its public surface stays narrow | It is the deep source/parser transaction boundary; edit-intent resolution is already separate. Split only when a codec, parser job, or transaction owner can move without sharing document internals. |
| Dart native document | Retain pending a codec-sized extraction | Its size comes from one serialized actor plus strict ABI decoding. A split is useful only if a stateless decoder can be tested independently without duplicating FFI lifecycle state. |
| Flutter render surface | Retain | Layout, paint, hit testing, selection geometry, and semantics are one custom render-object protocol and already consume immutable snapshots. |

The bounded input-window extraction is accepted: it moved state plus the
windowing/restoration invariants, has no controller callback, and is protected
against raw-setter regression. The native command executor is also accepted:
it moved command admission, invocation, ticket identity, and history ordering
below Flutter; its execution receipt keeps host adoption explicit without
exposing coordinator tickets. The next review must identify a typed outcome
that removes receipt-reconciliation or parser/lifecycle branches from Flutter;
another bag of controller fields or callback-forwarding helper does not count.

### M5 — qualification and architecture stop

- Run static analysis, complete native-backed Core and Flutter suites, actual
  paint scenarios, native platform canaries, and production-path scale gates.
- Re-measure dependency direction, controller responsibilities, publication
  sites, production source shape, and test organization.
- Compare the result with the state/transaction/view separation used by
  CodeMirror, ProseMirror, and Lexical.

The architecture program stops when the criteria below hold. Further work
returns to product testing and dogfood findings.

## Completion criteria

- Pure Dart can use `flark` without Flutter in its dependency graph.
- `flark_flutter` contains all Flutter imports and no Markdown policy.
- One typed command/receipt cycle produces one immutable bounded snapshot.
- Renderer and platform input consume that snapshot without controller reads.
- The Flutter controller is a small facade with one outward publication path;
  it contains no application state machine or asynchronous Core effect logic.
- Rust remains the only source and Markdown authority.
- No old/new dual path, mutable compatibility layer, or controller-sharing
  extraction remains.
- Direct boundary tests and the full behavior/paint/native/performance suites
  pass without a material regression.

Line count is a diagnostic. A large file fails the architecture only when it
combines independent authorities or exposes a broad, state-sharing interface.

## Falsifiers

Stop and revise the design if any milestone:

- introduces a second source, selection, presentation, or publication truth;
- moves methods without moving their mutable state and invariants;
- requires lower layers to import or call the Flutter controller;
- makes ordinary input wait on document-sized work or copies the full document
  into a host snapshot;
- needs a generic framework to express the editor's fixed command cycle; or
- passes tests only by weakening intermediate-frame, native, or performance
  assertions.
