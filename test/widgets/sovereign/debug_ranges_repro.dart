// ignore_for_file: avoid_print

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test('Debug Ranges: Check for overlapping ranges at EOL', () async {
    final text = '```\nabc\n```';
    // Indices:
    // ` ` ` ` \n a b c \n ` ` `
    // 0 1 2 3  4 5 6 7  8 9 10 11
    // Line 1 'abc' ends at 7.
    // Newline at 8.

    final controller = SovereignController(text: text);
    await Future.delayed(Duration.zero); // Wait for async parse

    print("--- HIDDEN RANGES ---");
    for (final range in controller.decoration.hiddenRanges) {
      print(
        "Range: [${range.start}, ${range.end}) Text: '${text.substring(range.start, range.end)}'",
      );
    }
    print("---------------------");

    // Test Projection of Offset 8 (Before \n, EOL)
    final raw = const TextSelection.collapsed(offset: 8);
    // We expect 8 to be valid.

    // Simulate set selection logic
    // We need access to private _projector, or rely on effect?
    // We can't access _projector directly.
    // But we can check effect:
    controller.selection = raw;
    print("Set Selection: 8. Result: ${controller.selection.baseOffset}");

    // Test Offset 7 (between 'c' and ' ') - actually 8 is EOL. 'c' is at 6 (index 6, len 1). 'abc' is 4,5,6.
    // 0123 456 7
    // So 'c' is index 6.
    // Wait.
    // 0: `
    // 1: `
    // 2: `
    // 3: \n
    // 4: a
    // 5: b
    // 6: c
    // 7: (Next char? is \n at 7?)
    // text[7] should be \n?
    // No, earlier I said `\n` is at 7 in user prompt?
    // Let's check string length.

    print("Text length: ${text.length}");
    for (int i = 0; i < text.length; i++) {
      print("$i: ${text[i]} code=${text.codeUnitAt(i)}");
    }
  });
}
