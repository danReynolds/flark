import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  group('SovereignMarkdownCommands capabilities', () {
    test('reports active style, heading, and quote state at selection', () {
      final controller = SovereignController(text: '## **alpha**');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 6);

      final capabilities = controller.commands.capabilitiesAtSelection();

      expect(capabilities.isComposing, isFalse);
      expect(capabilities.canMutate, isTrue);
      expect(capabilities.activeInlineStyle, SovereignInlineStyle.bold);
      expect(capabilities.activeHeadingLevel, 2);
      expect(capabilities.quoteActive, isFalse);
      expect(
        controller.commands.getInlineStyleAtSelection(),
        SovereignInlineStyle.bold,
      );
      expect(controller.commands.getHeadingLevelAtSelection(), 2);
      expect(controller.commands.isQuoteActiveAtSelection(), isFalse);
    });

    test('reports composing state as non-mutable', () {
      final controller = SovereignController(text: '> quote');
      addTearDown(controller.dispose);
      controller.value = const TextEditingValue(
        text: '> quote',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 0, end: 2),
      );

      final capabilities = controller.commands.capabilitiesAtSelection();

      expect(capabilities.isComposing, isTrue);
      expect(capabilities.canMutate, isFalse);
      expect(capabilities.quoteActive, isTrue);
    });

    test('resets active states after select-all delete', () {
      final controller = SovereignController(text: '## **alpha**');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection(
        baseOffset: 0,
        extentOffset: 11,
      );
      controller.value = const TextEditingValue(
        text: '',
        selection: TextSelection.collapsed(offset: 0),
      );

      final capabilities = controller.commands.capabilitiesAtSelection();

      expect(capabilities.activeInlineStyle, isNull);
      expect(capabilities.activeHeadingLevel, isNull);
      expect(capabilities.quoteActive, isFalse);
      expect(capabilities.canMutate, isTrue);
    });

    test('undo/redo restores capability state around full clear', () {
      final controller = SovereignController(text: '## **alpha**');
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 6);
      expect(
        controller.commands.capabilitiesAtSelection().activeInlineStyle,
        SovereignInlineStyle.bold,
      );
      expect(
        controller.commands.capabilitiesAtSelection().activeHeadingLevel,
        2,
      );

      controller.selection = const TextSelection(
        baseOffset: 0,
        extentOffset: 11,
      );
      controller.value = const TextEditingValue(
        text: '',
        selection: TextSelection.collapsed(offset: 0),
      );
      expect(
        controller.commands.capabilitiesAtSelection().activeInlineStyle,
        isNull,
      );
      expect(
        controller.commands.capabilitiesAtSelection().activeHeadingLevel,
        isNull,
      );

      controller.undo();
      controller.selection = const TextSelection.collapsed(offset: 6);
      expect(
        controller.commands.capabilitiesAtSelection().activeInlineStyle,
        SovereignInlineStyle.bold,
      );
      expect(
        controller.commands.capabilitiesAtSelection().activeHeadingLevel,
        2,
      );

      controller.redo();
      expect(
        controller.commands.capabilitiesAtSelection().activeInlineStyle,
        isNull,
      );
      expect(
        controller.commands.capabilitiesAtSelection().activeHeadingLevel,
        isNull,
      );
    });
  });

  group('SovereignMarkdownCommands transaction', () {
    test(
      'without transaction, multi-command edits undo one step at a time',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        controller.selection = const TextSelection.collapsed(offset: 0);
        controller.commands.insertHorizontalRule();
        controller.commands.insertHorizontalRule();

        expect(controller.text, '\n---\n\n---\n');

        controller.undo();
        expect(controller.text, '\n---\n');
      },
    );

    test(
      'runInTransaction groups multiple command edits into one undo unit',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        controller.selection = const TextSelection.collapsed(offset: 0);
        controller.commands.runInTransaction((commands) {
          commands.insertHorizontalRule();
          commands.insertHorizontalRule();
        });

        expect(controller.text, '\n---\n\n---\n');

        controller.undo();
        expect(controller.text, '');
        expect(controller.selection, const TextSelection.collapsed(offset: 0));
      },
    );
  });
}
