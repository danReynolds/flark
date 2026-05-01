import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('IME composition undo behavior', () {
    test('Undo after composition commit restores pre-composition text', () {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: 'a',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );
      controller.value = const TextEditingValue(
        text: 'あ',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );
      controller.value = const TextEditingValue(
        text: 'あ',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange.empty,
      );

      expect(controller.text, 'あ');
      controller.undo();
      expect(controller.text, '');

      controller.redo();
      expect(controller.text, 'あ');
    });

    test(
      'Composition commit does not merge into nearby typing undo transaction',
      () {
        final controller = SovereignController(text: '');
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: 'x',
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        );

        controller.value = const TextEditingValue(
          text: 'xa',
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 1, end: 2),
        );
        controller.value = const TextEditingValue(
          text: 'xあ',
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange(start: 1, end: 2),
        );
        controller.value = const TextEditingValue(
          text: 'xあ',
          selection: TextSelection.collapsed(offset: 2),
          composing: TextRange.empty,
        );

        expect(controller.text, 'xあ');

        controller.undo();
        expect(
          controller.text,
          'x',
          reason: 'First undo should revert committed composition only.',
        );

        controller.undo();
        expect(
          controller.text,
          '',
          reason: 'Second undo should revert prior typing.',
        );
      },
    );

    test('Undo/redo are no-op while composing is active', () {
      final controller = SovereignController(text: 'x');
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: 'xa',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 1, end: 2),
      );
      expect(controller.text, 'xa');

      controller.undo();
      expect(controller.text, 'xa');

      controller.redo();
      expect(controller.text, 'xa');
    });
  });
}
