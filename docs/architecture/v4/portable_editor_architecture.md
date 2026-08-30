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
Its projector also owns the inverse editing topology: legal source carets,
hidden-only selections, and the exact source grapheme adjacent to a rendered
Backspace/Delete command.

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
activation, neutral-line ownership, pending boundary restoration, and exact
same-row/cross-row/oversized selection restoration shared by any future host
adapter. Capacity-exceeding history selections retain their exact canonical
endpoints behind a bounded collapsed platform surrogate. The controller-facade
work is therefore smaller but
not complete. Typed source edits, semantic edits/actions, history, and
composition cancellation now enter one portable command executor that owns
native invocation, ordering, and private coordinator tickets; Flutter retains
receipt-to-input/viewport adoption rather than native command authority.
Flutter's delta and full-value text-service APIs now reduce to one immutable
platform observation before ordinary edit policy, so equivalent callbacks no
longer maintain parallel newline, Backspace, selection, and oversized-window
implementations. The same observation can be based on an explicit provisional
window, so pending/late semantic successors and certification-deferred input
also share one capture path across both callback models. Bounded native pumping,
streamed-head certification probes, edit-publication phase proof, and the
edit/adoption tail barriers now run through one pure-Dart parse driver. It
returns owned generation-bound publications and never installs Flutter state
or calls back into the controller. Flutter still owns timer admission and
viewport/input adoption, so this is an accepted parse/proof boundary rather
than a claim that parser lifecycle has fully left the facade. Ordinary source
edits now also pass one bounded host-neutral request to a pure-Dart source-edit
planner. That planner owns pending dependency continuation, structural edit
cell advancement, transient boundary retirement, and the fail-closed
optimistic-versus-certified publication decision; Flutter owns the subsequent
native command receipt and platform-window adoption. Committed semantic
receipts now cross a separate generation-safe portable adopter that publishes
source identity, resolves and adopts the parser-authored structural
transition, pins refresh navigation, and advances the bounded viewport as one
synchronous transaction. The pure input-window planner computes the paired
bounded splice result; Flutter installs it and retains only platform successor
reconciliation. Ordinary literal mutations now cross a companion pure
input-mutation planner that owns exact splice validation, parser-authored
inline-continuation rewrites, scalar-safe bounded results, canonical selection,
structural-certification classification, and an explicit composition-active
fact. Keeping that last fact separate from the possibly truncated platform
composing range prevents bounded windowing from ending a Core composition
group.

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
| Flutter controller | Further boundary work required | Bounded input-window state and literal mutation planning, platform successor lineage mutation, native command execution, native parse progression, edit-publication proof, ordinary source-edit planning, and committed semantic-receipt publication have moved, but the controller still executes successor effects and combines parser timer admission, history/composition host adoption, and lifecycle/publication routing. |
| Pure command executor | Retain | It is a closed typed native-command lane with private coordinator tickets and direct lifetime/ordering tests. It owns no host adoption state, callback registry, or generic dispatch. |
| Pure parse driver | Retain | It owns bounded native pumping, streamed-open certification probing, edit-publication phase proof, generation barriers, and owned parse publications. It has no timer, callback, Flutter, or host-state dependency and is directly contract-tested. |
| Pure source-edit planner | Retain | It synchronously evolves the coordinator's one pending-presentation snapshot from bounded portable input facts and returns one publication requirement. It has no Flutter, native-I/O, callback, or timer dependency; direct tests cover authorized continuity, exact fallback, single structural advancement, barriers, invalid input, and ambiguous-authority failure. |
| Pure semantic receipt adopter | Retain | It owns one synchronous, identity-checked committed-receipt transaction across source publication, pending presentation, refresh navigation, and bounded viewport state. It returns only the caret, continuation, and certification obligation a host must install, and direct native tests reject superseded receipts. |
| Flutter platform input bridge | Retain | It owns connection epochs, the serialized shadow, atomic delta validation, and one immutable normal form shared by delta/full-value callbacks against current or provisional input. It knows no Markdown, viewport, or source-mutation policy. |
| Flutter input transaction state | Retain | It is the sole mutable owner of callback scopes, paired actions, provisional/late successor lineage, capture and deferral transitions, certification-completer lifetime, and successor accounting. It returns typed shadow, resync, and late-promotion effects; it knows no Markdown, viewport, native document, or rendered presentation. |
| Pure input-window and mutation planners | Retain | They own capacity, scalar-aligned cuts, local-to-canonical selection equivalence, collapsed/same-row/cross-row/oversized restoration from parser and pending surface geometry, exact literal splice validation, parser-authored continuation rewrite, structural-certification classification, and composition activity without a frontend dependency. |
| Pure surface projector | Retain | It is the bounded source/display topology in both directions: immutable rows, legal caret normalization, hidden-only mutation classification, and rendered-grapheme deletion mapping. It owns no document, timer, callback, or frontend type. |
| Flutter input state | Retain | It owns one `TextEditingValue` window and its canonical mirrors, adapting immutable plans through named transitions. It imports no controller and its oversized-selection invariants have direct tests. |
| Rust runtime document | Retain while its public surface stays narrow | It is the deep source/parser transaction boundary; edit-intent resolution is already separate. Split only when a codec, parser job, or transaction owner can move without sharing document internals. |
| Dart native document | Retain pending a codec-sized extraction | Its size comes from one serialized actor plus strict ABI decoding. A split is useful only if a stateless decoder can be tested independently without duplicating FFI lifecycle state. |
| Flutter render surface | Retain | Layout, paint, hit testing, selection geometry, and semantics are one custom render-object protocol and already consume immutable snapshots. |

