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

    test('caps retained undo entries, dropping the oldest first', () {
      var state = FlarkEditorState.fromMarkdown('');
      var history = const FlarkHistoryStack(maxEntries: 3);

      for (var i = 0; i < 5; i++) {
        final transaction = FlarkTransaction.single(
          FlarkSourceOperation.insert(state.markdown.length, '$i'),
          selectionAfter: FlarkSelection.collapsed(state.markdown.length + 1),
        );
        history = history.record(
          transaction: transaction,
          documentBefore: state.document,
        );
        state = state.applyTransaction(transaction);
      }

      expect(state.markdown, '01234');
      expect(history.undoEntries, hasLength(3));
      expect(history.maxEntries, 3);

      // Only the newest three edits remain undoable.
      var result = history.undo(state);
      result = result.history.undo(result.state);
      result = result.history.undo(result.state);
      expect(result.state.markdown, '01');
      expect(result.history.canUndo, isFalse);
    });

    test('the cap survives undo/redo round trips', () {
      var state = FlarkEditorState.fromMarkdown('');
      var history = const FlarkHistoryStack(maxEntries: 2);
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert(0, 'a'),
        selectionAfter: const FlarkSelection.collapsed(1),
      );
      history = history.record(
        transaction: transaction,
        documentBefore: state.document,
      );
      state = state.applyTransaction(transaction);

      final undone = history.undo(state);
      expect(undone.history.maxEntries, 2);
      final redone = undone.history.redo(undone.state);
      expect(redone.history.maxEntries, 2);
    });
  });

  group('FlarkHistoryStack typing coalescing', () {
    FlarkTransaction typeAt(int offset, String char) {
      return FlarkTransaction.single(
        FlarkSourceOperation.insert(offset, char),
        metadata: const FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.input,
          userEvent: 'input.type',
        ),
      );
    }

    ({FlarkEditorState state, FlarkHistoryStack history}) record(
      List<FlarkTransaction> transactions,
    ) {
      var state = FlarkEditorState.fromMarkdown('');
      var history = const FlarkHistoryStack();
      for (final transaction in transactions) {
        final before = state.document;
        state = state.applyTransaction(transaction);
        history = history.record(
          transaction: transaction,
          documentBefore: before,
        );
      }
      return (state: state, history: history);
    }

    List<FlarkTransaction> typing(String text, {int from = 0}) {
      return [
        for (var i = 0; i < text.length; i++) typeAt(from + i, text[i]),
      ];
    }

    test('coalesces a typed word into a single undo entry', () {
      final result = record(typing('hello'));

      expect(result.state.markdown, 'hello');
      expect(result.history.undoEntries, hasLength(1));
      expect(result.history.undo(result.state).state.markdown, '');
    });

    test('breaks the undo group at word boundaries', () {
      final result = record(typing('ab cd'));

      expect(result.history.undoEntries, hasLength(2));
      final afterFirstUndo = result.history.undo(result.state);
      expect(afterFirstUndo.state.markdown, 'ab ');
      expect(
        afterFirstUndo.history.undo(afterFirstUndo.state).state.markdown,
        '',
      );
    });

    test('a non-contiguous insertion starts a new entry', () {
      final result = record([...typing('ab'), typeAt(0, 'X')]);

      expect(result.state.markdown, 'Xab');
      expect(result.history.undoEntries, hasLength(2));
    });

    test('a non-typing edit between letters breaks coalescing', () {
      final command = FlarkTransaction.single(
        FlarkSourceOperation.insert(1, '!'),
        metadata: const FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.command,
        ),
      );
      final result = record([typeAt(0, 'a'), command, typeAt(2, 'b')]);

      expect(result.state.markdown, 'a!b');
      expect(result.history.undoEntries, hasLength(3));
    });
  });
}
