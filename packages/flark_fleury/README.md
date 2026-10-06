# flark_fleury

A Fleury host for the Flark v5 Markdown kernel: cell rendering, input,
selection and focus, without Flutter or a second Markdown recognizer.

It runs on the Dart VM with native hooks and in a standalone Fleury browser
page with Wasm. The editing, theming, table and image APIs are the supported
second-host baseline; physical terminal/IME and large-document qualification
remain separate from host parity. See
[the milestone plan](../../docs/architecture/v5/fleury_host_plan.md) and
[the first review](../../docs/architecture/v5/fleury_host_review_2026_09_08.md).

## Use

```dart
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';

// Inside FleuryApp, with bounded width and height:
Expanded(child: FlarkEditor(initialMarkdown: '**Hello**', onChanged: saveMarkdown));
FlarkMarkdown(markdown: article, selectable: true);
```

The parser loads automatically on native Dart and browser hosts. Web compilation
embeds the parser: no manual Wasm asset placement or backend setup is needed.
`FlarkMarkdown` uses a separate read-only path without editing history or IME.
The parent owns its layout and scrolling.

Create `FlarkController(markdown: ...)` for custom controls, pass it to the editor,
and dispose it after unmounting. Read `controller.state.styles.bold`, call
`toggleStyle(FlarkStyle.bold)` or `setStyle(FlarkStyle.bold, enabled: true)`, and
use `controller.markdown` to save. `loadMarkdown` resets to fetched content without
a save event; `replaceMarkdown` is undoable. The [usage guide](../../website/src/content/docs/guides/using-flark.mdx)
covers readiness, state and the full shared API.

The minimal native/web entry points are `example/bin/consumer.dart` and
`example/web/consumer.dart`. The full theme playground uses
`flark_fleury_legacy.dart` with `FlarkCodeMirror()` from `flark_codemirror` as
its editor's code delegate.

## Install

```sh
dart pub add flark_fleury
```

0.5 is a preview; it depends on fleury 0.1 from pub.dev.

## Develop and run

In this repository the packages form one pub workspace, resolved from the
repository root with `flutter pub get`. Fleury resolves from pub.dev, as it
does for an app. The controllers are Fleury `Notifier`s, and widgets observe
them with `context.listen`. No local Fleury checkout or dependency override is
required; `dart tool/use_local_fleury.dart <path>` points the workspace at a
local checkout when you work on both. See the
[implementation and validation notes](../../docs/architecture/v5/fleury_tables_images_2026_09_15.md).

```sh
flutter pub get          # once, from the repository root
cd packages/flark_fleury
dart analyze --fatal-infos
dart test
cd example
dart run bin/main.dart
```

To package the terminal example with its native parser library on Dart
3.12+, run `dart build cli --target bin/main.dart --output build/native` from
`example`.
Run `build/native/bundle/bin/main` from that directory so the sample image's
relative asset path resolves. Copy the whole bundle when distributing it;
`dart compile exe` does not include native build-hook assets on this SDK.

For framework development only, `dart tool/use_local_fleury.dart /path/to/fleury`
points the repository's pub workspace at a local Fleury checkout through the
ignored `pubspec_overrides.yaml` at the repository root. Delete that file to
return to the reviewed revision.

For an AOT terminal build: `dart build cli`. On macOS ARM64, run
`build/cli/macos_arm64/bundle/bin/main`. Ctrl+Q quits the terminal example.

For the browser: `bash tool/build_web.sh`, then
`python3 -m http.server 8820 --bind 127.0.0.1 --directory build/web`.
Open the `/revisions/<hash>/` path printed by the build script on that server.
Each candidate has fresh paths for its JavaScript and the parser's Wasm module;
reloading `/` can retain older subresources in an embedded browser cache.
This executes Dart and the parser's Wasm module in the page; it does not
stream a remote terminal or Flutter app.

## Current boundaries

See the [Markdown coverage audit](../../docs/architecture/v5/markdown_coverage_2026_09_20.md)
for syntax support and remaining presentation gaps, including footnotes.

- Paragraphs, inline styles, headings, lists/tasks, quotes and code bodies.
  Pointer/keyboard selection, grapheme deletion, cell wrapping, scrolling,
  copy/cut, paste, composition transactions and kernel undo/redo.
