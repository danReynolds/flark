import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
import '../render_plan/render_plan.dart';
import 'flark_live_block_source_edit.dart';

final class FlarkLiveCodeFencePendingEchoDecision {
  const FlarkLiveCodeFencePendingEchoDecision({
    required this.consumed,
    required this.nextPendingText,
  });

  final bool consumed;
  final String? nextPendingText;
}

abstract final class FlarkLiveCodeFenceInputPolicy {
  static FlarkLiveBlockSourceEdit sourceEdit({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange range,
    required TextEditingValue value,
  }) {
    var replacementText = value.text;
    final replacementTextLengthForSelection = replacementText.length;

    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (_isOpeningEmptyUnclosedBodyEcho(
      context: context,
      range: range,
      value: value,
    )) {
      return FlarkLiveBlockSourceEdit(
        range: range,
        replacementText: '',
        editableRangeAfter: range,
        selectionAfter: FlarkSelection.collapsed(range.start),
      );
    }

    final closingLineStart = context?.closingLineStart;
    final typedClosingLineStart = context == null
        ? null
        : _typedBodyClosingLineStart(value: value, context: context);
    if (typedClosingLineStart != null && context != null) {
      final bodyText = _bodyTextBeforeTypedClosingLine(
        value.text,
        typedClosingLineStart,
      );
      final typedClosingLineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
        value.text,
        typedClosingLineStart,
      );
      final typedClosingLine = value.text.substring(
        typedClosingLineStart,
        typedClosingLineEnd,
      );
      final replacingExistingClose = context.closingLineEnd != null;
      final closeReplacementText = replacingExistingClose
          ? bodyText
          : _bodyTextWithTypedClosingLine(bodyText, typedClosingLine);
      final selectionAfter = replacingExistingClose
          ? (context.closingLineEndWithBreak ?? context.closingLineEnd!) -
                range.length +
                closeReplacementText.length
          : range.start + closeReplacementText.length;
      return FlarkLiveBlockSourceEdit(
        range: range,
        replacementText: closeReplacementText,
        editableRangeAfter: FlarkSourceRange(
          range.start,
          range.start + bodyText.length,
        ),
        selectionAfter: FlarkSelection.collapsed(selectionAfter),
      );
    }

    if (closingLineStart != null &&
        range.end == closingLineStart &&
        replacementText.isNotEmpty &&
        !_endsWithLineBreak(replacementText)) {
      replacementText = '$replacementText\n';
    }

