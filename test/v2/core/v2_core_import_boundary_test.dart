import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('v2 headless layers do not import Flutter or dart:ui', () {
    final headlessDirectories = [
      Directory('lib/src/v2/core'),
      Directory('lib/src/v2/markdown'),
      Directory('lib/src/v2/projection'),
      Directory('lib/src/v2/render_plan'),
    ];
    final dartFiles = headlessDirectories.expand(
      (directory) => directory
          .listSync(recursive: true)
          .whereType<File>()
          .where((file) => file.path.endsWith('.dart')),
    );

    for (final file in dartFiles) {
      final source = file.readAsStringSync();
      expect(
        source,
        isNot(contains('package:flutter/')),
        reason: '${file.path} must stay framework-independent.',
      );
      expect(
        source,
        isNot(contains("import 'dart:ui'")),
        reason: '${file.path} must stay framework-independent.',
      );
    }
  });
}
