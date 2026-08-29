import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  final controller = File('lib/src/controller.dart').readAsStringSync();
  final renderer = File('lib/src/render_surface.dart').readAsStringSync();

  test('controller has one immutable outward publication function', () {
    expect(
      RegExp(r'super\.notifyListeners\(').allMatches(controller),
      hasLength(1),
    );
    expect(
      RegExp(
        r'_snapshot\s*=\s*_captureEditorSnapshot\(',
      ).allMatches(controller),
      hasLength(1),
    );
    expect(controller, isNot(contains('FlarkSurfacePublication')));
    expect(controller, isNot(contains('surfacePublication')));
  });

  test('renderer reads visual truth only from the captured snapshot', () {
    final reads = RegExp(
      r'_controller\.([A-Za-z][A-Za-z0-9_]*)',
    ).allMatches(renderer).map((match) => match.group(1)!).toSet();

    expect(reads, {
      'addListener',
      'removeListener',
      'nextViewportPage',
      'previousViewportPage',
      'snapshot',
      'toggleTaskChecked',
    });
  });
}