    return FlarkLiveBlockSourceEdit(
      range: range,
      replacementText: replacementText,
      editableRangeAfter: FlarkSourceRange(
        range.start,
        range.start + replacementTextLengthForSelection,
      ),
      selectionAfter: _sourceSelectionAfterReplacement(
        range: range,
        localSelection: value.selection,
        replacementTextLength: replacementTextLengthForSelection,
      ),
    );
  }

  static TextEditingValue? normalizeLineBreakInsertionValue({
    required FlarkRenderBlock block,
    required String oldText,
    required TextEditingValue value,
  }) {
    if (block.codeBlock == null || oldText.isEmpty) return null;
    if (!value.text.startsWith(oldText)) return null;
    final inserted = value.text.substring(oldText.length);
    if (_isSingleLineBreak(inserted) || !_isOnlyLineBreaks(inserted)) {
      return null;
    }
    final firstBreak = _firstLineBreak(inserted);
    if (firstBreak == null) return null;
    final normalizedText = '$oldText$firstBreak';
    if (!_isCollapsedSelection(value.selection)) return null;
    final caret = value.selection.extentOffset;
    if (caret != normalizedText.length && caret != value.text.length) {
      return null;
    }
    return value.copyWith(
      text: normalizedText,
      selection: TextSelection.collapsed(
        offset: normalizedText.length,
        affinity: value.selection.affinity,
      ),
      composing: TextRange.empty,
    );
  }

  static String? displayTextAfterCompletingStandaloneOpener({
    required String oldDisplayText,
    required TextSelection oldSelection,
    required TextEditingValue newValue,
  }) {
    return valueAfterCompletingStandaloneOpener(
      oldDisplayText: oldDisplayText,
      oldSelection: oldSelection,
      newValue: newValue,
    )?.text;
  }

  static TextEditingValue? valueAfterCompletingStandaloneOpener({
    required String oldDisplayText,
    required TextSelection oldSelection,
    required TextEditingValue newValue,
  }) {
    final autoClosedText = _displayTextAfterAutoClosedEmptyFenceEcho(
      oldDisplayText: oldDisplayText,
      oldSelection: oldSelection,
      newValue: newValue,
    );
    if (autoClosedText != null) {
      return newValue.copyWith(
        text: autoClosedText,
        selection: TextSelection.collapsed(offset: autoClosedText.length),
        composing: TextRange.empty,
      );
    }

    final selection = newValue.selection;
    if (!selection.isValid || !selection.isCollapsed) return null;
    final text = newValue.text;
    final caret = selection.extentOffset;
    if (caret < 0 || caret > text.length) return null;

    final lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
      text,
      caret,
    );
    final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      text,
      lineStart,
    );
    if (caret != lineEnd) return null;
    if (lineEnd < text.length && text.codeUnitAt(lineEnd) == 0x0A) {
      return null;
    }

    final line = text.substring(lineStart, lineEnd);
    final fence = FlarkMarkdownFencedCodeScanner.fenceLine(line);
    if (fence == null) return null;
    if (_lineClosesExistingCodeFence(
      text: text,
      lineStart: lineStart,
      closingFence: fence,
    )) {
      return null;
    }

    final oldLineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
      oldDisplayText,
      caret.clamp(0, oldDisplayText.length),
    );
    final oldLineEnd = oldLineStart <= oldDisplayText.length
        ? FlarkMarkdownFencedCodeScanner.lineContentEnd(
            oldDisplayText,
            oldLineStart,
          )
        : oldDisplayText.length;
    if (oldLineStart <= oldLineEnd && oldLineEnd <= oldDisplayText.length) {
      final oldLine = oldDisplayText.substring(oldLineStart, oldLineEnd);
      final oldFence = FlarkMarkdownFencedCodeScanner.fenceLine(oldLine);
      if (oldFence != null && oldFence.infoString == null) return null;
    }

    final insertionOffset = fence.infoString == null
        ? lineEnd
        : _completedFenceMarkerEndFromOldLine(
            oldDisplayText: oldDisplayText,
            oldSelection: oldSelection,
            newLine: line,
            newLineStart: lineStart,
            fence: fence,
          );
    if (insertionOffset == null) return null;
    final nextText = text.replaceRange(insertionOffset, insertionOffset, '\n');
    return newValue.copyWith(
      text: nextText,
      selection: _shiftSelectionAfterInsertedText(
        selection,
        insertionOffset: insertionOffset,
        insertedLength: 1,
      ),
      composing: TextRange.empty,
    );
  }

  static String? markdownAfterAutoClosedStandaloneEcho({
    required String oldMarkdown,
    required TextEditingValue newValue,
  }) {
    if (!isCollapsedSelectionAt(newValue.selection, newValue.text.length)) {
      return null;
    }
    return displayTextAfterAutoClosedWholeTextEcho(
      oldDisplayText: oldMarkdown,
      newValue: newValue,
    );
  }

  static String? displayTextAfterAutoClosedWholeTextEcho({
    required String oldDisplayText,
    required TextEditingValue newValue,
  }) {
    final fence = FlarkMarkdownFencedCodeScanner.fenceLine(oldDisplayText);
    if (fence == null || fence.infoString != null) return null;
    final closingLine = _fenceMarkerText(fence);
    if (newValue.text != '$oldDisplayText\n$closingLine\n' &&
        newValue.text != '$oldDisplayText\n$closingLine') {
      return null;
    }
    return '$oldDisplayText\n';
  }

  static String? displayTextAfterAutoClosedWholeValueEcho(
    TextEditingValue value,
  ) {
    if (!isCollapsedSelectionAt(value.selection, value.text.length)) {
      return null;
    }
    final text = value.text;
    final openingLineEnd = text.indexOf('\n');
    if (openingLineEnd <= 0) return null;
    final closingLineStart = openingLineEnd + 1;
    final hasTrailingLineBreak = text.endsWith('\n');
    final closingLineEnd = hasTrailingLineBreak ? text.length - 1 : text.length;
    if (closingLineEnd <= closingLineStart) return null;
    final openingLine = text.substring(0, openingLineEnd);
    final closingLine = text.substring(closingLineStart, closingLineEnd);
    if (closingLine.contains('\n')) return null;

    final fence = FlarkMarkdownFencedCodeScanner.fenceLine(openingLine);
    if (fence == null || fence.infoString != null) return null;
    if (closingLine != _fenceMarkerText(fence)) return null;
    return '$openingLine\n';
  }

  static bool isOpeningLinePlatformEnter({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
    required String oldText,
    required TextEditingValue newValue,
  }) {
    if (block.codeBlock == null || range == null || oldText.isEmpty) {
      return false;
    }
    final openingLineRange = FlarkLiveCodeFenceInputPolicy.openingLineRange(
      markdown,
      block,
    );
    if (openingLineRange == null ||
        range.start != openingLineRange.start ||
        range.end != openingLineRange.end) {
      return false;
    }
    if (!newValue.text.startsWith(oldText)) return false;
    final inserted = newValue.text.substring(oldText.length);
    return _isOnlyLineBreaks(inserted) ||
        _isAutoClosedEmptyFenceText(oldText, newValue.text);
  }

  static int? existingBodyStartAfterOpeningLine({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
  }) {
    if (range == null || range.end >= markdown.length) return null;
    if (!_isLineBreakCodeUnit(markdown.codeUnitAt(range.end))) return null;
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null || context.openingLineStart != range.start) {
      return null;
    }
    return context.bodyStart > range.end ? context.bodyStart : null;
  }

  static bool isOpeningEmptyUnclosedNewlineEcho({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
    required String oldText,
    required TextEditingValue newValue,
  }) {
    if (block.codeBlock == null || range == null || !range.isCollapsed) {
      return false;
    }
    if (oldText.isNotEmpty) return false;
    if (!_isSingleLineBreak(newValue.text) ||
        !_isCollapsedSelection(newValue.selection)) {
      return false;
    }

    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null || context.closingLineStart != null) return false;
    return context.bodyStart == range.start &&
        context.bodyEnd(markdown) == range.start;
  }

  static bool isLanguageShortcutPlatformEcho({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
    required String oldText,
    required TextEditingValue value,
  }) {
    if (block.codeBlock == null || range == null || !range.isCollapsed) {
      return false;
    }
    if (oldText.isNotEmpty) return false;
    if (!isCollapsedSelectionAt(value.selection, value.text.length)) {
      return false;
    }
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    final language = context?.language;
    if (context == null ||
        context.infoString == null ||
        language == null ||
        language.isEmpty) {
      return false;
    }
    final bodyRange = context.bodyContentRange(markdown);
    if (!bodyRange.isCollapsed || range.start != bodyRange.start) return false;
    if (!value.text.startsWith(language)) return false;
    final inserted = value.text.substring(language.length);
    return _isOnlyLineBreaks(inserted);
  }

  static FlarkLiveBlockSourceEdit? languageShortcutEdit({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
    required String oldText,
    required TextEditingValue value,
  }) {
    if (block.codeBlock == null || range == null) return null;
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null || context.infoString != null || context.isClosed) {
      return null;
    }
    final bodyRange = context.bodyContentRange(markdown);
    if (range.start != bodyRange.start || range.end != bodyRange.end) {
      return null;
    }
    if (oldText.trim().isEmpty || oldText.contains('\n')) return null;
    if (!value.text.startsWith(oldText)) return null;
    final inserted = value.text.substring(oldText.length);
    final firstBreak = _firstLineBreak(inserted);
    if (firstBreak == null) return null;
    if (!isCollapsedSelectionAt(value.selection, value.text.length)) {
      return null;
    }

    final language = _languageShortcut(oldText);
    if (language == null) return null;

    final openingLine = markdown.substring(
      context.openingLineStart,
      context.openingLineEndWithBreak,
    );
    final openingContentEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      openingLine,
      0,
    );
    final openingBreak = openingLine.substring(openingContentEnd);
    final marker =
        '${context.openingIndent}'
        '${List.filled(context.markerLength, context.marker).join()}';
    final bodyText = _isOnlyLineBreaks(inserted)
        ? ''
        : inserted.substring(firstBreak.length);
    final replacementText = '$marker$language$openingBreak$bodyText';
    final bodyStart =
        context.openingLineStart + '$marker$language$openingBreak'.length;
    final bodyEnd = bodyStart + bodyText.length;
    return FlarkLiveBlockSourceEdit(
      range: FlarkSourceRange(context.openingLineStart, bodyRange.end),
      replacementText: replacementText,
      editableRangeAfter: FlarkSourceRange(bodyStart, bodyEnd),
      selectionAfter: FlarkSelection.collapsed(bodyEnd),
    );
  }

  static bool isTrailingLineBreakPlatformEcho({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
    required String oldText,
    required TextEditingValue value,
  }) {
    if (block.codeBlock == null || range == null || oldText.isEmpty) {
      return false;
    }
    if (!_endsWithLineBreak(oldText)) return false;
    if (!value.text.startsWith(oldText)) return false;
    final inserted = value.text.substring(oldText.length);
    if (!_isSingleLineBreak(inserted)) return false;
    if (!isCollapsedSelectionAt(value.selection, oldText.length)) {
      return false;
    }
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null) return false;
    final bodyEnd = context.bodyEnd(markdown);
    if (context.bodyStart <= bodyEnd &&
        bodyEnd <= markdown.length &&
        markdown.substring(context.bodyStart, bodyEnd) == oldText) {
      return true;
    }
    return _rangeCanEditCodeBodyText(
      markdown: markdown,
      context: context,
      range: range,
      text: oldText,
    );
  }

  static TextEditingValue? normalizePlatformLineBreakValue({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
    required String oldText,
    required TextEditingValue value,
  }) {
    if (block.codeBlock == null || range == null || oldText.isEmpty) {
      return null;
    }
    if (!value.text.startsWith(oldText)) return null;
    final inserted = value.text.substring(oldText.length);
    if (_isSingleLineBreak(inserted) || !_isOnlyLineBreaks(inserted)) {
      return null;
    }
    final firstBreak = _firstLineBreak(inserted);
    if (firstBreak == null) return null;
    final normalizedText = '$oldText$firstBreak';
    if (!_isCollapsedSelection(value.selection)) return null;
    final caret = value.selection.extentOffset;
    if (caret != normalizedText.length && caret != value.text.length) {
      return null;
    }
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null ||
        !_rangeCanEditCodeBodyText(
          markdown: markdown,
          context: context,
          range: range,
          text: oldText,
        )) {
      return null;
    }
    return value.copyWith(
      text: normalizedText,
      selection: TextSelection.collapsed(
        offset: normalizedText.length,
        affinity: value.selection.affinity,
      ),
      composing: TextRange.empty,
    );
  }

  static bool sourceTextEquals({
    required String markdown,
    required FlarkRenderBlock block,
    required String text,
  }) {
    if (block.codeBlock == null) return false;
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null) return false;
    final bodyEnd = context.bodyEnd(markdown);
    return context.bodyStart <= bodyEnd &&
        bodyEnd <= markdown.length &&
        markdown.substring(context.bodyStart, bodyEnd) == text;
  }

  static bool shouldHandleTypedClosingFence({
    required String markdown,
    required FlarkRenderBlock block,
    required TextEditingValue value,
  }) {
    if (block.codeBlock == null) return false;
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context == null) return false;
    return _typedBodyClosingLineStart(value: value, context: context) != null;
  }

  static String? pendingEchoText({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSourceRange? range,
    required String text,
  }) {
    if (block.codeBlock == null || range == null) return null;
    if (range.start < 0 ||
        range.start > range.end ||
        range.end > markdown.length ||
        markdown.substring(range.start, range.end) != text) {
      return null;
    }
    return text;
  }

  static FlarkLiveCodeFencePendingEchoDecision consumePendingEcho({
    required String? pendingText,
    required String markdown,
    required FlarkRenderBlock block,
    required TextEditingValue value,
  }) {
    final pending = pendingText;
    if (pending == null) {
      return const FlarkLiveCodeFencePendingEchoDecision(
        consumed: false,
        nextPendingText: null,
      );
    }
    if (!sourceTextEquals(markdown: markdown, block: block, text: pending)) {
      return const FlarkLiveCodeFencePendingEchoDecision(
        consumed: false,
        nextPendingText: null,
      );
    }
    if (!value.text.startsWith(pending)) {
      return FlarkLiveCodeFencePendingEchoDecision(
        consumed: false,
        nextPendingText: value.text == pending ? pending : null,
      );
    }
    final inserted = value.text.substring(pending.length);
    if (inserted.isEmpty) {
      return FlarkLiveCodeFencePendingEchoDecision(
        consumed: isCollapsedSelectionAt(value.selection, pending.length),
        nextPendingText: null,
      );
    }
    if (!_isSingleLineBreak(inserted)) {
      return FlarkLiveCodeFencePendingEchoDecision(
        consumed: false,
        nextPendingText: inserted.isNotEmpty ? null : pending,
      );
    }
    if (!_isCollapsedSelection(value.selection)) {
      return const FlarkLiveCodeFencePendingEchoDecision(
        consumed: false,
        nextPendingText: null,
      );
    }
    final caret = value.selection.extentOffset;
    if (caret != pending.length && caret != value.text.length) {
      return const FlarkLiveCodeFencePendingEchoDecision(
        consumed: false,
        nextPendingText: null,
      );
    }
    return const FlarkLiveCodeFencePendingEchoDecision(
      consumed: true,
      nextPendingText: null,
    );
  }

  static FlarkSourceRange? bodyRange(String markdown, FlarkRenderBlock block) {
    if (block.sourceRange.start < 0 ||
        block.sourceRange.end > markdown.length) {
      return null;
    }
    final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    );
    if (context != null) {
      return context.bodyContentRange(markdown);
    }

    final openerEnd = markdown.indexOf('\n', block.sourceRange.start);
    if (openerEnd < 0 || openerEnd >= block.sourceRange.end) return null;
    final bodyStart = openerEnd + 1;
    final closerStart = markdown.lastIndexOf('\n', block.sourceRange.end - 1);
    final bodyEnd = closerStart > bodyStart
        ? closerStart
        : block.sourceRange.end;
    return FlarkSourceRange(bodyStart, bodyEnd).validate(markdown.length);
  }

  static String copyText(String markdown, FlarkRenderBlock block) {
    final range = bodyRange(markdown, block);
    if (range == null) return '';
    return markdown.substring(range.start, range.end);
  }

  static FlarkSourceRange? openingLineRange(
    String markdown,
    FlarkRenderBlock block,
  ) {
    if (block.sourceRange.start < 0 ||
        block.sourceRange.start > markdown.length) {
      return null;
    }
    final lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
      markdown,
      block.sourceRange.start,
    );
    if (lineStart != block.sourceRange.start) return null;
    final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      markdown,
      lineStart,
    );
    return FlarkSourceRange(lineStart, lineEnd).validate(markdown.length);
  }

  static bool selectionInOpeningLine({
    required String markdown,
    required FlarkRenderBlock block,
    required FlarkSelection selection,
  }) {
    if (block.codeBlock == null) return false;
    final openingLineRange = FlarkLiveCodeFenceInputPolicy.openingLineRange(
      markdown,
      block,
    );
    if (openingLineRange == null) return false;
    return selection.start >= openingLineRange.start &&
        selection.end <= openingLineRange.end;
  }

  static String? languageFromSource(String markdown, FlarkRenderBlock block) {
    if (block.sourceRange.start < 0 ||
        block.sourceRange.start >= markdown.length ||
        block.sourceRange.end > markdown.length) {
      return null;
    }
    return FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
      markdown,
      block.sourceRange.start,
    )?.language;
  }

  static bool isCollapsedSelectionAt(TextSelection selection, int offset) {
    return _isCollapsedSelection(selection) && selection.extentOffset == offset;
  }
}

