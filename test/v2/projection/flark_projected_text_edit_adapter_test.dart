import 'package:test/test.dart';
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

    test('honors the platform caret past a shared suffix (autocorrect)', () {
      // "dont go" -> "don't go" keeps the trailing "t go"; the greedy diff
      // alone reads this as inserting "'" before the shared "t" and would put
      // the caret at 4 (mid-word). The platform's reported caret (5)
      // re-anchors the edit's right edge onto it.
      final projection = FlarkProjection(textLength: 'dont go'.length);
      final resolution = adapter.resolveDisplayEdit(
        currentMarkdown: 'dont go',
        projection: projection,
        oldDisplayText: 'dont go',
        newDisplayText: "don't go",
        sourceSelectionBefore: const FlarkSelection.collapsed(4),
        newDisplayCaret: 5,
      );

      expect(resolution, isNotNull);
      expect(
        resolution!.transaction
            .applyToDocument(FlarkDocument.fromMarkdown('dont go'))
            .markdown,
        "don't go",
      );
      expect(
        resolution.transaction.selectionAfter,
        const FlarkSelection.collapsed(5),
      );
    });

    test('without the platform caret the greedy diff caret lands mid-word', () {
      // The behavior the caret parameter corrects: the identical edit with no
      // platform caret recomputes 4, inside the corrected word.
      final projection = FlarkProjection(textLength: 'dont go'.length);
      final resolution = adapter.resolveDisplayEdit(
        currentMarkdown: 'dont go',
        projection: projection,
        oldDisplayText: 'dont go',
        newDisplayText: "don't go",
        sourceSelectionBefore: const FlarkSelection.collapsed(4),
      );

      expect(
        resolution!.transaction
            .applyToDocument(FlarkDocument.fromMarkdown('dont go'))
            .markdown,
        "don't go",
      );
      expect(
        resolution.transaction.selectionAfter,
        const FlarkSelection.collapsed(4),
      );
    });

    test('the platform caret re-anchors without changing the edit text', () {
      // A large retroactive drift: "i am " -> "I am " shares the whole " am "
      // tail, so the greedy caret would snap back to 1. Caret 5 wins, and the
      // resulting document is byte-identical either way.
      final projection = FlarkProjection(textLength: 'i am '.length);
      final resolution = adapter.resolveDisplayEdit(
        currentMarkdown: 'i am ',
        projection: projection,
        oldDisplayText: 'i am ',
        newDisplayText: 'I am ',
        sourceSelectionBefore: const FlarkSelection.collapsed(5),
        newDisplayCaret: 5,
      );

      expect(
        resolution!.transaction
            .applyToDocument(FlarkDocument.fromMarkdown('i am '))
            .markdown,
        'I am ',
      );
      expect(
        resolution.transaction.selectionAfter,
        const FlarkSelection.collapsed(5),
      );
    });

    test('a platform caret at the greedy end leaves the diff untouched', () {
      // Ordinary insertion whose caret already sits at the edit's end: no
      // suffix to absorb, so the operation and caret are the plain ones.
      final projection = FlarkProjection(textLength: 'ab'.length);
      final resolution = adapter.resolveDisplayEdit(
        currentMarkdown: 'ab',
        projection: projection,
        oldDisplayText: 'ab',
        newDisplayText: 'aXb',
        sourceSelectionBefore: const FlarkSelection.collapsed(1),
        newDisplayCaret: 2,
      );

      expect(
        resolution!.transaction.operations.single,
        FlarkSourceOperation.insert(1, 'X'),
      );
      expect(
        resolution.transaction.selectionAfter,
        const FlarkSelection.collapsed(2),
      );
    });

    test('a corrected caret at a styled run trailing edge lands inside it', () {
      // Autocorrect of a fully-styled word: `**dont**` (displays "dont"),
      // caret inside the bold run at its trailing edge (source 6). iOS reports
      // caret 5 in "don't". The correction must map that back INSIDE the run
      // (before the hidden closing `**`, source 7), not after it (source 9),
      // so the next character continues the bold instead of escaping it. The
      // markers carry the inline-run flags the real parse emits, which is what
      // the caret-aware display->source mapping keys on.
      final projection = FlarkProjection(
        textLength: 8,
        hiddenRanges: const [
          FlarkHiddenRange(
            range: FlarkSourceRange(0, 2),
            kind: FlarkHiddenRangeKind.inlineMarker,
            opensInlineRun: true,
          ),
          FlarkHiddenRange(
            range: FlarkSourceRange(6, 8),
            kind: FlarkHiddenRangeKind.inlineMarker,
            closesInlineRun: true,
          ),
        ],
      );
      final resolution = adapter.resolveDisplayEdit(
        currentMarkdown: '**dont**',
        projection: projection,
        oldDisplayText: 'dont',
        newDisplayText: "don't",
        sourceSelectionBefore: const FlarkSelection.collapsed(6),
        newDisplayCaret: 5,
      );

      expect(resolution, isNotNull);
      expect(
        resolution!.transaction
            .applyToDocument(FlarkDocument.fromMarkdown('**dont**'))
            .markdown,
        "**don't**",
      );
      expect(
        resolution.transaction.selectionAfter,
        const FlarkSelection.collapsed(7),
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
