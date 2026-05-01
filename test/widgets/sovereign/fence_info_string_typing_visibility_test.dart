import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Typing fence info string stays visible at caret (no typing regression)',
    () async {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.text = '``';

      controller.value = const TextEditingValue(
        text: '```',
        selection: TextSelection.collapsed(offset: 3),
      );

      // Allow any immediate parse/decoration work to run.
      await Future.delayed(Duration.zero);

      controller.value = const TextEditingValue(
        text: '```a',
        selection: TextSelection.collapsed(offset: 4),
      );

      // Opening fence markers are hidden, but the trailing info string should be
      // popped while the caret is at its end boundary.
      expect(controller.decoration.hiddenRanges, const [
        TextRange(start: 0, end: 3),
      ]);
    },
  );

  test(
    'Typing immediately after ``` does not reflow into inline wrapper (`x`)',
    () async {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```',
        selection: TextSelection.collapsed(offset: 3),
      );
      await Future.delayed(Duration.zero);

      final caret = controller.selection.baseOffset;
      final nextText = controller.text.replaceRange(caret, caret, 'f');
      controller.value = TextEditingValue(
        text: nextText,
        selection: TextSelection.collapsed(offset: caret + 1),
      );

      expect(controller.text, '```f');
      expect(controller.selection, const TextSelection.collapsed(offset: 4));
      expect(controller.decoration.hiddenRanges, const [
        TextRange(start: 0, end: 3),
      ]);
    },
  );
}
