import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Sovereign blockquote editing', () {
    test('Enter continues quote and second Enter exits quote', () {
      final controller = SovereignController(text: '> alpha');
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(controller.text, equals('> alpha\n> '));
      expect(controller.selection.baseOffset, equals(controller.text.length));

      controller.handleEnter();

      expect(controller.text, equals('> alpha\n\n'));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });

    test(
      'ArrowDown exits quote from last content line over trailing empty quote',
      () {
        final controller = SovereignController(text: '> alpha\n> \nnext');
        addTearDown(controller.dispose);

        // Start of quote content on first line (column after marker).
        controller.selection = const TextSelection.collapsed(offset: 2);
        final handled = controller.handleArrowDownKey();

        expect(handled, isTrue);
        expect(
          controller.selection.baseOffset,
          equals('> alpha\n> \n'.length + 2),
        );
      },
    );

    test(
      'ArrowUp exits quote from first content line over leading empty quote',
      () {
        final controller = SovereignController(text: 'before\n> \n> alpha');
        addTearDown(controller.dispose);

        final quoteLineStart = controller.text.lastIndexOf('> alpha');
        controller.selection = TextSelection.collapsed(
          offset: quoteLineStart + 2,
        );
        final handled = controller.handleArrowUpKey();

        expect(handled, isTrue);
        expect(controller.selection.baseOffset, equals(2));
      },
    );

    test(
      'Quoted task item Enter continues, then empty quote Enter exits quote',
      () {
        final controller = SovereignController(text: '> - [ ] todo');
        addTearDown(controller.dispose);

        controller.selection = TextSelection.collapsed(
          offset: controller.text.length,
        );
        controller.handleEnter();

        expect(controller.text, equals('> - [ ] todo\n> - [ ] '));
        expect(controller.selection.baseOffset, equals(controller.text.length));

        controller.handleEnter();

        expect(controller.text, equals('> - [ ] todo\n> - [ ] \n> '));
        expect(controller.selection.baseOffset, equals(controller.text.length));

        controller.handleEnter();

        expect(controller.text, equals('> - [ ] todo\n> - [ ] \n\n'));
        expect(controller.selection.baseOffset, equals(controller.text.length));
      },
    );

    test(
      'ArrowDown exits quote from last quoted task content line over trailing empty quote',
      () {
        final controller = SovereignController(text: '> - [ ] todo\n> \nnext');
        addTearDown(controller.dispose);

        controller.selection = const TextSelection.collapsed(offset: 6);
        final handled = controller.handleArrowDownKey();

        expect(handled, isTrue);
        expect(
          controller.selection.baseOffset,
          // Column is preserved but clamped to the target line length ("next").
          equals('> - [ ] todo\n> \n'.length + 'next'.length),
        );
      },
    );
  });
}
