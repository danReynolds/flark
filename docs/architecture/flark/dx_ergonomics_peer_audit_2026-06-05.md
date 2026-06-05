# Flark DX and Ergonomics Peer Audit

**Status date**: 2026-06-05
**Scope**: app-developer ergonomics for Markdown-native Flutter editing and
previewing.

## Decision Summary

Flark is in a strong position for Markdown-native apps. Its best ergonomic
claim is simpler than the rich-text peers: apps can keep Markdown as the source
of truth, import `package:flark/flark.dart`, and build with `MarkdownEditor`,
`Markdown`, and an optional shared `FlarkFlutterController`.

The main gap is not the core API. It is the missing batteries layer around it.
Peers give developers toolbar widgets, block/menu customization examples,
localization setup, and visible application patterns. Flark has the command and
capability primitives, but an app still has to assemble toolbar state, form
integration, save/dirty flows, link dialogs, and common recipes by hand.

Recommended focus:

1. Ship a stock `MarkdownToolbar` backed by existing controller command helpers
   and `FlarkMarkdownCommandQueries.capabilitiesAtSelection`.
2. Add form/save ergonomics: a `MarkdownEditorFormField` or equivalent cookbook
   plus dirty-state and validation patterns.
3. Write cookbook docs for the workflows real apps copy: simple editor, shared
   preview, toolbar, link edit, forms, document switching, parser failures,
   and custom block rendering.
4. Clarify editing-mode expectations. README examples emphasize
   `liveRendered`, while the widget default is `projected`; that may be the
   right default, but it needs an explicit product reason.
5. Defer bigger extension-author APIs until the app-level workflow gaps are
   closed.

## Sources Reviewed

- Flark public exports: `lib/flark.dart`, `lib/flark_core.dart`,
  `lib/flark_advanced.dart`.
- Flark app docs: `README.md`, `docs/getting_started.md`,
  `docs/api_surface.md`.
- Flark controller/widgets: `MarkdownEditor`, `Markdown`,
  `FlarkFlutterController`, command helpers, and command capability queries.
- Local peer benchmark fixtures:
  - `benchmark/peer/` for `flutter_quill`.
  - `benchmark/peer_supereditor/` for `super_editor`.
- Official peer docs checked on 2026-06-05:
  - <https://pub.dev/packages/flutter_quill>
  - <https://pub.dev/packages/super_editor>
  - <https://pub.dev/packages/appflowy_editor>
  - <https://docs.flutterquill.com/>

## Current Flark DX

Flark already has a clean app-level entry:

```dart
import 'package:flark/flark.dart';

MarkdownEditor(
  initialMarkdown: '# Hello',
  editingMode: FlarkMarkdownEditingMode.liveRendered,
  onChanged: saveMarkdown,
)
```

The important ergonomic choices are good:

- Markdown remains the stored document, not an import/export adapter around a
  private rich-text model.
- There are two public widgets: `MarkdownEditor` for editing and `Markdown` for
  read-only rendering.
- Apps that need shared state use `FlarkFlutterController.fromMarkdown(...)`.
- The controller owns parser configuration and exposes typed event streams:
  `events`, `markdownChanges`, and `selectionChanges`.
- Toolbar commands already have readable helpers such as `toggleStrong()`,
  `toggleBulletList()`, `insertTable(...)`, and link edit helpers.
- Active command state exists in the core command layer via
  `FlarkMarkdownCommandQueries.capabilitiesAtSelection(...)`.

The weak spots are also clear:

- No stock toolbar widget is exported, even though the controller has the needed
  commands.
- No promoted form field, validation, dirty-state, or save-button recipe exists.
- Active/can-run command state is not a first-class controller convenience, so
  toolbar authors must know to call lower-level command queries.
- The example app has toolbar code, but it is a dogfood workbench, not a small
  copy-pasteable integration sample.
- Parser ownership rules are correct but easy to trip in first-use code:
  `controller` and parser options are mutually exclusive at widget construction.
- The app-facing docs are short and clean, but they stop before the workflows
  that decide perceived DX: toolbar, link dialog, forms, document switching,
  custom block rendering, and save lifecycle.

## Peer Comparison

| Package | Primary model | First app path | Batteries | Ergonomic read |
| --- | --- | --- | --- | --- |
| Flark | Markdown source truth with projection/render plans | `MarkdownEditor(initialMarkdown: ...)`, `Markdown(markdown: ...)`, optional shared controller | Commands and shortcuts exist; toolbar/forms are missing | Best for Markdown-native apps; needs app workflow polish |
| flutter_quill | Quill Delta rich-text document | `QuillController.basic()`, `QuillSimpleToolbar`, `QuillEditor.basic(...)` | Strong toolbar/editor/localization ecosystem | Best turnkey WYSIWYG peer, but Markdown is not the document truth |
| super_editor | Rich document nodes, composer, editor pipeline | Build `MutableDocument`, `MutableDocumentComposer`, `Editor`, then `SuperEditor` | Deep customization, default editor widget, popovers/toolbars | Powerful architecture, heavier first app setup |
| appflowy_editor | Block document tree | `EditorState.blank(...)`, then `AppFlowyEditor(editorState: ...)` | Strong block customization, shortcuts, toolbar/menu docs | Strong Notion-like app ergonomics, heavier block model and package footprint |

### flutter_quill

`flutter_quill` has the strongest turnkey app ergonomics among the Flutter
peers. Its official setup centers a controller, editor, and stock toolbar:
`QuillController.basic()`, `QuillSimpleToolbar`, and `QuillEditor.basic(...)`.
It also documents localization setup and customization paths for toolbar and
editor widgets.

