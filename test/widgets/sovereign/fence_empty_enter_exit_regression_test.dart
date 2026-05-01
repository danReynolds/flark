import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test('Empty fence exits on second Enter and does not add extra fences', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```',
      selection: TextSelection.collapsed(offset: 3),
    );

    // Enter #1: enter fence body.
    controller.value = const TextEditingValue(
      text: '```\n',
      selection: TextSelection.collapsed(offset: 4),
    );
    expect(controller.text, '```\n');
    expect(controller.selection.baseOffset, 4);

    // Enter #2 on blank EOF line exits by inserting the closing fence.
    controller.value = const TextEditingValue(
      text: '```\n\n',
      selection: TextSelection.collapsed(offset: 5),
    );
    expect(controller.text, '```\n```\n');
    expect(controller.selection.baseOffset, 8);
    expect(controller.geometry.codeBlocks.length, 1);
    // Closed fences should not paint an extra trailing line.
    expect(controller.geometry.codeBlocks.first.endLine, 2);

    // Enter #3 is now outside the fence and should be a normal newline.
    controller.value = const TextEditingValue(
      text: '```\n```\n\n',
      selection: TextSelection.collapsed(offset: 9),
    );
    expect(controller.text, '```\n```\n\n');
    expect(controller.selection.baseOffset, 9);
  });
}