FlarkSelection _sourceSelectionAfterReplacement({
  required FlarkSourceRange range,
  required TextSelection localSelection,
  required int replacementTextLength,
}) {
  if (!localSelection.isValid) {
    return FlarkSelection.collapsed(range.start + replacementTextLength);
  }
  return FlarkSelection(
    baseOffset:
        range.start + localSelection.baseOffset.clamp(0, replacementTextLength),
    extentOffset:
        range.start +
        localSelection.extentOffset.clamp(0, replacementTextLength),
  );
}

bool _isOpeningEmptyUnclosedBodyEcho({
  required FlarkMarkdownFencedCodeContext? context,
  required FlarkSourceRange range,
  required TextEditingValue value,
}) {
  if (context == null || context.closingLineStart != null) return false;
  if (!range.isCollapsed || range.start != context.bodyStart) return false;
  return _isSingleLineBreak(value.text) &&
      FlarkLiveCodeFenceInputPolicy.isCollapsedSelectionAt(value.selection, 0);
}

int? _completedFenceMarkerEndFromOldLine({
  required String oldDisplayText,
  required TextSelection oldSelection,
  required String newLine,
  required int newLineStart,
  required FlarkMarkdownFenceLine fence,
}) {
  if (!oldSelection.isValid || !oldSelection.isCollapsed) return null;
  final oldCaret = oldSelection.extentOffset.clamp(0, oldDisplayText.length);
  final oldLineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    oldDisplayText,
    oldCaret,
  );
  final oldLineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
    oldDisplayText,
    oldLineStart,
  );
  if (oldCaret != oldLineEnd) return null;
  final oldLine = oldDisplayText.substring(oldLineStart, oldLineEnd);
  if (!_lineEndsWithIncompleteMatchingFenceMarker(oldLine, fence)) return null;
  final markerStart = _fenceMarkerStart(newLine);
  if (markerStart == null) return null;
  return newLineStart + markerStart + fence.markerLength;
}

