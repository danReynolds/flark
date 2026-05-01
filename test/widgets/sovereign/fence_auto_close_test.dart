import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test('Opening ``` + Enter only inserts a single newline', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```',
      selection: TextSelection.collapsed(offset: 3),
    );

    // Simulate pressing Enter at the end of the opening fence line.
    controller.value = const TextEditingValue(
      text: '```\n',
      selection: TextSelection.collapsed(offset: 4),
    );

    expect(controller.text, equals('```\n'));
    expect(controller.selection.baseOffset, equals(4));
  });
}
