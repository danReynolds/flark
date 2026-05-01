import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test('Enter on blank EOF line closes an unclosed fence and exits', () async {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    const baseText = '```\ncode\n';
    controller.value = const TextEditingValue(
      text: baseText,
      selection: TextSelection.collapsed(offset: baseText.length),
    );

    // Simulate pressing Enter on the blank EOF line.
    const enteredText = '```\ncode\n\n';
    controller.value = const TextEditingValue(
      text: enteredText,
      selection: TextSelection.collapsed(offset: enteredText.length),
    );

    expect(controller.text, equals('```\ncode\n```\n'));
    expect(controller.selection.baseOffset, equals('```\ncode\n```\n'.length));
  });

  test('Typing at EOF after fence exit continues on a new line', () async {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.value = const TextEditingValue(
      text: '```\ncode\n```',
      selection: TextSelection.collapsed(offset: '```\ncode\n```'.length),
    );

    controller.value = const TextEditingValue(
      text: '```\ncode\n```x',
      selection: TextSelection.collapsed(offset: '```\ncode\n```x'.length),
    );

    expect(controller.text, equals('```\ncode\n```\nx'));
    expect(controller.selection.baseOffset, equals('```\ncode\n```\nx'.length));
  });
}
