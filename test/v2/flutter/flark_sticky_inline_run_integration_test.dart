import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('Sticky inline run (controller parse adoption)', () {
    test('keeps a trailing-space run rendered through a real parse', () async {
      final controller = FlarkFlutterController.fromMarkdown('**foo **');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );

      await controller.parseNow();

      // The parser alone treats `**foo **` as literal; the sticky reconciler at
      // adoption keeps the markers hidden while the caret is inside the run.
      expect(controller.projection.projectText(controller.markdown), 'foo ');
    });

    test('reveals the markers once the caret leaves the run', () async {
      final controller = FlarkFlutterController.fromMarkdown('**foo **');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(0),
        userEvent: 'test',
      );

      await controller.parseNow();

      expect(controller.projection.projectText(controller.markdown), '**foo **');
    });

    test('a multi-word trailing-space run stays rendered', () async {
      final controller = FlarkFlutterController.fromMarkdown('**foo bar **');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(10),
        userEvent: 'test',
      );

      await controller.parseNow();

      expect(controller.projection.projectText(controller.markdown), 'foo bar ');
    });
  });
}
