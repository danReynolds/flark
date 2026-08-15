import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('FlarkV3BlockQuoteEditPolicy', () {
    test('Enter preserves marker-free display and inserts canonical quote', () {
      final lease = _quoteLease();

      expect(lease.displayText, 'alpha\nbeta');
      expect(lease.sourceToDisplayOffset(0), 0);
      expect(lease.sourceToDisplayOffset(2), 0);
      expect(lease.sourceToDisplayOffset(8), 6);
      expect(lease.sourceToDisplayOffset(10), 6);

      final edit = lease.applyDisplayEdit(
        displayStartUtf16: 5,
        displayEndUtf16: 5,
        replacement: '\n',
        nextDisplayValue: const TextEditingValue(
          text: 'alpha\n\nbeta',
          selection: TextSelection.collapsed(offset: 6),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 7),
        preferredSourceComposing: TextRange.empty,
      );

      expect(edit.sourceStartUtf16, 7);
      expect(edit.sourceEndUtf16, 7);
      expect(edit.sourceReplacement, '\n> ');
      expect(edit.displayReplacement, '\n');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 10));
      expect(edit.nextLease.displayText, 'alpha\n\nbeta');
      expect(edit.nextLease.sourceToDisplayOffset(10), 6);
      expect(
        edit.nextLease.displayToSourceOffset(
          6,
          affinity: FlarkV3InlineProjectionAffinity.upstream,
        ),
        8,
      );
      expect(
        edit.nextLease.displayToSourceOffset(
          6,
          affinity: FlarkV3InlineProjectionAffinity.downstream,
        ),
        10,
      );
    });

    test('deleting a display newline consumes the complete hidden prefix', () {
      final edit = _quoteLease().applyDisplayEdit(
        displayStartUtf16: 5,
        displayEndUtf16: 6,
        replacement: '',
        nextDisplayValue: const TextEditingValue(
          text: 'alphabeta',
          selection: TextSelection.collapsed(offset: 5),
        ),
        preferredSourceSelection: const TextSelection.collapsed(offset: 10),
        preferredSourceComposing: TextRange.empty,
      );

      expect(edit.sourceStartUtf16, 7);
      expect(edit.sourceEndUtf16, 10);
      expect(edit.sourceReplacement, isEmpty);
      expect(edit.nextLease.displayText, 'alphabeta');
      expect(edit.sourceSelection, const TextSelection.collapsed(offset: 7));
    });

    test('canonical continuation configuration remains bounded', () {
      expect(
        () => FlarkV3BlockQuoteEditPolicy(canonicalContinuationPrefix: ''),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3BlockQuoteEditPolicy(
          canonicalContinuationPrefix: List.filled(65, '>').join(),
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3BlockQuoteEditPolicy(canonicalLineEnding: '\n\r'),
        throwsArgumentError,
      );
    });
  });
}

FlarkV3ProjectedInputLease _quoteLease() {
  const source = '> alpha\n> beta';
  return FlarkV3ProjectedInputLease.fromSourceProjection(
    FlarkV3SourceProjection.fromSource(
      sourceStartUtf16: 0,
      sourceText: source,
      pieces: const [
        FlarkV3SourceProjectionPiece.hide(
          sourceStartUtf16: 0,
          sourceEndUtf16: 2,
        ),
        FlarkV3SourceProjectionPiece.copy(
          sourceStartUtf16: 2,
          sourceEndUtf16: 8,
        ),
        FlarkV3SourceProjectionPiece.hide(
          sourceStartUtf16: 8,
          sourceEndUtf16: 10,
        ),
        FlarkV3SourceProjectionPiece.copy(
          sourceStartUtf16: 10,
          sourceEndUtf16: 14,
        ),
      ],
      certifiedSourceVersion: FlarkV3SourceVersion(
        documentSession: FlarkV3DocumentSessionId(1, 2, 3, 4),
        revision: 9,
        metric: FlarkV3SourceMetric(bytes: source.length, utf16: source.length),
        contentHash: const FlarkV3ContentHash128(5, 6, 7, 8),
      ),
    ),
    editPolicy: FlarkV3BlockQuoteEditPolicy(),
  );
}
