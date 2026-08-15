# V3 package migration boundary

Status: implementation-boundary audit, updated 2026-07-21. This prevents the
parser research from being mistaken for a drop-in backend replacement. It is
based on the current package code and behavioral tests, not only RFC prose.
The canonical production decision is in the
[v3 architecture summary](../../docs/architecture/v3/architecture_summary.md).

## Decision

Treat v3 as a new Dart-first editor-engine implementation behind a deliberately
migrated public surface. The published `flark` package becomes the pure-Dart
engine; `flark_flutter` depends on it and owns Flutter integration. Reuse v2
product semantics and tests; do not preserve v2's document-scale internal
representations or the current single Flutter-dependent package merely to
reduce the apparent rewrite.

The parser can be exact and incremental while the product still janks if Dart
rebuilds a full `String`, line table, render plan, history list, or
`TextEditingValue` on every edit. Those costs are present in the current v2
shape and are outside the Rust parser decision.

## Current-code findings

The important current boundaries are concrete:

- The current root `pubspec.yaml` depends on `sdk:flutter`. Its
  `flark_core.dart` barrel is widget-free source, but it is not a standalone
  Dart package that can resolve without Flutter.
- The v3 source and host protocol/store/controller files import only Dart and
  package-neutral dependencies. Flutter imports are localized under
  `lib/src/v3/flutter/`, so the prototype already points in the selected
  dependency direction.
- The current web bridge loads Wasm through Flutter's `dart:ui_web` asset
  manager. The Dart engine needs a runtime-neutral loader accepting configured
  bytes, URI, or a platform implementation; Flutter may adapt bundled assets
  to that seam.
- `FlarkTextBuffer` owns one whole `String`, eagerly computes every line start
  in its constructor, and implements an edit with `String.replaceRange`
  followed by construction of another complete buffer.
- `FlarkDocument` normalizes all initial line endings, owns the buffer, and
  returns another immutable document for every changed range.
- `FlarkEditorState.markdown` exposes the whole string directly.
- `FlarkHistoryStack` retains immutable lists of transactions and inverses,
  bounds history only by entry count, and copies list shells while recording,
  undoing, and redoing.
- Ten v2 implementation files read `state.markdown`; the command layer alone
  has whole-source reads in table, input, block, link, image, capability, and
  inline flows.
- `FlarkRenderPlan` materializes an immutable list/tree of all render blocks
  and provides document-wide iterations and searches.
- The Flutter controller, parse scheduler, projections, and editable surfaces
  assume whole-document parse/result or whole-value access at several seams.

These choices are reasonable for v2's small-document implementation. They
cannot be the hidden foundation of a claimed 10/100 MiB live editor.

## Preserve behavior, replace ownership

### Preserve and promote into v3 contracts

- UTF-16 public selection and transaction coordinates.
- Atomic multi-operation semantics, including original-revision coordinates
  and stable same-offset insertion order.
- Exact transaction inversion, undo grouping for typing/IME, redo invalidation,
  selection mapping, and source-neutral transaction behavior.
- Existing command IDs, extension dispatch, handled/unhandled semantics, and
  source-intent policies where their behavior remains correct.
- Active-syntax reveal, composition pinning, selection geometry, cross-block
  gestures, table/code/task interactions, and accessibility learnings captured
  by the current test suite.
- Fresh-parse/export equivalence and clean source as the authoritative user
  document.

These tests are a behavioral oracle. Passing them through compatibility
adapters is required, but reproducing their old object graph is not.

### Replace in v3

| V2 internal | V3 boundary |
| --- | --- |
| Whole immutable `FlarkTextBuffer` and eager line list | Mutable current UTF-16 sum/piece tree with logical-line aggregates and bounded range reads |
| One immutable `FlarkDocument` root per edit | One session-owned current root plus lightweight revision/fingerprint tokens |
| Entry-count-only immutable history lists | Fixed byte- and entry-bounded inverse transaction ring with grouped records |
| Commands reading `state.markdown` | `FlarkSourceView` range, line, scalar, and anchor queries with explicit cold materialization |
| Full parse request/result | Revisioned worker session and compact edit/fact deltas |
| Document-wide immutable render plan | Persistent worker output plus viewport/active/layout query snapshots |
| Prediction/reconciliation of Markdown grammar | Current-revision authoritative facts or exact source-visible presentation |
| Whole-document `TextEditingValue` | Bounded active input island mapped to global source coordinates |
| Per-edit mapping of every projection/block | Stable IDs, edit transforms, persistent relative pages, and lazy query materialization |
| Implicit LF-normalizing constructor | Source-preserving v3 default plus explicit compatibility import transform |

## Dart-first engine seam

The concrete names may change, but the ownership must remain visible in the
types:

```text
FlarkDocumentSession
  source: FlarkMutableSource
  history: FlarkInverseHistory
  selection/composition/anchors
  worker: FlarkSemanticWorkerSession
  presentation: FlarkViewportPresentation

FlarkDocumentSnapshot
  uiRevision
  certifiedSourceRevision
  factRevision
  selection
  bounded source/query capabilities
```

