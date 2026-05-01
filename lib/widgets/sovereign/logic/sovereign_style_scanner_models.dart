part of 'sovereign_style_scanner.dart';

/// A contiguous run of styled text.
///
/// Invariant: [start] < [end].
class StyleRun {
  final int start;
  final int end;
  final SovereignStyle style;

  const StyleRun(this.start, this.end, this.style);

  @override
  String toString() => '[$start-$end: $style]';
}

/// Result of a budget-constrained scan.
class ScannerResult {
  /// The list of fully resolved style runs.
  final List<StyleRun> runs;

  /// The offset up to which the scan is valid and closed.
  ///
  /// If [complete] is true, [validTo] == text.length.
  /// If [complete] is false, any text from [validTo] to end should be rendered as plain text.
  final int validTo;

  /// Whether the scan completed successfully without hitting budget limits.
  final bool complete;

  const ScannerResult({
    required this.runs,
    required this.validTo,
    required this.complete,
  });
}

class _LinkSpanMatch {
  final int start;
  final int end;
  final int nextOffset;

  const _LinkSpanMatch({
    required this.start,
    required this.end,
    required this.nextOffset,
  });
}

enum SovereignLinkMatchKind { markdown, reference, autolink, bare }

@immutable
class SovereignLinkMatch {
  final SovereignLinkMatchKind kind;
  final int fullStart;
  final int fullEnd;
  final int displayStart;
  final int displayEnd;
  final int urlStart;
  final int urlEnd;
  final int? referenceLabelStart;
  final int? referenceLabelEnd;

  const SovereignLinkMatch({
    required this.kind,
    required this.fullStart,
    required this.fullEnd,
    required this.displayStart,
    required this.displayEnd,
    required this.urlStart,
    required this.urlEnd,
    this.referenceLabelStart,
    this.referenceLabelEnd,
  });

  String labelText(String text) => text.substring(displayStart, displayEnd);
  String urlText(String text) => text.substring(urlStart, urlEnd);
  String? referenceLabelText(String text) {
    final start = referenceLabelStart;
    final end = referenceLabelEnd;
    if (start == null || end == null) return null;
    if (start < 0 || end > text.length || start >= end) return null;
    return text.substring(start, end);
  }

  bool containsCaret(int caret) {
    return caret >= displayStart && caret <= displayEnd;
  }
}

@immutable
class SovereignImageMatch {
  final int fullStart;
  final int fullEnd;
  final int altStart;
  final int altEnd;
  final int urlStart;
  final int urlEnd;

  const SovereignImageMatch({
    required this.fullStart,
    required this.fullEnd,
    required this.altStart,
    required this.altEnd,
    required this.urlStart,
    required this.urlEnd,
  });

  String altText(String text) => text.substring(altStart, altEnd);
  String urlText(String text) => text.substring(urlStart, urlEnd);

  bool containsCaret(int caret) {
    return caret >= altStart && caret <= altEnd;
  }
}

@immutable
class SovereignReferenceDefinitionMatch {
  final int lineStart;
  final int lineEnd;
  final int labelStart;
  final int labelEnd;
  final int urlStart;
  final int urlEnd;

  const SovereignReferenceDefinitionMatch({
    required this.lineStart,
    required this.lineEnd,
    required this.labelStart,
    required this.labelEnd,
    required this.urlStart,
    required this.urlEnd,
  });

  String labelText(String text) => text.substring(labelStart, labelEnd);
  String urlText(String text) => text.substring(urlStart, urlEnd);
}

class _DetailedLinkMatch {
  final SovereignLinkMatchKind kind;
  final int fullStart;
  final int fullEnd;
  final int displayStart;
  final int displayEnd;
  final int urlStart;
  final int urlEnd;
  final int? referenceLabelStart;
  final int? referenceLabelEnd;
  final int nextOffset;

  const _DetailedLinkMatch({
    required this.kind,
    required this.fullStart,
    required this.fullEnd,
    required this.displayStart,
    required this.displayEnd,
    required this.urlStart,
    required this.urlEnd,
    this.referenceLabelStart,
    this.referenceLabelEnd,
    required this.nextOffset,
  });

  _LinkSpanMatch toSpanMatch() => _LinkSpanMatch(
        start: displayStart,
        end: displayEnd,
        nextOffset: nextOffset,
      );

  bool containsCaret(int caret) => caret >= displayStart && caret <= displayEnd;

  SovereignLinkMatch toPublic() => SovereignLinkMatch(
        kind: kind,
        fullStart: fullStart,
        fullEnd: fullEnd,
        displayStart: displayStart,
        displayEnd: displayEnd,
        urlStart: urlStart,
        urlEnd: urlEnd,
        referenceLabelStart: referenceLabelStart,
        referenceLabelEnd: referenceLabelEnd,
      );
}

class _DetailedImageMatch {
  final int fullStart;
  final int fullEnd;
  final int altStart;
  final int altEnd;
  final int urlStart;
  final int urlEnd;
  final int nextOffset;

  const _DetailedImageMatch({
    required this.fullStart,
    required this.fullEnd,
    required this.altStart,
    required this.altEnd,
    required this.urlStart,
    required this.urlEnd,
    required this.nextOffset,
  });

  _LinkSpanMatch toSpanMatch() =>
      _LinkSpanMatch(start: fullStart, end: fullEnd, nextOffset: nextOffset);

  bool containsCaret(int caret) => caret >= altStart && caret <= altEnd;

  SovereignImageMatch toPublic() => SovereignImageMatch(
        fullStart: fullStart,
        fullEnd: fullEnd,
        altStart: altStart,
        altEnd: altEnd,
        urlStart: urlStart,
        urlEnd: urlEnd,
      );
}

class _InlineDestinationMatch {
  final int urlStart;
  final int urlEnd;
  final int fullEnd;

  const _InlineDestinationMatch({
    required this.urlStart,
    required this.urlEnd,
    required this.fullEnd,
  });
}
