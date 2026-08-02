# flark_flutter

Flutter input, rendering, and widget adapters for the Dart-first
[`flark`](https://pub.dev/packages/flark) Markdown engine.

```dart
import 'package:flark_flutter/flark_flutter.dart';

FlarkMarkdownEditor(
  initialMarkdown: '# Hello\n\nEdit **Markdown** without losing the source.',
  editingMode: FlarkMarkdownEditingMode.liveRendered,
  onChanged: saveMarkdown,
)
```

The package provides Flutter editors, previews, controller integration,
commands, theming, selection, and form widgets. Pure-Dart consumers should
depend directly on `flark`; the engine does not depend on Flutter.

Use `package:flark_flutter/flark_flutter.dart` for the supported application
API. Advanced adapter integrations can import
`package:flark_flutter/flark_flutter_advanced.dart`.

Documentation and examples are available at
<https://danreynolds.github.io/flark/>.

## License

MIT. See [LICENSE](LICENSE).
