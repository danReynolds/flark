import 'flark_inline_flanking.dart';

/// The source span of one flanking-valid inline run: `[openStart, closeEnd)`
/// with content at `[contentStart, closeStart)` and its delimiter cluster
/// (`*`, `**`, `***`, `_`, `__`, `___`, `~~`).
final class FlarkInlineRunScan {
  const FlarkInlineRunScan({
    required this.openStart,
    required this.contentStart,
    required this.closeStart,
    required this.closeEnd,
    required this.marker,
  });

  final int openStart;
  final int contentStart;
  final int closeStart;
  final int closeEnd;

  /// The delimiter cluster text (identical on both edges).
  final String marker;
}

/// The gap between a run's closing delimiter and a caret sitting after it with
/// only horizontal whitespace in between — the state left behind when typed
/// trailing whitespace was committed outside the run so the source stays
/// valid. Typing a styled character in this gap re-enters the run, pulling the
/// whitespace back inside.
final class FlarkInlineReentryGap {
  const FlarkInlineReentryGap({required this.run, required this.whitespace});

  final FlarkInlineRunScan run;

  /// The horizontal whitespace between [run]'s closing delimiter and the
  /// caret. Never empty.
  final String whitespace;
}

/// Finds inline runs the way CommonMark would actually style them.
///
/// This is the single notion of "the run at the caret" for editing decisions:
/// delimiters must be unescaped, exact clusters (not a slice of a longer
/// delimiter run), flanking-valid on the correct side, and the content must be
/// non-blank and stay within one paragraph. Textual scans that skip any of
/// these checks disagree with the parser exactly on the malformed shapes this
/// package must not write — see `FlarkInlineDelimiterPlacement`.
abstract final class FlarkInlineRunScanner {
  /// The delimiter clusters that carry a marker-character family. Longer
  /// clusters come before their prefixes so a probe never matches inside a
  /// longer run; `~` last covers GFM's single-tilde strikethrough.
  static const List<String> allMarkers = [
    '***',
    '**',
    '*',
    '___',
    '__',
    '_',
    '~~',
    '~',
  ];

  /// The run delimited by exactly [marker] that encloses a collapsed caret at
  /// [caret], or null when no flanking-valid run does.
  ///
  /// A caret at the closing delimiter's start counts as inside (the run's
  /// trailing edge); a caret at the content start counts as inside (the
  /// leading edge). The scan never crosses a blank line.
  static FlarkInlineRunScan? validEnclosingRun(
    String source,
    int caret,
    String marker,
  ) {
    if (caret < 0 || caret > source.length) return null;
    var searchCeiling = caret;
    while (searchCeiling >= 0) {
      final openStart = _openerBefore(source, marker, searchCeiling);
      if (openStart == null) return null;
      final contentStart = openStart + marker.length;
      if (caret < contentStart) {
        searchCeiling = openStart - 1;
        continue;
      }
      final closeStart = _closerAfter(source, marker, contentStart);
      if (closeStart == null) return null;
      if (caret > closeStart) return null;
      if (!_isRunContent(source, contentStart, closeStart)) return null;
      return FlarkInlineRunScan(
        openStart: openStart,
        contentStart: contentStart,
        closeStart: closeStart,
        closeEnd: closeStart + marker.length,
        marker: marker,
      );
    }
    return null;
  }

  /// The run (of any marker in [allMarkers]) whose closing delimiter starts
  /// exactly at [caret] — i.e. the caret sits at a run's trailing edge — or
  /// null.
  static FlarkInlineRunScan? runClosingAt(String source, int caret) {
    for (final marker in allMarkers) {
      final run = validEnclosingRun(source, caret, marker);
      if (run != null && run.closeStart == caret) return run;
    }
    return null;
  }

  /// The run (of any marker in [allMarkers]) whose content starts exactly at
  /// [caret] — i.e. the caret sits at a run's leading edge — or null.
  static FlarkInlineRunScan? runOpeningAt(String source, int caret) {
    for (final marker in allMarkers) {
      final run = validEnclosingRun(source, caret, marker);
      if (run != null && run.contentStart == caret) return run;
    }
    return null;
  }

