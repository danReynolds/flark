# API Surface

## App Import

Most Flutter apps should import only:

```dart
import 'package:flark/flark.dart';
```

This exports the promoted application surface:

- `FlarkMarkdownEditor`
- `FlarkMarkdownEditorFormField`
- `Markdown`
- `FlarkFlutterController`
- editing modes, interaction config, overlay callbacks, and preview builders
- standard Markdown commands through `controller.commands`
- core runtime result and transaction types commonly needed by app toolbars
- `FlarkNativeComrakParseBackend` and native preflight diagnostics

## Headless Core

Use the core barrel for non-widget runtime work:

```dart
import 'package:flark/flark_core.dart';
```

This exports document state, source transactions, command registration,
projection, parser DTOs, and render plans without Flutter widgets.

## Advanced Integrations

Use the advanced barrel for custom parsers, native bridge tests, extension
work, or deeper render-plan integration:

```dart
import 'package:flark/flark_advanced.dart';
```

This is intentionally broader than the app import but still excludes
implementation-only Flutter widgets and schedulers.

## Widget Rule

There are three public widgets:

| Widget | Purpose |
| --- | --- |
| `FlarkMarkdownEditor` | Editable FlarkMarkdown. Pass either `initialMarkdown` or `controller`. |
| `FlarkMarkdownEditorFormField` | Editable Markdown wired into Flutter `FormField<String>`. Pass either `initialMarkdown` or `controller`. |
| `Markdown` | Read-only FlarkMarkdown. Pass either `markdown` or `controller`. |

Low-level editing widgets, read-only adapter widgets, parser schedulers, and
text delta adapters are implementation details behind those widgets.
