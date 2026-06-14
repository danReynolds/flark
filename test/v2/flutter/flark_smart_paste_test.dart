import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('Smart link paste', () {
    FlarkFlutterController controllerFor(String markdown, FlarkSelection sel) {
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      controller.applySelection(sel, userEvent: 'test');
      return controller;
    }

    test('pasting a URL over a selection wraps it as a link', () {
      final controller = controllerFor(
        'click here',
        const FlarkSelection(baseOffset: 0, extentOffset: 10),
      );

      final applied = controller.applyProjectedTextEdit(
        oldDisplayText: 'click here',
        newDisplayText: 'https://example.com',
      );

      expect(applied, isTrue);
      expect(controller.markdown, '[click here](https://example.com)');
    });

    test('wraps only the selected word, leaving surrounding text intact', () {
      final controller = controllerFor(
        'go here now',
        const FlarkSelection(baseOffset: 3, extentOffset: 7),
      );

      controller.applyProjectedTextEdit(
        oldDisplayText: 'go here now',
        newDisplayText: 'go https://x.io now',
      );

      expect(controller.markdown, 'go [here](https://x.io) now');
    });

    test('accepts www-style URLs', () {
      final controller = controllerFor(
        'site',
        const FlarkSelection(baseOffset: 0, extentOffset: 4),
      );

      controller.applyProjectedTextEdit(
        oldDisplayText: 'site',
        newDisplayText: 'www.example.com',
      );

      expect(controller.markdown, '[site](www.example.com)');
    });

    test('pasting non-URL text over a selection just replaces it', () {
      final controller = controllerFor(
        'click here',
        const FlarkSelection(baseOffset: 0, extentOffset: 10),
      );

      controller.applyProjectedTextEdit(
        oldDisplayText: 'click here',
        newDisplayText: 'tap now',
      );

      expect(controller.markdown, 'tap now');
    });

    test('pasting a URL with no selection inserts it plainly', () {
      final controller = controllerFor('', const FlarkSelection.collapsed(0));

      controller.applyProjectedTextEdit(
        oldDisplayText: '',
        newDisplayText: 'https://example.com',
      );

      expect(controller.markdown, 'https://example.com');
    });

    test('pasting a URL over a URL selection does not double-wrap', () {
      final controller = controllerFor(
        'https://old.com',
        const FlarkSelection(baseOffset: 0, extentOffset: 15),
      );

      controller.applyProjectedTextEdit(
        oldDisplayText: 'https://old.com',
        newDisplayText: 'https://new.com',
      );

      expect(controller.markdown, 'https://new.com');
    });
  });
}