The bounded input-window extraction is accepted: it moved state plus the
windowing/restoration invariants, has no controller callback, and is protected
against raw-setter regression. The native command executor is also accepted:
it moved command admission, invocation, ticket identity, and history ordering
below Flutter; its execution receipt keeps host adoption explicit without
exposing coordinator tickets. Ordinary platform callbacks now also converge on
one typed observation/adoption path, deleting the duplicate delta/full-value
policy without adding mutable state. Provisional, late, and
certification-deferred successors now reuse that same normal form too; the
controller no longer owns raw delta validation or extraction. Portable parse
progression and edit-publication proof are accepted on the same grounds: they
moved the native pump/probe, document-phase proof, and generation-barrier
invariants, issue identity-bound publications, and leave Flutter with explicit
host-state adoption. Portable ordinary source-edit planning is accepted too:
it moved the coupled pending-presentation transitions and certification
decision, reduced the controller without a callback or reverse dependency, and
fails closed when parser authority is absent or ambiguous. Committed
semantic-receipt adoption is accepted as well: it removed direct structural
transition, source-publication, refresh-anchor, and viewport-mutation authority
from Flutter, while the existing input-window planner now owns the paired
bounded splice calculation. Portable literal-mutation planning is accepted: it
replaced the controller's exact-splice, continuation, bounded-window, canonical
selection, and structural-certification branches with one immutable plan and
direct tests, without callbacks or reverse dependencies. The milestone review
also made Core composition activity explicit instead of inferring it from a
possibly truncated platform range. Platform successor lineage ownership is now
accepted too: capture, deferral, fallback insertion, late-lineage retirement,
certification-completer lifetime, and accounting moved behind typed effects,
and a focused gate caught and fixed the attempted inflation of the observed
successor metric by an internal fallback. The controller no longer mutates any
successor collection or provisional tail. Portable selection restoration is
accepted as a complete input-window transition: it removed the Flutter-only
same-row/cross-row branch tree and fixed capacity-exceeding history restoration
so exact canonical endpoints survive behind the bounded platform surrogate.
Projected-edit routing is accepted too: the legal-caret, hidden-selection, and
visible-grapheme deletion rules moved beside their source/display topology with
typed outcomes and direct tests. Unicode grapheme policy sits in the portable
text module, so the projector does not depend on history/session internals.
The next review must remove another
complete authority—successor effect execution, history/composition adoption, or
lifecycle admission—rather than reshuffling facade methods.

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
