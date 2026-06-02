# Flark v2 Rewrite Plan

Status date: 2026-05-02
Decision: bold rewrite with controlled migration

## Objective

Build Flark v2 into the ideal Dart/Flutter library for markdown editing
and previewing: source-faithful, spec-backed, reliable under real text input,
fast enough for production documents, easy to integrate, and architecturally
clean enough that the community can trust it as infrastructure.

The existing package is not thrown away. It remains:

- the behavior reference for current supported markdown features;
- the regression oracle for command/input/preview parity;
- the compatibility shell while v2 develops behind a deliberate API boundary;
- the source of fixture cases for tricky projection, cursor, table, link,
  image, raw HTML, and native parser behavior.

## Core Thesis

The current v1 architecture became much better during production hardening, but
it is still centered on `FlarkController extends TextEditingController`.
For a best-in-class library, that is the wrong center of gravity.

Flark v2 should put a pure editor runtime at the center:

```text
lib/
  flark.dart              public Flutter package surface
  flark_core.dart         optional headless core public surface
  src/
    core/
      document/
      selection/
      transaction/
      history/
      command/
      extension/
    markdown/
      parse/
      profiles/
      structure/
      source_editing/
    projection/
      hidden_ranges/
      cursor_masks/
      source_display_mapping/
      reconciliation/
    render_plan/
      blocks/
      inlines/
      media/
      theme_tokens/
    flutter/
      controller_adapter/
      editable_surface/
      preview_surface/
      overlays/
      text_input/
    native/
      bridge_v2/
      protocol/
      diagnostics/
```

The core owns source text, selection, transactions, markdown semantics,
projection, render plans, command dispatch, history, and extensions. Flutter
adapters own `EditableText`, `TextEditingController` interoperability, gestures,
selection overlays, focus, scroll, platform text input, and widgets.

## Non-Negotiable Design Principles

1. Source markdown is canonical.
2. CommonMark/GFM behavior is spec-backed, not incidental.
3. The core is pure Dart and imports no Flutter libraries.
4. All edits are typed transactions.
5. Commands and input policies return edit plans; they do not poke framework
   state directly.
6. Editable and read-only rendering share a render-plan pipeline.
7. Projection is a first-class model, not scattered `TextRange` helpers.
8. Native parser payloads are versioned and contract-tested.
9. Unknown syntax, unknown extensions, and unknown native payload fields degrade
   gracefully.
10. Public APIs stay small, typed, and intentionally named.

## Target Public API Shape

Initial v2 API names are placeholders, but the shape should be stable before
implementation:

```dart
final document = FlarkDocument.fromMarkdown(source);
final state = FlarkEditorState(document: document);

final result = engine.dispatch(
  state,
  FlarkCommand.toggleInlineStyle(FlarkInlineStyle.bold),
);

final renderPlan = markdownRuntime.renderPlanFor(result.state);
```

Flutter integration should look like:

```dart
final controller = FlarkFlutterController(
  initialMarkdown: source,
  profile: MarkdownSyntaxProfile.commonMarkGfm,
  extensions: [
    FlarkTablesExtension(),
    FlarkTaskListsExtension(),
  ],
);

FlarkEditor(controller: controller);
FlarkMarkdownView(markdown: source);
```

The public API should expose:

- `FlarkDocument`
- `FlarkTextBuffer`
- `FlarkSelection`
- `FlarkEditorState`
- `FlarkTransaction`
- `FlarkSourceOperation`
- `FlarkCommand`
- `FlarkExtension`
- `FlarkMarkdownRuntime`
- `FlarkRenderPlan`
- `FlarkFlutterController`
- `FlarkEditor`
- `FlarkMarkdownView`

It should not expose parser bridge structs, predictive reconciliation internals,
Flutter adapter hosts, or low-level scanner helpers.

## Core Runtime Model

### Document

The document is source markdown text plus derived indexes:

