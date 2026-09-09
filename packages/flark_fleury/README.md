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

Optional code editing uses `FlarkTreeSitter.fromAnalyzer(CodeAnalyzer())` from
`package:flark_tree_sitter/flark.dart` as the editor's `codeEditing` delegate.
For coloring, give the controller a `CodeHighlightWorker`; it owns/disposes that
worker. The application owns the synchronous delegate. Pending colors paint
the current text plainly; ranges from an older source never reach paint.
The example shows native and browser asset loading for both services.

## Develop and run

Fleury's companion packages are not published yet. A sibling checkout is
currently needed; setup writes ignored overrides and leaves that checkout alone.

```sh
dart tool/use_local_fleury.dart /path/to/fleury
dart pub get
dart analyze --fatal-infos
dart test
cd example
dart pub get
dart run bin/main.dart
```

For an AOT terminal build: `dart build cli`. On macOS ARM64, run
`build/cli/macos_arm64/bundle/bin/main`. Ctrl+Q quits the terminal example.

For the browser: `bash tool/build_web.sh`, then
`python3 -m http.server 8820 --bind 127.0.0.1 --directory build/web`.
Open `http://127.0.0.1:8820/`. This executes Dart and both Wasm modules in the
page; it does not stream a remote terminal or Flutter app.

## First-slice boundaries

- Paragraphs, inline styles, headings, lists/tasks, quotes and code bodies.
  Pointer/keyboard selection, grapheme deletion, cell wrapping, scrolling,
  copy/cut, paste, composition transactions and kernel undo/redo.
- Cmd/Ctrl+A selects a fence body first, then the document. Cmd/Ctrl+B/I and
  Z/Shift+Z/Y format/undo/redo. Tab/Shift+Tab indent code/lists and otherwise
  leave traversal to Fleury. Escape leaves editing focus.
- The theme playground has color swatches and copyable Dart configuration.
  Narrow views switch between controls and the live editor.
- Headings currently share one bold cell style across all six levels. Level
  styling/gutter cues remain a presentation follow-up; the shared kernel already
  recognizes and edits each level. Bare `#`/`##` and unordered markers stay
  visible until completed, so starting `**bold**` does not flash a list bullet.
- Tables currently appear as sequential cells; a real table surface is next.
  Links/images have styled text only: activation, editing popovers and image
  presentation are not implemented in this slice.
- Raw source uses a window of 8192 UTF-16 code units around the caret. Large-source scrolling
  and performance need separate qualification. No full M4 performance claim.
- Automated composition checks are not physical IME/terminal-protocol evidence.
  Gesture multiplicity, drag autoscroll, platform-specific shortcut refinements
  and full semantic editing actions remain follow-ups.
