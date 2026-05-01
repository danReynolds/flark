import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  const inlinePlaceholder = '\u2060';

  group('SovereignMarkdownCommands inline styles', () {
    test('toggleInlineStyle wraps collapsed caret with bold markers', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 0);

      final result = controller.commands.toggleInlineStyle(
        SovereignInlineStyle.bold,
      );

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('**$inlinePlaceholder**'));
      expect(controller.selection, const TextSelection.collapsed(offset: 3));
      expect(
        controller.commands.getInlineStyleAtSelection(),
        SovereignInlineStyle.bold,
      );
    });

    test(
      'toggleInlineStyle wraps whitespace selection without dropping markers',
      () {
        final controller = SovereignController(text: ' ');
        addTearDown(controller.dispose);
        controller.selection = const TextSelection(
          baseOffset: 0,
          extentOffset: 1,
        );

        controller.commands.toggleInlineStyle(SovereignInlineStyle.bold);

        expect(controller.text, equals('** **'));
      },
    );

    test(
      'toggleInlineStyle deactivates non-empty wrapper by moving caret out',
      () {
        final controller = SovereignController(text: '**abc**');
        addTearDown(controller.dispose);
        controller.value = const TextEditingValue(
          text: '**abc**',
          selection: TextSelection.collapsed(offset: 4),
        );

        controller.commands.toggleInlineStyle(SovereignInlineStyle.bold);

        expect(controller.text, equals('**abc**'));
        expect(controller.selection.baseOffset, equals(7));
        expect(controller.commands.getInlineStyleAtSelection(), isNull);
      },
    );

    test(
      'toggleInlineStyle moves trailing whitespace outside wrapper on deactivate',
      () {
        final controller = SovereignController(text: '**abc **');
        addTearDown(controller.dispose);
        controller.value = const TextEditingValue(
          text: '**abc **',
          selection: TextSelection.collapsed(offset: 6),
        );

        controller.commands.toggleInlineStyle(SovereignInlineStyle.bold);

        expect(controller.text, equals('**abc** '));
        expect(controller.selection.baseOffset, equals(8));
      },
    );

    test('toggleInlineStyle deactivates empty wrapper by removing markers', () {
      final controller = SovereignController(text: '****');
      addTearDown(controller.dispose);
      controller.value = const TextEditingValue(
        text: '****',
        selection: TextSelection.collapsed(offset: 2),
      );

      controller.commands.toggleInlineStyle(SovereignInlineStyle.bold);

      expect(controller.text, isEmpty);
      expect(controller.selection.baseOffset, equals(0));
      expect(controller.commands.getInlineStyleAtSelection(), isNull);
    });

    test(
      'toggleInlineStyle switches empty wrapper from bold to italic in-place',
      () {
        final controller = SovereignController(text: '****');
        addTearDown(controller.dispose);
        controller.value = const TextEditingValue(
          text: '****',
          selection: TextSelection.collapsed(offset: 2),
        );

        final result = controller.commands.toggleInlineStyle(
          SovereignInlineStyle.italic,
        );

        expect(result, isA<SovereignCommandApplied>());
        expect(controller.text, equals('*$inlinePlaceholder*'));
        expect(controller.selection, const TextSelection.collapsed(offset: 2));
        expect(
          controller.commands.getInlineStyleAtSelection(),
          SovereignInlineStyle.italic,
        );
      },
    );

    test(
      'switching bold to italic avoids adjacent triple-asterisk delimiter run',
      () {
        final controller = SovereignController(text: '**big things. **');
        addTearDown(controller.dispose);
        controller.value = const TextEditingValue(
          text: '**big things. **',
          selection: TextSelection.collapsed(offset: 14),
        );

        final result = controller.commands.toggleInlineStyle(
          SovereignInlineStyle.italic,
        );

        expect(result, isA<SovereignCommandApplied>());
        expect(controller.text, equals('**big things.** *$inlinePlaceholder*'));
        expect(controller.text.contains('***'), isFalse);
      },
    );

    test(
      'deactivateInlineStyle exits non-empty wrapper at suffix boundary',
      () {
        final controller = SovereignController(text: '**abc**');
        addTearDown(controller.dispose);
        controller.value = const TextEditingValue(
          text: '**abc**',
          selection: TextSelection.collapsed(offset: 4),
        );

        final result = controller.commands.deactivateInlineStyle();

        expect(result, isA<SovereignCommandApplied>());
        expect(controller.text, equals('**abc**'));
        expect(controller.selection.baseOffset, equals(7));
        expect(controller.commands.getInlineStyleAtSelection(), isNull);
      },
    );

    test(
      'deactivateInlineStyle clears composing and keeps next typed char outside wrapper',
      () {
        final controller = SovereignController(text: '**abc**');
        addTearDown(controller.dispose);
        controller.value = const TextEditingValue(
          text: '**abc**',
          selection: TextSelection.collapsed(offset: 5),
          composing: TextRange(start: 2, end: 5),
        );

        final result = controller.commands.deactivateInlineStyle();

        expect(result, isA<SovereignCommandApplied>());
        expect(controller.value.composing, TextRange.empty);
        expect(controller.selection.baseOffset, equals(7));

        final before = controller.value;
        final updatedText = before.text.replaceRange(
          before.selection.start,
          before.selection.end,
          'x',
        );
        controller.value = TextEditingValue(
          text: updatedText,
          selection: TextSelection.collapsed(
            offset: before.selection.start + 1,
          ),
          composing: TextRange.empty,
        );

        expect(controller.text, equals('**abc**x'));
        expect(controller.commands.getInlineStyleAtSelection(), isNull);
      },
    );

    test(
      'activeInlineStyleAtCursor reports style for selected placeholder wrapper',
      () {
        final controller = SovereignController(text: '**$inlinePlaceholder**');
        addTearDown(controller.dispose);
        controller.value = TextEditingValue(
          text: '**$inlinePlaceholder**',
          selection: const TextSelection(baseOffset: 0, extentOffset: 5),
        );

        final style = controller.commands.getInlineStyleAtSelection();

        expect(style, equals(SovereignInlineStyle.bold));
      },
    );
  });
}
