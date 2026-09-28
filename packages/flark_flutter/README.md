# Flark for Flutter

A Flutter surface over Flark's synchronous Dart editing kernel. Rust recognizes
Markdown; the kernel owns source, selection, editing intent, history and
live/source admission. Flutter owns input connections, focus, glyph geometry,
scrolling and painting.

This is an unpublished V5 implementation. Native macOS dogfood qualification is
still open; see the [implementation review](../../docs/architecture/v5/implementation_review_2026_09_04.md).
The currently tested SDK is Flutter 3.44.4 / Dart 3.12.2.

Code and source views use bundled Roboto Mono, so code columns have the same
width on native and web hosts without a remote font request. The package includes
its SIL Open Font License; the pinned upstream revision and file hashes are in
[`lib/assets/fonts/provenance.json`](lib/assets/fonts/provenance.json).

```dart
import 'package:flark_flutter/flark_flutter.dart';

// A complete composer inside a bounded-height parent.
FlarkEditor(initialMarkdown: '# My note', onChanged: saveMarkdown);

// Read-only content flows in its parent's layout and scrolling.
FlarkMarkdown(markdown: article, selectable: true);
```

Both load their parser automatically. No backend object, initialization helper or
hand-copied Wasm asset is required. Native development builds use Dart build hooks;
web builds include the parser in the compiled application.

For custom controls, create a controller once, then dispose it after unmounting:

```dart
final controller = FlarkController(markdown: '# My note');
FlarkEditor(controller: controller);
await controller.ready; // Only needed for immediate programmatic edits.
controller.toggleStyle(FlarkStyle.bold);
controller.insertText('Hello');
final bold = controller.state.styles.bold;
final markdown = controller.markdown;
controller.dispose();
```

`loadMarkdown` establishes fetched content and resets history without a save event.
`replaceMarkdown` is undoable. `onChanged` and `controller.changes` report edits,
including programmatic changes and undo; choose one save route.
See the [usage guide](../../website/src/content/docs/guides/using-flark.mdx) for
precise source edits, readiness/retry, state, and resource controls.

Run the minimal app with `cd example && flutter run -t lib/consumer.dart`.
The full [workbench](example/README.md) also exercises advanced parser and
code-service configuration through `flark_flutter_legacy.dart`.

## Markdown themes and controls

Both `FlarkEditor` and `FlarkMarkdown` accept the same `theme` override:

```dart
final markdownTheme = FlarkThemeData(
  styles: {
    FlarkTextRole.body: const TextStyle(fontSize: 18, height: 1.6),
    FlarkTextRole.link: const TextStyle(color: Color(0xff276044)),
  },
  metrics: {FlarkMetric.documentPadding: 24},
);
FlarkEditor(controller: controller, theme: markdownTheme);
FlarkMarkdown(markdown: article, theme: markdownTheme);
```

Import Flutter's `material.dart` for `TextStyle`/`Color`. For application-wide
defaults, add `FlarkThemeData` to `ThemeData.extensions`. Resolution is toolkit
defaults, ambient Flark overrides, then instance overrides. Maps merge by role;
text styles also merge by field. `copyWith` replaces the supplied maps, while
`merge` layers overrides. The legacy `style` argument overrides body typography
last, and specific Markdown role styles remain more specific. Body typography
now inherits the app's `textTheme.bodyLarge`; links use its primary color plus
an underline. Explicit values in a custom palette retain their chosen colors
when switching brightness.

`FlarkTextRole`, `FlarkColorRole`, `FlarkMetric` and `FlarkSyntaxRole` describe
the supported text, decoration, spacing and syntax-color options. Syntax
overrides are colors only; code-block typography belongs to its text role.
Block alignment follows a common containing edge: prose, quote rails, code
backgrounds and table frames begin there, with text padding inside each block.
`FlarkMetric.listIndent` sets the list content start; `listMarkerGap` reserves the
gap after its marker. Bullets and checkboxes center in the remaining gutter.
`quoteIndent` controls quoted content separately from lists. Code padding is
symmetric inside its background; row spacing remains outside it. These metrics
also drive nested layout and pointer targets.

Theme changes preserve source, selection and history. Fonts, text scaling and
spacing reflow the current surface; color-only updates retain scroll position
and image streams. Image dimensions reserve stable slots in loading, failure
and success states; decode/cache limits remain bounded independently of theme.

Plain link clicks keep the text editable and open a contextual popover. Use
Cmd/Ctrl-click to open, Cmd/Ctrl-K to edit, and Shift-F10/the context-menu key
for keyboard access to actions. Escape or typing dismisses the popover.
Read-only links open on ordinary click through the supplied callback.

