import 'models.dart';

/// A transaction-bound authorization derived from one parser-authored inline
/// continuity policy. It is separate from current-revision certification.
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

  FlarkProjectionContinuityReceipt? continueWith({
    required int startUtf16,
    required int endUtf16,
    required String replacement,
  }) {
    if (!_plainTextTransactionAllowed(
      authorizedContentUtf16,
      startUtf16,
      endUtf16,
      replacement,
    )) {
      return null;
    }
    final delta = replacement.length - (endUtf16 - startUtf16);
    return FlarkProjectionContinuityReceipt._(
      baseRevision: resultRevision,
      resultRevision: resultRevision + 1,
      authorizedContentUtf16: FlarkSourceRange(
        authorizedContentUtf16.start,
        authorizedContentUtf16.end + delta,
      ),
      editStartUtf16: startUtf16,
      editEndUtf16: endUtf16,
      replacement: replacement,
    );
  }
}

/// Executes the generic edit policy published by Rust with current inline
/// facts. This code recognizes no Markdown syntax; it only validates a typed
/// capability and binds it to one exact source transaction.
FlarkProjectionContinuityReceipt? authorizeInlineProjectionContinuity({
  required int revision,
  required List<FlarkInlineFact> facts,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  if (revision <= 0 || startUtf16 > endUtf16) return null;

  // Touching any parser-owned marker or replacement invalidates continuity.
  for (final fact in facts) {
    final source = fact.sourceUtf16;
    final content = fact.contentUtf16;
    final touchesSource = startUtf16 == endUtf16
        ? startUtf16 >= source.start && startUtf16 <= source.end
        : startUtf16 < source.end && source.start < endUtf16;
    if (touchesSource &&
        (startUtf16 < content.start || endUtf16 > content.end)) {
      return null;
    }
  }

  FlarkInlineFact? authority;
  for (final fact in facts) {
    if (fact.continuityPolicy != FlarkInlineContinuityPolicy.plainTextContent) {
      continue;
    }
    final content = fact.contentUtf16;
    if (!_plainTextTransactionAllowed(
      content,
      startUtf16,
      endUtf16,
      replacement,
    )) {
      continue;
    }
    if (authority == null || content.length < authority.contentUtf16.length) {
      authority = fact;
    }
  }
  if (authority == null) return null;
  return FlarkProjectionContinuityReceipt._(
    baseRevision: revision,
    resultRevision: revision + 1,
    authorizedContentUtf16: FlarkSourceRange(
      authority.contentUtf16.start,
      authority.contentUtf16.end + replacement.length - (endUtf16 - startUtf16),
    ),
    editStartUtf16: startUtf16,
    editEndUtf16: endUtf16,
    replacement: replacement,
  );
}

bool _plainTextTransactionAllowed(
  FlarkSourceRange content,
  int start,
  int end,
  String replacement,
) {
  // The parser publishes this policy only for constructs whose delimiters stay
  // valid under conservative plain-text edits at their content boundaries.
  // Empty constructs are never retained speculatively.
  if (start < content.start || end > content.end || end < start) return false;
  final resultingLength = content.length - (end - start) + replacement.length;
  if (resultingLength <= 0) return false;
  for (final scalar in replacement.runes) {
    if (scalar == 0x0a || scalar == 0x0d || scalar == 0x00) return false;
    if (scalar <= 0x7f && r'\*_~`[]<>'.codeUnits.contains(scalar)) {
      return false;
    }
  }
  return true;
}
