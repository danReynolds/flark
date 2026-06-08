# Flark DX Confidence Peer Loop

**Status date**: 2026-06-07
**Scope**: app-developer API ergonomics for Markdown-native Flutter editing.

## Verdict

Flark's public API is ergonomically solid for its chosen niche:
Markdown-native editing and previewing in Flutter apps. The current app surface
should not be churned for naming. The highest-leverage change is to make common
workflows visible through cookbook examples and a future web playground.

The reviewed peers support this direction:

- headless or UI-agnostic command APIs are standard among best-in-class web
  editors;
- stock toolbars are useful for turnkey rich-text packages but would make Flark
  more design-opinionated than its Markdown-native goal requires;
- active state, can-run state, and command verbs should stay near the
  controller;
- examples and playgrounds are the public proof that the API is easy.

## Changes From This Loop

- Added `docs/cookbook.md` with copy-pasteable app workflows.
- Linked the cookbook from `README.md` and `docs/README.md`.
- Updated `docs/getting_started.md` to show toolbar state rebuilding from the
  controller notifier.
- Fixed README positioning from "two clean widgets" to "two core widgets and a
  thin Flutter form wrapper."

No code API changes were made. The peer evidence does not justify speculative
new names or a package-owned toolbar.

## Peer Loops

### Loop 1: Flutter Quill

Source: <https://pub.dev/packages/flutter_quill>

Observed API shape:

- `QuillController.basic()`
- `QuillSimpleToolbar(controller: ...)`
- `QuillEditor.basic(controller: ...)`

Conclusion: Flutter Quill leads on turnkey WYSIWYG setup because it ships a
stock toolbar and editor pair. Flark should not copy the toolbar as the primary
DX because Flark is Markdown-source-first and should stay design-agnostic.

Action: keep toolbar UI app-owned, but document a short toolbar recipe.

### Loop 2: AppFlowy Editor

Source: <https://pub.dev/packages/appflowy_editor>

Observed API shape:

- `EditorState.blank(withInitialText: true)`
- `AppFlowyEditor(editorState: editorState)`
- documented customization areas for block components, shortcuts, themes,
  selection menus, and toolbar menus

Conclusion: AppFlowy has strong app-workflow documentation and a productized
block-editor ecosystem. Flark's first editor path is simpler for Markdown apps,
but the docs need the same workflow coverage.

Action: add recipes for forms, save lifecycle, toolbar, link editing, document
switching, and custom rendering.

### Loop 3: Super Editor

Source: <https://supereditor.dev/super-editor/guides/basics/build-an-editor/>

Observed API shape:

- documents are represented as `MutableDocument` instances;
- apps assemble document nodes and then mutate through editor requests;
- the system exposes rich document/editor/composer concepts.

Conclusion: Super Editor is architecturally powerful but more ceremonial than
Flark for a Markdown-native app. Flark should keep the simple
`MarkdownEditor(initialMarkdown: ...)` path and reserve deeper primitives for
core and advanced imports.

Action: document document-switching and controller ownership rules rather than
adding another public widget or controller variant.

### Loop 4: Tiptap

Sources:

- <https://tiptap.dev/docs/editor/api/commands>
- <https://tiptap.dev/docs/editor/getting-started/style-editor/custom-menus>

Observed API shape:

- commands live on the editor instance;
- menu examples use `editor.chain().focus().toggleBold().run()`;
- active state uses `editor.isActive('bold')`;
- can-run checks use `editor.can().toggleBold()`.

Conclusion: Tiptap validates Flark's `controller.commands` direction. Flark's
read names such as `strongActive` and `headingLevel`, plus mutation names such
as `toggleStrong()` and `insertTable(...)`, are aligned with peer conventions.
The only notable gap is per-command dry-run checks. That should be considered
later, after real app recipes show whether `canMutate` is too coarse.

Action: keep current names and document toolbar rebuild patterns.

### Loop 5: Lexical

Sources:

- <https://lexical.dev/>
- <https://lexical.dev/docs/concepts/commands>
- <https://lexical.dev/docs/react/plugins>

Observed API shape:

- Lexical is modular and does not directly own UI chrome;
- toolbar and plugin features dispatch commands from the editor instance;
- command handlers can report handled/unhandled behavior;
- plugins add history, links, lists, tables, checklists, markdown shortcuts, and
  other features.

Conclusion: Lexical supports Flark's headless command-result model and
UI-agnostic stance. Flark's facade is less extensible than Lexical's command
registration surface, but Flark exposes the lower-level command runtime through
core/advanced imports for extension work.

Action: do not add app-level plugin APIs in this pass. Keep app docs focused on
controller commands and common workflows.

### Loop 6: Slate

Source: <https://docs.slatejs.org/api/nodes/editor>

Observed API shape:

- low-level manipulation APIs such as `Editor.addMark(...)` and
  `Editor.removeMark(...)`;
- app authors compose formatting behavior themselves.

Conclusion: Slate is a good headless editing benchmark, but its API is lower
level than Flark should expose to a Flutter Markdown app. Flark should keep
common Markdown commands promoted rather than asking apps to manipulate marks
or blocks directly.

Action: keep `toggleStrong()`, `toggleEmphasis()`, `setHeadingLevel(...)`, and
similar Markdown verbs in the app barrel.

### Loop 7: Milkdown

Source: <https://milkdown.dev/>

Observed API shape:

- plugin-driven WYSIWYG Markdown editor framework;
- explicitly headless and CSS-free;
- public docs include a playground.

Conclusion: Milkdown is the closest web benchmark for Flark's intended niche:
Markdown remains central while the package stays UI-agnostic. Milkdown's
playground and recipe ecosystem are ahead of Flark's current public proof.

Action: add cookbook docs now and make a GitHub Pages playground the next DX
deliverable.

### Loop 8: CodeMirror And ProseMirror

Sources:

- <https://codemirror.net/docs/guide/>
- <https://prosemirror.net/docs/guide/>

Observed API shape:

- CodeMirror separates state, view, commands, keymaps, and extensions;
- ProseMirror uses editor state plus command functions such as `toggleMark`,
  `undo`, and `redo`;
- both treat composition and extension points as explicit API concepts.

Conclusion: Flark's split between app import, headless core import, and advanced
import is consistent with mature editor systems. The app API should stay small;
advanced composition belongs behind `flark_core.dart` and
`flark_advanced.dart`.

Action: preserve the current import tiers and document which tier to use.

## Confidence Assessment

| Area | Confidence | Reason |
| --- | --- | --- |
| First editor setup | High | `MarkdownEditor(initialMarkdown: ...)` is shorter than block/rich-text peers for Markdown apps. |
| Shared editor/preview state | High | One `FlarkFlutterController` maps cleanly to editor, preview, toolbar, save UI, and parser state. |
| Toolbar API | High | `controller.commands` matches Tiptap/Lexical/ProseMirror command conventions without imposing UI. |
| Form integration | High | `MarkdownEditorFormField` is thin and Flutter-native. |
| Public docs | Medium-high after this pass | Cookbook examples now cover the missing app workflows; live playground still remains. |
| Extensibility docs | Medium | Core/advanced imports exist, but extension author recipes are still sparse. |
| Peer-leading claim | Not yet public-proof complete | API is competitive; a GitHub Pages playground is needed before claiming DX leadership publicly. |

## Next DX Work

1. Build a GitHub Pages playground that runs the cookbook flows on web.
2. Add extension-author recipes for custom render-plan extensions and parser
   backends.
3. Revisit per-command can-run checks only after cookbook/playground usage shows
   concrete toolbar friction.