The tradeoff is the document model. The docs position Quill Delta as the stored
content representation and recommend persisting Delta JSON. That is appropriate
for rich-text apps, but it is not Markdown-native. For Markdown-centric
products, Flark can be simpler if it fills the missing toolbar/forms layer.

### super_editor

`super_editor` is the closest architectural peer for live-rendered block
editing. It exposes a rich document model, document composer, editor pipeline,
and `SuperEditor` widget. Its docs make the setup explicit: create a
`MutableDocument`, hold a `MutableDocumentComposer`, create an editor, then pass
that editor into `SuperEditor`.

That is powerful and flexible. It is also more ceremony than Flark's first app
path. Flark should not copy this shape for common Markdown apps. The useful
lesson is different: SuperEditor makes customization feel intentional, with
default editor behavior available and lower-level tools reachable when needed.

### appflowy_editor

`appflowy_editor` is the app-ergonomics peer for block editing. Its package docs
advertise block components, shortcut events, toolbar menus, and customization
examples. The first app path is simple for a block editor:
`EditorState.blank(withInitialText: true)` and
`AppFlowyEditor(editorState: editorState)`.

The tradeoff is again the model. Developers adopt AppFlowy's document tree and
block system. That fits Notion-like products; it is heavier than Flark's
Markdown-source promise. The lesson for Flark is the docs and component
surface, not the storage model.

## What Flark Should Lead With

Flark's pitch should be:

> Markdown-native editing for Flutter apps: one source string, one app import,
> one controller when you need shared editor/preview/toolbar state.

This is a narrower but sharper DX target than "general rich-text editor." It
matches apps that store Markdown for notes, documentation, comments, CMS
fields, issue bodies, changelogs, posts, or AI-generated drafts.

Flark should avoid competing with Quill on "rich text document platform" terms
and avoid competing with AppFlowy on "full block editor system" terms. The
winning route is to make Markdown editing feel as complete as those packages
feel for their native document models.

## Recommended Roadmap

### 1. Stock Toolbar

Add a package-level toolbar widget that consumes `FlarkFlutterController`.

Minimum viable shape:

```dart
MarkdownToolbar(controller: controller)
```

Expected defaults:

- Undo and redo.
- Heading picker or compact heading buttons.
- Bold, italic, inline code, strikethrough.
- Quote, bullet list, ordered list, task list.
- Link, code fence, table.
- Disabled state from `runtime.canUndo`, `runtime.canRedo`, and command results
  or capability queries.
- Active state from `FlarkMarkdownCommandQueries.capabilitiesAtSelection(...)`.

Keep it lightweight. The toolbar should be good enough for examples and common
apps, not a complete design system.

### 2. Controller Ergonomics for UI State

Promote a small controller convenience for toolbar state so apps do not reach
through `controller.state` and static query classes.

Candidate:

```dart
final capabilities = controller.markdownCommandCapabilities;
```

or:

```dart
final capabilities = controller.capabilitiesAtSelection();
```

This should wrap the existing query object rather than add a second concept.

Also consider controller-level `canUndo` and `canRedo` pass-through getters.
The runtime already has them; exposing them at the controller would make app
code less leaky without changing ownership.

### 3. Form and Save Lifecycle

Add either a real widget or a first-class cookbook:

```dart
MarkdownEditorFormField(
  initialMarkdown: body,
  validator: validateMarkdown,
  onSaved: saveMarkdown,
)
```

At minimum, document the pattern for:

- Initial document load.
- Switching documents.
- Save button enabled state.
- Dirty tracking.
- Validation.
- Parse-error handling.
- Disposal of app-owned controllers.

This matters because Markdown editors are often fields inside publish/comment
flows, not standalone editors.

### 4. Cookbook Docs

Add `docs/cookbook/` or a compact app-facing guide with recipes for:

- Simple uncontrolled editor.
- Controlled editor with shared preview.
- Toolbar.
- Link editing dialog.
- Read-only preview from current draft.
- Document switching.
- Form validation and save lifecycle.
- Parser error fallback.
- Custom block rendering.
- Overlay controls for links/tasks/tables/code fences.

The examples should be shorter than the dogfood app and written for copy-paste.

### 5. Editing Mode Defaults

Decide whether `projected` or `liveRendered` is the app-facing default.

Current facts:

- `MarkdownEditor` defaults to `FlarkMarkdownEditingMode.projected`.
- README and Getting Started examples explicitly choose `liveRendered`.

That may be correct: `projected` can be the safer default while `liveRendered`
is the richer product mode. But the docs should explain the choice in one
sentence so developers do not read the examples as disagreeing with the API.

### 6. Extension DX After App DX

Only after toolbar/forms/docs are in place, improve extension ergonomics:

- Small examples for custom render-plan extension.
- Block-builder examples for `Markdown`.
- Parser profile/backend examples.
- Extension author test harness guidance.

This should follow, not precede, the everyday app workflow work.

## What Not To Do

- Do not introduce a Delta-like document model. Flark's differentiator is
  source Markdown truth.
- Do not make a second public command API if controller helpers and command
  queries can be promoted cleanly.
- Do not expand `flark.dart` with low-level internals to solve docs problems.
- Do not build a giant toolbar configuration object before the stock toolbar is
  exercised in examples.
- Do not prioritize extension-author polish ahead of the first-app path.

## Next Concrete Slice

The highest-value next implementation slice is:

1. Add `MarkdownToolbar`.
2. Add `FlarkFlutterController` command-capability convenience getters.
3. Add a focused toolbar test around active state and command dispatch.
4. Add a short cookbook recipe using `MarkdownToolbar` with shared editor and
   preview.

That slice directly closes the biggest peer DX gap while reusing the command
work already landed.
