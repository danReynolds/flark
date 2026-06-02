import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/projection/projection.dart';

void main() {
  group('FlarkProjectedTextEditAdapter', () {
    const adapter = FlarkProjectedTextEditAdapter();

    test('rejects stale display edits', () {
      final projection = _boldProjection();

      expect(
        adapter.transactionFromDisplayEdit(
          currentMarkdown: '**bold**',
          projection: projection,
          oldDisplayText: 'stale',
          newDisplayText: 'bold!',
        ),
        isNull,
      );
    });

    test('replaces visible text while preserving hidden markers', () {
      final projection = _boldProjection();
      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: '**bold**',
        projection: projection,
        oldDisplayText: 'bold',
        newDisplayText: 'text',
        sourceSelectionBefore: const FlarkSelection(
          baseOffset: 2,
          extentOffset: 6,
        ),
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        const FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(2, 6),
          replacementText: 'text',
        ),
      );
      expect(
        transaction
            .applyToDocument(FlarkDocument.fromMarkdown('**bold**'))
            .markdown,
        '**text**',
      );
      expect(transaction.selectionAfter, const FlarkSelection.collapsed(6));
    });

    test('uses exact source selection to insert inside a styled span', () {
      final projection = _boldProjection();
      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: '**bold**',
        projection: projection,
        oldDisplayText: 'bold',
        newDisplayText: 'bold!',
        sourceSelectionBefore: const FlarkSelection.collapsed(6),
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        FlarkSourceOperation.insert(6, '!'),
      );
      expect(
        transaction
            .applyToDocument(FlarkDocument.fromMarkdown('**bold**'))
            .markdown,
        '**bold!**',
      );
      expect(transaction.selectionAfter, const FlarkSelection.collapsed(7));
    });

    test('uses exact source selection to insert after a styled span', () {
      final projection = _boldProjection();
      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: '**bold**',
        projection: projection,
        oldDisplayText: 'bold',
        newDisplayText: 'bold!',
        sourceSelectionBefore: const FlarkSelection.collapsed(8),
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        FlarkSourceOperation.insert(8, '!'),
      );
      expect(
        transaction
            .applyToDocument(FlarkDocument.fromMarkdown('**bold**'))
            .markdown,
        '**bold**!',
      );
      expect(transaction.selectionAfter, const FlarkSelection.collapsed(9));
    });

    test('replaces a visible entity through its source replacement range', () {
      final projection = FlarkProjection(
        textLength: 'A &amp; B'.length,
        replacementRanges: const [
          FlarkReplacementRange(
            range: FlarkSourceRange(2, 7),
            kind: FlarkReplacementRangeKind.htmlEntity,
            replacementText: '&',
          ),
        ],
      );

      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: 'A &amp; B',
        projection: projection,
        oldDisplayText: 'A & B',
        newDisplayText: 'A X B',
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        const FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(2, 7),
          replacementText: 'X',
        ),
      );
      expect(
        transaction
            .applyToDocument(FlarkDocument.fromMarkdown('A &amp; B'))
            .markdown,
        'A X B',
      );
      expect(transaction.selectionAfter, const FlarkSelection.collapsed(3));
    });

    test('falls back to downstream insertion affinity at opening markers', () {
      final projection = _boldProjection();
      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: '**bold**',
        projection: projection,
        oldDisplayText: 'bold',
        newDisplayText: '!bold',
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        FlarkSourceOperation.insert(2, '!'),
      );
      expect(
        transaction
            .applyToDocument(FlarkDocument.fromMarkdown('**bold**'))
            .markdown,
        '**!bold**',
      );
    });

    test('supports upstream fallback insertion affinity when requested', () {
      final projection = _boldProjection();
      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: '**bold**',
        projection: projection,
        oldDisplayText: 'bold',
        newDisplayText: 'bold!',
        fallbackInsertionAffinity: FlarkMapAffinity.upstream,
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        FlarkSourceOperation.insert(6, '!'),
      );
      expect(
        transaction
            .applyToDocument(FlarkDocument.fromMarkdown('**bold**'))
            .markdown,
        '**bold!**',
      );
    });
  });
}

FlarkProjection _boldProjection() {
  return FlarkProjection(
    textLength: 8,
    hiddenRanges: const [
      FlarkHiddenRange(
        range: FlarkSourceRange(0, 2),
        kind: FlarkHiddenRangeKind.inlineMarker,
      ),
      FlarkHiddenRange(
        range: FlarkSourceRange(6, 8),
        kind: FlarkHiddenRangeKind.inlineMarker,
      ),
    ],
  );
}
