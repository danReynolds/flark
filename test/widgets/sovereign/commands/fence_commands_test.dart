import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  group('SovereignMarkdownCommands fence commands', () {
    test(
      'insertFence inserts empty fenced template and places caret inside',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);
        controller.selection = const TextSelection.collapsed(offset: 0);

        final result = controller.commands.insertFence();

        expect(result, isA<SovereignCommandApplied>());
        expect(controller.text, equals('```\n\n```'));
        expect(controller.selection, const TextSelection.collapsed(offset: 4));
      },
    );

    test('insertFence wraps selected text as fenced body', () {
      final controller = SovereignController(text: 'print("hi");');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection(
        baseOffset: 0,
        extentOffset: 12,
      );

      final result = controller.commands.insertFence(language: 'dart');

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('```dart\nprint("hi");\n```'));
      expect(
        controller.selection,
        const TextSelection(baseOffset: 8, extentOffset: 20),
      );
    });

    test('insertFence with plain language keeps fence info string empty', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 0);

      final result = controller.commands.insertFence(language: 'plain');

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('```\n\n```'));
      expect(controller.selection, const TextSelection.collapsed(offset: 4));
    });

    test('insertFence rejects during IME composing', () {
      final controller = SovereignController(text: 'hello');
      addTearDown(controller.dispose);
      controller.value = const TextEditingValue(
        text: 'hello',
        selection: TextSelection.collapsed(offset: 5),
        composing: TextRange(start: 0, end: 5),
      );

      final result = controller.commands.insertFence();

      expect(result, isA<SovereignCommandRejected>());
      expect(
        (result as SovereignCommandRejected).reasonCode,
        SovereignCommandReasonCode.imeComposing,
      );
      expect(controller.text, equals('hello'));
    });
  });
}
