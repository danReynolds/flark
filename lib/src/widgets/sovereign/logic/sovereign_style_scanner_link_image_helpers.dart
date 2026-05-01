part of 'sovereign_style_scanner.dart';

class _ScannerLinkImageParsers {
  static _LinkSpanMatch? _matchLinkAt(String text, int offset) {
    if (offset < 0 || offset >= text.length) return null;
    final detailed = _matchLinkDetailedAt(text, offset);
    return detailed?.toSpanMatch();
  }

  static _DetailedLinkMatch? _matchLinkDetailedAt(String text, int offset) {
    if (offset < 0 || offset >= text.length) return null;

    final markdownLink = _matchMarkdownLinkDetailed(text, offset);
    if (markdownLink != null) return markdownLink;

    final referenceLink = _matchReferenceLinkDetailed(text, offset);
    if (referenceLink != null) return referenceLink;

    final angleAutolink = _matchAngleAutolinkDetailed(text, offset);
    if (angleAutolink != null) return angleAutolink;

    return _matchBareUrlDetailed(text, offset);
  }

  static _LinkSpanMatch? _matchMarkdownImage(String text, int offset) {
    return _matchMarkdownImageDetailed(text, offset)?.toSpanMatch();
  }

  static _DetailedImageMatch? _matchMarkdownImageDetailed(
    String text,
    int offset,
  ) {
    if (offset < 0 || offset + 3 >= text.length) return null;
    if (_isEscapedAt(text, offset)) return null;
    if (text.codeUnitAt(offset) != 33 || text.codeUnitAt(offset + 1) != 91) {
      return null; // ![
    }

    final len = text.length;
    var altEnd = -1;
    var i = offset + 2;
    while (i < len) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return null;
      if (cu == 93) {
        altEnd = i;
        break;
      }
      i++;
    }
    if (altEnd == -1) return null;
    if (altEnd + 1 >= len || text.codeUnitAt(altEnd + 1) != 40) {
      return null; // (
    }

    final destination = _parseInlineDestination(text, altEnd + 1);
    if (destination == null) return null;

