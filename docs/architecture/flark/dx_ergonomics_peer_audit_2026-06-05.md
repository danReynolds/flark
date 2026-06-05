# Flark DX and Ergonomics Peer Audit

**Status date**: 2026-06-05
**Scope**: app-developer ergonomics for Markdown-native Flutter editing and
previewing.

## Decision Summary

Flark is in a strong position for Markdown-native apps. Its best ergonomic
claim is simpler than the rich-text peers: apps can keep Markdown as the source
of truth, import `package:flark/flark.dart`, and build with `MarkdownEditor`,
`Markdown`, and an optional shared `FlarkFlutterController`.

The main gap is not the core API. It is the missing app-workflow layer around
it. Peers give developers command verbs, active-state queries, can-run checks,
form integration, block/menu customization examples, and visible application
patterns. Flark has most command primitives already, but an app still has to
assemble command read state, toolbar wiring, save/dirty flows, link dialogs, and
common recipes by hand.

Recommended focus:

1. Keep Flark UI-agnostic: do not lead with a stock toolbar. Promote
   controller-level command state and can-run getters so apps can build their
   own toolbar/menu UI without reaching into static query helpers.
2. Keep mutation names as direct command verbs: `toggleStrong()`,
   `toggleEmphasis()`, `setHeadingLevel(...)`, `insertTable(...)`, `undo()`,
   and `redo()`.
3. Add form/save ergonomics: `MarkdownEditorFormField` plus dirty-state and
   validation patterns.
4. Write cookbook docs for the workflows real apps copy: simple editor, shared
   preview, toolbar, link edit, forms, document switching, parser failures,
   and custom block rendering.
5. Clarify editing-mode expectations. Treat `source` and `liveRendered` as the
   app-facing modes; keep `projected` documented as a markerless/plain advanced
   mode unless it gains visual inline styling.
6. Defer bigger extension-author APIs until the app-level workflow gaps are
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
  - <https://tiptap.dev/docs/editor/api/commands>
  - <https://tiptap.dev/docs/editor/getting-started/style-editor/custom-menus>
  - <https://quilljs.com/docs/api>
  - <https://github.com/facebook/lexical/blob/main/packages/lexical-playground/src/plugins/ToolbarPlugin/index.tsx>
  - <https://github.com/ProseMirror/prosemirror-commands/blob/master/src/commands.ts>
  - <https://obsidian.md/help/edit-and-read>
  - <https://github.com/nhn/tui.editor/blob/master/docs/en/getting-started.md>

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

- No promoted form field, validation, dirty-state, or save-button recipe existed
  before this pass.
- Active/can-run command state is not a first-class controller convenience, so
  toolbar/menu authors must know to call lower-level command queries.
- The current read-state type is named `FlarkMarkdownCommandCapabilities`, but
  most of its fields are active selection state. That name is misleading as a
  public umbrella for all command read state.
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
| Flark | Markdown source truth with projection/render plans | `MarkdownEditor(initialMarkdown: ...)`, `Markdown(markdown: ...)`, optional shared controller | Commands and shortcuts exist; command-state/forms/docs need polish | Best for Markdown-native apps; needs app workflow polish |
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

### 1. UI-Agnostic Command State

Do not ship a stock toolbar as the primary DX answer. A package toolbar would
make Flark take a visual/product stance that many apps should own themselves.
The best peer pattern is closer to Tiptap: provide command verbs, active-state
queries, and can-run checks, then show toolbar recipes that use any markup or
Flutter widgets the app wants.

Promote a small controller convenience for toolbar state so apps do not reach
through `controller.state` and static query classes.

Recommended read-side naming:

```dart
final state = controller.markdownCommandState;

state.strongActive;
state.emphasisActive;
state.activeHeadingLevel;
state.canMutate;
controller.canUndo;
controller.canRedo;
```

The public type should be named `FlarkMarkdownCommandState`, not
`FlarkMarkdownCommandCapabilities`. The current data mixes:

- active formatting or block context, such as `activeInlineStyles`,
  `activeHeadingLevel`, `quoteActive`, `bulletListActive`, and `tableActive`;
- can-run state, such as `canMutate`, `canUndo`, and `canRedo`.

`Capabilities` works only for the second bucket. For the full read side,
`CommandState` is more honest and maps well to peer language:

- Tiptap uses command verbs, `.can()` for dry-run availability, and
  `isActive(...)` for active node/mark state.
- Quill uses mutation verbs such as `format`, `formatText`, and `removeFormat`,
  and exposes active formatting through `getFormat(...)`.