bool _lineEndsWithIncompleteMatchingFenceMarker(
  String oldLine,
  FlarkMarkdownFenceLine fence,
) {
  if (oldLine.isEmpty) return true;
  final markerStart = _fenceMarkerStart(oldLine);
  if (markerStart == null) return false;
  final expectedPrefix = oldLine.substring(0, markerStart);
  if (expectedPrefix != fence.indent) return false;

  var markerEnd = markerStart;
  while (markerEnd < oldLine.length &&
      oldLine.substring(markerEnd, markerEnd + 1) == fence.marker) {
    markerEnd += 1;
  }
  return markerEnd == oldLine.length &&
      markerEnd - markerStart > 0 &&
      markerEnd - markerStart < 3;
}

int? _fenceMarkerStart(String line) {
  var index = 0;
  while (index < line.length && index < 3) {
    final codeUnit = line.codeUnitAt(index);
    if (codeUnit != 32 && codeUnit != 9) break;
    index++;
  }
  if (index >= line.length) return null;
  final codeUnit = line.codeUnitAt(index);
  if (codeUnit != 96 && codeUnit != 126) return null;
  return index;
}

TextSelection _shiftSelectionAfterInsertedText(
  TextSelection selection, {
  required int insertionOffset,
  required int insertedLength,
}) {
  int shiftOffset(int offset) {
    if (offset < insertionOffset) return offset;
    return offset + insertedLength;
  }

  return TextSelection(
    baseOffset: shiftOffset(selection.baseOffset),
    extentOffset: shiftOffset(selection.extentOffset),
    affinity: selection.affinity,
    isDirectional: selection.isDirectional,
  );
}

