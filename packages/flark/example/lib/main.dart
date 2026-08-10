import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/material.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final configured = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  final libraryPath =
      configured ??
      File(
        '../../../native/comrak_bridge/target/debug/libflark_abi.dylib',
      ).absolute.path;
  try {
    final controller = await FlarkEditorController.open(
      _demoDocument,
      libraryPath: libraryPath,
    );
    runApp(FlarkExample(controller: controller));
  } catch (error, stackTrace) {
    runApp(FlarkStartupFailure(error: error, stackTrace: stackTrace));
  }
}

final class FlarkExample extends StatefulWidget {
  const FlarkExample({required this.controller, super.key});

  final FlarkEditorController controller;

  @override
  State<FlarkExample> createState() => _FlarkExampleState();
}

final class _FlarkExampleState extends State<FlarkExample> {
  @override
  void dispose() {
    widget.controller.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        brightness: Brightness.light,
        scaffoldBackgroundColor: const Color(0xfff8f7f4),
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xff246bfd),
          brightness: Brightness.light,
        ),
      ),
      home: Scaffold(
        body: SafeArea(
          child: Column(
            children: [
              AnimatedBuilder(
                animation: widget.controller,
                builder: (context, _) => _StatusBar(
                  status: widget.controller.status,
                  revision: widget.controller.revision,
                  pendingEdits: widget.controller.pendingEdits,
                  sourceBytes: widget.controller.sourceByteLength,
                ),
              ),
              const Divider(height: 1),
              Expanded(
                child: ColoredBox(
                  color: const Color(0xfffffefa),
                  child: FlarkEditor(
                    controller: widget.controller,
                    autofocus: true,
                    textStyle: const TextStyle(
                      color: Color(0xff25272b),
                      fontSize: 17,
                      height: 1.48,
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

final class _StatusBar extends StatelessWidget {
  const _StatusBar({
    required this.status,
    required this.revision,
    required this.pendingEdits,
    required this.sourceBytes,
  });

  final FlarkEditorStatus status;
  final int revision;
  final int pendingEdits;
  final int sourceBytes;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 42,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 18),
        child: Row(
          children: [
            const Text(
              'FLARK',
              style: TextStyle(fontWeight: FontWeight.w800, letterSpacing: 1.4),
            ),
            const SizedBox(width: 16),
            Text(status.name),
            const Spacer(),
            Text(
              'rev $revision  •  $pendingEdits pending  •  '
              '${(sourceBytes / 1024).toStringAsFixed(1)} KiB',
              style: const TextStyle(
                color: Color(0xff6b7078),
                fontFeatures: [FontFeature.tabularFigures()],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

final class FlarkStartupFailure extends StatelessWidget {
  const FlarkStartupFailure({
    required this.error,
    required this.stackTrace,
    super.key,
  });

  final Object error;
  final StackTrace stackTrace;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        body: Padding(
          padding: const EdgeInsets.all(32),
          child: SelectableText(
            'Flark could not open its native runtime.\n\n$error\n\n$stackTrace',
          ),
        ),
      ),
    );
  }
}

const _demoDocument = '''
# Flark v4

A live Markdown editor built around a bounded incremental core and a custom
Flutter render surface.

## What this slice proves

- The complete source and parser state live off Flutter's UI isolate.
- Platform text input sees a bounded active window, not the full document.
- Typing paints optimistically before the native revision is acknowledged.
- Certified rows replace neutral source without changing document authority.

> Click a row and type quickly. Markdown punctuation appears while that row is
> active and projects away again after the incremental revision settles.

```dart
final editor = FlarkEditor(controller: controller);
```

This is the macOS proving ground. Android and iOS use the same Dart and native
boundaries once device benchmark hardware is available.
''';
