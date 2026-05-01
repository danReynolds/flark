import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Fenced code closing fence hides when block ends with trailing newline',
    () async {
      final controller = SovereignController(text: '');
      addTearDown(controller.dispose);

      const text = '```\ncode\n```\n';
      controller.value = const TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );

      final open = text.indexOf('```');
      final close = text.lastIndexOf('```');
      expect(open, 0);
      expect(close, isNot(0));

      // Predictive (sync) hiding: both fences should be hidden immediately.
      expect(
        controller.decoration.hiddenRanges,
        containsAll(<TextRange>[
          TextRange(start: open, end: open + 3),
          TextRange(start: close, end: close + 3),
        ]),
      );

      // Authoritative (async) hiding: should remain true after parse.
      await Future.delayed(const Duration(milliseconds: 300));
      expect(
        controller.decoration.hiddenRanges,
        containsAll(<TextRange>[
          TextRange(start: open, end: open + 3),
          TextRange(start: close, end: close + 3),
        ]),
      );
    },
  );
}
