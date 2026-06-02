import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('FlarkTextDeltaAdapter', () {
    const adapter = FlarkTextDeltaAdapter();

    test('maps insertion deltas to source transactions', () {
      final transaction = adapter.transactionFromDelta(
        const TextEditingDeltaInsertion(
          oldText: 'ab',
          textInserted: 'c',
          insertionOffset: 2,
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
        currentMarkdown: 'ab',
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        FlarkSourceOperation.insert(2, 'c'),
      );
      expect(transaction.selectionAfter, const FlarkSelection.collapsed(3));
      expect(transaction.metadata.intent, FlarkTransactionIntent.input);
      expect(transaction.metadata.addToHistory, isTrue);
    });

    test('maps deletion deltas to source transactions', () {
      final transaction = adapter.transactionFromDelta(
        const TextEditingDeltaDeletion(
          oldText: 'abcd',
          deletedRange: TextRange(start: 1, end: 3),
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
        currentMarkdown: 'abcd',
      );

      expect(transaction, isNotNull);
      expect(transaction!.operations.single, FlarkSourceOperation.delete(1, 3));
    });

    test('maps replacement deltas to source transactions', () {
      final transaction = adapter.transactionFromDelta(
        const TextEditingDeltaReplacement(
          oldText: 'abcd',
          replacementText: 'XY',
          replacedRange: TextRange(start: 1, end: 3),
          selection: TextSelection.collapsed(offset: 3),
          composing: TextRange.empty,
        ),
        currentMarkdown: 'abcd',
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        const FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(1, 3),
          replacementText: 'XY',
        ),
      );
    });

    test('maps non-text deltas to selection transactions outside history', () {
      final transaction = adapter.transactionFromDelta(
        const TextEditingDeltaNonTextUpdate(
          oldText: 'abcd',
          selection: TextSelection(baseOffset: 1, extentOffset: 3),
          composing: TextRange.empty,
        ),
        currentMarkdown: 'abcd',
      );

      expect(transaction, isNotNull);
      expect(transaction!.operations, isEmpty);
      expect(
        transaction.selectionAfter,
        const FlarkSelection(baseOffset: 1, extentOffset: 3),
      );
      expect(transaction.metadata.intent, FlarkTransactionIntent.selection);
      expect(transaction.metadata.addToHistory, isFalse);
    });

    test('rejects stale deltas and invalid source ranges', () {
      expect(
        adapter.transactionFromDelta(
          const TextEditingDeltaInsertion(
            oldText: 'old',
            textInserted: '!',
            insertionOffset: 3,
            selection: TextSelection.collapsed(offset: 4),
            composing: TextRange.empty,
          ),
          currentMarkdown: 'new',
        ),
        isNull,
      );
      expect(
        adapter.transactionFromDelta(
          const TextEditingDeltaDeletion(
            oldText: 'abc',
            deletedRange: TextRange(start: 1, end: 9),
            selection: TextSelection.collapsed(offset: 1),
            composing: TextRange.empty,
          ),
          currentMarkdown: 'abc',
        ),
        isNull,
      );
    });
  });
}
