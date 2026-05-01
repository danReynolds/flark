import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  group('SovereignMarkdownCommands block styles', () {
    test('setHeadingLevel applies H2 prefix to current line', () {
      final controller = SovereignController(text: 'alpha');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 5);

      final result = controller.commands.setHeadingLevel(2);

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('## alpha'));
      expect(controller.commands.getHeadingLevelAtSelection(), equals(2));
    });

    test('setHeadingLevel(null) removes heading markers from current line', () {
      final controller = SovereignController(text: '### alpha');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 6);

      final result = controller.commands.setHeadingLevel(null);

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('alpha'));
      expect(controller.commands.getHeadingLevelAtSelection(), isNull);
    });

    test('toggleQuote toggles quote prefix and quote detection', () {
      final controller = SovereignController(text: 'quote me');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 0);

      final first = controller.commands.toggleQuote();
      expect(first, isA<SovereignCommandApplied>());
      expect(controller.text, equals('> quote me'));
      expect(controller.commands.isQuoteActiveAtSelection(), isTrue);

      final second = controller.commands.toggleQuote();
      expect(second, isA<SovereignCommandApplied>());
      expect(controller.text, equals('quote me'));
      expect(controller.commands.isQuoteActiveAtSelection(), isFalse);
    });

    test('toggleBulletList toggles list marker on current line', () {
      final controller = SovereignController(text: 'item');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 2);

      final first = controller.commands.toggleBulletList();
      expect(first, isA<SovereignCommandApplied>());
      expect(controller.text, equals('- item'));

      final second = controller.commands.toggleBulletList();
      expect(second, isA<SovereignCommandApplied>());
      expect(controller.text, equals('item'));
    });

    test(
      'toggleBulletList on quoted line inserts marker after quote prefix',
      () {
        final controller = SovereignController(text: '> item');
        addTearDown(controller.dispose);
        controller.selection = const TextSelection.collapsed(offset: 3);

        final first = controller.commands.toggleBulletList();
        expect(first, isA<SovereignCommandApplied>());
        expect(controller.text, equals('> - item'));

        final second = controller.commands.toggleBulletList();
        expect(second, isA<SovereignCommandApplied>());
        expect(controller.text, equals('> item'));
      },
    );

    test('toggleTaskList inserts task marker for plain line fallback', () {
      final controller = SovereignController(text: 'item');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 0);

      final result = controller.commands.toggleTaskList();

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('- [ ] item'));
    });

    test('toggleTaskList fallback preserves leading quote prefix', () {
      final controller = SovereignController(text: '> item');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 3);

      final result = controller.commands.toggleTaskList();

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('> - [ ] item'));
    });

    test('toggleTaskList inserts checkbox on existing bullet list line', () {
      final controller = SovereignController(text: '- item');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 2);

      final result = controller.commands.toggleTaskList();

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('- [ ] item'));
    });

    test('toggleTaskList toggles task checkbox state', () {
      final controller = SovereignController(text: '- [ ] item');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 4);

      final first = controller.commands.toggleTaskList();
      expect(first, isA<SovereignCommandApplied>());
      expect(controller.text, equals('- [x] item'));

      final second = controller.commands.toggleTaskList();
      expect(second, isA<SovereignCommandApplied>());
      expect(controller.text, equals('- [ ] item'));
    });

    test('insertHorizontalRule inserts markdown hr and advances caret', () {
      final controller = SovereignController(text: 'alpha');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 5);

      final result = controller.commands.insertHorizontalRule();

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('alpha\n---\n'));
      expect(controller.selection, const TextSelection.collapsed(offset: 10));
    });
  });
}
