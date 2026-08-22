import 'models.dart';

/// The parser-authored pre-edit proof carried by one pending presentation.
///
/// The variants differ only in their declared presentation consequence:
/// literal envelopes retain a transformed certified projection, while edit
/// cells expose one exact affected island and retain only certified content
/// outside it. Frontends may advance this typed proof, but may not infer or
/// widen its Markdown meaning.
sealed class FlarkPendingDependencyAuthority {
  const FlarkPendingDependencyAuthority();

  int get resultRevision;
  FlarkSourceRange get affectedUtf16;
  bool get presentsExactIsland;

  FlarkPendingDependencyAuthority? continueWith({
    required int startUtf16,
    required int endUtf16,
    required String replacement,
  });
}

/// Result-revision authority for one parser-authored projection edit cell.
final class FlarkProjectionEditCellReceipt
    extends FlarkPendingDependencyAuthority {
  FlarkProjectionEditCellReceipt._({
    required this.baseRevision,
    required this.resultRevision,
    required this.baseAffectedUtf16,
    required this.affectedUtf16,
    required this.triggerUtf16,
    required this.matcher,
    required this.retainBlockShell,
    required this.retainOutsideClosure,
    required this.presentClosureExact,
    required this.chainResultCell,
    required this.terminalSpaceAvailable,
    required this.exactScalar,
    required this.editStartUtf16,
    required this.editEndUtf16,
    required this.replacement,
  });

  final int baseRevision;
  @override
  final int resultRevision;
  final FlarkSourceRange baseAffectedUtf16;
  @override
  final FlarkSourceRange affectedUtf16;
  final FlarkSourceRange triggerUtf16;
  final FlarkProjectionEditMatcher matcher;
  final bool retainBlockShell;
  final bool retainOutsideClosure;
  final bool presentClosureExact;
  final bool chainResultCell;
  final bool terminalSpaceAvailable;
  final int? exactScalar;
  final int editStartUtf16;
  final int editEndUtf16;
  final String replacement;

  /// Advances only a parser-declared chainable cell. One-shot local dependency
  /// cells deliberately return null until a fresh parser publication arrives.
  @override
  bool get presentsExactIsland => true;

  @override
  FlarkProjectionEditCellReceipt? continueWith({
    required int startUtf16,
    required int endUtf16,
    required String replacement,
  }) {
    if (!chainResultCell) return null;
    return _authorizeCurrentProjectionEditCell(
      revision: resultRevision,
      cell: _CurrentProjectionEditCell(
        matcher: matcher,
        affectedUtf16: affectedUtf16,
        triggerUtf16: triggerUtf16,
        retainBlockShell: retainBlockShell,
        retainOutsideClosure: retainOutsideClosure,
        presentClosureExact: presentClosureExact,
        chainResultCell: chainResultCell,
        terminalSpaceAvailable: terminalSpaceAvailable,
        exactScalar: exactScalar,
      ),
      startUtf16: startUtf16,
      endUtf16: endUtf16,
      replacement: replacement,
    );
  }
}

final class _CurrentProjectionEditCell {
  const _CurrentProjectionEditCell({
    required this.matcher,
    required this.affectedUtf16,
    required this.triggerUtf16,
    required this.retainBlockShell,
    required this.retainOutsideClosure,
    required this.presentClosureExact,
    required this.chainResultCell,
    required this.terminalSpaceAvailable,
    required this.exactScalar,
  });

  final FlarkProjectionEditMatcher matcher;
  final FlarkSourceRange affectedUtf16;
  final FlarkSourceRange triggerUtf16;
  final bool retainBlockShell;
  final bool retainOutsideClosure;
  final bool presentClosureExact;
  final bool chainResultCell;
  final bool terminalSpaceAvailable;
  final int? exactScalar;
}

