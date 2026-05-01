import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Fence info string is visible while caret is at end-of-opening-line',
    () async {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.text = '```dart\nprint(1);\n```';
      await Future.delayed(const Duration(milliseconds: 300));

      // Sanity: info string is hidden by default.
      expect(
        controller.decoration.hiddenRanges.any(
          (r) => r.start == 3 && r.end == 7,
        ),
        isTrue,
      );

      // Place caret at the end of the info string (just before the newline).
      controller.selection = const TextSelection.collapsed(offset: 7);

      // The info string should be "popped" so the user can see/edit it.
      expect(
        controller.decoration.hiddenRanges.any(
          (r) => r.start == 3 && r.end == 7,
        ),
        isFalse,
      );
    },
  );
}
