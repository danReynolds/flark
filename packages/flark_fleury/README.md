# flark_fleury

An initial Fleury host for the Flark v5 Markdown kernel: cell rendering, input,
selection and focus, without Flutter or a second Markdown recognizer.

This is the first M4 slice, not complete host parity. It runs on the Dart VM
with native hooks and in a standalone Fleury browser page with Wasm. See
[the milestone plan](../../docs/architecture/v5/fleury_host_plan.md) and
[the first review](../../docs/architecture/v5/fleury_host_review_2026_09_08.md).

## Use

```dart
import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';

final editor = FlarkEditor(createParseBackend(), text: '**Hello**');
final controller = FlarkFleuryController(editor);

// Inside your FleuryApp, with bounded width/height:
final view = Expanded(child: FlarkEditorView(
  controller: controller,
  autofocus: true,
));
// Dispose controller after the view unmounts. The editor is borrowed.
```

`FlarkView(controller: controller)` uses the same projection and cell styles
with editing disabled. Both support selection/copy. Set a `FlarkCellTheme`
on one view or put it in `ThemeData.extensions`. Defaults derive semantic roles
from Fleury and preserve the terminal's unspecified foreground/background.

Clicking a link shows Open, Edit, Remove and Close. Cmd/Ctrl+click invokes
`onOpenLink` directly; Cmd/Ctrl+K opens the link form. Supply `onOpenLink` to
choose the application/browser opener and `baseUri` to resolve relative URLs.
The default form needs a `FleuryApp`/`Navigator`. Customize the controls through
`linkPopoverBuilder` and `presentResourceEditor`; their actions keep the shared
resource session's revision and selection guards. Read-only views expose Open
and Close. The default opener allows HTTP, HTTPS and mailto URIs.
The popover sizes to its contents within the viewport. Browser pointer cursors
follow the painted link text and task checkbox markers; task labels remain
editable text. Read-only checkboxes have no activation action.

A custom `FlarkCellTheme` specifies the editor's cell styles. For an explicit
light/dark surface, set its `body` foreground/background, Fleury's
`ThemeData.textStyle`, and a surrounding `Container.color`, as the example
does. Changing `ThemeData.brightness` alone does not paint a terminal background.
`FlarkCellTheme.codePadding` controls leading cells inside a code surface
(default `2`, example `0` for flush alignment). It never changes Markdown
source indentation and applies equally to wrapped code lines.

Optional code editing uses `FlarkTreeSitter.fromAnalyzer(CodeAnalyzer())` from
`package:flark_tree_sitter/flark.dart` as the editor's `codeEditing` delegate.
For coloring, give the controller a `CodeHighlightWorker`; it owns/disposes that
worker. Both hosts use `FlarkCodeHighlighting` from
`package:flark_tree_sitter/flark_highlighting.dart` for the bounded asynchronous
cache and worker lifecycle. The Fleury surface supplies its visible rows; the
active fence takes priority. The application owns the synchronous delegate. Pending colors paint
the current text plainly; ranges from an older source never reach paint.
The example shows native and browser asset loading for both services.

## Develop and run

The host and example pin Fleury's companion packages to reviewed Git revision
`d37f5a56bcd164acb40ad7dd45ccd633b42cc0a3` (Fleury PR #253). A sibling checkout
is no longer required. The committed `dependency_overrides.fleury` keeps the
companions' hosted core constraint on that same revision until publication.
An application consuming this unpublished host must copy that core override
into its own root pubspec; the example demonstrates the complete setup.

```sh
dart pub get
dart analyze --fatal-infos
dart test
cd example
dart pub get
dart run bin/main.dart
```

For framework development only, `dart tool/use_local_fleury.dart /path/to/fleury`
writes ignored local overrides. Remove the generated host/example
`pubspec_overrides.yaml` files to return to the reviewed pins.

For an AOT terminal build: `dart build cli`. On macOS ARM64, run
`build/cli/macos_arm64/bundle/bin/main`. Ctrl+Q quits the terminal example.

For the browser: `bash tool/build_web.sh`, then
`python3 -m http.server 8820 --bind 127.0.0.1 --directory build/web`.
Open the `/revisions/<hash>/` path printed by the build script on that server.
Each candidate has fresh paths for JavaScript, both Wasm modules and the worker;
reloading `/` can retain older subresources in an embedded browser cache.
This executes Dart and both Wasm modules in the
page; it does not stream a remote terminal or Flutter app.

## First-slice boundaries

- Paragraphs, inline styles, headings, lists/tasks, quotes and code bodies.
  Pointer/keyboard selection, grapheme deletion, cell wrapping, scrolling,
  copy/cut, paste, composition transactions and kernel undo/redo.
- Cmd/Ctrl+A selects a fence body first, then the document. Cmd/Ctrl+B/I and
  Z/Shift+Z/Y format/undo/redo. Tab/Shift+Tab indent code/lists and otherwise
  leave traversal to Fleury. Escape leaves editing focus.
- The theme playground has a bordered customization panel, spaced color
  swatches and copyable Dart configuration. Arrow keys preview colors, Enter
  applies the choice, and `#` opens hex entry. Focus leaves the panel layout stable.
  Preset accents have light/dark counterparts so the chosen hue stays readable
  when switching themes. Custom hex values outside the presets remain exact.
  Narrow views switch between controls and the live editor.
- Headings currently share one bold cell style across all six levels. Level
  styling remains a presentation decision: Fleury uses one font size per cell
  grid on both terminal and browser. Per-element font sizes require a Fleury
  rendering change. The shared kernel recognizes and edits each level.
  Bare `#`/`##` and unordered markers stay
  visible until completed, so starting `**bold**` does not flash a list bullet.
- In editable documents, plain link clicks show controls and Cmd/Ctrl-click
  opens the URL. Read-only link clicks open directly when `onOpenLink` is supplied,
  matching Flutter. Unsafe/unhandled links retain non-mutating controls.
- Tables currently appear as sequential cells; a real table surface is next.
  Links have activation and replaceable editing controls; images currently have
  styled text only, with image presentation still open.
- Raw source uses a window of 8192 UTF-16 code units around the caret. Large-source scrolling
  and performance need separate qualification. No full M4 performance claim.
- Automated composition checks are not physical IME/terminal-protocol evidence.
  Gesture multiplicity, drag autoscroll, platform-specific shortcut refinements
  and full semantic editing actions remain follow-ups.

The [visual dogfood review](../../docs/architecture/v5/fleury_visual_review_2026_09_12.md)
records the browser checks and the local Fleury dependency fixes they require.