String? _displayTextAfterAutoClosedEmptyFenceEcho({
  required String oldDisplayText,
  required TextSelection oldSelection,
  required TextEditingValue newValue,
}) {
  final wholeTextEcho =
      FlarkLiveCodeFenceInputPolicy.displayTextAfterAutoClosedWholeTextEcho(
        oldDisplayText: oldDisplayText,
        newValue: newValue,
      );
  if (wholeTextEcho != null) return wholeTextEcho;

  if (!oldSelection.isValid || !oldSelection.isCollapsed) return null;
  final selection = newValue.selection;
  if (!selection.isValid || !selection.isCollapsed) return null;

  final oldLineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
    oldDisplayText,
    FlarkMarkdownFencedCodeScanner.lineStartForOffset(
      oldDisplayText,
      oldSelection.extentOffset.clamp(0, oldDisplayText.length),
    ),
  );
  final oldLineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    oldDisplayText,
    oldSelection.extentOffset.clamp(0, oldDisplayText.length),
  );
  if (oldSelection.extentOffset != oldLineEnd) return null;
  if (oldLineEnd < oldDisplayText.length &&
      oldDisplayText.codeUnitAt(oldLineEnd) == 0x0A) {
    return null;
  }

  final oldLine = oldDisplayText.substring(oldLineStart, oldLineEnd);
  final fence = FlarkMarkdownFencedCodeScanner.fenceLine(oldLine);
  if (fence == null || fence.infoString != null) return null;
  if (_lineClosesExistingCodeFence(
    text: oldDisplayText,
    lineStart: oldLineStart,
    closingFence: fence,
  )) {
    return null;
  }

  final closingLine = _fenceMarkerText(fence);
  final autoClosedWithBreak = oldDisplayText.replaceRange(
    oldLineEnd,
    oldLineEnd,
    '\n$closingLine\n',
  );
  final autoClosedWithoutBreak = oldDisplayText.replaceRange(
    oldLineEnd,
    oldLineEnd,
    '\n$closingLine',
  );
  if (newValue.text != autoClosedWithBreak &&
      newValue.text != autoClosedWithoutBreak) {
    return null;
  }
  return oldDisplayText.replaceRange(oldLineEnd, oldLineEnd, '\n');
}

