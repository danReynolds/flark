# Flark

Markdown-first editing and rendering for Flutter.

Flark gives Flutter apps two core widgets and a thin Flutter form wrapper:

Live demo and package site: <https://danreynolds.github.io/flark/>

```dart
import 'package:flark/flark.dart';

MarkdownEditor(
  initialMarkdown: '# Hello\n\nEdit **Markdown** without losing the source.',
  editingMode: FlarkMarkdownEditingMode.liveRendered,
  onChanged: saveMarkdown,
)
```

```dart
Markdown(markdown: '# Preview')
```

The document truth stays Markdown. The editor, preview, toolbar commands,
projection layer, and rendered block widgets all work from that same source
document instead of converting user content into a private rich-text model.

![Flark visual surfaces](test/v2/flutter/goldens/flark_v2_surfaces.png)

## Why Flark

- `MarkdownEditor` edits Markdown in source or live-rendered mode.
- `MarkdownEditorFormField` wires the editor into Flutter `Form` validation,
  saving, and reset flows.
- `Markdown` renders read-only Markdown from a string or a shared controller.
- `FlarkFlutterController` keeps editor, preview, toolbar, undo, redo, parser
  state, and render plans in sync.
- The default parser is Comrak: native FFI on macOS, iOS, Android, and Linux;
  packaged WASM on web.
- The headless Dart core owns transactions, commands, projection, history, and
  render plans without importing Flutter.

## Shared Editor and Preview

Use a controller when multiple surfaces should track the same document:

```dart
final controller = FlarkFlutterController.fromMarkdown(
  '# Hello\n\nEdit **Markdown** without losing the source.',
);

Column(
  children: [
    Expanded(child: MarkdownEditor(controller: controller)),
    Expanded(child: Markdown(controller: controller)),
  ],
)
```

`initialMarkdown` is only used to create a widget-owned controller. For
document switching, pass a new widget key or manage a `FlarkFlutterController`
yourself.

## Toolbar Commands

Toolbar code talks to the controller, not the widget tree:

```dart
IconButton(
  icon: const Icon(Icons.format_bold),
  onPressed: () => controller.commands.toggleStrong(),
)

IconButton(
  icon: const Icon(Icons.table_chart),
  onPressed: () => controller.commands.insertTable(columns: 3, bodyRows: 2),
)
```

Command helpers return `FlarkEditorRuntimeResult`, so advanced integrations can
inspect whether a command was handled, ignored, or rejected.

## Imports

Most apps should use one import:

```dart
import 'package:flark/flark.dart';
```

Advanced imports are split by intent:

- `package:flark/flark_core.dart`: headless document/runtime/projection/render
  plan APIs.
- `package:flark/flark_advanced.dart`: full parser, native bridge, extension,
  and Flutter integration surface.

Deep imports under `src/` are for Flark internals and white-box package tests.

## Performance

Editing stays on the synchronous fast path — a keystroke applies in
microseconds through 100 KB documents:

| Document | Keystroke apply (median) | Native parse + decode (median) |
| --- | --- | --- |
| 1 KB | 4 µs | 1 ms |
| 100 KB | 172 µs | 55 ms |
| 1 MB | 5.5 ms | ~0.5 s |

Both paths are linear in document size. See [Benchmarks](doc/benchmarks.md) for
the enforced lane and methodology.

## Documentation

- [Getting Started](doc/getting_started.md)
- [Cookbook](doc/cookbook.md)
- [API Surface](doc/api_surface.md)
- [Parser and Platforms](doc/parser_and_platforms.md)
- [Development and Verification](doc/development.md)
- [Benchmarks](doc/benchmarks.md)
- [Architecture Notes](doc/README.md)

## Example App

The `example/` app is the dogfood workbench and GitHub Pages site. It imports
only `package:flark/flark.dart` and exercises source, live-rendered, form,
toolbar, docs, and read-only rendering flows.

```bash
cd example
flutter run -d macos
```

## Verification

Fast local confidence gate:

```bash
./scripts/verify_package_confidence.sh
```

Full release gate:

```bash
./scripts/verify_release.sh
```

Visual baselines:

```bash
flutter test test/v2/flutter/flark_v2_visual_golden_test.dart
```

## License

Flark is available under the MIT license. See [LICENSE](LICENSE).