- UTF-16 source offsets for Flutter compatibility;
- UTF-8 mapping for native parser payloads;
- line index;
- optional piece-table/rope implementation if benchmarks justify it;
- document revision and content hash.

Start with a simple immutable text buffer unless early benchmarks prove it is
insufficient. Do not overbuild a rope before the transaction model is stable.

### Selection

Selection is part of editor state and is mapped through transactions. It should
support:

- collapsed and ranged selections;
- future multi-selection without making v1 adapters support it immediately;
- source offsets independent of visual/projection offsets;
- explicit selection affinity where Flutter platform behavior needs it.

### Transactions

Every change flows through `FlarkTransaction`:

- source operations: insert, delete, replace, multi-replace;
- old and new selection;
- mapped positions;
- undo grouping metadata;
- command/user-event metadata;
- parse invalidation range;
- projection invalidation range;
- optional extension effects.

Transactions are created by commands, input policies, paste handlers, or
adapter deltas. Applying a transaction returns a new `FlarkEditorState`.

### Commands

Commands are typed and prioritized. A command handler receives immutable state
and returns either:

- handled with a transaction;
- handled with no-op;
- not handled;
- rejected with a typed reason.

This gives toolbar actions, keyboard shortcuts, markdown transforms, and custom
extensions the same contract.

### Markdown Runtime

The markdown runtime owns:

- parse profile selection;
- parser backend dispatch;
- block/inline source spans;
- source-to-render classification;
- raw HTML policy;
- source-first table model;
- link/reference/image resolution;
- semantic diagnostics.

The native backend remains authoritative where supported. The protocol should
be schema-versioned, forward-compatible, and backed by fixture tests.

### Projection

Projection is the hardest core problem and must be explicit:

- hidden marker ranges;
- exclusion ranges for inline/block scanning;
- cursor masks;
- source/display mapping;
- visual affordance mapping for links/images/tasks/tables;
- ambiguity zones during predictive vs authoritative reconciliation.

Projection must be testable without Flutter. Flutter should ask the core for
mapping decisions, not recalculate them ad hoc.

### Render Plan

The render plan is a typed, platform-neutral tree:

- block nodes with source spans and semantic roles;
- inline runs with marks, targets, and marker visibility;
- overlay affordances such as task checkboxes and link/image actions;
- media descriptors rather than widgets;
- theme-token references rather than Flutter `TextStyle`s.

Flutter turns this into spans, painters, widgets, and overlays. A future web or
server renderer could consume the same plan.

## Migration Strategy

This is a rewrite, but not a destructive big bang.

1. Keep v1 public APIs working while v2 is built in parallel.
2. Add v2 core under `lib/src/v2` until the public shape is ready.
3. Port tests feature by feature into a headless v2 test suite.
4. Build adapter parity behind hidden/dev-only entry points.
5. Compare v1 and v2 outputs for markdown fixtures, command results, projection
   ranges, and render plans.
6. Promote the v2 Flutter adapter only when it reaches parity for the current
   supported feature matrix.
7. Deprecate v1 internals after v2 becomes the implementation behind the public
   widgets.

## Phased Execution Plan

### Phase 0: RFC and Acceptance Criteria

Deliverables:

- v2 research matrix;
- v2 rewrite plan;
- v2 execution plan/log;
- public "why v2" architecture note;
- initial public API sketch.

Exit criteria:

- the core/non-core boundary is documented;
- the first implementation slice is small enough to land safely;
- no code starts by extending or depending on `TextEditingController`.

### Phase 1: Headless Core Skeleton

Deliverables:

- `FlarkDocument`;
- `FlarkSelection`;
- `FlarkEditorState`;
- `FlarkSourceOperation`;
- `FlarkTransaction`;
- transaction apply/map/invert basics;
- core-only tests.

Exit criteria:

- can apply insert/delete/replace transactions without Flutter;
- undo metadata is represented even if full history is not complete;
- selection maps through edits deterministically.

### Phase 2: Command and Extension Runtime

