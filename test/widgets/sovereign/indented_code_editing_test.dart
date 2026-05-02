import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  group('Sovereign indented code editing', () {
    test('Enter continues indentation for 4-space indented code line', () {
      final controller = SovereignController(
        text: '    final x = 1;',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(controller.text, equals('    final x = 1;\n    '));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });

    test('Second Enter on blank indented line exits code block', () {
      final controller = SovereignController(
        text: '    final x = 1;',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();
      controller.handleEnter();

      expect(controller.text, equals('    final x = 1;\n\n'));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });

    test('Enter continues indentation for tab-indented code line', () {
      final controller = SovereignController(
        text: '\tfinal x = 1;',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(controller.text, equals('\tfinal x = 1;\n\t'));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });

    test('Tab-indented list Enter continues list marker, not code indent', () {
      final controller = SovereignController(
        text: '\t- item',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(controller.text, equals('\t- item\n\t- '));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });

    test('Enter does not treat ordinary 2-space indent as code block', () {
      final controller = SovereignController(
        text: '  not code',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      controller.handleEnter();

      expect(controller.text, equals('  not code\n'));
      expect(controller.selection.baseOffset, equals(controller.text.length));
    });

    test('Backspace at code indent boundary removes one space indent unit', () {
      const initial = '        code';
      final controller = SovereignController(
        text: initial,
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 8);

      final old = controller.value;
      controller.value = old.copyWith(
        text: initial.replaceRange(7, 8, ''),
        selection: const TextSelection.collapsed(offset: 7),
        composing: TextRange.empty,
      );

      expect(controller.text, equals('    code'));
      expect(controller.selection.baseOffset, 4);
    });

    test('Backspace at first code indent exits indentation', () {
      const initial = '    code';
      final controller = SovereignController(
        text: initial,
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 4);

      final old = controller.value;
      controller.value = old.copyWith(
        text: initial.replaceRange(3, 4, ''),
        selection: const TextSelection.collapsed(offset: 3),
        composing: TextRange.empty,
      );

      expect(controller.text, equals('code'));
      expect(controller.selection.baseOffset, 0);
    });

    test('Backspace at tab code indent removes the tab unit', () {
      const initial = '\tcode';
      final controller = SovereignController(
        text: initial,
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 1);

      final old = controller.value;
      controller.value = old.copyWith(
        text: initial.replaceRange(0, 1, ''),
        selection: const TextSelection.collapsed(offset: 0),
        composing: TextRange.empty,
      );

      expect(controller.text, equals('code'));
      expect(controller.selection.baseOffset, 0);
    });

    test('Backspace on tab-indented list keeps normal list outdent', () {
      const initial = '\t- item';
      final controller = SovereignController(
        text: initial,
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 1);

      final old = controller.value;
      controller.value = old.copyWith(
        text: initial.replaceRange(0, 1, ''),
        selection: const TextSelection.collapsed(offset: 0),
        composing: TextRange.empty,
      );

      expect(controller.text, equals('- item'));
      expect(controller.selection.baseOffset, 0);
    });
  });
}
