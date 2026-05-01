import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Vertical cursor movement preserves preferred column across short lines',
    () {
      final controller = SovereignController(text: 'abcdef\nxy\nabcdef\n');
      addTearDown(controller.dispose);

      controller.selection = const TextSelection.collapsed(offset: 5);

      expect(controller.handleArrowDownKey(), isTrue);
      expect(controller.selection.baseOffset, equals('abcdef\n'.length + 2));

      expect(controller.handleArrowDownKey(), isTrue);
      expect(
        controller.selection.baseOffset,
        equals('abcdef\nxy\n'.length + 5),
      );
    },
  );

  test('Horizontal selection change resets preferred vertical column', () {
    final controller = SovereignController(text: 'abcdef\nxy\nabcdef\n');
    addTearDown(controller.dispose);

    controller.selection = const TextSelection.collapsed(offset: 5);
    expect(controller.handleArrowDownKey(), isTrue);
    expect(controller.selection.baseOffset, equals('abcdef\n'.length + 2));

    controller.selection = TextSelection.collapsed(
      offset: 'abcdef\n'.length + 1,
    );
    expect(controller.handleArrowDownKey(), isTrue);
    expect(controller.selection.baseOffset, equals('abcdef\nxy\n'.length + 1));
  });
}
