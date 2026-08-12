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
    required String? rowContent,
  }) : _rowContent = rowContent;

  final int baseRevision;
  final int resultRevision;
  final FlarkSourceRange authorizedContentUtf16;
  final int editStartUtf16;
  final int editEndUtf16;
  final String replacement;

  /// Exact bounded content retained only for parser-authorized row edits.
  /// Inline-fact continuity remains range-only because the fact itself owns
  /// the complete safe content boundary.
  final String? _rowContent;

  FlarkProjectionContinuityReceipt? continueWith({
    required int startUtf16,
    required int endUtf16,
    required String replacement,
  }) {
    final retainedRowContent = _rowContent;
    if (retainedRowContent == null) {
      if (!_plainTextTransactionAllowed(
        authorizedContentUtf16,
        startUtf16,
        endUtf16,
        replacement,
      )) {
        return null;
      }
    } else if (!_rowPlainTextTransactionAllowed(
      authorizedContentUtf16,
      retainedRowContent,
      startUtf16,
      endUtf16,
      replacement,
    )) {
      return null;
    }
    final delta = replacement.length - (endUtf16 - startUtf16);
    final nextRowContent = retainedRowContent?.replaceRange(
      startUtf16 - authorizedContentUtf16.start,
      endUtf16 - authorizedContentUtf16.start,
      replacement,
    );
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
      rowContent: nextRowContent,
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
    rowContent: null,
  );
}

/// Binds a parser-authored row policy to one exact conservative source edit.
/// The bounded exact row content lets deletions fail closed when they touch a
/// boundary or could join Markdown-sensitive source.
FlarkProjectionContinuityReceipt? authorizeRowProjectionContinuity({
  required int revision,
  required FlarkViewportRowContinuityPolicy policy,
  required FlarkSourceRange editableUtf16,
  required String editableText,
  required List<FlarkInlineFact> inlineFacts,
  required int startUtf16,
  required int endUtf16,
  required String replacement,
}) {
  if (revision <= 0 ||
      policy != FlarkViewportRowContinuityPolicy.plainTextEdit ||
      editableText.length != editableUtf16.length ||
      !_rowPlainTextTransactionAllowed(
        editableUtf16,
        editableText,
        startUtf16,
        endUtf16,
        replacement,
      )) {
    return null;
  }
  for (final fact in inlineFacts) {
    final source = fact.sourceUtf16;
    final touchesFact = startUtf16 == endUtf16
        ? source.start <= startUtf16 && startUtf16 <= source.end
        : startUtf16 < source.end && source.start < endUtf16;
    if (touchesFact) {
      return null;
    }
  }
  final nextText = editableText.replaceRange(
    startUtf16 - editableUtf16.start,
    endUtf16 - editableUtf16.start,
    replacement,
  );
  return FlarkProjectionContinuityReceipt._(
    baseRevision: revision,
    resultRevision: revision + 1,
    authorizedContentUtf16: FlarkSourceRange(
      editableUtf16.start,
      editableUtf16.end + replacement.length,
    ),
    editStartUtf16: startUtf16,
    editEndUtf16: endUtf16,
    replacement: replacement,
    rowContent: nextText,
  );
}

bool _rowPlainTextTransactionAllowed(
  FlarkSourceRange content,
  String exactContent,
  int start,
  int end,
  String replacement,
) {
  if (!_plainTextTransactionAllowed(content, start, end, replacement)) {
    return false;
  }
  if (exactContent.length != content.length) return false;
  if (start == end) {
    if (replacement.isEmpty) return false;
    // At a block content boundary, otherwise-inline-safe punctuation can
    // create a heading, quote, list, or other block construct. Fail closed;
    // the parser will publish the new presentation for the result revision.
    if (start == content.start &&
        _containsMarkdownSensitiveAscii(replacement)) {
      return false;
    }
    return true;
  }

  // A boundary deletion can expose a block marker that was previously plain
  // text. Deletes in the interior are retained only when the removed source
  // and the two newly adjacent scalars are all syntax-insensitive.
  if (start == content.start) return false;
  final localStart = start - content.start;
  final localEnd = end - content.start;
  final removed = exactContent.substring(localStart, localEnd);
  if (!_plainTextReplacementAllowed(removed)) return false;
  final left = exactContent.substring(localStart - 1, localStart);
  final right = localEnd == exactContent.length
      ? ''
      : exactContent.substring(localEnd, localEnd + 1);
  return !_containsMarkdownSensitiveAscii(left) &&
      !_containsMarkdownSensitiveAscii(right);
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
  return _plainTextReplacementAllowed(replacement);
}

bool _plainTextReplacementAllowed(String replacement) {
  for (final scalar in replacement.runes) {
    if (scalar == 0x0a || scalar == 0x0d || scalar == 0x00) return false;
    if (scalar <= 0x7f && r'\*_~`[]<>'.codeUnits.contains(scalar)) {
      return false;
    }
  }
  return true;
}

bool _containsMarkdownSensitiveAscii(String value) {
  const sensitive = r'\*_~`[]<>#>-+.!()|';
  return value.runes.any(
    (scalar) => scalar <= 0x7f && sensitive.codeUnits.contains(scalar),
  );
}
