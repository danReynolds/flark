import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test('Repro: fence extension keeps both fences hidden', () async {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    controller.text = '```\nabc\n```';
    await Future.delayed(const Duration(milliseconds: 300));

    expect(controller.decoration.hiddenRanges.length, 2);

    controller.value = const TextEditingValue(
      text: '```\nabc\n\n```',
      selection: TextSelection.collapsed(offset: 8),
    );

    // Sync predictive decoration should hide both fences.
    expect(controller.decoration.hiddenRanges.length, 2);

    await Future.delayed(const Duration(milliseconds: 300));

    // Authoritative parse should also hide both fences.
    expect(controller.decoration.hiddenRanges.length, 2);
  });
}
