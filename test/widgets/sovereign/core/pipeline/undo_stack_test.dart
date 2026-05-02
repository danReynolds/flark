import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/pipeline/undo_stack.dart';
import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

void main() {
  group('UndoStack', () {
    test('ignores selection-only operations', () {
      final stack = UndoStack();

      stack.push(_selectionOp(id: 1));

      expect(stack.canUndo, isFalse);
      expect(stack.canRedo, isFalse);
    });

    test('pops grouped undo operations in reverse edit order', () {
      final stack = UndoStack()
        ..push(_textOp(id: 1, beforeText: '', afterText: 'a', group: 1))
        ..push(_textOp(id: 2, beforeText: 'a', afterText: 'ab', group: 1))
        ..push(_textOp(id: 3, beforeText: 'ab', afterText: 'abc', group: 2));

      expect(stack.popUndo().map((op) => op.id), <int>[3]);
      expect(stack.popUndo().map((op) => op.id), <int>[2, 1]);
      expect(stack.canUndo, isFalse);
    });

    test('redo returns grouped operations in file order', () {
      final stack = UndoStack()
        ..push(_textOp(id: 1, beforeText: '', afterText: 'a', group: 1))
        ..push(_textOp(id: 2, beforeText: 'a', afterText: 'ab', group: 1));

      expect(stack.popUndo().map((op) => op.id), <int>[2, 1]);

      expect(stack.canRedo, isTrue);
      expect(stack.popRedo().map((op) => op.id), <int>[1, 2]);
      expect(stack.canRedo, isFalse);
      expect(stack.canUndo, isTrue);
    });

    test('new text operation clears redo history', () {
      final stack = UndoStack()
        ..push(_textOp(id: 1, beforeText: '', afterText: 'a', group: 1));
      stack.popUndo();

      stack.push(_textOp(id: 2, beforeText: '', afterText: 'b', group: 2));

      expect(stack.canRedo, isFalse);
      expect(stack.popUndo().map((op) => op.id), <int>[2]);
    });
  });
}

EditOp _selectionOp({required int id}) {
  const value = TextEditingValue(text: 'abc');
  return EditOp(
    id: id,
    kind: EditOpKind.selection,
    before: value,
    after: value.copyWith(selection: const TextSelection.collapsed(offset: 1)),
    undoGroupId: id,
  );
}

EditOp _textOp({
  required int id,
  required String beforeText,
  required String afterText,
  required int group,
}) {
  final insertedText = afterText.length >= beforeText.length
      ? afterText.substring(beforeText.length)
      : '';
  return EditOp(
    id: id,
    kind: EditOpKind.text,
    before: TextEditingValue(text: beforeText),
    after: TextEditingValue(text: afterText),
    replacedRange: TextRange.collapsed(beforeText.length),
    insertedText: insertedText,
    undoGroupId: group,
  );
}
