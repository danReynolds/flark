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
  });
}
