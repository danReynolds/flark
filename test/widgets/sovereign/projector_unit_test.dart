import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/logic/projector.dart';
import 'package:sovereign_editor/widgets/sovereign/models/decoration_model.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_tree.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

void main() {
  group('Projector Logic', () {
    test('Snap Logic at Boundaries', () {
      // Setup: Text "abc[HIDDEN]def"
      // Indices: 012 [345] 678
      // Range: [3, 6] (length 3).
      final hidden = [const TextRange(start: 3, end: 6)];
      final model = DecorationModel(
        tree: BlockTree.empty(), // Dummy
        lineIndex: LineIndex.empty(), // Dummy
        originRevision: 0,
        hiddenRanges: hidden,
      );
      final projector = Projector(model);

      // 1. Valid Spots
      expect(
        projector
            .projectSelection(sel(0), previousSelection: sel(0))
            .baseOffset,
        0,
      );
      expect(
        projector
            .projectSelection(sel(2), previousSelection: sel(2))
            .baseOffset,
        2,
      );
      expect(
        projector
            .projectSelection(sel(6), previousSelection: sel(6))
            .baseOffset,
        6,
      );

      // 2. Inside Snapping (Forward Bias)
      // Moving from 2 -> 3 (Into start)
      expect(
        projector
            .projectSelection(sel(3), previousSelection: sel(2))
            .baseOffset,
        3,
      );

      // Moving from 2 -> 4 (Middle)
      expect(
        projector
            .projectSelection(sel(4), previousSelection: sel(0))
            .baseOffset,
        6,
      );

      // 3. Inside Snapping (Backward Bias)
      // Moving from 7 -> 5 (Into end)
      expect(
        projector
            .projectSelection(sel(5), previousSelection: sel(7))
            .baseOffset,
        3,
      );
    });

    test('EOL Degenerate Case (User Bug Repro)', () {
      // Setup: "line1" then hidden newline? Or hidden marker?
      // Scenario: "line1" ends at 5. Hidden range at [2, 5] ??
      // User says: "Enter applied at lineEnd - 1".
      // Text: "abc". Length 3.
      // Hidden Range: [0, 3] (Full overlap?) No.

      // Hypothetical "Bad Range": [2, 3) (The 'c').
      // Text: ab[c]
      // 01 2
      // Cursor at 3 (EOL).
      // If Projector thinks 3 is inside [2, 3)? No.

      // User Theory: "degenerate geometry around hidden ranges".
      // Let's test overlapping range behavior.

      final hidden = [const TextRange(start: 2, end: 3)]; // Hides char at 2.
      final model = DecorationModel.empty().copyWith(hiddenRanges: hidden);
      final projector = Projector(model);

      // Cursor at 3. Previous at 3.
      // `_isInside(3)` -> 3 > 2 && 3 < 3 -> False.
      expect(
        projector
            .projectSelection(sel(3), previousSelection: sel(3))
            .baseOffset,
        3,
      );

      // What if range is [2, 4]? (Past EOL)
      // Text len 3. Range [2, 4].
      final hidden2 = [const TextRange(start: 2, end: 4)];
      final projector2 = Projector(
        DecorationModel.empty().copyWith(hiddenRanges: hidden2),
      );

      // Cursor at 3 (Inside 2-4).
      // `_isInside(3)` -> 3 > 2 && 3 < 4 -> True.
      // WAS_BEFORE check: prev=3. start=2. 3 <= 2 -> False.
      // Snaps to START (2).
      // BUG REPRODUCED?
      // If EOL is 3. And hidden range extends past EOL (e.g. covers \n).
      // Then cursor at 3 is considered "Inside".
      // And if "wasBefore" check fails (e.g. static cursor), it biases Backward?
      // 3 <= 2 is False.
      // So it snaps to 2 (lineEnd - 1).

      expect(
        projector2
            .projectSelection(sel(3), previousSelection: sel(3))
            .baseOffset,
        2,
      );
    });

    test('Code fence tap snaps forward', () {
      // Hidden code fence marker of length 3; caret taps inside it should
      // snap to the end so Enter lands after the fence.
      final hidden = [const TextRange(start: 6, end: 9)];
      final model = DecorationModel.empty().copyWith(hiddenRanges: hidden);
      final projector = Projector(model);

      final projected = projector.projectSelection(
        sel(7),
        previousSelection: sel(7),
      );

      expect(projected.baseOffset, 9);
    });
  });
}

TextSelection sel(int offset) => TextSelection.collapsed(offset: offset);
