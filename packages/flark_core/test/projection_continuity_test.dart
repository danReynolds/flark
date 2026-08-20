import 'package:flark_core/src/models.dart';
import 'package:flark_core/src/projection_continuity.dart';
import 'package:test/test.dart';

void main() {
  const word = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.asciiWordInsertion,
    sourceBytes: FlarkSourceRange(2, 6),
    sourceUtf16: FlarkSourceRange(2, 6),
  );
  const trailingSpace = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.singleAsciiSpaceInsertion,
    sourceBytes: FlarkSourceRange(8, 8),
    sourceUtf16: FlarkSourceRange(8, 8),
  );

  test('parser word envelope binds one exact contained insertion', () {
    final receipt = authorizeRowProjectionContinuity(
      revision: 4,
      envelopes: const [word],
      authorizedContentUtf16: const FlarkSourceRange(0, 8),
      startUtf16: 4,
      endUtf16: 4,
      replacement: 'er2',
    );

    expect(receipt, isNotNull);
    expect(receipt!.baseRevision, 4);
    expect(receipt.resultRevision, 5);
    expect(receipt.editStartUtf16, 4);
    expect(receipt.editEndUtf16, 4);
    expect(receipt.authorizedContentUtf16, isA<FlarkSourceRange>());
    expect(receipt.authorizedContentUtf16.start, 0);
    expect(receipt.authorizedContentUtf16.end, 11);
  });

  test('edit-class and range mismatches fail closed', () {
    for (final mismatch in [
      (start: 1, end: 1, replacement: 'x'),
      (start: 4, end: 5, replacement: 'x'),
      (start: 4, end: 4, replacement: ' '),
      (start: 4, end: 4, replacement: '*'),
    ]) {
      expect(
        authorizeRowProjectionContinuity(
          revision: 1,
          envelopes: const [word],
          authorizedContentUtf16: const FlarkSourceRange(0, 8),
          startUtf16: mismatch.start,
          endUtf16: mismatch.end,
          replacement: mismatch.replacement,
        ),
        isNull,
      );
    }
  });

  test('authorized content must contain the edit and every envelope', () {
    const escapedAfter = FlarkLiteralSafeEnvelope(
      editClass: FlarkLiteralEditClass.asciiWordInsertion,
      sourceBytes: FlarkSourceRange(9, 10),
      sourceUtf16: FlarkSourceRange(9, 10),
    );

    for (final authorized in [
      const FlarkSourceRange(0, 5),
      const FlarkSourceRange(3, 8),
      const FlarkSourceRange(6, 8),
    ]) {
      expect(
        authorizeRowProjectionContinuity(
          revision: 1,
          envelopes: const [word],
          authorizedContentUtf16: authorized,
          startUtf16: 4,
          endUtf16: 4,
          replacement: 'x',
        ),
        isNull,
      );
    }

    expect(
      authorizeRowProjectionContinuity(
        revision: 1,
        envelopes: const [word, escapedAfter],
        authorizedContentUtf16: const FlarkSourceRange(0, 8),
        startUtf16: 4,
        endUtf16: 4,
        replacement: 'x',
      ),
      isNull,
      reason: 'one malformed parser envelope invalidates the whole proof set',
    );
  });

  test('single-space boundary envelope authorizes only its exact position', () {
    final receipt = authorizeRowProjectionContinuity(
      revision: 7,
      envelopes: const [trailingSpace],
      authorizedContentUtf16: const FlarkSourceRange(0, 8),
      startUtf16: 8,
      endUtf16: 8,
      replacement: ' ',
    );
    expect(receipt, isNotNull);
    expect(receipt!.authorizedContentUtf16.end, 9);

    for (final mismatch in [
      (start: 7, replacement: ' '),
      (start: 8, replacement: '  '),
      (start: 8, replacement: 'x'),
    ]) {
      expect(
        authorizeRowProjectionContinuity(
          revision: 7,
          envelopes: const [trailingSpace],
          authorizedContentUtf16: const FlarkSourceRange(0, 8),
          startUtf16: mismatch.start,
          endUtf16: mismatch.start,
          replacement: mismatch.replacement,
        ),
        isNull,
      );
    }
  });
}