int? _typedBodyClosingLineStart({
  required TextEditingValue value,
  required FlarkMarkdownFencedCodeContext context,
}) {
  final selection = value.selection;
  if (!selection.isValid || !selection.isCollapsed) return null;
  final caret = selection.extentOffset;
  if (caret < 0 || caret > value.text.length) return null;
  final lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
    value.text,
    caret,
  );
  final lineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
    value.text,
    lineStart,
  );
  if (caret != lineEnd) return null;
  final line = value.text.substring(lineStart, lineEnd);
  final fence = FlarkMarkdownFencedCodeScanner.fenceLine(line);
  if (fence == null) return null;
  if (fence.closes(context)) return lineStart;
  return null;
}

String _bodyTextBeforeTypedClosingLine(String text, int closingLineStart) {
  var end = closingLineStart.clamp(0, text.length);
  if (end > 0 && text.codeUnitAt(end - 1) == 0x0A) end--;
  if (end > 0 && text.codeUnitAt(end - 1) == 0x0D) end--;
  return text.substring(0, end);
}

String _bodyTextWithTypedClosingLine(String bodyText, String closingLine) {
  if (bodyText.isEmpty) return closingLine;
  return '$bodyText\n$closingLine';
}

bool _rangeCanEditCodeBodyText({
  required String markdown,
  required FlarkMarkdownFencedCodeContext context,
  required FlarkSourceRange range,
  required String text,
}) {
  final bodyEnd = context.bodyEnd(markdown);
  return range.start == context.bodyStart &&
      range.end <= bodyEnd &&
      range.start + text.length <= bodyEnd;
}

