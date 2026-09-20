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
import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';

final editor = FlarkEditor(createParseBackend(), text: '**Hello**');
final controller = FlarkFleuryController(editor);

// Inside your FleuryApp, with bounded width/height:
final view = Expanded(child: FlarkEditorView(
  controller: controller,
  autofocus: true,
  showToolbar: true,
));
// Dispose controller after the view unmounts. The editor is borrowed.
```

`FlarkView(controller: controller)` uses the same projection and cell styles
with editing disabled. Both support selection/copy. Set a `FlarkCellTheme`
on one view or put it in `ThemeData.extensions`. Defaults derive semantic roles
from Fleury and preserve the terminal's unspecified foreground/background.

Custom formatting controls can read `controller.styleState(Style.strong)` and
call `controller.setStyle(Style.strong, enabled: true)` (or `false`). State
reports on/off/mixed separately from command availability; controller listeners
observe caret, selection, pending formatting and history changes. See the shared
[formatting contract](../../docs/architecture/v5/formatting_controls.md).

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
Block geometry is explicit and independent of source indentation:

- `listIndent` (default `4` cells) gives bullet and task lists a stable content
  start; markers center in the shared gutter. Ordered labels can expand it.
- `quoteIndent` (default `2` cells) reserves the rail and the gap before text.
- `codePadding` (default `2`, example `1`) pads both sides **inside** a code
  background. Fractional values round up to whole text cells.
- `codePaddingRows` (default `0`, example `0.25`) paints top/bottom padding in
  eighth-row increments. A nonzero edge reserves a decorative row, which
  pointer and keyboard navigation skip. Unpainted space is outside the block.
  With an unknown terminal background the edge uses a full painted row.

Paragraphs, quote rails, code backgrounds and table frames share their containing
block's outer edge. Tables use edge-anchored Unicode frame glyphs where the cell
width policy permits them, with inset internal rules and ordinary glyph fallbacks.
Code padding and list gutters keep wrapped text and pointer/caret geometry in
agreement; changing them never inserts spaces into Markdown source.

Heading defaults use typography at one font size: H1/H2 are bold, H3 bold
italic, H4 italic, H5 regular, and H6 dim. Bands, dividers, underlines and level
labels are opt-in. The inherited Fleury theme accents H1 and uses body color for
deeper levels. The example adds a slate H2 and an explicit muted H6 color, with
light/dark counterparts. Without color H1/H2 can look alike; use the optional
level gutter when exact hierarchy matters or italics are unavailable.

`heading` remains the common text style. Override individual levels with
`headingStyles`, and style decorations with `headingBand`, `headingDivider`,
and `headingIndicator`. A null `headingBand` derives its fill from body colors.
For example:

```dart
FlarkCellTheme(
  heading: CellStyle(foreground: RgbColor(147, 197, 253), bold: true),
  headingStyles: {
    2: FlarkHeadingStyle(style: CellStyle(foreground: RgbColor(176, 195, 220))),
    3: FlarkHeadingStyle(style: CellStyle(
      foreground: RgbColor(220, 230, 240), italic: true,
    )),
  },
  headingGutter: true,
)
```

`headingGutter` reserves three cells beside the whole rendered document for
H1–H6 labels, keeping text and block edges aligned. Narrow surfaces fall back to
inline labels where space permits. Divider rows and labels are decorations:
they never enter copied Markdown, source offsets, selection or undo history.
Arrow navigation skips dividers, and clicking a divider places the caret in its
heading. Wrapped headings retain the same text origin while focused or selected.

Set `showToolbar: true` to opt into the built-in composer controls (off by
default for existing embedded views): paragraph/H1–H6, bold, italic,
strikethrough, inline code, link/image forms, Undo/Redo and source mode. Inside
a fence it adds the shared 14-language catalog, Automatic and Plain text.
Language choices persist through `SetCodeLanguage` and participate in Undo.
An open picker is invalidated when its source or selection changes. Read-only
views omit all authoring controls. `thematicBreak` styles horizontal rules,
which retain their containing block's gutter and exact Markdown source.

The example opens as a large document composer with New document, Copy Markdown
and optional Customize theme controls. New document can be undone. The theme
panel moves above the document on narrow windows; the editor stays mounted.
Its **Heading styles** disclosure edits each level's color, bold,
italic, underline, band, divider and label, plus the global gutter.
**Copy theme Dart** includes the resolved six-level palette and overrides.
**Reset theme** restores them without changing the draft.
**Heading sample** loads a document with all six levels; the web example also
accepts `?sample=headings`, and the terminal example accepts `--headings`.

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
`ffb6d22d488a1979658578f4fc5634bb17b37b7f` ([Fleury #262](https://github.com/danReynolds/fleury/pull/262)). A sibling checkout
is no longer required. The committed `dependency_overrides.fleury` keeps the
companions' hosted core constraint on that same revision until publication.
An application consuming this unpublished host must copy that core override
into its own root pubspec; the example demonstrates the complete setup.

The pinned framework includes the image-composition and block-frame fixes.
No local Fleury checkout or ignored dependency override is required. See the
[implementation and validation notes](../../docs/architecture/v5/fleury_tables_images_2026_09_15.md).

```sh
dart pub get
dart analyze --fatal-infos
dart test
cd example
dart pub get
dart run bin/main.dart
```

To package the terminal example with its native libraries on Dart 3.12+, run
`dart build cli --target bin/main.dart --output build/native` from `example`.
Run `build/native/bundle/bin/main` from that directory so the sample image's
relative asset path resolves. Copy the whole bundle when distributing it;
`dart compile exe` does not include native build-hook assets on this SDK.

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

## Current boundaries

See the [Markdown coverage audit](../../docs/architecture/v5/markdown_coverage_2026_09_20.md)
for syntax support and remaining presentation gaps, including footnotes.

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
