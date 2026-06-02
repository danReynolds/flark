# Flark v2 Research Matrix

Status date: 2026-05-02

## Scope

This research pass informs the v2 rewrite direction for Flark as a
best-in-class Dart/Flutter markdown editing and previewing library. The current
Flark package remains the reference implementation, regression oracle, and
compatibility baseline while v2 is designed as a cleaner architecture.

## Summary Thesis

Flark v2 should be a source-first markdown editor engine with a headless
Dart core and Flutter adapters. The canonical document must remain markdown
text, not an HTML tree, Quill Delta, or app-specific rich-text document. The
core should own document state, transactions, command policy, markdown parse
profiles, projection, and render-plan generation. Flutter should own text
input, layout, gestures, overlays, and platform integration.

## Source Findings

| Area | Source | Relevant finding | Decision for Flark v2 |
| --- | --- | --- | --- |
| Markdown baseline | CommonMark spec, https://spec.commonmark.org/spec | CommonMark treats Markdown as a plain text format for structured documents and defines a parsing strategy with block structure before inline structure. | CommonMark fixtures become the baseline conformance suite. Source markdown remains canonical. |
| GFM dialect | GitHub Flavored Markdown spec, https://github.github.com/gfm/ | GFM is a strict superset of CommonMark and adds extension areas including tables, task list items, strikethrough, autolinks, and disallowed raw HTML. | Model syntax support as explicit parse profiles: `commonMarkCore`, `commonMarkGfm`, and future extension profiles. |
| Native parser | Comrak, https://comrak.ee/ | Comrak is a Rust CommonMark and GFM compatible parser and renderer. | Keep native `comrak` as the authoritative parser backend on native platforms, but version the bridge protocol and contract-test every payload schema. |
| Dart parser ecosystem | `markdown` package, https://pub.dev/packages/markdown | The Dart package parses Markdown to an HTML-shaped AST, supports CommonMark/GFM-ish extension sets, and tracks CommonMark compliance with spec-derived tests. | Use it as a useful reference and possible web/fallback parser candidate, but do not make an HTML AST the editor core model. |
| Flutter renderer ecosystem | `flutter_markdown_plus`, https://pub.dev/packages/flutter_markdown_plus | The maintained continuation of `flutter_markdown` is a renderer, built on `markdown`, and defaults to GitHub Flavored Markdown. It explicitly does not support inline HTML. | The ecosystem already has renderers. Flark should differentiate by offering live source editing plus preview parity, not just Markdown-to-widget rendering. |
| Code editor architecture | CodeMirror 6 guide, https://codemirror.net/docs/guide/ | CodeMirror separates immutable editor state from the view, uses transactions for document/selection/state updates, addresses positions as UTF-16 offsets, has extension facets, and maps positions through changes. | Adopt a functional headless core with transaction objects, UTF-16 offsets, position mapping, and a composable extension registry. |
| Rich editor architecture | ProseMirror guide, https://prosemirror.net/docs/guide/ | ProseMirror splits state/view/transform modules. Transactions and transform steps create new state, map selections, support undo/collab, and plugins store immutable state slots. | Treat every edit, command, input policy, paste, and table operation as a `SourceEditTransaction` with mapped selection and metadata. |
| Headless/editor-state model | Lexical docs, https://lexical.dev/docs/concepts/editor-state and https://lexical.dev/docs/concepts/commands | Lexical uses immutable editor states, read/update phases, batched reconciliation, typed commands, priorities, and handled propagation. | Build a typed command registry with priorities and deterministic handling. Do not let commands mutate framework state directly. |
| Operation model | Slate docs, https://docs.slatejs.org/concepts/05-operations and https://docs.slatejs.org/concepts/04-transforms | Slate distinguishes high-level transforms from granular low-level operations that are applyable, composable, undoable, and useful for collaboration. | Store v2 edit history as typed source operations/transactions, not only diffs between Flutter `TextEditingValue`s. |
| Markdown editor product shape | Milkdown, https://milkdown.dev/core | Milkdown is a plugin-driven, headless WYSIWYG Markdown editor framework built on ProseMirror, Y.js, and Remark. | Flark should be plugin-driven and headless, but its core value should be source-faithful Markdown editing in Flutter rather than DOM/ProseMirror semantics. |
| Flutter text input boundary | `EditableText`, https://api.flutter.dev/flutter/widgets/EditableText-class.html | `EditableText` is Flutter's low-level text input field and interacts with `TextInput`, controller updates, actions, shortcuts, selection, cursor movement, and gestures. | Keep Flutter input and layout behind adapter interfaces. `EditableText` is a consumer of core state, not the core architecture. |
| Granular Flutter input | `TextEditingDelta`, https://api.flutter.dev/flutter/services/TextEditingDelta-class.html and `DeltaTextInputClient`, https://api.flutter.dev/flutter/services/DeltaTextInputClient-mixin.html | Flutter exposes granular insertion/deletion/replacement/non-text deltas when clients opt into the delta model. | Add a delta-capable Flutter adapter after the core transaction engine exists. Do not make delta handling block the initial headless core. |
| Rich Flutter alternatives | `super_editor`, https://pub.dev/packages/super_editor | Super Editor is a composable document editor/renderer with `Document`, `DocumentComposer`, and `Editor` objects. It is actively evolving core editor APIs. | Learn from its document/composer/editor split, but avoid making Flark a general rich document editor. |
| Rich Flutter alternatives | `appflowy_editor`, https://pub.dev/packages/appflowy_editor | AppFlowy Editor is a customizable rich-text editor with block components, shortcuts, themes, selection menus, and Markdown/Delta import paths. | Treat it as a feature benchmark for extensibility and UX, not as a source-canonical markdown architecture. |
| Rich Flutter alternatives | `flutter_quill`, https://pub.dev/packages/flutter_quill | Flutter Quill stores content as Quill Delta JSON and recommends saving Delta instead of HTML/Markdown for fidelity. | This validates a key differentiation: Flark should be the library to choose when Markdown source itself is the durable content format. |

## Architectural Implications

1. The v2 core must not extend `TextEditingController`.
2. The v2 core must not expose Flutter types in document, transaction,
   markdown, projection, or render-plan packages.
3. Markdown source text is the canonical document. ASTs, block trees, render
   plans, spans, widgets, and previews are projections.
4. CommonMark/GFM conformance should be automated and visible in docs.
5. Every edit should flow through a typed transaction with source operations,
   mapped selection, undo metadata, user-event metadata, and parse invalidation
   metadata.
6. Projection needs first-class data structures for hidden marker ranges,
   cursor masks, visual/source offset mapping, ambiguity zones, and stale
   authoritative parse reconciliation.
7. The read-only preview and editable surface should consume the same render
   plan, with only interaction affordances differing.
8. Extension points should be deliberate and typed: commands, key bindings,
   parse extensions, render nodes, inline actions, media resolvers, and theme
   tokens.

## Ecosystem Gap

Flutter has strong markdown renderers and several rich document editors. What
it lacks is a source-faithful, CommonMark/GFM-centered live markdown editor
whose editable surface and read-only preview share a parser-backed render
pipeline, whose command/input model is transactionally testable outside
Flutter, and whose native parser bridge is packaged like a normal Dart/Flutter
dependency.