/// Matches one exact source splice against parser-authored affected geometry.
FlarkProjectionEditCellReceipt? authorizeProjectionEditCell({
  required int revision,
  required List<FlarkProjectionEditCell> cells,
  required FlarkSourceRange authorizedContentUtf16,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  if (revision <= 0 ||
      authorizedContentUtf16.start > authorizedContentUtf16.end ||
      cells.any(
        (cell) =>
            cell.affectedUtf16.start > cell.affectedUtf16.end ||
            cell.triggerUtf16.start > cell.triggerUtf16.end ||
            cell.affectedUtf16.start < authorizedContentUtf16.start ||
            cell.affectedUtf16.end > authorizedContentUtf16.end ||
            cell.triggerUtf16.start < cell.affectedUtf16.start ||
            cell.triggerUtf16.end > cell.affectedUtf16.end,
      )) {
    return null;
  }
  final matches = cells
      .map(
        (cell) => _CurrentProjectionEditCell(
          matcher: cell.matcher,
          affectedUtf16: cell.affectedUtf16,
          triggerUtf16: cell.triggerUtf16,
          retainBlockShell: cell.retainBlockShell,
          retainOutsideClosure: cell.retainOutsideClosure,
          presentClosureExact: cell.presentClosureExact,
          chainResultCell: cell.chainResultCell,
          terminalSpaceAvailable:
              cell.matcher ==
                  FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd &&
              cell.terminalSpaceAvailable,
          exactScalar: cell.exactScalar,
        ),
      )
      .where(
        (cell) =>
            _projectionEditCellMatches(cell, startUtf16, endUtf16, replacement),
      )
      .toList(growable: false);
  if (matches.length != 1) return null;
  return _authorizeCurrentProjectionEditCell(
    revision: revision,
    cell: matches.single,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    replacement: replacement,
  );
}

FlarkProjectionEditCellReceipt? _authorizeCurrentProjectionEditCell({
  required int revision,
  required _CurrentProjectionEditCell cell,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  if (!_projectionEditCellMatches(cell, startUtf16, endUtf16, replacement)) {
    return null;
  }
  final delta = replacement.length - (endUtf16 - startUtf16);
  final resultAffected = FlarkSourceRange(
    cell.affectedUtf16.start,
    cell.affectedUtf16.end + delta,
  );
  final resultTrigger = switch (cell.matcher) {
    FlarkProjectionEditMatcher.anyNoCrLfSplice ||
    FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral => FlarkSourceRange(
      cell.triggerUtf16.start,
      cell.triggerUtf16.end + delta,
    ),
    FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd => FlarkSourceRange(
      resultAffected.end,
      resultAffected.end,
    ),
    _ => _transformInsertionRange(
      cell.triggerUtf16,
      startUtf16,
      replacement.length,
      growsWithInsertion: false,
    ),
  };
  return FlarkProjectionEditCellReceipt._(
    baseRevision: revision,
    resultRevision: revision + 1,
    baseAffectedUtf16: cell.affectedUtf16,
    affectedUtf16: resultAffected,
    triggerUtf16: resultTrigger,
    matcher: cell.matcher,
    retainBlockShell: cell.retainBlockShell,
    retainOutsideClosure: cell.retainOutsideClosure,
    presentClosureExact: cell.presentClosureExact,
    chainResultCell: cell.chainResultCell,
    terminalSpaceAvailable:
        cell.matcher ==
            FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd &&
        replacement != ' ',
    exactScalar: cell.exactScalar,
    editStartUtf16: startUtf16,
    editEndUtf16: endUtf16,
    replacement: replacement,
  );
}

bool _projectionEditCellMatches(
  _CurrentProjectionEditCell cell,
  int startUtf16,
  int endUtf16,
  String replacement,
) {
  if (!cell.retainBlockShell ||
      !cell.presentClosureExact ||
      startUtf16 > endUtf16 ||
      startUtf16 < cell.triggerUtf16.start ||
      endUtf16 > cell.triggerUtf16.end ||
      startUtf16 < cell.affectedUtf16.start ||
      endUtf16 > cell.affectedUtf16.end) {
    return false;
  }
  return switch (cell.matcher) {
    FlarkProjectionEditMatcher.anyNoCrLfSplice =>
      (startUtf16 != endUtf16 || replacement.isNotEmpty) &&
          !replacement.contains('\n') &&
          !replacement.contains('\r'),
    FlarkProjectionEditMatcher.asciiLiteralSpliceInLiteral =>
      cell.chainResultCell &&
          cell.retainOutsideClosure &&
          ((replacement.isNotEmpty &&
                  replacement.codeUnits.every(_isAsciiAlphanumeric)) ||
              (replacement == ' ' &&
                  startUtf16 == endUtf16 &&
                  startUtf16 > cell.triggerUtf16.start &&
                  startUtf16 < cell.triggerUtf16.end)),
    FlarkProjectionEditMatcher.deleteOneAsciiUnitInLiteral =>
      !cell.chainResultCell &&
          cell.retainOutsideClosure &&
          endUtf16 == startUtf16 + 1 &&
          replacement.isEmpty,
    FlarkProjectionEditMatcher.insertSingleAsciiSpaceAtPoint =>
      !cell.chainResultCell &&
          cell.retainOutsideClosure &&
          cell.triggerUtf16.length == 0 &&
          startUtf16 == endUtf16 &&
          startUtf16 == cell.triggerUtf16.start &&
          replacement == ' ',
    FlarkProjectionEditMatcher.appendAsciiLiteralAtLineEnd =>
      cell.chainResultCell &&
          cell.retainOutsideClosure &&
          cell.triggerUtf16.length == 0 &&
          startUtf16 == endUtf16 &&
          startUtf16 == cell.affectedUtf16.end &&
          startUtf16 == cell.triggerUtf16.start &&
          (replacement.isNotEmpty &&
                  replacement.codeUnits.every(
                    (unit) =>
                        _isAsciiAlphanumeric(unit) ||
                        _isSafeAsciiProsePunctuation(unit),
                  ) ||
              (replacement == ' ' && cell.terminalSpaceAvailable)),
    FlarkProjectionEditMatcher.insertExactScalarAtPoint =>
      !cell.chainResultCell &&
          cell.retainOutsideClosure &&
          cell.triggerUtf16.length == 0 &&
          startUtf16 == endUtf16 &&
          startUtf16 == cell.triggerUtf16.start &&
          cell.exactScalar != null &&
          replacement.runes.length == 1 &&
          replacement.runes.single == cell.exactScalar,
  };
}

bool _isAsciiAlphanumeric(int unit) =>
    (unit >= 0x30 && unit <= 0x39) ||
    (unit >= 0x41 && unit <= 0x5a) ||
    (unit >= 0x61 && unit <= 0x7a);

bool _isSafeAsciiProsePunctuation(int unit) => switch (unit) {
  0x22 || 0x27 || 0x2c || 0x2e || 0x3a || 0x3b || 0x3f => true,
  _ => false,
};

/// One parser-authored literal-safe envelope bound to an exact source
/// transaction and its result revision.
///
/// This receipt carries no Markdown policy. The parser already proved the
/// matching edit class safe at the published position; Core performs only
/// edit-class matching and range containment.
final class FlarkProjectionContinuityReceipt
    extends FlarkPendingDependencyAuthority {
  FlarkProjectionContinuityReceipt._({
    required this.baseRevision,
    required this.resultRevision,
    required this.authorizedContentUtf16,
    required this.editStartUtf16,
    required this.editEndUtf16,
    required this.replacement,
    required List<FlarkLiteralSafeEnvelope> literalSafeEnvelopes,
  }) : literalSafeEnvelopes = List.unmodifiable(literalSafeEnvelopes);

  final int baseRevision;
  @override
  final int resultRevision;
  final FlarkSourceRange authorizedContentUtf16;
  final int editStartUtf16;
  final int editEndUtf16;
  final String replacement;

  /// Parser-authored envelopes transformed into [resultRevision]
  /// coordinates. A successor can retain presentation only by matching this
  /// carried proof set; Core never reclassifies Markdown itself.
  final List<FlarkLiteralSafeEnvelope> literalSafeEnvelopes;

  /// Binds one successor insertion to the still-live parser proof set.
  ///
  /// Non-empty envelopes are closed over their declared insertion class and
  /// grow with a matching edit; non-empty parser proofs with identical byte
  /// and UTF-16 geometry grow as one bundle. A non-empty space envelope is
  /// open at both boundaries; only a parser-published zero-width proof can
  /// authorize either exact point, and the matched point is consumed. Thus a
  /// single trailing space cannot authorize a second edit without fresh
  /// parser certification.
  @override
  FlarkSourceRange get affectedUtf16 => authorizedContentUtf16;

  @override
  bool get presentsExactIsland => false;

  @override
  FlarkProjectionContinuityReceipt? continueWith({
    required int startUtf16,
    required int endUtf16,
    required String replacement,
  }) => authorizeRowProjectionContinuity(
    revision: resultRevision,
    envelopes: literalSafeEnvelopes,
    authorizedContentUtf16: authorizedContentUtf16,
    startUtf16: startUtf16,
    endUtf16: endUtf16,
    replacement: replacement,
  );
}

/// Binds one edit to exactly one of the parser-authored authority vocabularies.
///
/// Edit cells are the more specific dependency-island representation and take
/// precedence over legacy literal envelopes. Keeping that precedence here
/// prevents each frontend from maintaining a parallel policy branch while the
/// wire records remain backward-compatible.
FlarkPendingDependencyAuthority? bindPendingDependencyAuthority({
  required int revision,
  required List<FlarkProjectionEditCell> cells,
  required List<FlarkLiteralSafeEnvelope> envelopes,
  required FlarkSourceRange authorizedContentUtf16,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) =>
    authorizeProjectionEditCell(
      revision: revision,
      cells: cells,
      authorizedContentUtf16: authorizedContentUtf16,
      startUtf16: startUtf16,
      endUtf16: endUtf16,
      replacement: replacement,
    ) ??
    authorizeRowProjectionContinuity(
      revision: revision,
      envelopes: envelopes,
      authorizedContentUtf16: authorizedContentUtf16,
      startUtf16: startUtf16,
      endUtf16: endUtf16,
      replacement: replacement,
    );

/// Binds one exact edit to a parser-published literal-safe envelope.
FlarkProjectionContinuityReceipt? authorizeRowProjectionContinuity({
  required int revision,
  required List<FlarkLiteralSafeEnvelope> envelopes,
  required FlarkSourceRange authorizedContentUtf16,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  if (revision <= 0 ||
      authorizedContentUtf16.start > authorizedContentUtf16.end ||
      startUtf16 != endUtf16 ||
      envelopes.any(
        (envelope) =>
            envelope.sourceUtf16.start > envelope.sourceUtf16.end ||
            envelope.sourceUtf16.start < authorizedContentUtf16.start ||
            envelope.sourceUtf16.end > authorizedContentUtf16.end,
      )) {
    return null;
  }
  final matchingIndexes = <int>[];
  int? startByte;
  for (var index = 0; index < envelopes.length; index += 1) {
    final envelope = envelopes[index];
    if (envelope.sourceUtf16.start > startUtf16 ||
        startUtf16 > envelope.sourceUtf16.end ||
        !_matchesEnvelope(envelope, startUtf16, replacement)) {
      continue;
    }
    final candidateStartByte = _bytePositionForUtf16Insertion(
      envelope,
      startUtf16,
    );
    if (candidateStartByte == null ||
        (startByte != null && startByte != candidateStartByte)) {
      return null;
    }
    startByte = candidateStartByte;
    matchingIndexes.add(index);
  }
  if (matchingIndexes.isEmpty || startByte == null) return null;

  // Every admitted replacement class is ASCII-only, so its byte and UTF-16
  // deltas are identical. The edit's byte position comes from the matching
  // parser envelope rather than from host source inspection.
  final delta = replacement.length;
  final matchingSet = matchingIndexes.toSet();
  final oneShotAsterisk = matchingIndexes.any(
    (index) =>
        envelopes[index].editClass ==
        FlarkLiteralEditClass.singleAsciiAsteriskInsertion,
  );
  final transformedEnvelopes = <FlarkLiteralSafeEnvelope>[];
  for (var index = 0; index < envelopes.length; index += 1) {
    final envelope = envelopes[index];
    final matched = matchingSet.contains(index);
    if ((matched && envelope.sourceUtf16.length == 0) ||
        (oneShotAsterisk &&
            matchingIndexes.any(
              (matchingIndex) =>
                  _sameEnvelopeGeometry(envelope, envelopes[matchingIndex]),
            ))) {
      continue;
    }
    final sharesMatchedNonemptyGeometry =
        envelope.sourceUtf16.length > 0 &&
        matchingIndexes.any(
          (matchingIndex) =>
              _sameEnvelopeGeometry(envelope, envelopes[matchingIndex]),
        );
    final foreignInsertionCrossesEnvelope =
        !matched &&
        !sharesMatchedNonemptyGeometry &&
        ((envelope.sourceBytes.start < startByte &&
                startByte < envelope.sourceBytes.end) ||
            (envelope.sourceUtf16.start < startUtf16 &&
                startUtf16 < envelope.sourceUtf16.end));
    if (foreignInsertionCrossesEnvelope) {
      // Positional containment does not prove that a different edit class
      // preserves this envelope's vocabulary. Only an exact same-geometry
      // parser bundle is closed over the matched insertion.
      continue;
    }
    transformedEnvelopes.add(
      FlarkLiteralSafeEnvelope(
        editClass: envelope.editClass,
        sourceBytes: _transformInsertionRange(
          envelope.sourceBytes,
          startByte,
          delta,
          growsWithInsertion: matched || sharesMatchedNonemptyGeometry,
        ),
        sourceUtf16: _transformInsertionRange(
          envelope.sourceUtf16,
          startUtf16,
          delta,
          growsWithInsertion: matched || sharesMatchedNonemptyGeometry,
        ),
      ),
    );
  }
  return FlarkProjectionContinuityReceipt._(
    baseRevision: revision,
    resultRevision: revision + 1,
    authorizedContentUtf16: FlarkSourceRange(
      authorizedContentUtf16.start,
      authorizedContentUtf16.end + replacement.length,
    ),
    editStartUtf16: startUtf16,
    editEndUtf16: endUtf16,
    replacement: replacement,
    literalSafeEnvelopes: transformedEnvelopes,
  );
}

bool _matchesEnvelope(
  FlarkLiteralSafeEnvelope envelope,
  int startUtf16,
  String replacement,
) {
  if (!_matchesEditClass(envelope.editClass, replacement)) return false;
  return switch (envelope.editClass) {
    FlarkLiteralEditClass.singleAsciiSpaceInsertion =>
      envelope.sourceUtf16.length == 0 ||
          (envelope.sourceUtf16.start < startUtf16 &&
              startUtf16 < envelope.sourceUtf16.end),
    FlarkLiteralEditClass.singleAsciiAsteriskInsertion =>
      envelope.sourceUtf16.start < startUtf16 &&
          startUtf16 < envelope.sourceUtf16.end,
    FlarkLiteralEditClass.asciiWordInsertion => true,
  };
}

int? _bytePositionForUtf16Insertion(
  FlarkLiteralSafeEnvelope envelope,
  int startUtf16,
) {
  final utf16 = envelope.sourceUtf16;
  final bytes = envelope.sourceBytes;
  if (utf16.length == 0) {
    return bytes.length == 0 ? bytes.start : null;
  }
  if (bytes.length != utf16.length) return null;
  return bytes.start + (startUtf16 - utf16.start);
}

bool _sameEnvelopeGeometry(
  FlarkLiteralSafeEnvelope left,
  FlarkLiteralSafeEnvelope right,
) =>
    left.sourceBytes.start == right.sourceBytes.start &&
    left.sourceBytes.end == right.sourceBytes.end &&
    left.sourceUtf16.start == right.sourceUtf16.start &&
    left.sourceUtf16.end == right.sourceUtf16.end;

FlarkSourceRange _transformInsertionRange(
  FlarkSourceRange range,
  int start,
  int delta, {
  required bool growsWithInsertion,
}) {
  if (growsWithInsertion && range.length > 0) {
    return FlarkSourceRange(range.start, range.end + delta);
  }
  if (range.start >= start) {
    return FlarkSourceRange(range.start + delta, range.end + delta);
  }
  if (range.end <= start) return range;
  return FlarkSourceRange(range.start, range.end + delta);
}

bool _matchesEditClass(FlarkLiteralEditClass editClass, String replacement) =>
    switch (editClass) {
      FlarkLiteralEditClass.asciiWordInsertion =>
        replacement.isNotEmpty &&
            replacement.codeUnits.every(
              (unit) =>
                  (unit >= 0x30 && unit <= 0x39) ||
                  (unit >= 0x41 && unit <= 0x5a) ||
                  (unit >= 0x61 && unit <= 0x7a),
            ),
      FlarkLiteralEditClass.singleAsciiSpaceInsertion => replacement == ' ',
      FlarkLiteralEditClass.singleAsciiAsteriskInsertion => replacement == '*',
    };