Deliverables:

- typed command registry;
- command priorities;
- handled/no-op/rejected result model;
- extension registration model;
- core command tests for inline style, heading, list, quote, fence, and
  thematic break basics.

Exit criteria:

- toolbar and keyboard policies can share command handlers;
- extension state cannot mutate editor state out of band.

### Phase 3: Markdown Parse Protocol v2

Deliverables:

- parser backend interface independent of Flutter;
- native bridge v2 payload schema;
- schema versioning;
- CommonMark/GFM fixture importer;
- source-span and UTF-8/UTF-16 mapping tests.

Exit criteria:

- authoritative parse output is deterministic and forward-compatible;
- unknown native fields do not crash Dart decoding;
- conformance status can be generated.

### Phase 4: Projection Core

Deliverables:

- hidden range model;
- cursor mask model;
- source/display mapper;
- predictive projection API;
- authoritative reconciliation API;
- projection fixtures ported from v1.

Exit criteria:

- projection behavior is tested without Flutter;
- escaped delimiter, reference link, table, image, and raw HTML cases match the
  intended v1 behavior or documented v2 improvements.

### Phase 5: Render Plan

Deliverables:

- block render plan;
- inline render plan;
- media/action descriptors;
- task/list/table/fence descriptors;
- read/edit render parity tests.

Exit criteria:

- editable and read-only surfaces can consume the same render plan;
- Flutter-specific styling is not present in the core plan.

### Phase 6: Flutter Adapter

Deliverables:

- `FlarkFlutterController`;
- `EditableText` adapter;
- v1 compatibility bridge where useful;
- widget layer for editor and preview;
- text input, shortcuts, selection, scroll, overlays.

Exit criteria:

- basic editor works from v2 state;
- adapter translates Flutter input into core transactions;
- adapter can expose current public widget APIs without leaking v2 internals.

### Phase 7: Feature Parity and Polish

Deliverables:

- tables;
- images/media;
- reference links;
- raw HTML literal policy;
- escapes/entities;
- indented code;
- thematic breaks;
- block/inline command parity;
- performance budgets.

Exit criteria:

- current support matrix is green under v2;
- v1/v2 oracle comparisons are passing or differences are explicitly accepted;
- release gates cover v2 core, adapter, native bridge, and example app.

### Phase 8: Public Release Path

Deliverables:

- stable public API;
- migration guide;
- package docs;
- example app updated to v2;
- license and repository metadata once owner decisions are made;
- publish dry run.

Exit criteria:

- v2 is the default implementation;
- v1 internals are removed or clearly deprecated;
- docs explain source-canonical design and integration tradeoffs.

## Early Risks

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| Projection complexity | Hidden markdown markers, cursor safety, and source/display offsets are the core difficulty. | Build projection as its own headless module with exhaustive fixtures before Flutter adapter work. |
| Flutter IME behavior | Composition, autocorrect, selection handles, and shortcuts can bypass naive command flows. | Keep adapters thin, add delta-model support after transaction core, and test platform text input cases in the example harness. |
| Native bridge drift | Parser payload changes can silently break Dart assumptions. | Version every payload, contract-test schemas, and preserve unknown-field tolerance. |
| Overbuilding rich-doc abstractions | A generic rich document editor would dilute the markdown-source value. | Keep markdown source canonical. Render plans are projections, not persisted state. |
| Big-bang rewrite risk | Replacing everything at once would lose hard-won behavior. | Build v2 beside v1 and use v1 as fixture/oracle until parity is proven. |

## First Implementation Slice

The first code slice should be deliberately small:

1. Create `lib/src/v2/core/` with pure Dart document, selection, source
   operation, transaction, and editor state types.
2. Add tests for insert/delete/replace, selection mapping, transaction metadata,
   and transaction inversion basics.
3. Add no Flutter imports in this slice.
4. Keep public v1 APIs unchanged.

This slice creates the new center of gravity without touching native parsing or
widgets yet.
