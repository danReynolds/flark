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
  const headingWord = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.asciiWordInsertion,
    sourceBytes: FlarkSourceRange(2, 9),
    sourceUtf16: FlarkSourceRange(2, 9),
  );
  const headingSpace = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.singleAsciiSpaceInsertion,
    sourceBytes: FlarkSourceRange(2, 9),
    sourceUtf16: FlarkSourceRange(2, 9),
  );
  const headingTerminalSpace = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.singleAsciiSpaceInsertion,
    sourceBytes: FlarkSourceRange(9, 9),
    sourceUtf16: FlarkSourceRange(9, 9),
  );
  const strongAsterisk = FlarkLiteralSafeEnvelope(
    editClass: FlarkLiteralEditClass.singleAsciiAsteriskInsertion,
    sourceBytes: FlarkSourceRange(4, 8),
    sourceUtf16: FlarkSourceRange(4, 8),
  );
  const plainHeadingCell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.anyNoCrLfSplice,
    affectedBytes: FlarkSourceRange(2, 12),
    affectedUtf16: FlarkSourceRange(2, 12),
    triggerBytes: FlarkSourceRange(2, 12),
    triggerUtf16: FlarkSourceRange(2, 12),
    retainBlockShell: true,
    retainOutsideClosure: false,
    presentClosureExact: true,
    chainResultCell: true,
  );
  const strongOpeningSpaceCell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.insertSingleAsciiSpaceAtPoint,
    affectedBytes: FlarkSourceRange(2, 10),
    affectedUtf16: FlarkSourceRange(2, 10),
    triggerBytes: FlarkSourceRange(4, 4),
    triggerUtf16: FlarkSourceRange(4, 4),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: false,
  );
  const literalWordCell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
    affectedBytes: FlarkSourceRange(0, 17),
    affectedUtf16: FlarkSourceRange(0, 17),
    triggerBytes: FlarkSourceRange(0, 16),
    triggerUtf16: FlarkSourceRange(0, 16),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
  );
  const literalDeleteCell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.deleteOneAsciiUnitInLiteral,
    affectedBytes: FlarkSourceRange(0, 17),
    affectedUtf16: FlarkSourceRange(0, 17),
    triggerBytes: FlarkSourceRange(0, 16),
    triggerUtf16: FlarkSourceRange(0, 16),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: false,
  );
  const terminalLiteralAppendCell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd,
    affectedBytes: FlarkSourceRange(42, 55),
    affectedUtf16: FlarkSourceRange(42, 55),
    triggerBytes: FlarkSourceRange(55, 55),
    triggerUtf16: FlarkSourceRange(55, 55),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: true,
    terminalSpaceAvailable: true,
  );
  const exactBracketCell = FlarkProjectionEditCell(
    matcher: FlarkProjectionEditMatcher.insertExactScalarAtPoint,
    affectedBytes: FlarkSourceRange(7, 15),
    affectedUtf16: FlarkSourceRange(7, 15),
    triggerBytes: FlarkSourceRange(11, 11),
    triggerUtf16: FlarkSourceRange(11, 11),
    retainBlockShell: true,
    retainOutsideClosure: true,
    presentClosureExact: true,
    chainResultCell: false,
    exactScalar: 0x5b,
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
    expect(
      receipt.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.sourceBytes.start,
          envelope.sourceBytes.end,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [(2, 9, 2, 9)],
    );
  });

  test('same-range edit classes chain before the terminal position', () {
    final atStart = authorizeRowProjectionContinuity(
      revision: 11,
      envelopes: const [headingWord, headingSpace, headingTerminalSpace],
      authorizedContentUtf16: const FlarkSourceRange(2, 9),
      startUtf16: 2,
      endUtf16: 2,
      replacement: 'A',
    );
    expect(atStart, isNotNull);
    expect(
      atStart!.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.editClass,
          envelope.sourceBytes.start,
          envelope.sourceBytes.end,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [
        (FlarkLiteralEditClass.asciiWordInsertion, 2, 10, 2, 10),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 2, 10, 2, 10),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 10, 10, 10, 10),
      ],
      reason: 'same-range parser proofs form one transform bundle',
    );

    final internalSpace = atStart.continueWith(
      startUtf16: 3,
      endUtf16: 3,
      replacement: ' ',
    );
    expect(internalSpace, isNotNull);
    expect(internalSpace!.baseRevision, 12);
    expect(internalSpace.resultRevision, 13);
    expect(
      internalSpace.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.editClass,
          envelope.sourceBytes.start,
          envelope.sourceBytes.end,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [
        (FlarkLiteralEditClass.asciiWordInsertion, 2, 11, 2, 11),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 2, 11, 2, 11),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 11, 11, 11, 11),
      ],
    );

    final wordAfterSpace = internalSpace.continueWith(
      startUtf16: 4,
      endUtf16: 4,
      replacement: 'Z9',
    );
    expect(wordAfterSpace, isNotNull);
    expect(
      wordAfterSpace!.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.sourceBytes.start,
          envelope.sourceBytes.end,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [(2, 13, 2, 13), (2, 13, 2, 13), (13, 13, 13, 13)],
    );
  });

  test('a nonempty space envelope rejects its start boundary', () {
    expect(
      authorizeRowProjectionContinuity(
        revision: 14,
        envelopes: const [headingWord, headingSpace, headingTerminalSpace],
        authorizedContentUtf16: const FlarkSourceRange(2, 9),
        startUtf16: 2,
        endUtf16: 2,
        replacement: ' ',
      ),
      isNull,
      reason: 'leading space normalization is not an identity projection proof',
    );
  });

  test('internal spaces can repeat while the terminal remains one-shot', () {
    final firstInternal = authorizeRowProjectionContinuity(
      revision: 15,
      envelopes: const [headingWord, headingSpace, headingTerminalSpace],
      authorizedContentUtf16: const FlarkSourceRange(2, 9),
      startUtf16: 8,
      endUtf16: 8,
      replacement: ' ',
    );
    expect(firstInternal, isNotNull);

    final secondInternal = firstInternal!.continueWith(
      startUtf16: 9,
      endUtf16: 9,
      replacement: ' ',
    );
    expect(secondInternal, isNotNull);
    expect(
      secondInternal!.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.editClass,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [
        (FlarkLiteralEditClass.asciiWordInsertion, 2, 11),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 2, 11),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 11, 11),
      ],
    );

    final terminal = secondInternal.continueWith(
      startUtf16: 11,
      endUtf16: 11,
      replacement: ' ',
    );
    expect(terminal, isNotNull);
    expect(
      terminal!.literalSafeEnvelopes.map((envelope) => envelope.editClass),
      [
        FlarkLiteralEditClass.asciiWordInsertion,
        FlarkLiteralEditClass.singleAsciiSpaceInsertion,
      ],
    );
    expect(
      terminal.continueWith(startUtf16: 12, endUtf16: 12, replacement: ' '),
      isNull,
      reason: 'the consumed terminal proof cannot authorize a second space',
    );
  });

  test('a direct terminal space consumes only the zero-width proof', () {
    final terminal = authorizeRowProjectionContinuity(
      revision: 18,
      envelopes: const [headingWord, headingSpace, headingTerminalSpace],
      authorizedContentUtf16: const FlarkSourceRange(2, 9),
      startUtf16: 9,
      endUtf16: 9,
      replacement: ' ',
    );

    expect(terminal, isNotNull);
    expect(
      terminal!.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.editClass,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [
        (FlarkLiteralEditClass.asciiWordInsertion, 2, 9),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 2, 9),
      ],
      reason: 'the nonempty space proof is open at its terminal position',
    );
    expect(
      terminal.continueWith(startUtf16: 10, endUtf16: 10, replacement: ' '),
      isNull,
    );
  });

  test('word at envelope end moves the one-shot terminal proof', () {
    final wordAtEnd = authorizeRowProjectionContinuity(
      revision: 22,
      envelopes: const [headingWord, headingSpace, headingTerminalSpace],
      authorizedContentUtf16: const FlarkSourceRange(2, 9),
      startUtf16: 9,
      endUtf16: 9,
      replacement: 'x2',
    );

    expect(wordAtEnd, isNotNull);
    expect(
      wordAtEnd!.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.editClass,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [
        (FlarkLiteralEditClass.asciiWordInsertion, 2, 11),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 2, 11),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 11, 11),
      ],
    );

    final terminal = wordAtEnd.continueWith(
      startUtf16: 11,
      endUtf16: 11,
      replacement: ' ',
    );
    expect(terminal, isNotNull);
    expect(terminal!.literalSafeEnvelopes, hasLength(2));
    expect(
      terminal.continueWith(startUtf16: 12, endUtf16: 12, replacement: ' '),
      isNull,
    );
  });

  test('word edits shift a trailing boundary until one space consumes it', () {
    final firstWord = authorizeRowProjectionContinuity(
      revision: 20,
      envelopes: const [word, trailingSpace],
      authorizedContentUtf16: const FlarkSourceRange(0, 8),
      startUtf16: 6,
      endUtf16: 6,
      replacement: 'x',
    );
    expect(firstWord, isNotNull);
    expect(
      firstWord!.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.editClass,
          envelope.sourceBytes.start,
          envelope.sourceBytes.end,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [
        (FlarkLiteralEditClass.asciiWordInsertion, 2, 7, 2, 7),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 9, 9, 9, 9),
      ],
    );

    final secondWord = firstWord.continueWith(
      startUtf16: 7,
      endUtf16: 7,
      replacement: 'Y2',
    );
    expect(secondWord, isNotNull);
    expect(
      secondWord!.literalSafeEnvelopes.map(
        (envelope) => (
          envelope.editClass,
          envelope.sourceUtf16.start,
          envelope.sourceUtf16.end,
        ),
      ),
      [
        (FlarkLiteralEditClass.asciiWordInsertion, 2, 9),
        (FlarkLiteralEditClass.singleAsciiSpaceInsertion, 11, 11),
      ],
    );

    final boundary = secondWord.continueWith(
      startUtf16: 11,
      endUtf16: 11,
      replacement: ' ',
    );
    expect(boundary, isNotNull);
    expect(
      boundary!.literalSafeEnvelopes.map((envelope) => envelope.editClass),
      [FlarkLiteralEditClass.asciiWordInsertion],
      reason: 'the matched zero-width boundary is consumed',
    );
    expect(
      boundary.continueWith(startUtf16: 12, endUtf16: 12, replacement: ' '),
      isNull,
      reason: 'a second trailing space requires fresh parser authority',
    );
  });

  test('foreign-class containing envelopes are not carried forward', () {
    const innerWord = FlarkLiteralSafeEnvelope(
      editClass: FlarkLiteralEditClass.asciiWordInsertion,
      sourceBytes: FlarkSourceRange(4, 6),
      sourceUtf16: FlarkSourceRange(4, 6),
    );
    const widerSpace = FlarkLiteralSafeEnvelope(
      editClass: FlarkLiteralEditClass.singleAsciiSpaceInsertion,
      sourceBytes: FlarkSourceRange(2, 8),
      sourceUtf16: FlarkSourceRange(2, 8),
    );

    final receipt = authorizeRowProjectionContinuity(
      revision: 30,
      envelopes: const [innerWord, widerSpace],
      authorizedContentUtf16: const FlarkSourceRange(0, 10),
      startUtf16: 5,
      endUtf16: 5,
      replacement: 'x',
    );

    expect(receipt, isNotNull);
    expect(
      receipt!.literalSafeEnvelopes.map((envelope) => envelope.editClass),
      [FlarkLiteralEditClass.asciiWordInsertion],
      reason:
          'a class-specific proof cannot survive a foreign insertion merely '
          'because its range contains that insertion',
    );
    expect(
      receipt.continueWith(startUtf16: 7, endUtf16: 7, replacement: ' '),
      isNull,
    );
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
    expect(receipt.literalSafeEnvelopes, isEmpty);
    expect(
      receipt.continueWith(startUtf16: 9, endUtf16: 9, replacement: ' '),
      isNull,
    );

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

  test('complete ATX edit cell chains arbitrary non-newline splices', () {
    final first = authorizeProjectionEditCell(
      revision: 20,
      cells: const [plainHeadingCell],
      authorizedContentUtf16: const FlarkSourceRange(2, 12),
      startUtf16: 6,
      endUtf16: 8,
      replacement: '😀x',
    );
    expect(first, isNotNull);
    expect(
      (first!.baseAffectedUtf16.start, first.baseAffectedUtf16.end),
      (2, 12),
    );
    expect((first.affectedUtf16.start, first.affectedUtf16.end), (2, 13));
    expect(first.resultRevision, 21);

    final second = first.continueWith(
      startUtf16: 3,
      endUtf16: 4,
      replacement: '',
    );
    expect(second, isNotNull);
    expect(second!.baseRevision, 21);
    expect(second.resultRevision, 22);
    expect(
      (second.baseAffectedUtf16.start, second.baseAffectedUtf16.end),
      (2, 13),
    );
    expect((second.affectedUtf16.start, second.affectedUtf16.end), (2, 12));
  });

  test('complete edit cell rejects noops, line breaks, and crossings', () {
    for (final edit in [
      (start: 4, end: 4, replacement: ''),
      (start: 4, end: 4, replacement: '\n'),
      (start: 4, end: 4, replacement: '\r'),
      (start: 1, end: 1, replacement: 'x'),
      (start: 11, end: 13, replacement: 'x'),
    ]) {
      expect(
        authorizeProjectionEditCell(
          revision: 20,
          cells: const [plainHeadingCell],
          authorizedContentUtf16: const FlarkSourceRange(2, 12),
          startUtf16: edit.start,
          endUtf16: edit.end,
          replacement: edit.replacement,
        ),
        isNull,
      );
    }
  });

  test('local dependency cell admits one exact space and is consumed', () {
    final receipt = authorizeProjectionEditCell(
      revision: 30,
      cells: const [strongOpeningSpaceCell],
      authorizedContentUtf16: const FlarkSourceRange(2, 25),
      startUtf16: 4,
      endUtf16: 4,
      replacement: ' ',
    );
    expect(receipt, isNotNull);
    expect(
      (receipt!.baseAffectedUtf16.start, receipt.baseAffectedUtf16.end),
      (2, 10),
    );
    expect((receipt.affectedUtf16.start, receipt.affectedUtf16.end), (2, 11));
    expect(receipt.retainOutsideClosure, isTrue);
    expect(
      receipt.continueWith(startUtf16: 5, endUtf16: 5, replacement: ' '),
      isNull,
    );

    for (final edit in [
      (start: 3, replacement: ' '),
      (start: 4, replacement: 'x'),
      (start: 4, replacement: '  '),
    ]) {
      expect(
        authorizeProjectionEditCell(
          revision: 30,
          cells: const [strongOpeningSpaceCell],
          authorizedContentUtf16: const FlarkSourceRange(2, 25),
          startUtf16: edit.start,
          endUtf16: edit.start,
          replacement: edit.replacement,
        ),
        isNull,
      );
    }
  });

  test('parser-parameterized scalar cell is exact and one-shot', () {
    final receipt = authorizeProjectionEditCell(
      revision: 33,
      cells: const [exactBracketCell],
      authorizedContentUtf16: const FlarkSourceRange(0, 23),
      startUtf16: 11,
      endUtf16: 11,
      replacement: '[',
    );
    expect(receipt, isNotNull);
    expect((receipt!.affectedUtf16.start, receipt.affectedUtf16.end), (7, 16));
    expect(receipt.exactScalar, 0x5b);
    expect(
      receipt.continueWith(startUtf16: 12, endUtf16: 12, replacement: '['),
      isNull,
    );

    for (final edit in [
      (start: 10, end: 10, replacement: '['),
      (start: 11, end: 11, replacement: ']'),
      (start: 11, end: 11, replacement: '[['),
      (start: 11, end: 12, replacement: '['),
    ]) {
      expect(
        authorizeProjectionEditCell(
          revision: 33,
          cells: const [exactBracketCell],
          authorizedContentUtf16: const FlarkSourceRange(0, 23),
          startUtf16: edit.start,
          endUtf16: edit.end,
          replacement: edit.replacement,
        ),
        isNull,
        reason: '$edit',
      );
    }
  });

  test('Strong envelope admits one rendered asterisk and is consumed', () {
    final receipt = authorizeRowProjectionContinuity(
      revision: 35,
      envelopes: const [strongAsterisk],
      authorizedContentUtf16: const FlarkSourceRange(2, 25),
      startUtf16: 6,
      endUtf16: 6,
      replacement: '*',
    );
    expect(receipt, isNotNull);
    expect(receipt!.literalSafeEnvelopes, isEmpty);
    expect(
      receipt.continueWith(startUtf16: 7, endUtf16: 7, replacement: '*'),
      isNull,
    );
    for (final edit in [
      (start: 4, end: 4, replacement: '*'),
      (start: 8, end: 8, replacement: '*'),
      (start: 3, end: 3, replacement: '*'),
      (start: 6, end: 6, replacement: '**'),
      (start: 6, end: 6, replacement: '_'),
      (start: 6, end: 7, replacement: '*'),
    ]) {
      expect(
        authorizeRowProjectionContinuity(
          revision: 35,
          envelopes: const [strongAsterisk],
          authorizedContentUtf16: const FlarkSourceRange(2, 25),
          startUtf16: edit.start,
          endUtf16: edit.end,
          replacement: edit.replacement,
        ),
        isNull,
        reason: '$edit',
      );
    }
  });

  test(
    'literal word cell chains ASCII insertion and replacement beside outside facts',
    () {
      final first = authorizeProjectionEditCell(
        revision: 40,
        cells: const [literalWordCell],
        authorizedContentUtf16: const FlarkSourceRange(0, 55),
        startUtf16: 0,
        endUtf16: 0,
        replacement: 'k',
      );
      expect(first, isNotNull);
      expect((first!.affectedUtf16.start, first.affectedUtf16.end), (0, 18));
      expect((first.triggerUtf16.start, first.triggerUtf16.end), (0, 17));
      expect(first.retainOutsideClosure, isTrue);

      final second = first.continueWith(
        startUtf16: 1,
        endUtf16: 1,
        replacement: 'eep',
      );
      expect(second, isNotNull);
      expect((second!.affectedUtf16.start, second.affectedUtf16.end), (0, 21));
      expect((second.triggerUtf16.start, second.triggerUtf16.end), (0, 20));
      expect(second.resultRevision, 42);

      final spaced = second.continueWith(
        startUtf16: 4,
        endUtf16: 4,
        replacement: ' ',
      );
      expect(spaced, isNotNull);
      expect((spaced!.triggerUtf16.start, spaced.triggerUtf16.end), (0, 21));

      final replacement = spaced.continueWith(
        startUtf16: 5,
        endUtf16: 9,
        replacement: 'word',
      );
      expect(replacement, isNotNull);
      expect(
        (replacement!.triggerUtf16.start, replacement.triggerUtf16.end),
        (0, 21),
      );
    },
  );

  test(
    'literal cell admits a guarded multiword paste only in its interior',
    () {
      final receipt = authorizeProjectionEditCell(
        revision: 40,
        cells: const [literalWordCell],
        authorizedContentUtf16: const FlarkSourceRange(0, 55),
        startUtf16: 4,
        endUtf16: 12,
        replacement: 'briefly pending',
      );
      expect(receipt, isNotNull);
      expect(receipt!.resultRevision, 41);
      expect((receipt.affectedUtf16.start, receipt.affectedUtf16.end), (0, 24));

      for (final edit in [
        (start: 0, end: 4, replacement: 'briefly pending'),
        (start: 4, end: 16, replacement: 'briefly pending'),
        (start: 4, end: 12, replacement: 'briefly.pending'),
        (start: 4, end: 12, replacement: '   '),
      ]) {
        expect(
          authorizeProjectionEditCell(
            revision: 40,
            cells: const [literalWordCell],
            authorizedContentUtf16: const FlarkSourceRange(0, 55),
            startUtf16: edit.start,
            endUtf16: edit.end,
            replacement: edit.replacement,
          ),
          isNull,
          reason: '$edit',
        );
      }
    },
  );

  test('literal word cell rejects syntax, deletion, and boundary edits', () {
    for (final edit in [
      (start: 0, end: 0, replacement: ' '),
      (start: 17, end: 17, replacement: ' '),
      (start: 0, end: 0, replacement: '*'),
      (start: 0, end: 0, replacement: 'é'),
      (start: 0, end: 1, replacement: ''),
      (start: 17, end: 17, replacement: 'x'),
    ]) {
      expect(
        authorizeProjectionEditCell(
          revision: 40,
          cells: const [literalWordCell],
          authorizedContentUtf16: const FlarkSourceRange(0, 55),
          startUtf16: edit.start,
          endUtf16: edit.end,
          replacement: edit.replacement,
        ),
        isNull,
        reason: '$edit',
      );
    }
  });

  test('literal space insertion stays strictly inside its trimmed trigger', () {
    const padded = FlarkProjectionEditCell(
      matcher: FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral,
      affectedBytes: FlarkSourceRange(0, 6),
      affectedUtf16: FlarkSourceRange(0, 6),
      triggerBytes: FlarkSourceRange(1, 5),
      triggerUtf16: FlarkSourceRange(1, 5),
      retainBlockShell: true,
      retainOutsideClosure: true,
      presentClosureExact: true,
      chainResultCell: true,
    );
    expect(
      authorizeProjectionEditCell(
        revision: 45,
        cells: const [padded],
        authorizedContentUtf16: const FlarkSourceRange(0, 6),
        startUtf16: 3,
        endUtf16: 3,
        replacement: ' ',
      ),
      isNotNull,
    );
    for (final boundary in [1, 5]) {
      expect(
        authorizeProjectionEditCell(
          revision: 45,
          cells: const [padded],
          authorizedContentUtf16: const FlarkSourceRange(0, 6),
          startUtf16: boundary,
          endUtf16: boundary,
          replacement: ' ',
        ),
        isNull,
        reason: 'space at trimmed trigger boundary $boundary',
      );
    }
  });

  test('literal one-unit deletion is admitted once and cannot chain', () {
    final receipt = authorizeProjectionEditCell(
      revision: 50,
      cells: const [literalDeleteCell],
      authorizedContentUtf16: const FlarkSourceRange(0, 55),
      startUtf16: 3,
      endUtf16: 4,
      replacement: '',
    );
    expect(receipt, isNotNull);
    expect((receipt!.affectedUtf16.start, receipt.affectedUtf16.end), (0, 16));
    expect(receipt.resultRevision, 51);
    expect(
      receipt.continueWith(startUtf16: 2, endUtf16: 3, replacement: ''),
      isNull,
    );
    for (final edit in [
      (start: 3, end: 3, replacement: ''),
      (start: 3, end: 5, replacement: ''),
      (start: 3, end: 4, replacement: 'x'),
    ]) {
      expect(
        authorizeProjectionEditCell(
          revision: 50,
          cells: const [literalDeleteCell],
          authorizedContentUtf16: const FlarkSourceRange(0, 55),
          startUtf16: edit.start,
          endUtf16: edit.end,
          replacement: edit.replacement,
        ),
        isNull,
        reason: '$edit',
      );
    }
  });

  test('terminal literal append chains words but never two spaces', () {
    final spaced = authorizeProjectionEditCell(
      revision: 60,
      cells: const [terminalLiteralAppendCell],
      authorizedContentUtf16: const FlarkSourceRange(0, 55),
      startUtf16: 55,
      endUtf16: 55,
      replacement: ' ',
    );
    expect(spaced, isNotNull);
    expect((spaced!.affectedUtf16.start, spaced.affectedUtf16.end), (42, 56));
    expect(spaced.terminalSpaceAvailable, isFalse);
    expect(
      spaced.continueWith(startUtf16: 56, endUtf16: 56, replacement: ' '),
      isNull,
      reason: 'a carried terminal proof must not create a hard line break',
    );

    final wordStart = spaced.continueWith(
      startUtf16: 56,
      endUtf16: 56,
      replacement: 'T',
    );
    expect(wordStart, isNotNull);
    expect(wordStart!.terminalSpaceAvailable, isTrue);
    final word = wordStart.continueWith(
      startUtf16: 57,
      endUtf16: 57,
      replacement: 'esting',
    );
    expect(word, isNotNull);
    expect((word!.affectedUtf16.start, word.affectedUtf16.end), (42, 63));
    final punctuation = word.continueWith(
      startUtf16: 63,
      endUtf16: 63,
      replacement: '.',
    );
    expect(punctuation, isNotNull);
    expect(punctuation!.terminalSpaceAvailable, isTrue);
    final nextSpace = punctuation.continueWith(
      startUtf16: 64,
      endUtf16: 64,
      replacement: ' ',
    );
    expect(nextSpace, isNotNull);
    expect(nextSpace!.terminalSpaceAvailable, isFalse);

    for (final edit in [
      (start: 54, end: 54, replacement: 'x'),
      (start: 55, end: 55, replacement: '*'),
      (start: 55, end: 55, replacement: '['),
      (start: 55, end: 55, replacement: '&'),
      (start: 55, end: 55, replacement: 'é'),
      (start: 54, end: 55, replacement: ''),
    ]) {
      expect(
        authorizeProjectionEditCell(
          revision: 60,
          cells: const [terminalLiteralAppendCell],
          authorizedContentUtf16: const FlarkSourceRange(0, 55),
          startUtf16: edit.start,
          endUtf16: edit.end,
          replacement: edit.replacement,
        ),
        isNull,
        reason: '$edit',
      );
    }
  });

  test('ambiguous and escaped projection cells fail closed', () {
    expect(
      authorizeProjectionEditCell(
        revision: 30,
        cells: const [strongOpeningSpaceCell, strongOpeningSpaceCell],
        authorizedContentUtf16: const FlarkSourceRange(2, 25),
        startUtf16: 4,
        endUtf16: 4,
        replacement: ' ',
      ),
      isNull,
    );
    expect(
      authorizeProjectionEditCell(
        revision: 30,
        cells: const [strongOpeningSpaceCell],
        authorizedContentUtf16: const FlarkSourceRange(5, 25),
        startUtf16: 4,
        endUtf16: 4,
        replacement: ' ',
      ),
      isNull,
    );
  });
}