  /// The re-entry gap ending at [caret]: a flanking-valid run's closing
  /// delimiter followed by nothing but horizontal whitespace up to the caret.
  /// Returns null when the caret is not in such a gap.
  static FlarkInlineReentryGap? reentryGapAt(String source, int caret) {
    if (caret <= 0 || caret > source.length) return null;
    var wsStart = caret;
    while (wsStart > 0 &&
        _isHorizontalWhitespace(source.codeUnitAt(wsStart - 1))) {
      wsStart -= 1;
    }
    if (wsStart == caret) return null;
    for (final marker in allMarkers) {
      if (wsStart < marker.length) continue;
      final closeStart = wsStart - marker.length;
      final run = validEnclosingRun(source, closeStart, marker);
      if (run != null && run.closeStart == closeStart) {
        return FlarkInlineReentryGap(
          run: run,
          whitespace: source.substring(wsStart, caret),
        );
      }
    }
    return null;
  }

  /// The nearest opener of [marker] at or before [searchCeiling]: unescaped,
  /// an exact cluster, and flanking-valid as an opener.
  static int? _openerBefore(String source, String marker, int searchCeiling) {
    var probe = searchCeiling.clamp(0, source.length);
    while (probe >= 0) {
      final index = source.lastIndexOf(marker, probe);
      if (index < 0) return null;
      if (_isExactCluster(source, index, marker) &&
          !FlarkInlineFlanking.isEscaped(source, index) &&
          FlarkInlineFlanking.canOpen(source, index, index + marker.length)) {
        return index;
      }
      probe = index - 1;
    }
    return null;
  }

  /// The nearest closer of [marker] at or after [searchFloor]: unescaped, an
  /// exact cluster, and flanking-valid as a closer. Stops at a blank line.
  static int? _closerAfter(String source, String marker, int searchFloor) {
    var probe = searchFloor.clamp(0, source.length);
    while (probe <= source.length - marker.length) {
      final index = source.indexOf(marker, probe);
      if (index < 0) return null;
      if (_containsBlankLine(source, searchFloor, index)) return null;
      if (_isExactCluster(source, index, marker) &&
          !FlarkInlineFlanking.isEscaped(source, index) &&
          FlarkInlineFlanking.canClose(source, index, index + marker.length)) {
        return index;
      }
      probe = index + marker.length;
    }
    return null;
  }

  /// Whether `[contentStart, closeStart)` is legal run content: non-empty,
  /// not blank, and within one paragraph.
  static bool _isRunContent(String source, int contentStart, int closeStart) {
    if (contentStart >= closeStart) return false;
    if (source.substring(contentStart, closeStart).trim().isEmpty) {
      return false;
    }
    return !_containsBlankLine(source, contentStart, closeStart);
  }

  /// Whether the [marker] at [index] is a maximal delimiter run — not a slice
  /// of a longer run of the same character (`**` inside `***`).
  static bool _isExactCluster(String source, int index, String marker) {
    final markerChar = marker.codeUnitAt(0);
    if (index > 0 && source.codeUnitAt(index - 1) == markerChar) return false;
    final after = index + marker.length;
    return after >= source.length || source.codeUnitAt(after) != markerChar;
  }

  /// Whether `[start, end)` contains a blank line (a newline followed by only
  /// horizontal whitespace before the next newline).
  static bool _containsBlankLine(String source, int start, int end) {
    var cursor = start;
    while (cursor < end) {
      if (source.codeUnitAt(cursor) != 0x0A) {
        cursor += 1;
        continue;
      }
      var lookahead = cursor + 1;
      while (lookahead < end &&
          _isHorizontalWhitespace(source.codeUnitAt(lookahead))) {
        lookahead += 1;
      }
      if (lookahead < end && source.codeUnitAt(lookahead) == 0x0A) return true;
      cursor = lookahead;
    }
    return false;
  }

  static bool _isHorizontalWhitespace(int codeUnit) {
    return codeUnit == 0x20 || codeUnit == 0x09;
  }
}
