import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  test(
    'Closing fence marker stays hidden when caret is at marker start',
    () async {
      const text = '```\ncode\n```\nnext';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      // Wait for initial parse/decorations.
      for (var i = 0; i < 100; i++) {
        if (controller.decoration.hiddenRanges.isNotEmpty) {
          break;
        }
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }

      // Closing fence starts at offset 9 in this fixture.
      controller.selection = const TextSelection.collapsed(offset: 9);
      await Future<void>.delayed(const Duration(milliseconds: 1));

      expect(
        controller.decoration.hiddenRanges,
        contains(const TextRange(start: 9, end: 12)),
      );
    },
  );

  test(
    'Fence + adjacent inline backtick wrapper does not produce overlapping hidden ranges',
    () async {
      const text = '```\ncode\n````';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);

      // Wait for initial parse/decorations.
      for (var i = 0; i < 100; i++) {
        if (controller.decoration.hiddenRanges.isNotEmpty) break;
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }

      final closingFenceStart = text.lastIndexOf('\n') + 1;
      expect(closingFenceStart, isNonNegative);

      // Place caret between the closing fence (```) and an adjacent trailing
      // backtick so selection-centered empty-inline ranges can overlap the
      // fence marker if projection ranges are not normalized.
      controller.selection = TextSelection.collapsed(
        offset: closingFenceStart + 3,
      );
      await Future<void>.delayed(const Duration(milliseconds: 1));

      final hidden = controller.decoration.hiddenRanges;
      expect(
        hidden,
        anyElement(
          predicate<TextRange>(
            (range) =>
                range.start == closingFenceStart &&
                range.end >= closingFenceStart + 3,
          ),
        ),
      );

      var prevEnd = -1;
      for (final range in hidden) {
        expect(
          range.start >= prevEnd,
          isTrue,
          reason: 'hidden ranges overlap: $hidden',
        );
        prevEnd = range.end;
      }
    },
  );
}
