import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/pipeline/undo_redo_coordinator.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/pipeline/undo_stack.dart';
import 'package:sovereign_editor/widgets/sovereign/models/edit_op.dart';

void main() {
  group('SovereignUndoRedoCoordinator', () {
    test('undo restores grouped operations to earliest before state', () {
      final host = _FakeUndoRedoHost(
        value: const TextEditingValue(text: 'abc'),
      );
      host.undoStack
        ..push(_textOp(id: 1, beforeText: '', afterText: 'a', group: 1))
        ..push(_textOp(id: 2, beforeText: 'a', afterText: 'ab', group: 1))
        ..push(_textOp(id: 3, beforeText: 'ab', afterText: 'abc', group: 1));

      SovereignUndoRedoCoordinator(host).undo();

      expect(host.restoredValue?.text, isEmpty);
      expect(host.undoStack.canRedo, isTrue);
    });

    test('redo restores grouped operations to latest after state', () {
      final host = _FakeUndoRedoHost(
        value: const TextEditingValue(text: 'ab'),
      );
      host.undoStack
        ..push(_textOp(id: 1, beforeText: '', afterText: 'a', group: 1))
        ..push(_textOp(id: 2, beforeText: 'a', afterText: 'ab', group: 1));
      SovereignUndoRedoCoordinator(host).undo();
      host.value = const TextEditingValue(text: '');

      SovereignUndoRedoCoordinator(host).redo();

      expect(host.restoredValue?.text, 'ab');
    });

    test('undo and redo are no-ops while composing', () {
      final host = _FakeUndoRedoHost(
        value: const TextEditingValue(
          text: 'a',
          composing: TextRange(start: 0, end: 1),
        ),
      );
      host.undoStack.push(_textOp(id: 1, beforeText: '', afterText: 'a'));

      final coordinator = SovereignUndoRedoCoordinator(host);
      coordinator.undo();
      coordinator.redo();

      expect(host.restoredValue, isNull);
    });
  });
}

class _FakeUndoRedoHost implements SovereignUndoRedoHost {
  _FakeUndoRedoHost({required this.value});

  @override
  TextEditingValue value;

  @override
  final UndoStack undoStack = UndoStack();

  TextEditingValue? restoredValue;

  @override
  void applyRestoration(TextEditingValue value) {
    restoredValue = value;
  }
}

EditOp _textOp({
  required int id,
  required String beforeText,
  required String afterText,
  int group = 1,
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
