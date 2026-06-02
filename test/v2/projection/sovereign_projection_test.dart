import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';
import 'package:sovereign_editor/src/v2/projection/projection.dart';

void main() {
  group('SovereignProjection', () {
    test('maps source offsets to display offsets through hidden ranges', () {
      final projection = SovereignProjection(
        textLength: 9,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(0, 2),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
          SovereignHiddenRange(
            range: SovereignSourceRange(7, 9),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );

      expect(projection.displayLength, 5);
      expect(projection.sourceToDisplayOffset(0), 0);
      expect(projection.sourceToDisplayOffset(1), 0);
      expect(projection.sourceToDisplayOffset(2), 0);
      expect(projection.sourceToDisplayOffset(7), 5);
      expect(projection.sourceToDisplayOffset(9), 5);
    });

    test('maps display offsets back to source offsets after hidden ranges', () {
      final projection = SovereignProjection(
        textLength: 9,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(0, 2),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
          SovereignHiddenRange(
            range: SovereignSourceRange(7, 9),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );

      expect(projection.displayToSourceOffset(0), 2);
      expect(projection.displayToSourceOffset(1), 3);
      expect(projection.displayToSourceOffset(5), 9);
      expect(
        projection.displayToSourceOffset(
          5,
          affinity: SovereignMapAffinity.upstream,
        ),
        7,
      );
    });

    test('normalizes cursor offsets out of hidden ranges', () {
      final mask = SovereignCursorMask(
        textLength: 9,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(0, 2),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );

      expect(mask.allows(0), isTrue);
      expect(mask.allows(1), isFalse);
      expect(mask.allows(2), isTrue);
      expect(mask.normalize(1, affinity: SovereignMapAffinity.upstream), 0);
      expect(mask.normalize(1, affinity: SovereignMapAffinity.downstream), 2);
    });

    test('builds cursor masks from single-pass range iterables', () {
      final mask = SovereignCursorMask(
        textLength: 4,
        hiddenRanges: _SinglePassIterable(const [
          SovereignHiddenRange(
            range: SovereignSourceRange(1, 3),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ]),
      );

      expect(mask.allows(1), isTrue);
      expect(mask.allows(2), isFalse);
      expect(mask.normalize(2), 3);
    });

    test('rejects overlapping hidden ranges', () {
      expect(
        () => SovereignProjection(
          textLength: 10,
          hiddenRanges: const [
            SovereignHiddenRange(
              range: SovereignSourceRange(1, 4),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
            SovereignHiddenRange(
              range: SovereignSourceRange(3, 5),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
          ],
        ),
        throwsA(isA<StateError>()),
      );
    });

    test('builds hidden ranges from parser projection payloads', () {
      final parseResult = SovereignMarkdownParseResult.fromJson({
        'schemaVersion': SovereignMarkdownParseProtocol.currentSchemaVersion,
        'revision': 1,
        'sourceTextLength': 8,
        'blocks': const [],
        'inlineTokens': const [],
        'hiddenRanges': [
          {
            'type': 'inlineMarker',
            'sourceRange': {'start': 0, 'end': 2},
            'attributes': {'marker': '**'},
          },
          {
            'type': 'inlineMarker',
            'sourceRange': {'start': 6, 'end': 8},
            'attributes': {'marker': '**'},
          },
        ],
      });

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(projection.displayLength, 4);
      expect(
        projection.hiddenRanges.first.kind,
        SovereignHiddenRangeKind.inlineMarker,
      );
      expect(projection.sourceToDisplayOffset(2), 0);
      expect(projection.sourceToDisplayOffset(6), 4);
      expect(projection.displayToSourceOffset(0), 2);
      expect(projection.displayToSourceOffset(4), 8);
    });

    test('projects replacement ranges with stable source/display mapping', () {
      const source = 'A &amp; B &#x1F600; C';
      final ampStart = source.indexOf('&amp;');
      final ampEnd = ampStart + '&amp;'.length;
      final emojiStart = source.indexOf('&#x1F600;');
      final emojiEnd = emojiStart + '&#x1F600;'.length;
      final projection = SovereignProjection(
        textLength: source.length,
        replacementRanges: [
          SovereignReplacementRange(
            range: SovereignSourceRange(ampStart, ampEnd),
            kind: SovereignReplacementRangeKind.htmlEntity,
            replacementText: '&',
          ),
          SovereignReplacementRange(
            range: SovereignSourceRange(emojiStart, emojiEnd),
            kind: SovereignReplacementRangeKind.htmlEntity,
            replacementText: '😀',
          ),
        ],
      );

      expect(projection.projectText(source), 'A & B 😀 C');
      expect(projection.displayLength, 'A & B 😀 C'.length);
      expect(projection.sourceToDisplayOffset(ampStart), 2);
      expect(projection.sourceToDisplayOffset(ampEnd), 3);
      expect(projection.displayToSourceOffset(2), ampStart);
      expect(projection.displayToSourceOffset(3), ampEnd);
      expect(projection.sourceToDisplayOffset(emojiStart), 6);
      expect(projection.sourceToDisplayOffset(emojiEnd), 8);
      expect(
        projection.displayToSourceOffset(
          7,
          affinity: SovereignMapAffinity.upstream,
        ),
        emojiStart,
      );
      expect(
        projection.displayToSourceOffset(
          7,
          affinity: SovereignMapAffinity.downstream,
        ),
        emojiEnd,
      );
      expect(projection.cursorMask.allows(ampStart), isTrue);
      expect(projection.cursorMask.allows(ampStart + 1), isFalse);
      expect(
        projection.cursorMask.normalize(
          ampStart + 1,
          affinity: SovereignMapAffinity.upstream,
        ),
        ampStart,
      );
      expect(projection.cursorMask.normalize(ampStart + 1), ampEnd);
    });

    test('builds replacement ranges from parser projection payloads', () {
      final parseResult = SovereignMarkdownParseResult.fromJson({
        'schemaVersion': SovereignMarkdownParseProtocol.currentSchemaVersion,
        'revision': 1,
        'sourceTextLength': 7,
        'blocks': const [],
        'inlineTokens': const [],
        'replacementRanges': [
          {
            'type': 'htmlEntity',
            'sourceRange': {'start': 2, 'end': 7},
            'replacementText': '&',
          },
        ],
      });

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(
        projection.replacementRanges.single.kind,
        SovereignReplacementRangeKind.htmlEntity,
      );
      expect(projection.projectText('A &amp;'), 'A &');
    });

    test('rejects overlapping hidden and replacement ranges', () {
      expect(
        () => SovereignProjection(
          textLength: 10,
          hiddenRanges: const [
            SovereignHiddenRange(
              range: SovereignSourceRange(2, 4),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
          ],
          replacementRanges: const [
            SovereignReplacementRange(
              range: SovereignSourceRange(3, 8),
              kind: SovereignReplacementRangeKind.htmlEntity,
              replacementText: '&',
            ),
          ],
        ),
        throwsA(isA<StateError>()),
      );
    });

    test('keeps unknown parser hidden range kinds forward compatible', () {
      final parseResult = SovereignMarkdownParseResult.fromJson({
        'schemaVersion': 99,
        'revision': 1,
        'sourceTextLength': 2,
        'blocks': const [],
        'inlineTokens': const [],
        'hiddenRanges': [
          {
            'type': 'futureMarker',
            'sourceRange': {'start': 0, 'end': 1},
          },
        ],
      });

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(
        projection.hiddenRanges.single.kind,
        SovereignHiddenRangeKind.unknown,
      );
      expect(projection.displayLength, 1);
    });

    test('projects escaped delimiters from parser hidden ranges', () {
      const source = r'\*literal\* and **bold**';
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: source.length,
        blocks: const [],
        inlineTokens: const [],
        hiddenRanges: [
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.escapeMarker,
            type: 'escapeMarker',
            sourceRange: SovereignSourceRange(0, 1),
          ),
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.escapeMarker,
            type: 'escapeMarker',
            sourceRange: SovereignSourceRange(9, 10),
          ),
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
            type: 'inlineMarker',
            sourceRange: SovereignSourceRange(16, 18),
          ),
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
            type: 'inlineMarker',
            sourceRange: SovereignSourceRange(22, 24),
          ),
        ],
      );

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(projection.projectText(source), '*literal* and bold');
      expect(
        projection.hiddenRanges.first.kind,
        SovereignHiddenRangeKind.escapeMarker,
      );
      expect(projection.cursorMask.allows(0), isTrue);
      expect(projection.cursorMask.allows(9), isTrue);
      expect(projection.cursorMask.allows(17), isFalse);
    });

    test(
      'projects reference links by hiding inline markers and definitions',
      () {
        const source = '[label][id]\n\n[id]: https://example.com "Title"\n';
        final definitionStart = source.indexOf('[id]:');
        final parseResult = SovereignMarkdownParseResult(
          schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
          revision: 1,
          sourceTextLength: source.length,
          blocks: const [],
          inlineTokens: const [],
          hiddenRanges: [
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: SovereignSourceRange(0, 1),
            ),
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
              type: 'inlineMarker',
              sourceRange: SovereignSourceRange(6, 11),
            ),
            SovereignMarkdownHiddenRange(
              kind: SovereignMarkdownHiddenRangeKind.referenceDefinition,
              type: 'referenceDefinition',
              sourceRange: SovereignSourceRange(definitionStart, source.length),
            ),
          ],
        );

        final projection = SovereignProjection.fromParseResult(parseResult);

        expect(projection.projectText(source), 'label\n\n');
        expect(projection.displayToSourceOffset(0), 1);
        expect(projection.sourceToDisplayOffset(definitionStart), 7);
        expect(
          projection.hiddenRanges.last.kind,
          SovereignHiddenRangeKind.referenceDefinition,
        );
      },
    );

    test('rejects projecting text with the wrong source length', () {
      final projection = SovereignProjection(textLength: 4);

      expect(
        () => projection.projectText('too long'),
        throwsA(isA<ArgumentError>()),
      );
    });

    test('maps source selections to projected display selections', () {
      final projection = SovereignProjection(
        textLength: 9,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(0, 2),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
          SovereignHiddenRange(
            range: SovereignSourceRange(7, 9),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );

      expect(
        projection.sourceSelectionToDisplay(
          const SovereignSelection(baseOffset: 2, extentOffset: 7),
        ),
        const SovereignSelection(baseOffset: 0, extentOffset: 5),
      );
      expect(
        projection.sourceSelectionToDisplay(
          const SovereignSelection.collapsed(1),
          affinity: SovereignMapAffinity.upstream,
        ),
        const SovereignSelection.collapsed(0),
      );
      expect(
        projection.sourceSelectionToDisplay(
          const SovereignSelection.collapsed(1),
        ),
        const SovereignSelection.collapsed(0),
      );
    });

    test('maps display selections back to source selections', () {
      final projection = SovereignProjection(
        textLength: 9,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(0, 2),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
          SovereignHiddenRange(
            range: SovereignSourceRange(7, 9),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );

      expect(
        projection.displaySelectionToSource(
          const SovereignSelection(baseOffset: 0, extentOffset: 5),
        ),
        const SovereignSelection(baseOffset: 2, extentOffset: 9),
      );
      expect(
        projection.displaySelectionToSource(
          const SovereignSelection(baseOffset: 0, extentOffset: 5),
          affinity: SovereignMapAffinity.upstream,
        ),
        const SovereignSelection(baseOffset: 0, extentOffset: 7),
      );
    });

    test('normalizes offsets inside ambiguity zones', () {
      final projection = SovereignProjection(
        textLength: 10,
        ambiguityZones: const [
          SovereignProjectionAmbiguityZone(
            range: SovereignSourceRange(2, 5),
            kind: SovereignProjectionAmbiguityKind.delimiterRun,
            preferredAffinity: SovereignMapAffinity.upstream,
          ),
        ],
      );

      expect(projection.normalizeAmbiguousOffset(1), 1);
      expect(projection.normalizeAmbiguousOffset(3), 2);
      expect(projection.normalizeAmbiguousOffset(5), 5);
    });

    test('builds ambiguity zones from parser payloads', () {
      final parseResult = SovereignMarkdownParseResult.fromJson({
        'schemaVersion': SovereignMarkdownParseProtocol.currentSchemaVersion,
        'revision': 1,
        'sourceTextLength': 6,
        'blocks': const [],
        'inlineTokens': const [],
        'ambiguityZones': [
          {
            'type': 'linkReference',
            'sourceRange': {'start': 1, 'end': 5},
            'preferredAffinity': 'upstream',
          },
        ],
      });

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(
        projection.ambiguityZones.single.kind,
        SovereignProjectionAmbiguityKind.linkReference,
      );
      expect(
        projection.ambiguityZones.single.preferredAffinity,
        SovereignMapAffinity.upstream,
      );
      expect(projection.normalizeAmbiguousOffset(3), 1);
    });

    test(
      'predicts projection ranges through edits outside sensitive ranges',
      () {
        final projection = SovereignProjection(
          textLength: 10,
          hiddenRanges: const [
            SovereignHiddenRange(
              range: SovereignSourceRange(5, 7),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
          ],
          ambiguityZones: const [
            SovereignProjectionAmbiguityZone(
              range: SovereignSourceRange(8, 10),
              kind: SovereignProjectionAmbiguityKind.delimiterRun,
            ),
          ],
        );
        final transaction = SovereignTransaction.single(
          SovereignSourceOperation.insert(0, 'XX'),
        );

        final prediction = projection.predictAfter(
          transaction,
          textLengthAfter: 12,
        );

        expect(prediction.touchedProjectionSensitiveRange, isFalse);
        expect(prediction.invalidatedRange, const SovereignSourceRange(0, 0));
        expect(
          prediction.projection.hiddenRanges.single.range,
          const SovereignSourceRange(7, 9),
        );
        expect(
          prediction.projection.ambiguityZones.single.range,
          const SovereignSourceRange(10, 12),
        );
      },
    );

    test('marks predictions sensitive when edits touch hidden ranges', () {
      final projection = SovereignProjection(
        textLength: 10,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(5, 7),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );
      final transaction = SovereignTransaction.single(
        SovereignSourceOperation.insert(6, 'x'),
      );

      final prediction = projection.predictAfter(
        transaction,
        textLengthAfter: 11,
      );

      expect(prediction.touchedProjectionSensitiveRange, isTrue);
      expect(
        prediction.projection.hiddenRanges.single.range,
        const SovereignSourceRange(5, 8),
      );
    });

    test('predicts and invalidates replacement ranges through edits', () {
      final projection = SovereignProjection(
        textLength: 12,
        replacementRanges: const [
          SovereignReplacementRange(
            range: SovereignSourceRange(2, 7),
            kind: SovereignReplacementRangeKind.htmlEntity,
            replacementText: '&',
          ),
        ],
      );

      final shifted = projection.predictAfter(
        SovereignTransaction.single(SovereignSourceOperation.insert(0, 'X')),
        textLengthAfter: 13,
      );
      final touched = projection.predictAfter(
        SovereignTransaction.single(SovereignSourceOperation.insert(3, '!')),
        textLengthAfter: 13,
      );

      expect(shifted.touchedProjectionSensitiveRange, isFalse);
      expect(
        shifted.projection.replacementRanges.single.range,
        const SovereignSourceRange(3, 8),
      );
      expect(touched.touchedProjectionSensitiveRange, isTrue);
      expect(
        touched.projection.replacementRanges.single.range,
        const SovereignSourceRange(2, 8),
      );
    });

    test('drops hidden ranges replaced by a full-document transaction', () {
      final projection = SovereignProjection(
        textLength: 10,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(0, 2),
            kind: SovereignHiddenRangeKind.markdownMarker,
          ),
          SovereignHiddenRange(
            range: SovereignSourceRange(5, 7),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );
      final transaction = SovereignTransaction.single(
        const SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(0, 10),
          replacementText: 'replacement',
        ),
      );

      final prediction = projection.predictAfter(
        transaction,
        textLengthAfter: 11,
      );

      expect(prediction.touchedProjectionSensitiveRange, isTrue);
      expect(prediction.projection.hiddenRanges, isEmpty);
    });

    test('reconciles predicted and authoritative projections', () {
      final predicted = SovereignProjection(
        textLength: 10,
        hiddenRanges: const [
          SovereignHiddenRange(
            range: SovereignSourceRange(2, 4),
            kind: SovereignHiddenRangeKind.inlineMarker,
          ),
        ],
      );
      final stable = predicted.reconcileWith(
        SovereignProjection(
          textLength: 10,
          hiddenRanges: const [
            SovereignHiddenRange(
              range: SovereignSourceRange(2, 4),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
          ],
        ),
      );
      final changed = predicted.reconcileWith(
        SovereignProjection(
          textLength: 10,
          hiddenRanges: const [
            SovereignHiddenRange(
              range: SovereignSourceRange(2, 5),
              kind: SovereignHiddenRangeKind.inlineMarker,
            ),
          ],
        ),
      );

      expect(stable.isStable, isTrue);
      expect(changed.isStable, isFalse);
      expect(changed.hiddenRangesChanged, isTrue);
      expect(changed.displayLengthDelta, -1);
    });

    test('reconciles replacement range changes as unstable', () {
      final predicted = SovereignProjection(
        textLength: 7,
        replacementRanges: const [
          SovereignReplacementRange(
            range: SovereignSourceRange(2, 7),
            kind: SovereignReplacementRangeKind.htmlEntity,
            replacementText: '&',
          ),
        ],
      );

      final changed = predicted.reconcileWith(
        SovereignProjection(
          textLength: 7,
          replacementRanges: const [
            SovereignReplacementRange(
              range: SovereignSourceRange(2, 7),
              kind: SovereignReplacementRangeKind.htmlEntity,
              replacementText: '+',
            ),
          ],
        ),
      );

      expect(changed.isStable, isFalse);
      expect(changed.replacementRangesChanged, isTrue);
      expect(changed.displayLengthDelta, 0);
    });

    test('projects table syntax from parser hidden ranges', () {
      const source = '| A | B |\n| --- | ---: |\n| c | d |\n';
      final separatorStart = source.indexOf('| ---');
      final bodyStart = source.indexOf('| c');
      final hiddenRanges = [
        ..._pipeHiddenRanges(source.substring(0, separatorStart), 0),
        SovereignMarkdownHiddenRange(
          kind: SovereignMarkdownHiddenRangeKind.blockMarker,
          type: 'blockMarker',
          sourceRange: SovereignSourceRange(separatorStart, bodyStart),
        ),
        ..._pipeHiddenRanges(source.substring(bodyStart), bodyStart),
      ];
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: source.length,
        blocks: const [],
        inlineTokens: const [],
        hiddenRanges: hiddenRanges,
        ambiguityZones: [
          SovereignMarkdownAmbiguityZone(
            kind: SovereignMarkdownAmbiguityKind.tableBoundary,
            type: 'tableBoundary',
            sourceRange: SovereignSourceRange(separatorStart, bodyStart),
          ),
        ],
      );

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(projection.projectText(source), ' A  B \n c  d \n');
      expect(projection.sourceToDisplayOffset(bodyStart), 7);
      expect(
        projection.ambiguityZones.single.kind,
        SovereignProjectionAmbiguityKind.tableBoundary,
      );
    });

    test('projects image media syntax to its accessible label', () {
      const source = '![alt](image.png "Title")';
      final labelStart = source.indexOf('alt');
      final labelEnd = labelStart + 'alt'.length;
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: source.length,
        blocks: const [],
        inlineTokens: const [],
        hiddenRanges: [
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.inlineMarker,
            type: 'inlineMarker',
            sourceRange: SovereignSourceRange(0, labelStart),
          ),
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.linkDestination,
            type: 'linkDestination',
            sourceRange: SovereignSourceRange(labelEnd, source.length),
          ),
        ],
      );

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(projection.projectText(source), 'alt');
      expect(
        projection.hiddenRanges.last.kind,
        SovereignHiddenRangeKind.linkDestination,
      );
    });

    test('projects raw html tags while preserving literal text', () {
      const source = '<span>raw</span>';
      final textStart = source.indexOf('raw');
      final textEnd = textStart + 'raw'.length;
      final parseResult = SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: source.length,
        blocks: const [],
        inlineTokens: const [],
        hiddenRanges: [
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.rawHtml,
            type: 'rawHtml',
            sourceRange: SovereignSourceRange(0, textStart),
          ),
          SovereignMarkdownHiddenRange(
            kind: SovereignMarkdownHiddenRangeKind.rawHtml,
            type: 'rawHtml',
            sourceRange: SovereignSourceRange(textEnd, source.length),
          ),
        ],
        ambiguityZones: [
          SovereignMarkdownAmbiguityZone(
            kind: SovereignMarkdownAmbiguityKind.rawHtml,
            type: 'rawHtml',
            sourceRange: SovereignSourceRange(0, source.length),
          ),
        ],
      );

      final projection = SovereignProjection.fromParseResult(parseResult);

      expect(projection.projectText(source), 'raw');
      expect(
        projection.hiddenRanges.first.kind,
        SovereignHiddenRangeKind.rawHtml,
      );
      expect(
        projection.ambiguityZones.single.kind,
        SovereignProjectionAmbiguityKind.rawHtml,
      );
    });
  });
}

final class _SinglePassIterable<T> extends Iterable<T> {
  _SinglePassIterable(this._values);

  final Iterable<T> _values;
  bool _used = false;

  @override
  Iterator<T> get iterator {
    if (_used) {
      throw StateError('Single-pass iterable was iterated more than once.');
    }
    _used = true;
    return _values.iterator;
  }
}

List<SovereignMarkdownHiddenRange> _pipeHiddenRanges(
  String text,
  int sourceOffset,
) {
  return [
    for (final match in RegExp(r'\|').allMatches(text))
      SovereignMarkdownHiddenRange(
        kind: SovereignMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: SovereignSourceRange(
          sourceOffset + match.start,
          sourceOffset + match.end,
        ),
      ),
  ];
}