For different controls, supply `linkPopoverBuilder` and/or
`presentResourceEditor`. The popover receives `FlarkLinkActions`; the presenter
receives a `FlarkResourceSession` with values and guarded `save`, `remove` and
`open` operations. No Markdown serialization or selection restoration is needed
in custom UI. Close your dialog/sheet after successful application or cancel;
complete its returned future when presentation ends. A session becomes inert
after its target changes, it closes, or the host is replaced/disposed. Rejected
submissions return `false`, letting the custom form display its own error.

The complete [branded popover and editing sheet](example/lib/custom_controls.dart)
demonstrate both hooks. Default controls use the ambient Material component
themes. URL launching and image provision remain application callbacks.

## Code regions

Fenced code is highlighted, indented and detected by `flark_codemirror`,
CodeMirror's language modes ported to Dart, for 25 languages. A fence without
a language takes the one it looks like. A manual choice is authoritative;
unknown languages and plain text keep ordinary whitespace editing, and a fence
in an unknown language still indents by its brackets.

```dart
// Advanced integration uses the explicit legacy entry point.
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/code.dart';

final editor = FlarkEditor(
  markdownBackend,
  text: markdown,
  codeEditing: FlarkCodeMirror(),
);
final controller = FlarkController(editor);
```

`FlarkCodeMirror` runs synchronously on the UI thread and needs no loading,
assets, worker or disposal; one instance may serve any number of editors.
Colors, indentation, selection and undo publish together, on the same frame
as the edit. Highlighting is cached per snippet and language, so rows whose
text and language are unchanged keep their layouts. Snippets over 8,192 code
units stay plain. `FlarkCodeMirror.only([CodeMirrorLanguages.python, ...])`
highlights only the languages an app names and keeps the others out of its
build; the toolbar's language menu lists them.

Use controller commands while a Flutter input connection is active so composing
state, notices and kernel publication remain coherent. Caller-owned focus nodes
are supported. Replacing a controller invalidates the old input connection and
pending clipboard actions. Clipboard cut/paste also checks the revision before
mutating source.

The editor supports inline styles, paragraphs, headings, lists, quotes, task
checkboxes, code rows, tables and source inspection. Tab navigates table cells;
Return moves to the next row in the same column, then exits the table. Deletion
at a table-cell boundary rejects atomically; restructuring belongs in source
mode. Link and Image toolbar controls insert or edit the resource at the
selection. Cmd/Ctrl+K opens the link dialog; Cmd/Ctrl-click opens a link through
the embedding application's callback. Read-only views open links with a normal
click. Removing a link preserves its inline content; removing an image deletes
the complete image. Each edit is one undoable kernel command and stale dialog
submissions leave the newer document unchanged.

Images display a preview below editable alt text. Click the preview to edit it.
Loading and failure retain the same layout slot, so neither changes the caret
or following text. The surface loads only visible images, retains at most 16
streams, and requests decodes bounded by 960 × 640 pixels. Flutter's shared image
cache retains its own application-level policy. Unsupported URLs and failed
requests show an unavailable placeholder.

```dart
FlarkEditor(
  controller: controller,
  baseUri: Uri.parse('https://example.com/notes/'),
  onOpenLink: (uri) { /* open using the application's URL launcher */ },
  // Optional: imageProvider: (uri) => an asset/file/authenticated provider.
);
```

The same resource options are available on `FlarkMarkdown`. The default image
provider permits HTTP(S); `baseUri` resolves relative destinations. Web image
servers must permit cross-origin requests. The host's open-link action permits
HTTP(S) and mailto URLs. Comrak supplies resolved URLs and titles, including
reference definitions and escaped/entity values; the Flutter package adds no
Markdown recognizer. The workbench uses `url_launcher` and its web origin as
the base URI. Uploads, attachment storage and image resizing are outside this
slice.

Inside a rendered fence, Cmd/Ctrl+A selects its code body first; repeat to select
the document. An empty fence stays empty on the first press so a following paste
fills that region. Source mode and selections spanning regions select the entire
document. The shared kernel command is `SelectAll()`. Browser copy, cut and paste
shortcuts defer to native clipboard events; programmatic native Flutter actions
continue to use the platform clipboard API.

Source mode is an explicit bounded page over the complete source: at most
4,097 UTF-16 units and 128 physical line breaks per page, without splitting a
surrogate pair or CRLF. Page navigation changes the caret, not the source.
Selection, copy, edits and Undo continue to use global offsets. A host must
qualify its own byte, line, block, run, block-length, container-depth and writable-source limits.

Advanced `flark_flutter_legacy.dart` integrations can use
`FlarkSourceView(controller: controller)` for read-only source inspection
using those same bounded pages and follows the editor's canonical caret.

`onPaint` is an optional observation point after actual visible glyph drawing.
It records the snapshot revision, rendered text, resolved styles, selection,
caret and engine frame number. The test suite includes pixel readback and
fault calibration; matching metadata alone is not proof that glyphs were drawn.
