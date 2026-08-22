# API Surface

## App Import

Most Flutter apps should import only:

```dart
import 'package:flark_flutter/flark_flutter.dart';
```

This re-exports the shared engine types and adds the promoted Flutter
application surface:

- `FlarkMarkdownEditor`
- `FlarkMarkdownEditorFormField`
- `Markdown`
- `FlarkFlutterController`
- editing modes, interaction config, overlay callbacks, and preview builders
- standard Markdown commands through `controller.commands`
- core runtime result and transaction types commonly needed by app toolbars
- `FlarkNativeComrakParseBackend` and native preflight diagnostics

## Dart Engine

Pure-Dart consumers normally import the Flutter-independent engine package:

```dart
import 'package:flark/flark.dart';
```

For narrower or preview surfaces, `package:flark/flark_core.dart` exports the
v2 core and `package:flark/flark_v3.dart` exposes the in-progress v3
document-session engine. None imports Flutter.

The v3 preview barrel is deliberately small: the document runtime, source-edit
values, semantic revision status, bounded query values, and explicit Web asset
configuration. Parser bindings, host stores, source certification, endpoint
frames, and attachment choreography are not normal application API.

## Advanced Integrations

Use the advanced barrel for custom parsers, native bridge tests, extension
work, or deeper render-plan integration:

```dart
import 'package:flark_flutter/flark_flutter_advanced.dart';
```

Use `package:flark/flark_advanced.dart` when the integration is Dart-only.
Both advanced barrels are intentionally broader than the normal app imports.
Official adapter work that needs the unstable v3 assembly SPI uses
`package:flark/flark_adapter.dart`; applications should not import it.

## Widget Rule

There are three public widgets:

| Widget | Purpose |
| --- | --- |
| `FlarkMarkdownEditor` | Editable FlarkMarkdown. Pass either `initialMarkdown` or `controller`. |
| `FlarkMarkdownEditorFormField` | Editable Markdown wired into Flutter `FormField<String>`. Pass either `initialMarkdown` or `controller`. |
| `Markdown` | Read-only FlarkMarkdown. Pass either `markdown` or `controller`. |

Low-level editing widgets, read-only adapter widgets, parser schedulers, and
text delta adapters are implementation details behind those widgets.
