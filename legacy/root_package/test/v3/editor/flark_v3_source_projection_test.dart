import 'package:flark/flark_adapter.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3SourceProjection', () {
    test('requires exhaustive pieces and maps collapsed hidden prefixes', () {
      final projection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 10,
        sourceText: '    one\n    two',
        pieces: const [
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 10,
            sourceEndUtf16: 14,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 14,
            sourceEndUtf16: 18,
          ),
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 18,
            sourceEndUtf16: 22,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 22,
            sourceEndUtf16: 25,
          ),
        ],
      );

      expect(projection.displayText, 'one\ntwo');
      expect(projection.sourceToDisplayOffset(12), 0);
      expect(projection.sourceToDisplayOffset(20), 4);
      expect(
        projection.displayToSourceOffset(
          0,
          affinity: FlarkV3SourceProjectionAffinity.upstream,
        ),
        10,
      );
      expect(
        projection.displayToSourceOffset(
          0,
          affinity: FlarkV3SourceProjectionAffinity.downstream,
        ),
        14,
      );
      expect(
        projection.displayToSourceOffset(
          4,
          affinity: FlarkV3SourceProjectionAffinity.upstream,
        ),
        18,
      );
      expect(
        projection.displayToSourceOffset(
          4,
          affinity: FlarkV3SourceProjectionAffinity.downstream,
        ),
        22,
      );

      expect(
        () => FlarkV3SourceProjection.fromSource(
          sourceStartUtf16: 10,
          sourceText: 'abc',
          pieces: const [
            FlarkV3SourceProjectionPiece.copy(
              sourceStartUtf16: 10,
              sourceEndUtf16: 11,
            ),
            FlarkV3SourceProjectionPiece.copy(
              sourceStartUtf16: 12,
              sourceEndUtf16: 13,
            ),
          ],
        ),
        throwsStateError,
      );
      expect(
        () => FlarkV3SourceProjection.fromSource(
          sourceStartUtf16: 0,
          sourceText: 'x',
          maximumDisplayUtf16: 1,
          pieces: const [
            FlarkV3SourceProjectionPiece.replace(
              sourceStartUtf16: 0,
              sourceEndUtf16: 1,
              displayText: 'xx',
            ),
          ],
        ),
        throwsRangeError,
      );
    });

    test(
      'replacement literals normalize display without losing source map',
      () {
        final projection = FlarkV3SourceProjection.fromSource(
          sourceStartUtf16: 0,
          sourceText: '\u0000\r\n',
          pieces: const [
            FlarkV3SourceProjectionPiece.replace(
              sourceStartUtf16: 0,
              sourceEndUtf16: 1,
              displayText: '\uFFFD',
            ),
            FlarkV3SourceProjectionPiece.replace(
              sourceStartUtf16: 1,
              sourceEndUtf16: 3,
              displayText: '\n',
            ),
          ],
        );

        expect(projection.displayText, '\uFFFD\n');
        expect(projection.sourceToDisplayOffset(0), 0);
        expect(projection.sourceToDisplayOffset(1), 1);
        expect(projection.sourceToDisplayOffset(2), 1);
        expect(projection.sourceToDisplayOffset(3), 2);
        expect(
          projection.displayToSourceOffset(
            1,
            affinity: FlarkV3SourceProjectionAffinity.downstream,
          ),
          1,
        );
        expect(
          projection.displayToSourceOffset(
            2,
            affinity: FlarkV3SourceProjectionAffinity.upstream,
          ),
          3,
        );
        expect(
          () => projection.replaceSourceRange(
            sourceStartUtf16: 2,
            sourceEndUtf16: 2,
            replacement: FlarkV3SourceProjectionReplacement.identity('x'),
          ),
          throwsStateError,
        );
      },
    );

    test(
      'replacement edits consume complete source tokens and preserve cooked text',
      () {
        const cooked = '\u2242\u0338';
        final projection = FlarkV3SourceProjection.fromSource(
          sourceStartUtf16: 10,
          sourceText: 'a&NotEqualTilde;b',
          pieces: const [
            FlarkV3SourceProjectionPiece.copy(
              sourceStartUtf16: 10,
              sourceEndUtf16: 11,
            ),
            FlarkV3SourceProjectionPiece.replace(
              sourceStartUtf16: 11,
              sourceEndUtf16: 26,
              displayText: cooked,
            ),
            FlarkV3SourceProjectionPiece.copy(
              sourceStartUtf16: 26,
              sourceEndUtf16: 27,
            ),
          ],
        );

        expect(projection.displayText, 'a${cooked}b');
        final replaceFirstScalar = projection.expandDisplayEditOverReplacements(
          displayStartUtf16: 1,
          displayEndUtf16: 2,
          replacement: 'x',
        );
        expect(
          (
            replaceFirstScalar.displayStartUtf16,
            replaceFirstScalar.displayEndUtf16,
          ),
          (1, 3),
        );
        expect(replaceFirstScalar.replacement, 'x\u0338');
        expect(
          projection.displayText.replaceRange(
            replaceFirstScalar.displayStartUtf16,
            replaceFirstScalar.displayEndUtf16,
            replaceFirstScalar.replacement,
          ),
          projection.displayText.replaceRange(1, 2, 'x'),
        );

        final insertBetweenScalars = projection
            .expandDisplayEditOverReplacements(
              displayStartUtf16: 2,
              displayEndUtf16: 2,
              replacement: 'x',
            );
        expect(
          (
            insertBetweenScalars.displayStartUtf16,
            insertBetweenScalars.displayEndUtf16,
          ),
          (1, 3),
        );
        expect(insertBetweenScalars.replacement, '\u2242x\u0338');

        for (final boundary in const [1, 3]) {
          final boundaryInsertion = projection
              .expandDisplayEditOverReplacements(
                displayStartUtf16: boundary,
                displayEndUtf16: boundary,
                replacement: 'x',
              );
          expect(
            (
              boundaryInsertion.displayStartUtf16,
              boundaryInsertion.displayEndUtf16,
              boundaryInsertion.replacement,
            ),
            (boundary, boundary, 'x'),
          );
        }
      },
    );

    test('replacement source carets never split a display surrogate pair', () {
      const source = '&#x1F600;';
      final projection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: source,
        pieces: [
          FlarkV3SourceProjectionPiece.replace(
            sourceStartUtf16: 0,
            sourceEndUtf16: source.length,
            displayText: '\u{1F600}',
          ),
        ],
      );

      expect(projection.displayText.length, 2);
      for (var sourceOffset = 0; sourceOffset < source.length; sourceOffset++) {
        expect(projection.sourceToDisplayOffset(sourceOffset), 0);
      }
      expect(projection.sourceToDisplayOffset(source.length), 2);
      expect(
        projection.displayToSourceOffset(
          1,
          affinity: FlarkV3SourceProjectionAffinity.downstream,
        ),
        source.length,
      );

      for (final edit in const [
        (start: 1, end: 1, replacement: 'x'),
        (start: 0, end: 1, replacement: ''),
      ]) {
        expect(
          () => projection.expandDisplayEditOverReplacements(
            displayStartUtf16: edit.start,
            displayEndUtf16: edit.end,
            replacement: edit.replacement,
          ),
          throwsStateError,
        );
      }
    });

    test('canonical replacement may differ from its displayed replacement', () {
      final projection = FlarkV3SourceProjection.fromSource(
        sourceStartUtf16: 0,
        sourceText: '    x',
        pieces: const [
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 0,
            sourceEndUtf16: 4,
          ),
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 4,
            sourceEndUtf16: 5,
          ),
        ],
      );
      final replacement = FlarkV3SourceProjectionReplacement.projected(
        sourceReplacement: '\n    ',
        pieces: const [
          FlarkV3SourceProjectionPiece.copy(
            sourceStartUtf16: 0,
            sourceEndUtf16: 1,
          ),
          FlarkV3SourceProjectionPiece.hide(
            sourceStartUtf16: 1,
            sourceEndUtf16: 5,
          ),
        ],
      );

      expect(replacement.sourceReplacement, '\n    ');
      expect(replacement.displayReplacement, '\n');
      final next = projection.replaceSourceRange(
        sourceStartUtf16: 5,
        sourceEndUtf16: 5,
        replacement: replacement,
      );
      expect(next.sourceText, '    x\n    ');
      expect(next.displayText, 'x\n');
      expect(
        next.displayToSourceOffset(
          2,
          affinity: FlarkV3SourceProjectionAffinity.upstream,
        ),
        6,
      );
      expect(
        next.displayToSourceOffset(
          2,
          affinity: FlarkV3SourceProjectionAffinity.downstream,
        ),
        10,
      );
    });
  });
}
