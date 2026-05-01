import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Enter after typing on the fence opener line does not hide content',
    () async {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.value = const TextEditingValue(
        text: '```test',
        selection: TextSelection.collapsed(offset: 7),
      );

      controller.value = const TextEditingValue(
        text: '```test\n',
        selection: TextSelection.collapsed(offset: 8),
      );

      expect(controller.text, equals('```test\n'));

      // Opening ticks are hidden, but unknown opener text ("test") is treated as
      // code content and must not be hidden.
      expect(controller.decoration.hiddenRanges, const [
        TextRange(start: 0, end: 3),
      ]);
    },
  );
}
