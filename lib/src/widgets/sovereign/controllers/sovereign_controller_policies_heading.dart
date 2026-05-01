part of 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

abstract final class _HeadingPolicy {
  static final List<_EditTransformRule> rules = <_EditTransformRule>[
    _EditTransformRule(name: 'enter-heading', priority: 34, apply: _onEnter),
  ];

  static TextEditingValue _onEnter(
    _PolicyContext context,
    TextEditingValue newValue,
  ) {
    final helpers = context.helpers;
    final oldValue = context.oldValue;
    final caret = context.intent.enterCaret;
    if (caret == null) return newValue;

    final oldText = oldValue.text;
    final newText = newValue.text;
    if (caret < 0 || caret > oldText.length) return newValue;

    if (helpers.fenceContextForCaret(
          oldText,
          caret,
          includeUnclosedEof: true,
        ) !=
        null) {
      return newValue;
    }

    final oldLine = helpers.lineIndex.lineAtOffset(caret);
    final lineStart = helpers.lineIndex.offsetAtLine(oldLine);
    final lineEndWithBreak = helpers.lineEndWithBreak(oldText, oldLine);
    final lineEnd = helpers.lineContentEnd(
      oldText,
      lineStart,
      lineEndWithBreak,
    );
    if (lineEnd <= lineStart) return newValue;

    final lineText = oldText.substring(lineStart, lineEnd);
    final heading = MarkdownMarkerGrammar.matchAtxHeading(
      lineText,
      dialect: MarkdownMarkerDialect.commonMark,
    );
    if (heading == null) return newValue;

    final markerStart = lineStart + heading.markerStartIndex;
    final markerEnd = markerStart + heading.level;
    if (caret < markerEnd) return newValue;

    var contentStart = markerEnd;
    while (contentStart < lineEnd) {
      final cu = oldText.codeUnitAt(contentStart);
      if (cu == 32 || cu == 9) {
        contentStart++;
        continue;
      }
      break;
    }
    if (contentStart < lineEnd) {
      return newValue; // Non-empty heading: normal line split (no continuation).
    }

    // Empty ATX heading: Enter exits heading mode by stripping the heading
    // marker from the originating line while preserving leading indentation.
    final newlineIndex = caret.clamp(0, newText.length);
    if (markerStart >= newlineIndex) return newValue;
    final exited = newText.replaceRange(markerStart, newlineIndex, '');
    final caretShift = newlineIndex - markerStart;
    final targetCaret = (newValue.selection.baseOffset - caretShift).clamp(
      0,
      exited.length,
    );
    return newValue.copyWith(
      text: exited,
      selection: TextSelection.collapsed(offset: targetCaret),
      composing: TextRange.empty,
    );
  }
}
