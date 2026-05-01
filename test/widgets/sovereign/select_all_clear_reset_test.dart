import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  int visibleLength(String text, List<TextRange> hiddenRanges) {
    var hiddenLen = 0;
    for (final range in hiddenRanges) {
      hiddenLen += range.end - range.start;
    }
    return text.length - hiddenLen;
  }

  group('Select-all clear reset', () {
    test(
      'clearing all text immediately resets decoration/projection state before next typing',
      () async {
        final controller = SovereignController(
          text: '- **old**\n```dart\nprint(1);\n```',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);

        controller.value = TextEditingValue(
          text: '',
          selection: const TextSelection.collapsed(offset: 0),
        );

        expect(controller.text, isEmpty);
        expect(controller.decoration.hiddenRanges, isEmpty);
        expect(controller.decoration.tree.blocks, isEmpty);

        // Let any queued parse complete; empty state should remain empty.
        await Future<void>.delayed(Duration.zero);
        await Future<void>.delayed(Duration.zero);

        expect(controller.text, isEmpty);
        expect(controller.decoration.hiddenRanges, isEmpty);
        expect(controller.decoration.tree.blocks, isEmpty);

        controller.value = const TextEditingValue(
          text: 'x',
          selection: TextSelection.collapsed(offset: 1),
        );

        expect(controller.text, 'x');
        expect(controller.decoration.hiddenRanges, isEmpty);
        expect(controller.decoration.tree.blocks, isEmpty);
      },
    );

    test(
      'projected select-all backspace clears hidden fence markers and state',
      () async {
        final controller = SovereignController(
          text: '```dart\nfinal x = 1;\n```',
          syntaxEngine: const V1SyntaxEngineAdapter(),
        );
        addTearDown(controller.dispose);

        await Future<void>.delayed(Duration.zero);
        await Future<void>.delayed(Duration.zero);

        final oldValue = controller.value;
        final visibleLen = visibleLength(
          oldValue.text,
          controller.decoration.hiddenRanges,
        );
        controller.selection = TextSelection(
          baseOffset: 0,
          extentOffset: visibleLen,
        );

        final current = controller.value;
        controller.value = TextEditingValue(
          text: current.text.replaceRange(
            current.selection.start,
            current.selection.end,
            '',
          ),
          selection: const TextSelection.collapsed(offset: 0),
        );

        expect(controller.text, isEmpty);
        expect(controller.decoration.hiddenRanges, isEmpty);
        expect(controller.decoration.tree.blocks, isEmpty);

        controller.value = const TextEditingValue(
          text: 'hello',
          selection: TextSelection.collapsed(offset: 5),
        );

        expect(controller.text, 'hello');
        expect(controller.decoration.hiddenRanges, isEmpty);
      },
    );
  });
}
