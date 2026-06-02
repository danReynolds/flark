import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';

void main() {
  group('FlarkHistoryStack', () {
    test('undoes and redoes a recorded transaction', () {
      final initial = FlarkEditorState.fromMarkdown('abc');
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert(3, 'd'),
        metadata: const FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: 'input.type',
        ),
      );

      final edited = initial.applyTransaction(transaction);
      final history = const FlarkHistoryStack().record(
        transaction: transaction,
        documentBefore: initial.document,
      );

      final undone = history.undo(edited);
      expect(undone.state.markdown, 'abc');
      expect(undone.state.selection, initial.selection);
      expect(undone.history.canRedo, isTrue);

      final redone = undone.history.redo(undone.state);
      expect(redone.state.markdown, 'abcd');
      expect(redone.state.selection, edited.selection);
      expect(redone.history.canUndo, isTrue);
      expect(redone.history.canRedo, isFalse);
    });

    test('groups adjacent transactions with the same undo group id', () {
      var state = FlarkEditorState.fromMarkdown('');
      var history = const FlarkHistoryStack();

      final first = FlarkTransaction.single(
        FlarkSourceOperation.insert(0, 'a'),
        metadata: const FlarkTransactionMetadata(undoGroupId: 7),
      );
      final afterFirst = state.applyTransaction(first);
      history = history.record(
        transaction: first,
        documentBefore: state.document,
      );
      state = afterFirst;

      final second = FlarkTransaction.single(
        FlarkSourceOperation.insert(1, 'b'),
        metadata: const FlarkTransactionMetadata(undoGroupId: 7),
      );
      final afterSecond = state.applyTransaction(second);
      history = history.record(
        transaction: second,
        documentBefore: state.document,
      );

      expect(afterSecond.markdown, 'ab');
      expect(history.undoEntries, hasLength(1));

      final undone = history.undo(afterSecond);
      expect(undone.state.markdown, '');
      expect(undone.history.canRedo, isTrue);
    });

    test('recording a new transaction clears redo history', () {
      final initial = FlarkEditorState.fromMarkdown('a');
      final first = FlarkTransaction.single(
        FlarkSourceOperation.insert(1, 'b'),
      );
      final afterFirst = initial.applyTransaction(first);
      final history = const FlarkHistoryStack().record(
        transaction: first,
        documentBefore: initial.document,
      );
      final undone = history.undo(afterFirst);

      final replacement = FlarkTransaction.single(
        FlarkSourceOperation.insert(1, 'c'),
      );
      final afterReplacement = undone.state.applyTransaction(replacement);
      final nextHistory = undone.history.record(
        transaction: replacement,
        documentBefore: undone.state.document,
      );

      expect(afterReplacement.markdown, 'ac');
      expect(nextHistory.canRedo, isFalse);
      expect(nextHistory.canUndo, isTrue);
    });

    test('does not record transactions that opt out of history', () {
      final initial = FlarkEditorState.fromMarkdown('abc');
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert(3, '!'),
        metadata: const FlarkTransactionMetadata(addToHistory: false),
      );

      final history = const FlarkHistoryStack().record(
        transaction: transaction,
        documentBefore: initial.document,
      );

      expect(history.canUndo, isFalse);
      expect(history.canRedo, isFalse);
    });

    test('does not record source-neutral transactions', () {
      final initial = FlarkEditorState.fromMarkdown('abc');
      final transaction = FlarkTransaction.single(
        const FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(0, 3),
          replacementText: 'abc',
        ),
      );

      final history = const FlarkHistoryStack().record(
        transaction: transaction,
        documentBefore: initial.document,
      );

      expect(history.canUndo, isFalse);
      expect(history.canRedo, isFalse);
    });
  });
}
