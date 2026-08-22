# Getting started

Flark v4 is a native Flutter editor. Its active product targets are macOS,
Android, and iOS; Linux and Web are not active targets. Rust builds
automatically as a bundled native asset, so development machines and package
consumers need a stable Rust toolchain with `cargo` or `rustup` on `PATH`.

## Run the example

From a checkout:

```sh
cd packages/flark/example
flutter pub get
flutter run -d macos --profile
```

The [platform guide](parser_and_platforms.md) lists supported target
architectures and build-only smoke commands.

## Open an editor

Apps import the supported `flark` barrel. The caller owns the asynchronous
controller lifecycle.

```dart
import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flutter/widgets.dart';

final class MarkdownEditor extends StatefulWidget {
  const MarkdownEditor({super.key});

  @override
  State<MarkdownEditor> createState() => _MarkdownEditorState();
}

final class _MarkdownEditorState extends State<MarkdownEditor> {
  FlarkEditorController? controller;

  @override
  void initState() {
    super.initState();
    unawaited(_open());
  }

  Future<void> _open() async {
    final opened = await FlarkEditorController.open('# Hello Flark\n');
    if (!mounted) {
      await opened.close();
      return;
    }
    setState(() => controller = opened);
  }

  @override
  void dispose() {
    final current = controller;
    if (current != null) unawaited(current.close());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final current = controller;
    if (current == null) return const SizedBox.shrink();
    return FlarkEditor(controller: current, autofocus: true);
  }
}
```

`FlarkEditorController` is a `ChangeNotifier`; listen to it or rebuild with a
`ListenableBuilder` when application chrome depends on selection, history, or
opening state. The editor itself already observes the controller.

## Read-only rendering

Reuse the same controller with `FlarkMarkdownView` when an application needs a
non-editable presentation without creating a second parser:

```dart
FlarkMarkdownView(controller: controller)
```

The caller still owns and closes the controller.