- Lexical uses `dispatchCommand(...)`, `FORMAT_TEXT_COMMAND`, `CAN_UNDO_COMMAND`,
  `CAN_REDO_COMMAND`, and toolbar-state booleans like `isBold` and `isItalic`.
- ProseMirror names the mutation primitive `Command` and exposes direct helpers
  like `toggleMark(...)`, `setBlockType(...)`, and `chainCommands(...)`.

Recommended mutation-side naming: keep the direct controller helpers. They are
already in the right shape:

```dart
controller.toggleStrong();
controller.toggleEmphasis();
controller.toggleInlineCode();
controller.setHeadingLevel(2);
controller.toggleBulletList();
controller.insertTable(columns: 3, bodyRows: 2);
controller.undo();
controller.redo();
```

Recommended app toolbar recipe shape:

```dart
final state = controller.markdownCommandState;

IconButton(
  icon: const Icon(Icons.format_bold),
  isSelected: state.strongActive,
  onPressed: state.canMutate ? controller.toggleStrong : null,
)
```

This keeps Flark design-agnostic while making toolbar implementation obvious.

### 2. Form and Save Lifecycle

`MarkdownEditorFormField` is the right Flutter-native wrapper. It should stay
thin: the purpose is Flutter `Form` wiring, not a second editor abstraction.

```dart
MarkdownEditorFormField(
  initialMarkdown: body,
  validator: validateMarkdown,
  onSaved: saveMarkdown,
)
```

Document the pattern for:

- Initial document load.
- Switching documents.
- Save button enabled state.
- Dirty tracking.
- Validation.
- Parse-error handling.
- Disposal of app-owned controllers.

This matters because Markdown editors are often fields inside publish/comment
flows, not standalone editors.

### 3. Cookbook Docs

Add `docs/cookbook/` or a compact app-facing guide with recipes for:

- Simple uncontrolled editor.
- Controlled editor with shared preview.
- Design-agnostic toolbar using `markdownCommandState` and controller command
  helpers.
- Link editing dialog.
- Read-only preview from current draft.
- Document switching.
- Form validation and save lifecycle.
- Parser error fallback.
- Custom block rendering.
- Overlay controls for links/tasks/tables/code fences.

The examples should be shorter than the dogfood app and written for copy-paste.

The cookbook should later become the GitHub Pages playground: examples should
run live on the package home page so developers can try the modes, toolbar
recipe, and form behavior in a browser.

### 4. Editing Modes

Current facts:

- `MarkdownEditor` defaults to `FlarkMarkdownEditingMode.projected`.
- README and Getting Started examples explicitly choose `liveRendered`.
- `projected` hides Markdown markers but does not render inline styles; bold and
  italic content is plain text unless app UI also shows active state.
- `liveRendered` uses the same projected source mapping but adds rendered inline
  styling and editable block widgets.

Other Markdown live editors usually lead with two editable concepts:

- Source mode: all Markdown syntax stays visible.
- Live Preview or WYSIWYG mode: Markdown is edited inline with rendered
  formatting and hidden or reduced syntax.

Obsidian documents exactly that split: Live Preview hides most Markdown syntax
while showing formatted text inline, while Source mode displays syntax exactly.
Toast UI Editor uses `initialEditType: 'markdown' | 'wysiwyg'`, and its
Markdown mode has a separate preview style. In that peer context, Flark's
`projected` mode is unusual as an app-facing mode: it hides syntax without
showing visual formatting.

Recommendation:

- Lead docs with `source` for exact Markdown and `liveRendered` for normal
  app-facing live editing.
- Keep `projected` as advanced/minimal markerless editing, or rename it to a
  clearer public term such as `markerless` if it remains part of the public API.
- Do not present all three modes as equally intuitive product choices until
  `projected` gains visible formatting or an explicit use case.

### 5. Extension DX After App DX

Only after command-state/forms/docs are in place, improve extension ergonomics:

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
- Do not ship a design-opinionated toolbar as the primary answer to toolbar DX.
  Ship command-state primitives and recipes first.
- Do not prioritize extension-author polish ahead of the first-app path.

## Next Concrete Slice

The highest-value next implementation slice is:

1. Add `FlarkFlutterController.markdownCommandState`, `canUndo`, and `canRedo`.
2. Rename or alias `FlarkMarkdownCommandCapabilities` to
   `FlarkMarkdownCommandState` while the public surface is still flexible.
3. Add focused tests around active state and command dispatch.
4. Add a short cookbook recipe showing a custom toolbar with shared editor and
   preview.

That slice closes the biggest peer DX gap without making Flark own app visual
design.
