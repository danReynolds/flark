import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('insertHtmlAsMarkdown', () {
    test('inserts converted HTML at the caret', () {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      final applied = controller.insertHtmlAsMarkdown(
        '<b>bold</b> and <i>italic</i>',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '**bold** and *italic*');
    });

    test('replaces a selection with the converted HTML', () {
      final controller = FlarkFlutterController.fromMarkdown('replace me');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection(baseOffset: 0, extentOffset: 10),
        userEvent: 'test',
      );

      controller.insertHtmlAsMarkdown('<h2>Title</h2>');

      expect(controller.markdown, '## Title');
    });

    test('returns false for HTML that converts to nothing', () {
      final controller = FlarkFlutterController.fromMarkdown('x');
      addTearDown(controller.dispose);

      expect(controller.insertHtmlAsMarkdown('   '), isFalse);
      expect(controller.markdown, 'x');
    });
  });
}
