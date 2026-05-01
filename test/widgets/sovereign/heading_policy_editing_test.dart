import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Heading policy editing', () {
    test('Enter on empty ATX heading exits heading mode', () {
      const initial = '# ';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 2);
      controller.handleEnter();

      expect(controller.text, '\n');
      expect(controller.selection.baseOffset, 1);
    });

    test('Enter on empty indented ATX heading preserves indent on exit', () {
      const initial = '  ## ';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(offset: initial.length);
      controller.handleEnter();

      expect(controller.text, '  \n');
      expect(controller.selection.baseOffset, 3);
    });

    test('Enter on non-empty heading performs a normal line split', () {
      const initial = '## Title';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = TextSelection.collapsed(offset: initial.length);
      controller.handleEnter();

      expect(controller.text, '## Title\n');
      expect(controller.selection.baseOffset, initial.length + 1);
    });

    test('Enter before heading marker does not trigger heading policy', () {
      const initial = '# Title';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 0);
      controller.handleEnter();

      expect(controller.text, '\n# Title');
      expect(controller.selection.baseOffset, 1);
    });
  });
}