    return _DetailedImageMatch(
      fullStart: offset,
      fullEnd: destination.fullEnd,
      altStart: offset + 2,
      altEnd: altEnd,
      urlStart: destination.urlStart,
      urlEnd: destination.urlEnd,
      nextOffset: destination.fullEnd,
    );
  }

  static _DetailedLinkMatch? _matchMarkdownLinkDetailed(
    String text,
    int offset,
  ) {
    if (text.codeUnitAt(offset) != 91) return null; // [
    if (_isEscapedAt(text, offset)) return null;
    if (offset > 0 && text.codeUnitAt(offset - 1) == 33) return null; // !

    final len = text.length;
    var labelEnd = -1;
    var i = offset + 1;
    while (i < len) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        // Escape next code unit.
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return null;
      if (cu == 93) {
        labelEnd = i;
        break;
      }
      i++;
    }
    if (labelEnd == -1 || labelEnd <= offset + 1) return null;
    if (labelEnd + 1 >= len || text.codeUnitAt(labelEnd + 1) != 40) {
      return null;
    }

    final destination = _parseInlineDestination(text, labelEnd + 1);
    if (destination == null) return null;

    return _DetailedLinkMatch(
      kind: SovereignLinkMatchKind.markdown,
      fullStart: offset,
      fullEnd: destination.fullEnd,
      displayStart: offset + 1,
      displayEnd: labelEnd,
      urlStart: destination.urlStart,
      urlEnd: destination.urlEnd,
      nextOffset: destination.fullEnd,
    );
  }

  static _DetailedLinkMatch? _matchReferenceLinkDetailed(
    String text,
    int offset,
  ) {
    if (text.codeUnitAt(offset) != 91) return null; // [
    if (_isEscapedAt(text, offset)) return null;
    if (offset > 0 && text.codeUnitAt(offset - 1) == 33) return null; // ![

    final len = text.length;
    var labelEnd = -1;
    var i = offset + 1;
    while (i < len) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return null;
      if (cu == 93) {
        labelEnd = i;
        break;
      }
      i++;
    }
    if (labelEnd == -1 || labelEnd <= offset + 1) return null;

    if (labelEnd + 1 >= len || text.codeUnitAt(labelEnd + 1) != 91) {
      return null; // second [
    }

    final refLabelStart = labelEnd + 2;
    i = refLabelStart;
    var refLabelEnd = -1;
    while (i < len) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return null;
      if (cu == 93) {
        refLabelEnd = i;
        break;
      }
      i++;
    }
    if (refLabelEnd == -1) return null;

    // Avoid matching inline markdown links `[label](...)`.
    if (labelEnd + 1 < len && text.codeUnitAt(labelEnd + 1) == 40) return null;

    final refHasLabel = refLabelEnd > refLabelStart;
    final effectiveRefStart = refHasLabel ? refLabelStart : offset + 1;
    final effectiveRefEnd = refHasLabel ? refLabelEnd : labelEnd;

    return _DetailedLinkMatch(
      kind: SovereignLinkMatchKind.reference,
      fullStart: offset,
      fullEnd: refLabelEnd + 1,
      displayStart: offset + 1,
      displayEnd: labelEnd,
      // For reference links, urlStart/urlEnd carry the effective reference label
      // range; the editor resolves the definition URL separately.
      urlStart: effectiveRefStart,
      urlEnd: effectiveRefEnd,
      referenceLabelStart: effectiveRefStart,
      referenceLabelEnd: effectiveRefEnd,
      nextOffset: refLabelEnd + 1,
    );
  }

  static _DetailedLinkMatch? _matchAngleAutolinkDetailed(
    String text,
    int offset,
  ) {
    if (text.codeUnitAt(offset) != 60) return null; // <
    if (_isEscapedAt(text, offset)) return null;

    final close = text.indexOf('>', offset + 1);
    if (close == -1) return null;
    if (close <= offset + 1) return null;

    final inner = text.substring(offset + 1, close);
    if (!_looksLikeUrl(inner)) return null;

    return _DetailedLinkMatch(
      kind: SovereignLinkMatchKind.autolink,
      fullStart: offset,
      fullEnd: close + 1,
      displayStart: offset + 1,
      displayEnd: close,
      urlStart: offset + 1,
      urlEnd: close,
      nextOffset: close + 1,
    );
  }

  static _DetailedLinkMatch? _matchBareUrlDetailed(String text, int offset) {
    if (!_hasUrlPrefixAt(text, offset)) return null;
    if (!_isLeftUrlBoundary(text, offset)) return null;

    final len = text.length;
    var end = offset;
    while (end < len) {
      final cu = text.codeUnitAt(end);
      if (_isUrlTerminator(cu)) break;
      end++;
    }
    if (end <= offset) return null;

    while (
        end > offset && _isTrailingUrlPunctuation(text.codeUnitAt(end - 1))) {
      end--;
    }
    if (end <= offset) return null;

    return _DetailedLinkMatch(
      kind: SovereignLinkMatchKind.bare,
      fullStart: offset,
      fullEnd: end,
      displayStart: offset,
      displayEnd: end,
      urlStart: offset,
      urlEnd: end,
      nextOffset: end,
    );
  }

  static bool _hasUrlPrefixAt(String text, int offset) {
    const http = 'http://';
    const https = 'https://';
    if (offset + http.length <= text.length && text.startsWith(http, offset)) {
      return true;
    }
    if (offset + https.length <= text.length &&
        text.startsWith(https, offset)) {
      return true;
    }
    return false;
  }

  static bool _looksLikeUrl(String value) {
    final lower = value.toLowerCase();
    return lower.startsWith('http://') ||
        lower.startsWith('https://') ||
        lower.startsWith('mailto:');
  }

  static bool _isLeftUrlBoundary(String text, int offset) {
    if (offset <= 0) return true;
    final prev = text.codeUnitAt(offset - 1);
    return !_isAsciiAlphaNumeric(prev) && prev != 47 && prev != 95;
  }

  static bool _isAsciiAlphaNumeric(int cu) {
    return (cu >= 48 && cu <= 57) ||
        (cu >= 65 && cu <= 90) ||
        (cu >= 97 && cu <= 122);
  }

  static bool _isUrlTerminator(int cu) {
    return cu == 32 ||
        cu == 9 ||
        cu == 10 ||
        cu == 13 ||
        cu == 60 ||
        cu == 62 ||
        cu == 34 ||
        cu == 39;
  }

  static bool _isTrailingUrlPunctuation(int cu) {
    return cu == 46 ||
        cu == 44 ||
        cu == 33 ||
        cu == 63 ||
        cu == 58 ||
        cu == 59 ||
        cu == 41;
  }

  static bool _isEscapedAt(String text, int offset) {
    if (offset <= 0 || offset > text.length) return false;
    var backslashes = 0;
    var i = offset - 1;
    while (i >= 0 && text.codeUnitAt(i) == 92) {
      backslashes++;
      i--;
    }
    return backslashes.isOdd;
  }

  static _InlineDestinationMatch? _parseInlineDestination(
    String text,
    int openParenOffset,
  ) {
    if (openParenOffset < 0 ||
        openParenOffset >= text.length ||
        text.codeUnitAt(openParenOffset) != 40) {
      return null;
    }

    final len = text.length;
    var i = openParenOffset + 1;
    if (i > len) return null;
    if (i >= len) return null;

    int urlStart;
    var urlEnd = -1;

    if (text.codeUnitAt(i) == 60) {
      // <destination>
      urlStart = i + 1;
      i = urlStart;
      var close = -1;
      while (i < len) {
        final cu = text.codeUnitAt(i);
        if (cu == 92) {
          i += 2;
          continue;
        }
        if (cu == 10 || cu == 13) return null;
        if (cu == 62) {
          close = i;
          break;
        }
        i++;
      }
      if (close == -1) return null;
      urlEnd = close;
      i = close + 1;
    } else {
      // Bare destination: no spaces, supports simple escaped chars and balanced parens.
      urlStart = i;
      var depth = 0;
      var closedByParen = false;
      while (i < len) {
        final cu = text.codeUnitAt(i);
        if (cu == 92) {
          i += 2;
          continue;
        }
        if (cu == 10 || cu == 13) return null;
        if (cu == 32 || cu == 9) {
          break;
        }
        if (cu == 40) {
          depth++;
          i++;
          continue;
        }
        if (cu == 41) {
          if (depth == 0) {
            urlEnd = i;
            closedByParen = true;
            break;
          }
          depth--;
          i++;
          continue;
        }
        i++;
      }

      if (closedByParen) {
        return _InlineDestinationMatch(
          urlStart: urlStart,
          urlEnd: urlEnd,
          fullEnd: i + 1,
        );
      }

      urlEnd = i;
      if (urlEnd < urlStart) return null;
    }

    while (i < len) {
      final cu = text.codeUnitAt(i);
      if (cu == 32 || cu == 9) {
        i++;
        continue;
      }
      break;
    }
    if (i >= len) return null;
    if (text.codeUnitAt(i) == 41) {
      return _InlineDestinationMatch(
        urlStart: urlStart,
        urlEnd: urlEnd,
        fullEnd: i + 1,
      );
    }

    final opener = text.codeUnitAt(i);
    int closer;
    if (opener == 34 || opener == 39) {
      closer = opener;
    } else if (opener == 40) {
      closer = 41;
    } else {
      return null;
    }

    i++;
    while (i < len) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return null;
      if (cu == closer) {
        i++;
        break;
      }
      i++;
    }
    if (i > len) return null;
    if (i == len) return null;

    while (i < len) {
      final cu = text.codeUnitAt(i);
      if (cu == 32 || cu == 9) {
        i++;
        continue;
      }
      break;
    }
    if (i >= len || text.codeUnitAt(i) != 41) return null;

    return _InlineDestinationMatch(
      urlStart: urlStart,
      urlEnd: urlEnd,
      fullEnd: i + 1,
    );
  }

  static List<TextRange> _extractLinkHiddenRanges(
    String text,
    int start,
    int end,
  ) {
    if (start < 0 || end > text.length || start >= end) return const [];

    if (start > 0 && text.codeUnitAt(start - 1) == 91) {
      final markdown = _matchMarkdownLinkDetailed(text, start - 1);
      if (markdown != null &&
          markdown.displayStart == start &&
          markdown.displayEnd == end) {
        return <TextRange>[
          TextRange(start: markdown.fullStart, end: markdown.displayStart),
          TextRange(start: markdown.displayEnd, end: markdown.fullEnd),
        ];
      }
      final reference = _matchReferenceLinkDetailed(text, start - 1);
      if (reference != null &&
          reference.displayStart == start &&
          reference.displayEnd == end) {
        return <TextRange>[
          TextRange(start: reference.fullStart, end: reference.displayStart),
          TextRange(start: reference.displayEnd, end: reference.fullEnd),
        ];
      }
    }

    if (start > 0 && text.codeUnitAt(start - 1) == 60) {
      final angle = _matchAngleAutolinkDetailed(text, start - 1);
      if (angle != null &&
          angle.displayStart == start &&
          angle.displayEnd == end) {
        return <TextRange>[
          TextRange(start: angle.fullStart, end: angle.displayStart),
          TextRange(start: angle.displayEnd, end: angle.fullEnd),
        ];
      }
    }

    return const [];
  }

  static List<TextRange> _extractImageHiddenRanges(
    String text,
    int start,
    int end,
  ) {
    if (start < 0 || end > text.length || end - start < 5) return const [];
    if (!text.startsWith('![', start)) return const [];

    var labelEnd = -1;
    var i = start + 2;
    while (i < end) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return const [];
      if (cu == 93) {
        labelEnd = i;
        break;
      }
      i++;
    }
    if (labelEnd == -1) return const [];
    if (labelEnd + 1 >= end || text.codeUnitAt(labelEnd + 1) != 40) {
      return const [];
    }

    var urlEnd = -1;
    i = labelEnd + 2;
    while (i < end) {
      final cu = text.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 10 || cu == 13) return const [];
      if (cu == 41) {
        urlEnd = i;
        break;
      }
      i++;
    }
    if (urlEnd == -1 || urlEnd + 1 != end) return const [];

    return [
      TextRange(start: start, end: (start + 2).clamp(start, end)),
      TextRange(start: labelEnd, end: end),
    ];
  }

  static String? _lookupReferenceDefinitionUrl(String text, String rawLabel) {
    return _lookupReferenceDefinition(text, rawLabel)?.urlText(text);
  }

  static SovereignReferenceDefinitionMatch? _lookupReferenceDefinition(
    String text,
    String rawLabel,
  ) {
    final wanted = _normalizeReferenceLabel(rawLabel);
    if (wanted.isEmpty) return null;
    var lineStart = 0;
    while (lineStart <= text.length) {
      final lineEndWithBreak = (() {
        final next = text.indexOf('\n', lineStart);
        return next == -1 ? text.length : next + 1;
      })();
      final lineEnd = (lineEndWithBreak > lineStart &&
              text.codeUnitAt(lineEndWithBreak - 1) == 10)
          ? lineEndWithBreak - 1
          : lineEndWithBreak;
      if (lineEnd > lineStart) {
        final line = text.substring(lineStart, lineEnd);
        final marker = _matchReferenceDefinitionLine(line);
        if (marker != null &&
            _normalizeReferenceLabel(marker.label) == wanted) {
          return SovereignReferenceDefinitionMatch(
            lineStart: lineStart,
            lineEnd: lineEnd,
            labelStart: lineStart + marker.labelStart,
            labelEnd: lineStart + marker.labelEnd,
            urlStart: lineStart + marker.urlStart,
            urlEnd: lineStart + marker.urlEnd,
          );
        }
      }
      if (lineEndWithBreak <= lineStart) break;
      lineStart = lineEndWithBreak;
      if (lineStart == text.length) break;
    }
    return null;
  }

  static String _normalizeReferenceLabel(String label) {
    final trimmed = label.trim().toLowerCase();
    if (trimmed.isEmpty) return '';
    final out = StringBuffer();
    var inSpace = false;
    for (final cu in trimmed.codeUnits) {
      final isSpace = cu == 32 || cu == 9 || cu == 10 || cu == 13;
      if (isSpace) {
        if (!inSpace) out.write(' ');
        inSpace = true;
      } else {
        out.writeCharCode(cu);
        inSpace = false;
      }
    }
    return out.toString();
  }

  static _ReferenceDefinitionLineMatch? _matchReferenceDefinitionLine(
    String line,
  ) {
    if (line.isEmpty) return null;
    var i = 0;
    var indent = 0;
    while (i < line.length) {
      final cu = line.codeUnitAt(i);
      if (cu == 32) {
        indent++;
        i++;
        continue;
      }
      if (cu == 9) {
        indent += 4 - (indent % 4);
        i++;
        continue;
      }
      break;
    }
    if (indent > 3 || i >= line.length || line.codeUnitAt(i) != 91) return null;
    final labelStart = i + 1;
    var labelEnd = -1;
    i = labelStart;
    while (i < line.length) {
      final cu = line.codeUnitAt(i);
      if (cu == 92) {
        i += 2;
        continue;
      }
      if (cu == 93) {
        labelEnd = i;
        break;
      }
      i++;
    }
    if (labelEnd <= labelStart) return null;
    if (labelEnd + 1 >= line.length || line.codeUnitAt(labelEnd + 1) != 58) {
      return null;
    }
    i = labelEnd + 2;
    while (i < line.length) {
      final cu = line.codeUnitAt(i);
      if (cu == 32 || cu == 9) {
        i++;
        continue;
      }
      break;
    }
    if (i >= line.length) return null;

    String url;
    if (line.codeUnitAt(i) == 60) {
      final close = line.indexOf('>', i + 1);
      if (close == -1 || close <= i + 1) return null;
      url = line.substring(i + 1, close);
    } else {
      var end = i;
      while (end < line.length) {
        final cu = line.codeUnitAt(end);
        if (cu == 32 || cu == 9) break;
        end++;
      }
      if (end <= i) return null;
      url = line.substring(i, end);
    }
    return _ReferenceDefinitionLineMatch(
      label: line.substring(labelStart, labelEnd),
      url: url,
      labelStart: labelStart,
      labelEnd: labelEnd,
      urlStart: i,
      urlEnd: i + url.length,
    );
  }
}
