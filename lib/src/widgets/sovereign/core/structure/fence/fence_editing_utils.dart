import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_code_highlighter.dart';
import '../navigation/navigation_line_utils.dart';

class FenceEditingUtils {
  const FenceEditingUtils._();

  static const Map<int, int> smartPairMap = <int, int>{
    40: 41, // ( )
    91: 93, // [ ]
    123: 125, // { }
    34: 34, // " "
    39: 39, // ' '
  };

  static const Set<int> closerChars = <int>{41, 93, 125, 34, 39};

  static bool isAutoOutdentCloser(int codeUnit) {
    return codeUnit == 125 || // }
        codeUnit == 93 || // ]
        codeUnit == 41; // )
  }

  static String preferredIndentUnit(String currentIndent) {
    if (currentIndent.contains('\t')) return '\t';
    if (currentIndent.length >= 4 && currentIndent.length % 4 == 0) {
      return '    ';
    }
    return '  ';
  }

  static String removeOneIndentUnit(String indent, String unit) {
    if (indent.isEmpty) return indent;

    // Always outdent from line start (editor-style), never from the suffix.
    final first = indent.codeUnitAt(0);
    if (first == 9) {
      return indent.substring(1); // leading tab
    }

    int leadingSpaces = 0;
    while (leadingSpaces < indent.length &&
        indent.codeUnitAt(leadingSpaces) == 32) {
      leadingSpaces++;
    }
    if (leadingSpaces == 0) return indent;

    int requestedSpaces;
    if (unit == '\t') {
      // For mixed whitespace with a tab-preferred context, remove a sensible
      // leading space unit before touching deeper indentation tokens.
      requestedSpaces = leadingSpaces >= 4 ? 4 : (leadingSpaces >= 2 ? 2 : 1);
    } else {
      var spacesOnlyUnit = true;
      for (var i = 0; i < unit.length; i++) {
        if (unit.codeUnitAt(i) != 32) {
          spacesOnlyUnit = false;
          break;
        }
      }
      requestedSpaces = spacesOnlyUnit && unit.isNotEmpty ? unit.length : 2;
    }

    final removeSpaces = requestedSpaces.clamp(1, leadingSpaces);
    return indent.substring(removeSpaces);
  }

  static bool shouldIncreaseIndentForFenceLine(
    String trimmedBeforeCaret,
    String? language,
  ) {
    if (trimmedBeforeCaret.isEmpty) return false;
    final last = trimmedBeforeCaret.codeUnitAt(trimmedBeforeCaret.length - 1);
    if (last == 123 || last == 91 || last == 40) {
      return true; // { [ (
    }
    if (last == 58) {
      return _languageUsesColonIndent(language);
    }
    return false;
  }

  static String normalizeFencedMultilineInsert({
    required String insertedText,
    required String baseIndent,
  }) {
    var normalized =
        insertedText.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
    if (!normalized.contains('\n')) return insertedText;

    final lines = normalized.split('\n');
    if (lines.length < 2) return insertedText;

    final nonEmptyLines = <String>[
      for (final line in lines)
        if (line.trim().isNotEmpty) line,
    ];
    if (nonEmptyLines.isEmpty) return insertedText;

    final commonIndent = commonLeadingWhitespace(nonEmptyLines);
    final adjustedLines = <String>[];
    for (var i = 0; i < lines.length; i++) {
      final line = lines[i];
      if (line.isEmpty) {
        adjustedLines.add(line);
        continue;
      }

      var dedented = line;
      if (commonIndent.isNotEmpty && line.startsWith(commonIndent)) {
        dedented = line.substring(commonIndent.length);
      }
      if (i > 0) {
        dedented = '$baseIndent$dedented';
      }
      adjustedLines.add(dedented);
    }

    normalized = adjustedLines.join('\n');
    return normalized;
  }

  static String commonLeadingWhitespace(List<String> lines) {
    if (lines.isEmpty) return '';

    var common = NavigationLineUtils.leadingWhitespacePrefix(lines.first);
    for (var i = 1; i < lines.length; i++) {
      final leading = NavigationLineUtils.leadingWhitespacePrefix(lines[i]);
      final max =
          common.length < leading.length ? common.length : leading.length;
      var prefixLen = 0;
      while (prefixLen < max &&
          common.codeUnitAt(prefixLen) == leading.codeUnitAt(prefixLen)) {
        prefixLen++;
      }
      common = common.substring(0, prefixLen);
      if (common.isEmpty) break;
    }
    return common;
  }

  static bool fenceHasVisibleContent({
    required String text,
    required int fenceStart,
    required int openLineEnd,
    int? closeLineStart,
  }) {
    if (fenceStart < 0 || fenceStart >= text.length) return false;
    final openContentEnd =
        (openLineEnd > 0 && text.codeUnitAt(openLineEnd - 1) == 10)
            ? openLineEnd - 1
            : openLineEnd;
    final infoStart = (fenceStart + 3).clamp(0, text.length);

    if (infoStart < openContentEnd &&
        !_isRecognizedFenceInfoTail(
          text: text,
          infoStart: infoStart,
          infoEnd: openContentEnd,
        )) {
      return true;
    }

    final bodyStart = openLineEnd.clamp(0, text.length);
    final bodyEnd = (closeLineStart ?? text.length).clamp(0, text.length);
    for (var i = bodyStart; i < bodyEnd; i++) {
      final cu = text.codeUnitAt(i);
      if (cu == 10 || cu == 13) continue;
      return true;
    }
    return false;
  }

  static bool _languageUsesColonIndent(String? language) {
    switch (language) {
      case 'python':
      case 'yaml':
      case 'bash':
        return true;
      default:
        return false;
    }
  }

  static bool _isRecognizedFenceInfoTail({
    required String text,
    required int infoStart,
    required int infoEnd,
  }) {
    if (infoStart >= infoEnd) return true;

    var tokenStart = infoStart;
    while (tokenStart < infoEnd) {
      final cu = text.codeUnitAt(tokenStart);
      if (cu != 32 && cu != 9) break;
      tokenStart++;
    }
    if (tokenStart >= infoEnd) return true;

    var tokenEnd = tokenStart;
    while (tokenEnd < infoEnd) {
      final cu = text.codeUnitAt(tokenEnd);
      if (cu == 32 || cu == 9) break;
      tokenEnd++;
    }
    if (tokenStart >= tokenEnd) return true;

    final token = text.substring(tokenStart, tokenEnd).trim().toLowerCase();
    if (token.isEmpty) return true;
    if (SovereignCodeHighlighter.normalizeFenceTag(token) == null) {
      return false;
    }

    for (var i = tokenEnd; i < infoEnd; i++) {
      final cu = text.codeUnitAt(i);
      if (cu != 32 && cu != 9) return false;
    }
    return true;
  }
}
