import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/core/pipeline/undo_grouping_policy.dart';
import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

void main() {
  group('UndoGroupingPolicy', () {
    test('merges nearby character insertions in same group', () {
      final now = DateTime(2026, 1, 1, 12, 0, 0, 500);
      final previousOp = _textOp(
        id: 1,
        before: const TextEditingValue(
          text: '',
          selection: TextSelection.collapsed(offset: 0),
        ),
        after: const TextEditingValue(
          text: 'a',
          selection: TextSelection.collapsed(offset: 1),
        ),
        insertedText: 'a',
        replacedRange: const TextRange(start: 0, end: 0),
        undoGroupId: 3,
      );
      final rawOp = _textOp(
        id: 2,
        before: const TextEditingValue(
          text: 'a',
          selection: TextSelection.collapsed(offset: 1),
        ),
        after: const TextEditingValue(
          text: 'ab',
          selection: TextSelection.collapsed(offset: 2),
        ),
        insertedText: 'b',
        replacedRange: const TextRange(start: 1, end: 1),
      );
      final state = UndoGroupingState(
        currentUndoGroup: 3,
        commandTransactionUndoGroupId: null,
        lastOp: previousOp,
        lastOpTime: now.subtract(const Duration(milliseconds: 200)),
        commandTransactionDepth: 0,
        undoBoundaryDepth: 0,
        forceUndoBoundaryForNextTextOp: false,
      );

      final decision = UndoGroupingPolicy.assignUndoGroup(
        rawOp: rawOp,
        state: state,
        now: now,
      );

      expect(decision.op.undoGroupId, 3);
      expect(decision.state.currentUndoGroup, 3);
    });

    test('starts new group for newline insertion', () {
      final now = DateTime(2026, 1, 1, 12, 0, 0, 500);
      final previousOp = _textOp(
        id: 1,
        before: const TextEditingValue(
          text: 'a',
          selection: TextSelection.collapsed(offset: 1),
        ),
        after: const TextEditingValue(
          text: 'ab',
          selection: TextSelection.collapsed(offset: 2),
        ),
        insertedText: 'b',
        replacedRange: const TextRange(start: 1, end: 1),
        undoGroupId: 3,
      );
      final rawOp = _textOp(
        id: 2,
        before: const TextEditingValue(
          text: 'ab',
          selection: TextSelection.collapsed(offset: 2),
        ),
        after: const TextEditingValue(
          text: 'ab\n',
          selection: TextSelection.collapsed(offset: 3),
        ),
        insertedText: '\n',
        replacedRange: const TextRange(start: 2, end: 2),
      );
      final state = UndoGroupingState(
        currentUndoGroup: 3,
        commandTransactionUndoGroupId: null,
        lastOp: previousOp,
        lastOpTime: now.subtract(const Duration(milliseconds: 120)),
        commandTransactionDepth: 0,
        undoBoundaryDepth: 0,
        forceUndoBoundaryForNextTextOp: false,
      );

      final decision = UndoGroupingPolicy.assignUndoGroup(
        rawOp: rawOp,
        state: state,
        now: now,
      );

      expect(decision.op.undoGroupId, 4);
      expect(decision.state.currentUndoGroup, 4);
    });

    test('pins group inside command transaction', () {
      final now = DateTime(2026, 1, 1, 12, 0, 0, 500);
      final rawOp = _textOp(
        id: 2,
        before: const TextEditingValue(
          text: 'ab',
          selection: TextSelection.collapsed(offset: 2),
        ),
        after: const TextEditingValue(
          text: 'abc',
          selection: TextSelection.collapsed(offset: 3),
        ),
        insertedText: 'c',
        replacedRange: const TextRange(start: 2, end: 2),
      );
      final state = UndoGroupingState(
        currentUndoGroup: 9,
        commandTransactionUndoGroupId: null,
        lastOp: null,
        lastOpTime: null,
        commandTransactionDepth: 1,
        undoBoundaryDepth: 0,
        forceUndoBoundaryForNextTextOp: true,
      );

      final decision = UndoGroupingPolicy.assignUndoGroup(
        rawOp: rawOp,
        state: state,
        now: now,
      );

      expect(decision.op.undoGroupId, 10);
      expect(decision.state.currentUndoGroup, 10);
      expect(decision.state.commandTransactionUndoGroupId, 10);
      expect(decision.state.forceUndoBoundaryForNextTextOp, isFalse);
    });
  });
}

EditOp _textOp({
  required int id,
  required TextEditingValue before,
  required TextEditingValue after,
  required String insertedText,
  required TextRange replacedRange,
  int undoGroupId = 0,
}) {
  final beforeText = before.text;
  final replacedText = beforeText.substring(
    replacedRange.start,
    replacedRange.end,
  );
  return EditOp(
    id: id,
    kind: EditOpKind.text,
    before: before,
    after: after,
    replacedRange: replacedRange,
    insertedText: insertedText,
    replacedText: replacedText,
    undoGroupId: undoGroupId,
  );
}
