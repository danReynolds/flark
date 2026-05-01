import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Typing at closing fence boundary keeps closing fence on its own line',
    () {
      const initial = '```\nfinal x = 2;\n```\n';
      final controller = SovereignController(text: initial);
      addTearDown(controller.dispose);

      final closeFenceStart = initial.indexOf('\n```') + 1;
      expect(closeFenceStart, greaterThan(0));

      controller.selection = TextSelection.collapsed(offset: closeFenceStart);
      controller.value = TextEditingValue(
        text: initial.replaceRange(closeFenceStart, closeFenceStart, 'f'),
        selection: TextSelection.collapsed(offset: closeFenceStart + 1),
      );

      expect(controller.text, '```\nfinal x = 2;\nf\n```\n');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: closeFenceStart + 1),
      );
      expect(controller.text.contains('f```'), isFalse);
    },
  );
}