String? _languageShortcut(String text) {
  final normalized = text.trim().toLowerCase();
  if (normalized.isEmpty || normalized != text.trim()) return null;
  return switch (normalized) {
    'plain' || 'plaintext' || 'text' => 'text',
    'dart' => 'dart',
    'md' || 'markdown' => 'markdown',
    'json' => 'json',
    'yaml' || 'yml' => 'yaml',
    'sql' => 'sql',
    'javascript' || 'js' => 'javascript',
    'typescript' || 'ts' => 'typescript',
    'python' || 'py' => 'python',
    'rust' || 'rs' => 'rust',
    'swift' => 'swift',
    'kotlin' || 'kt' => 'kotlin',
    'shell' || 'sh' || 'bash' || 'zsh' => 'shell',
    _ => null,
  };
}

String? _firstLineBreak(String text) {
  if (text.isEmpty) return null;
  if (text.startsWith('\r\n')) return '\r\n';
  final first = text.codeUnitAt(0);
  if (first == 0x0A) return '\n';
  if (first == 0x0D) return '\r';
  return null;
}

bool _endsWithLineBreak(String text) {
  if (text.isEmpty) return false;
  final codeUnit = text.codeUnitAt(text.length - 1);
  return codeUnit == 0x0A || codeUnit == 0x0D;
}