- Cmd/Ctrl+A selects a fence body first, then the document. Cmd/Ctrl+B/I and
  Z/Shift+Z/Y format/undo/redo. Tab/Shift+Tab indent code/lists and otherwise
  leave traversal to Fleury. Escape leaves editing focus.
- Fences are colored, detected and indented by the editor's code delegate,
  `FlarkCodeMirror` in the playground, synchronously: layout colors a fence in
  the first frame that shows it and keeps an unchanged fence's colors across
  edits. The language picker offers a `FlarkCodeMirror` delegate's languages,
  and `CodeMirrorLanguages.all` otherwise.
- The theme playground has a bordered customization panel, spaced color
  swatches and copyable Dart configuration. Arrow keys preview colors, Enter
  applies the choice, and `#` opens hex entry. Focus leaves the panel layout stable.
  Preset accents have light/dark counterparts so the chosen hue stays readable
  when switching themes. Custom hex values outside the presets remain exact.
  Narrow views switch between controls and the live editor.
- Headings use per-level text styles and optional decorations. Level styling
  remains a presentation decision: Fleury uses one font size per cell
  grid on both terminal and browser. Per-element font sizes require a Fleury
  rendering change. The shared kernel recognizes and edits each level.
  Bare `#`/`##` and unordered markers stay
  visible until completed, so starting `**bold**` does not flash a list bullet.
- In editable documents, plain link clicks show controls and Cmd/Ctrl-click
  opens the URL. Read-only link clicks open directly when `onOpenLink` is supplied,
  matching Flutter. Unsafe/unhandled links retain non-mutating controls.
- Tables share column widths, honor Markdown alignment, wrap cell contents,
  and paint borders with `tableBorder` and headers with `tableHeader`.
  Tab/Shift-Tab traverses cells; Enter moves to the next row in the same column
  and exits after the last row, matching Flutter. Extremely narrow viewports
  stack cells with numbered column labels. Omitted trailing cells retain their
  own caret position without changing the source. First insertion creates the
  missing delimiters and content together; Undo restores the original row.
- Standalone images show the preview first. Clicking it opens Open/Edit/Remove
  and reveals a centered, editable alt-text line immediately below the image.
  Leaving the resource hides that line without moving following content.
  Images inside prose, table cells or containers retain their projected text
  and fixed-height preview below it. `imagePreviewRows` defaults to 8; zero
  hides previews and keeps the editable alt text.
  The default `FlarkImagePreview` resolves HTTP(S) images, loads at most eight
  visible slots, limits each response to 4 MiB and four million pixels, decodes
  the first frame, and retains a thumbnail no larger than 960 by 640 pixels.
  Native decoding, resizing and PNG preparation run in an isolate; browser
  previews use asynchronous native bitmap/PNG preparation with bounded pixel
  readback. Across editors, at most two preparations run and eight wait.
  Disposal drops queued work and terminates native workers. A cancelled browser
  operation holds its slot until its native promise settles. The supplied PNG
  avoids encoding during normal placement paint; terminal protocol conversion
  and clipped iTerm2 placement remain separate performance qualification work.
  Loading/error/success share the same geometry. Leaving the viewport cancels
  the request and releases its decoded image. Browser requests follow CORS;
  SVG rendering is not supplied by Fleury's raster image widget.
  `imagePreviewBuilder(context, resource, uri)` can replace loading and paint
  for application assets, authenticated resources, or different placeholders.
  Set `baseUri` to resolve relative resources. The example bundles `demo.png`;
  its terminal launcher resolves that asset through the same builder hook.
- Raw source uses a window of 8192 UTF-16 code units around the caret. Large-source scrolling
  and performance need separate qualification. No full M4 performance claim.
- Automated composition checks are not physical IME/terminal-protocol evidence.
  Gesture multiplicity, drag autoscroll, platform-specific shortcut refinements
  and full semantic editing actions remain follow-ups.

The [visual dogfood review](../../docs/architecture/v5/fleury_visual_review_2026_09_12.md)
records the browser checks and the local Fleury dependency fixes they require.

The [table/image implementation review](../../docs/architecture/v5/fleury_tables_images_2026_09_15.md)
records the new host journeys, compositor fixes and local dependency setup.
