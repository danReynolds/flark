import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/projection/projection.dart';

void main() {
  group('SovereignProjectedTextEditAdapter', () {
    const adapter = SovereignProjectedTextEditAdapter();

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
        sourceSelectionBefore: const SovereignSelection(
          baseOffset: 2,
          extentOffset: 6,
        ),
      );

      expect(transaction, isNotNull);
      expect(
        transaction!.operations.single,
        const SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(2, 6),
          replacementText: 'text',
        ),
      );
      expect(
        transaction
            .applyToDocument(SovereignDocument.fromMarkdown('**bold**'))
            .markdown,
        '**text**',
      );
      expect(transaction.selectionAfter, const SovereignSelection.collapsed(6));
    });

    test('uses exact source selection to insert inside a styled span', () {
      final projection = _boldProjection();
      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: '**bold**',
        projection: projection,
        oldDisplayText: 'bold',
        newDisplayText: 'bold!',
        sourceSelectionBefore: const SovereignSelection.collapsed(6),
      );

      expect(transaction, isNotNull);
      expect(transaction!.operations.single,
          SovereignSourceOperation.insert(6, '!'));
      expect(
        transaction
            .applyToDocument(SovereignDocument.fromMarkdown('**bold**'))
            .markdown,
        '**bold!**',
      );
      expect(transaction.selectionAfter, const SovereignSelection.collapsed(7));
    });

    test('uses exact source selection to insert after a styled span', () {
      final projection = _boldProjection();
      final transaction = adapter.transactionFromDisplayEdit(
        currentMarkdown: '**bold**',
        projection: projection,
        oldDisplayText: 'bold',
        newDisplayText: 'bold!',
        sourceSelectionBefore: const SovereignSelection.collapsed(8),
      );

      expect(transaction, isNotNull);
      expect(transaction!.operations.single,
          SovereignSourceOperation.insert(8, '!'));
      expect(
        transaction
            .applyToDocument(SovereignDocument.fromMarkdown('**bold**'))
            .markdown,
        '**bold**!',
      );
      expect(transaction.selectionAfter, const SovereignSelection.collapsed(9));
    });

    test('replaces a visible entity through its source replacement range', () {
      final projection = SovereignProjection(
        textLength: 'A &amp; B'.length,
        replacementRanges: const [
          SovereignReplacementRange(
            range: SovereignSourceRange(2, 7),
            kind: SovereignReplacementRangeKind.htmlEntity,
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
        const SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(2, 7),
          replacementText: 'X',
        ),
      );
      expect(
        transaction
            .applyToDocument(SovereignDocument.fromMarkdown('A &amp; B'))
            .markdown,
        'A X B',
      );
      expect(transaction.selectionAfter, const SovereignSelection.collapsed(3));
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
      expect(transaction!.operations.single,
          SovereignSourceOperation.insert(2, '!'));
      expect(
        transaction
            .applyToDocument(SovereignDocument.fromMarkdown('**bold**'))
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
        fallbackInsertionAffinity: SovereignMapAffinity.upstream,
      );

      expect(transaction, isNotNull);
      expect(transaction!.operations.single,
          SovereignSourceOperation.insert(6, '!'));
      expect(
        transaction
            .applyToDocument(SovereignDocument.fromMarkdown('**bold**'))
            .markdown,
        '**bold!**',
      );
    });
  });
}

SovereignProjection _boldProjection() {
  return SovereignProjection(
    textLength: 8,
    hiddenRanges: const [
      SovereignHiddenRange(
        range: SovereignSourceRange(0, 2),
        kind: SovereignHiddenRangeKind.inlineMarker,
      ),
      SovereignHiddenRange(
        range: SovereignSourceRange(6, 8),
        kind: SovereignHiddenRangeKind.inlineMarker,
      ),
    ],
  );
}
