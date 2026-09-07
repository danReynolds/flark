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

final controller = FlarkController(
  FlarkEditor(createParseBackend(), text: '# A note\n\nStart writing.'),
);

// Place inside a bounded-height parent.
FlarkEditorWidget(controller: controller, autofocus: true);

// The same presentation in a read-only host.
FlarkMarkdownView(controller: controller);

controller.command(const InsertText('hello'));
final markdown = controller.text;

// Dispose the caller-owned controller after removing its widgets.
controller.dispose();
```

On Flutter web, asynchronously load the declared
`packages/flark/lib/assets/wasm/flark_parse.wasm` asset and create a
`WasmParseBackend.fromBytes` from `package:flark/wasm.dart`. The example's
conditional `backend.dart` shows both transports. The kernel remains Dart-first;
the host does not contain a second Markdown interpreter.

## Optional Tree-sitter code regions

The workbench uses `flark_tree_sitter` for all 14 code languages and Automatic
language detection. There is one shared language integration. The
[capability table](../flark_tree_sitter/SINGLE_ENGINE_REVIEW.md) lists exact
indentation coverage and grammar versions. A manual choice is authoritative;
unknown languages and plain text retain ordinary whitespace editing.

```dart
import 'package:flark_flutter/code.dart';

// Load native assets or bundled Wasm and warm editing queries before mounting.
final code = await FlarkTreeSitter.load();
final editor = FlarkEditor(markdownBackend, text: markdown, codeEditing: code);
final controller = FlarkController(
  editor,
  codeColors: FlarkCodeColors(editor),
);

// After removing the editor widgets:
controller.dispose(); // terminates its coloring worker
code.dispose();       // releases the caller-owned synchronous analyzer
```

One `FlarkTreeSitter` may serve sequential documents. Each controller owns its
own coloring worker. Source, indentation, selection and undo publish
synchronously. Colors arrive separately and may only describe the exact current
snippet and language; pending text uses the plain code style. Code colors do not
change font metrics. The active fence and visible fences take priority, with a
32-entry / 65,536-code-unit cache. Unsupported or oversized snippets remain plain; worker failure leaves current text plain and is observable
through `controller.codeColors?.failure`.

For custom web asset routing, `FlarkTreeSitter.fromAnalyzer` accepts an already
loaded `CodeAnalyzer`, and `FlarkCodeColors` accepts `workerUri` / `wasmUri`.
Default assets are bundled by Flutter. CSP and physical-device qualification
remain the embedding application's responsibility. The
[host integration review](../flark_tree_sitter/HOST_INTEGRATION_REVIEW.md)
records automated evidence; the
[browser dogfood review](../flark_tree_sitter/BROWSER_DOGFOOD_REVIEW.md) records
hands-on journeys and the remaining transition/performance/device gates.

Use controller commands while a Flutter input connection is active so composing
state, notices and kernel publication remain coherent. Caller-owned focus nodes
are supported. Replacing a controller invalidates the old input connection and
pending clipboard actions. Clipboard cut/paste also checks the revision before
mutating source.

The editor supports inline styles, paragraphs, headings, lists, quotes, task
checkboxes, code rows, tables and source inspection. Tab navigates table cells;
Return moves to the next row in the same column, then exits the table. Deletion
at a table-cell boundary rejects atomically; restructuring belongs in source
mode. Images currently show their alt text; image previews and link-editing
popovers remain unfinished surface work.

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

`FlarkSourceView(controller: controller)` provides read-only source inspection
using those same bounded pages and follows the editor's canonical caret.

`onPaint` is an optional observation point after actual visible glyph drawing.
It records the snapshot revision, rendered text, resolved styles, selection,
caret and engine frame number. The test suite includes pixel readback and
fault calibration; matching metadata alone is not proof that glyphs were drawn.
