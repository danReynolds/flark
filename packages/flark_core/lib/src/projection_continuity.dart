import 'models.dart';

/// One parser-authored literal-safe envelope bound to an exact source
/// transaction and its result revision.
///
/// This receipt carries no Markdown policy. The parser already proved the
/// matching edit class safe at the published position; Core performs only
/// edit-class matching and range containment.
final class FlarkProjectionContinuityReceipt {
  const FlarkProjectionContinuityReceipt._({
    required this.baseRevision,
    required this.resultRevision,
    required this.authorizedContentUtf16,
    required this.editStartUtf16,
    required this.editEndUtf16,
    required this.replacement,
  });

  final int baseRevision;
  final int resultRevision;
  final FlarkSourceRange authorizedContentUtf16;
  final int editStartUtf16;
  final int editEndUtf16;
  final String replacement;
}

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
  final matching = envelopes.where(
    (envelope) =>
        envelope.sourceUtf16.start <= startUtf16 &&
        startUtf16 <= envelope.sourceUtf16.end &&
        _matchesEditClass(envelope.editClass, replacement),
  );
  if (matching.isEmpty) return null;
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
  );
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
    };