`FlarkDocumentSnapshot` is not an immutable copy of the whole document. It is a
read-consistent revision token and query view over session-owned storage. A
transaction mutates the session atomically, returns an applied-transaction
receipt, and publishes a new lightweight snapshot.

Commands receive a source-query interface with:

- document length and logical-line lookup;
- bounded `readRange` and code-unit/scalar access;
- selection-local context and stable anchors;
- explicit async/cold APIs for large reads; and
- transaction builders that capture inverses before mutation.

No command may obtain the complete document merely to inspect one line, table,
fence, link, or image. A command whose semantics genuinely require a large
range declares that cost and routes cooperatively.

The pure-Dart boundary also owns parser-host lifecycle, native isolate/Web
Worker orchestration, revision adoption, and platform-neutral fact queries.
Flutter's `TextEditingValue`, `TextSelection`, `TextRange`, `InlineSpan`,
`Color`, widgets, and frame scheduler stay in `flark_flutter`.

## Public API compatibility policy

Preserve source-level API names where doing so does not lie about cost or
ownership. In particular:

- the pre-1.0 package split is intentional: `package:flark/flark.dart` is the
  Dart engine and `package:flark_flutter/flark_flutter.dart` is the Flutter
  surface; the latter may re-export common engine types;
- transaction, selection, command, theme, and extension types can usually be
  adapted directly;
- `controller.markdown` is either an explicitly cold `O(document)`
  compatibility materialization or is superseded by async/streaming export;
- initialization from a large string/file has an async candidate/certification
  state instead of blocking the UI until fully indexed;
- parse/render snapshots become bounded query objects rather than mandatory
  document-wide Dart ASTs; and
- newline preservation is a v3 behavior change with an explicit normalizing
  import option for callers depending on v2.

Because launch has not occurred, prefer an honest breaking correction over a
permanent synchronous API trap. Migration aliases or deprecated cold getters
are acceptable; rebuilding full strings internally to preserve the illusion
of constant-time compatibility is not.

## Test migration strategy

Classify, do not rewrite away, the current suite:

1. **Pure behavior oracles:** transaction ordering, selections, command output,
   undo/redo grouping, Markdown interactions, source ranges, projection maps,
   and public exports. Run unchanged where possible.
2. **Representation-coupled tests:** identity of immutable documents/lists,
   whole-value controller assumptions, prediction flags, and LF-normalized
   ingest. Replace with v3 ownership/revision/newline contracts while retaining
   the underlying user-visible behavior being tested.
3. **Parser semantic oracles:** native Comrak fixtures, non-ASCII source ranges,
   tables, tasks, links, code, HTML, incomplete syntax, and clean export.
   Differential against the exact v3 worker at every revision.
4. **Product/device gates:** IME, active reveal, selection chrome, semantics,
   shaping, wrapping, parser-to-paint, backlog, paste, and undo. Run against the
   composed v3 session rather than prototype simulators before acceptance.
5. **Large-resource gates:** assert bounded synchronous source work, mounted
   objects, message pages, cache bytes, retained roots, and UI frame tails at
   1/10/100 MiB.

Tests may change when an old representation is intentionally deleted, but each
deleted assertion needs a named v3 invariant. “The new implementation is
different” is not a reason to lose a learned behavior.

## Sequencing

1. Establish the pure-Dart `flark` package and dependent `flark_flutter`
   package before the v3 public API freezes; move core tests to `package:test`.
2. Accept the composed Rust edit-to-facts runtime and source/output protocols.
3. Implement the mutable-current Dart source and inverse history behind a new
   internal session API.
4. Port transaction/selection/command behavior to bounded source queries.
5. Integrate runtime-neutral native/Wasm worker transport and revision-safe
   query facts.
6. Replace the render-plan and projected-editable internals with viewport
   materialization and one active input island.
7. Run the classified v2 behavior suite plus v3 large/device gates.
8. Only then switch the public controller/editor defaults and remove the
   prediction/full-document hot paths.

This permits parallel migration without running two Markdown grammars. During
development, v2 and v3 may be separate engines selected at construction; a
single editing session never merges v2 predicted facts with v3 authoritative
facts.

## Acceptance boundary

V3 is not ready to replace v2 merely when Rust fixtures pass. The package
migration is accepted only when:

- the `flark` package resolves, analyzes, and tests in a Dart-only environment
  with no Flutter SDK, Flutter dependency, `dart:ui`, or Flutter asset API;
- ordinary input performs bounded synchronous Dart work and never constructs a
  whole-document string, line list, render plan, or editable value;
- every retained v2 behavioral learning has an executable v3 oracle;
- revision/source/fact adoption survives paste, composition, undo,
  supersession, worker restart, and stale replies;
- viewport queries and semantic/accessibility pages remain bounded at scale;
- a clean full parse/export agrees with the incrementally presented revision;
  and
- floor native/web devices meet the parser-to-paint and frame-tail targets.