bool _isSingleLineBreak(String text) {
  return text == '\n' || text == '\r\n';
}

bool _isOnlyLineBreaks(String text) {
  if (text.isEmpty) return false;
  for (final codeUnit in text.codeUnits) {
    if (codeUnit != 0x0A && codeUnit != 0x0D) return false;
  }
  return true;
}

bool _isCollapsedSelection(TextSelection selection) {
  return selection.isValid && selection.isCollapsed;
}

bool _isAutoClosedEmptyFenceText(String oldText, String newText) {
  final fence = FlarkMarkdownFencedCodeScanner.fenceLine(oldText);
  if (fence == null || fence.infoString != null) return false;
  final closingLine = _fenceMarkerText(fence);
  return newText == '$oldText\n$closingLine\n' ||
      newText == '$oldText\n$closingLine';
}

bool _lineClosesExistingCodeFence({
  required String text,
  required int lineStart,
  required FlarkMarkdownFenceLine closingFence,
}) {
  FlarkMarkdownFenceLine? openFence;
  var scanLineStart = 0;
  while (scanLineStart < lineStart && scanLineStart < text.length) {
    final scanLineEnd = FlarkMarkdownFencedCodeScanner.lineContentEnd(
      text,
      scanLineStart,
    );
    final line = text.substring(scanLineStart, scanLineEnd);
    final fence = FlarkMarkdownFencedCodeScanner.fenceLine(line);
    if (fence != null) {
      if (openFence == null) {
        openFence = fence;
      } else if (_fenceLineCloses(openFence, fence)) {
        openFence = null;
      }
    }

    final next = FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
      text,
      scanLineStart,
    );
    if (next <= scanLineStart) break;
    scanLineStart = next;
  }

  return openFence != null && _fenceLineCloses(openFence, closingFence);
}

bool _fenceLineCloses(
  FlarkMarkdownFenceLine openFence,
  FlarkMarkdownFenceLine candidate,
) {
  return candidate.canClose &&
      candidate.marker == openFence.marker &&
      candidate.markerLength >= openFence.markerLength;
}

String _fenceMarkerText(FlarkMarkdownFenceLine fence) {
  return fence.indent + List.filled(fence.markerLength, fence.marker).join();
}

bool _isLineBreakCodeUnit(int codeUnit) {
  return codeUnit == 0x0A || codeUnit == 0x0D;
}
