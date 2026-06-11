// Host-mode live rendering: block text segmentation, the live-rendered
// text controller and render state, and the decorated chrome painter
// used when the document renders as a single projected field.

part of '../flark_projected_editable_text.dart';

List<_LiveRenderedTextSegment> _blockTextSegments({
  required int textLength,
  required int globalDisplayStart,
  required FlarkRenderBlock block,
}) {
  if (textLength <= 0) return const [];

  final boundaries = <int>{0, textLength};
  for (final run in block.inlineRuns) {
    final start = (run.displayRange.start - globalDisplayStart).clamp(
      0,
      textLength,
    );
    final end = (run.displayRange.end - globalDisplayStart).clamp(
      0,
      textLength,
    );
    if (start >= end) continue;
    boundaries
      ..add(start)
      ..add(end);
  }

  final sorted = boundaries.toList()..sort();
  final segments = <_LiveRenderedTextSegment>[];
  for (var index = 0; index < sorted.length - 1; index++) {
    final start = sorted[index];
    final end = sorted[index + 1];
    if (start >= end) continue;
    final signature = _LiveRenderedTextStyleSignature.forRange(
      globalDisplayStart + start,
      globalDisplayStart + end,
      blocks: [block],
      runs: block.inlineRuns,
    );
    if (segments.isNotEmpty && segments.last.signature == signature) {
      final previous = segments.removeLast();
      segments.add(
        _LiveRenderedTextSegment(
          start: previous.start,
          end: end,
          signature: signature,
        ),
      );
      continue;
    }
    segments.add(
      _LiveRenderedTextSegment(start: start, end: end, signature: signature),
    );
  }
  return List.unmodifiable(segments);
}

TextStyle _blockTextStyle(
  TextStyle baseStyle,
  FlarkRenderBlock block,
  FlarkMarkdownThemeData theme,
) {
  if (block.styleToken == FlarkRenderTextStyleToken.body &&
      block.codeBlock == null &&
      block.kind != FlarkMarkdownBlockKind.blockquote) {
    return baseStyle;
  }
  final signature = _LiveRenderedTextStyleSignature.forRange(
    block.displayRange.start,
    block.displayRange.end,
    blocks: [block],
    runs: const [],
  );
  return signature.resolve(baseStyle, theme);
}

bool _rangeOverlapsText(FlarkSourceRange range, String text) {
  return range.end > 0 && range.start < text.length && range.start < range.end;
}

final class _ListMarkerInfo {
  const _ListMarkerInfo.unordered() : orderedLabel = null;
  const _ListMarkerInfo.ordered(this.orderedLabel);

  final String? orderedLabel;
}

_ListMarkerInfo _listMarkerInfo(String markdown, FlarkRenderBlock block) {
  final line = _sourceLineForBlock(markdown, block);
  final ordered = _orderedListMarkerLabel(line);
  if (block.listItem?.kind == FlarkRenderListKind.ordered || ordered != null) {
    return _ListMarkerInfo.ordered(ordered ?? '1.');
  }
  return const _ListMarkerInfo.unordered();
}

String? _orderedListMarkerLabel(
  String line, {
  bool requireFollowingWhitespace = false,
}) {
  var index = _skipHorizontalWhitespace(line, 0);
  final digitStart = index;
  while (index < line.length &&
      index - digitStart < 9 &&
      _isAsciiDigit(line.codeUnitAt(index))) {
    index++;
  }
  if (index == digitStart) return null;
  if (index < line.length && _isAsciiDigit(line.codeUnitAt(index))) {
    return null;
  }
  if (index >= line.length) return null;

  final delimiter = line.codeUnitAt(index);
  if (delimiter != 0x2E && delimiter != 0x29) return null;
  index++;
  if (requireFollowingWhitespace &&
      (index >= line.length ||
          !_isHorizontalWhitespace(line.codeUnitAt(index)))) {
    return null;
  }
  return line.substring(digitStart, index);
}

int _skipHorizontalWhitespace(String text, int start) {
  var index = start;
  while (index < text.length &&
      _isHorizontalWhitespace(text.codeUnitAt(index))) {
    index++;
  }
  return index;
}

bool _isHorizontalWhitespace(int codeUnit) {
  return codeUnit == 0x20 || codeUnit == 0x09;
}

bool _isAsciiDigit(int codeUnit) {
  return codeUnit >= 0x30 && codeUnit <= 0x39;
}

String _sourceLineForBlock(String markdown, FlarkRenderBlock block) {
  if (block.sourceRange.start < 0 ||
      block.sourceRange.start >= markdown.length ||
      block.sourceRange.start >= block.sourceRange.end) {
    return '';
  }
  final lineEnd = markdown.indexOf('\n', block.sourceRange.start);
  final effectiveLineEnd = lineEnd < 0 || lineEnd > block.sourceRange.end
      ? block.sourceRange.end
      : lineEnd;
  return markdown.substring(block.sourceRange.start, effectiveLineEnd);
}

void _replaceSourceRange({
  required FlarkFlutterController controller,
  required FlarkSourceRange range,
  required String replacementText,
  required String userEvent,
  int? undoGroupId,
  FlarkSelection? selectionAfter,
}) {
  range.validate(controller.markdown.length);
  controller.applyTransaction(
    FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: range,
        replacementText: replacementText,
      ),
      selectionBefore: controller.selection,
      selectionAfter:
          selectionAfter ??
          FlarkSelection.collapsed(range.start + replacementText.length),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: userEvent,
        undoGroupId: undoGroupId,
        parseInvalidationRange: range,
        projectionInvalidationRange: range,
      ),
    ),
  );
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

String _sanitizeTableCell(String value) {
  return value.replaceAll('\n', ' ').replaceAll('|', r'\|');
}

FlarkSelection _tableCellSelectionAfterReplacement({
  required _ParsedEditableTableCell cell,
  required TextEditingValue value,
}) {
  final selection = value.selection;
  final replacementText = cell.replacementText(value.text);
  if (!selection.isValid) {
    return FlarkSelection.collapsed(cell.range.start + replacementText.length);
  }
  return FlarkSelection(
    baseOffset:
        cell.range.start +
        cell.replacementPrefix.length +
        _sourceOffsetInsideSanitizedTableCell(value.text, selection.baseOffset),
    extentOffset:
        cell.range.start +
        cell.replacementPrefix.length +
        _sourceOffsetInsideSanitizedTableCell(
          value.text,
          selection.extentOffset,
        ),
  );
}

int _sourceOffsetInsideSanitizedTableCell(String value, int localOffset) {
  final limit = localOffset.clamp(0, value.length);
  var sourceOffset = 0;
  for (var index = 0; index < limit; index++) {
    final codeUnit = value.codeUnitAt(index);
    sourceOffset += codeUnit == 124 ? 2 : 1;
  }
  return sourceOffset;
}

int _localOffsetInsideSanitizedTableCell(String value, int sourceOffset) {
  final target = sourceOffset.clamp(0, _sanitizeTableCell(value).length);
  if (target == 0) return 0;
  var consumedSource = 0;
  for (var index = 0; index < value.length; index++) {
    final codeUnit = value.codeUnitAt(index);
    consumedSource += codeUnit == 124 ? 2 : 1;
    if (target <= consumedSource) return index + 1;
  }
  return value.length;
}

final class _TableCellInputFormatter extends TextInputFormatter {
  const _TableCellInputFormatter();

  @override
  TextEditingValue formatEditUpdate(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    final text = newValue.text;
    if (!text.contains('\n') && !text.contains('\r')) return newValue;

    final normalized = StringBuffer();
    var removedCodeUnitsBeforeBase = 0;
    var removedCodeUnitsBeforeExtent = 0;
    var index = 0;
    while (index < text.length) {
      final codeUnit = text.codeUnitAt(index);
      if (codeUnit == 0x0D) {
        normalized.write(' ');
        if (index + 1 < text.length && text.codeUnitAt(index + 1) == 0x0A) {
          final lineFeedIndex = index + 1;
          if (lineFeedIndex < newValue.selection.baseOffset) {
            removedCodeUnitsBeforeBase++;
          }
          if (lineFeedIndex < newValue.selection.extentOffset) {
            removedCodeUnitsBeforeExtent++;
          }
          index += 2;
          continue;
        }
        index++;
        continue;
      }
      if (codeUnit == 0x0A) {
        normalized.write(' ');
        index++;
        continue;
      }
      normalized.writeCharCode(codeUnit);
      index++;
    }

    final selection = newValue.selection;
    final normalizedText = normalized.toString();
    return newValue.copyWith(
      text: normalizedText,
      selection: selection.isValid
          ? TextSelection(
              baseOffset: (selection.baseOffset - removedCodeUnitsBeforeBase)
                  .clamp(0, normalizedText.length),
              extentOffset:
                  (selection.extentOffset - removedCodeUnitsBeforeExtent).clamp(
                    0,
                    normalizedText.length,
                  ),
              affinity: selection.affinity,
              isDirectional: selection.isDirectional,
            )
          : selection,
      composing: TextRange.empty,
    );
  }
}

final class _FlarkLiveRenderedTextController extends TextEditingController {
  _FlarkLiveRenderedTextState renderState = _FlarkLiveRenderedTextState.empty;

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    final theme = FlarkMarkdownTheme.of(context);
    final baseStyle = style ?? DefaultTextStyle.of(context).style;
    final composingRange = withComposing && value.isComposingRangeValid
        ? value.composing
        : null;
    if (!renderState.hasRenderPlan ||
        renderState.segments.isEmpty ||
        text.isEmpty) {
      return _plainTextSpan(
        baseStyle: baseStyle,
        composingRange: composingRange,
      );
    }

    final children = <TextSpan>[];
    var cursor = 0;
    for (final segment in renderState.segments) {
      if (segment.start > cursor) {
        _appendStyledText(
          children,
          start: cursor,
          end: segment.start,
          style: baseStyle,
          composingRange: composingRange,
        );
      }
      _appendStyledText(
        children,
        start: segment.start,
        end: segment.end,
        style: segment.signature.resolve(baseStyle, theme),
        composingRange: composingRange,
      );
      cursor = segment.end;
    }
    if (cursor < text.length) {
      _appendStyledText(
        children,
        start: cursor,
        end: text.length,
        style: baseStyle,
        composingRange: composingRange,
      );
    }

    return TextSpan(style: baseStyle, children: children);
  }

  TextSpan _plainTextSpan({
    required TextStyle baseStyle,
    required TextRange? composingRange,
  }) {
    if (composingRange == null) return TextSpan(style: baseStyle, text: text);
    return TextSpan(
      style: baseStyle,
      children: [
        TextSpan(text: composingRange.textBefore(text)),
        TextSpan(
          text: composingRange.textInside(text),
          style: baseStyle.merge(
            const TextStyle(decoration: TextDecoration.underline),
          ),
        ),
        TextSpan(text: composingRange.textAfter(text)),
      ],
    );
  }

  void _appendStyledText(
    List<TextSpan> spans, {
    required int start,
    required int end,
    required TextStyle style,
    required TextRange? composingRange,
  }) {
    if (start >= end) return;
    if (composingRange == null ||
        end <= composingRange.start ||
        start >= composingRange.end) {
      spans.add(TextSpan(text: text.substring(start, end), style: style));
      return;
    }

    final composingStart = composingRange.start.clamp(start, end);
    final composingEnd = composingRange.end.clamp(start, end);
    if (start < composingStart) {
      spans.add(
        TextSpan(text: text.substring(start, composingStart), style: style),
      );
    }
    spans.add(
      TextSpan(
        text: text.substring(composingStart, composingEnd),
        style: style.merge(
          const TextStyle(decoration: TextDecoration.underline),
        ),
      ),
    );
    if (composingEnd < end) {
      spans.add(
        TextSpan(text: text.substring(composingEnd, end), style: style),
      );
    }
  }
}

final class _FlarkLiveRenderedTextState {
  const _FlarkLiveRenderedTextState({
    required this.displayText,
    required this.renderPlan,
    required this.hasRenderPlan,
    required this.segments,
  });

  factory _FlarkLiveRenderedTextState.fromRenderPlan({
    required String displayText,
    required FlarkRenderPlan renderPlan,
    required bool hasRenderPlan,
  }) {
    return _FlarkLiveRenderedTextState(
      displayText: displayText,
      renderPlan: renderPlan,
      hasRenderPlan: hasRenderPlan,
      segments: hasRenderPlan
          ? _LiveRenderedTextSegment.buildSegments(
              displayText: displayText,
              renderPlan: renderPlan,
            )
          : const [],
    );
  }

  static final empty = _FlarkLiveRenderedTextState(
    displayText: '',
    renderPlan: FlarkRenderPlan(blocks: const []),
    hasRenderPlan: false,
    segments: const [],
  );

  final String displayText;
  final FlarkRenderPlan renderPlan;
  final bool hasRenderPlan;
  final List<_LiveRenderedTextSegment> segments;
}

final class _LiveRenderedTextSegment {
  const _LiveRenderedTextSegment({
    required this.start,
    required this.end,
    required this.signature,
  });

  final int start;
  final int end;
  final _LiveRenderedTextStyleSignature signature;

  static List<_LiveRenderedTextSegment> buildSegments({
    required String displayText,
    required FlarkRenderPlan renderPlan,
  }) {
    if (displayText.isEmpty) return const [];

    final boundaries = <int>{0, displayText.length};
    final blocks = renderPlan.allBlocks
        .where(
          (block) => !_isCollapsedOrOutside(block.displayRange, displayText),
        )
        .toList();
    final runs = renderPlan.allInlineRuns
        .where((run) => !_isCollapsedOrOutside(run.displayRange, displayText))
        .toList();

    for (final block in blocks) {
      boundaries
        ..add(block.displayRange.start.clamp(0, displayText.length))
        ..add(block.displayRange.end.clamp(0, displayText.length));
    }
    for (final run in runs) {
      boundaries
        ..add(run.displayRange.start.clamp(0, displayText.length))
        ..add(run.displayRange.end.clamp(0, displayText.length));
    }

    final sortedBoundaries = boundaries.toList()..sort();
    final segments = <_LiveRenderedTextSegment>[];
    for (var index = 0; index < sortedBoundaries.length - 1; index++) {
      final start = sortedBoundaries[index];
      final end = sortedBoundaries[index + 1];
      if (start >= end) continue;
      final signature = _LiveRenderedTextStyleSignature.forRange(
        start,
        end,
        blocks: blocks,
        runs: runs,
      );
      if (segments.isNotEmpty && segments.last.signature == signature) {
        final previous = segments.removeLast();
        segments.add(
          _LiveRenderedTextSegment(
            start: previous.start,
            end: end,
            signature: signature,
          ),
        );
      } else {
        segments.add(
          _LiveRenderedTextSegment(
            start: start,
            end: end,
            signature: signature,
          ),
        );
      }
    }

    return List.unmodifiable(segments);
  }

  static bool _isCollapsedOrOutside(
    FlarkSourceRange range,
    String displayText,
  ) {
    return range.isCollapsed ||
        range.start >= displayText.length ||
        range.end <= 0;
  }
}

final class _LiveRenderedTextStyleSignature {
  const _LiveRenderedTextStyleSignature({
    this.headingLevel,
    this.codeBlock = false,
    this.blockquote = false,
    this.strong = false,
    this.emphasis = false,
    this.inlineCode = false,
    this.strikethrough = false,
    this.link = false,
  });

  final int? headingLevel;
  final bool codeBlock;
  final bool blockquote;
  final bool strong;
  final bool emphasis;
  final bool inlineCode;
  final bool strikethrough;
  final bool link;

  static _LiveRenderedTextStyleSignature forRange(
    int start,
    int end, {
    required List<FlarkRenderBlock> blocks,
    required List<FlarkRenderInlineRun> runs,
  }) {
    int? headingLevel;
    var codeBlock = false;
    var blockquote = false;
    for (final block in blocks) {
      if (!_covers(block.displayRange, start, end)) continue;
      if (block.codeBlock != null) codeBlock = true;
      if (block.kind == FlarkMarkdownBlockKind.blockquote) {
        blockquote = true;
      }
      headingLevel ??= _headingLevel(block.styleToken);
    }

    var strong = false;
    var emphasis = false;
    var inlineCode = false;
    var strikethrough = false;
    var link = false;
    for (final run in runs) {
      if (!_covers(run.displayRange, start, end)) continue;
      switch (run.styleToken) {
        case FlarkRenderTextStyleToken.strong:
          strong = true;
        case FlarkRenderTextStyleToken.emphasis:
          emphasis = true;
        case FlarkRenderTextStyleToken.inlineCode:
          inlineCode = true;
        case FlarkRenderTextStyleToken.strikethrough:
          strikethrough = true;
        case FlarkRenderTextStyleToken.link:
          link = true;
        case FlarkRenderTextStyleToken.body:
        case FlarkRenderTextStyleToken.heading1:
        case FlarkRenderTextStyleToken.heading2:
        case FlarkRenderTextStyleToken.heading3:
        case FlarkRenderTextStyleToken.heading4:
        case FlarkRenderTextStyleToken.heading5:
        case FlarkRenderTextStyleToken.heading6:
        case FlarkRenderTextStyleToken.image:
        case FlarkRenderTextStyleToken.rawHtml:
        case FlarkRenderTextStyleToken.unknown:
          break;
      }
    }

    return _LiveRenderedTextStyleSignature(
      headingLevel: headingLevel,
      codeBlock: codeBlock,
      blockquote: blockquote,
      strong: strong,
      emphasis: emphasis,
      inlineCode: inlineCode,
      strikethrough: strikethrough,
      link: link,
    );
  }

  TextStyle resolve(TextStyle baseStyle, FlarkMarkdownThemeData theme) {
    var style = baseStyle;
    if (codeBlock) {
      style = style
          .copyWith(
            color: theme.codeTextColor,
            fontFamily: 'monospace',
            height: 1.35,
          )
          .merge(theme.codeTextStyle);
    } else if (blockquote) {
      style = style
          .copyWith(color: theme.quoteTextColor)
          .merge(theme.quoteTextStyle);
    }

    if (headingLevel != null) {
      final baseSize = baseStyle.fontSize ?? 14;
      style = style
          .copyWith(
            fontSize: baseSize + (7 - headingLevel!) * 2,
            fontWeight: FontWeight.w700,
          )
          .merge(theme.headingTextStyle)
          .merge(theme.headingLevelTextStyle(headingLevel!));
    }
    if (strong) {
      style = style
          .copyWith(fontWeight: FontWeight.w700)
          .merge(theme.strongTextStyle);
    }
    if (emphasis) {
      style = style
          .copyWith(fontStyle: FontStyle.italic)
          .merge(theme.emphasisTextStyle);
    }
    if (inlineCode) {
      // No backgroundColor here: the chrome underlay paints the run's
      // highlight from layout boxes so trailing whitespace is covered and
      // the height is uniform (Flutter's span backgrounds skip line-trailing
      // whitespace and use different box geometry).
      style = style
          .copyWith(fontFamily: 'monospace')
          .merge(theme.inlineCodeTextStyle);
    }
    if (strikethrough) {
      style = style
          .copyWith(decoration: TextDecoration.lineThrough)
          .merge(theme.strikethroughTextStyle);
    }
    if (link) {
      style = style
          .copyWith(
            color: theme.linkColor,
            decoration: TextDecoration.underline,
          )
          .merge(theme.linkTextStyle);
    }
    return style;
  }

  @override
  bool operator ==(Object other) {
    return other is _LiveRenderedTextStyleSignature &&
        other.headingLevel == headingLevel &&
        other.codeBlock == codeBlock &&
        other.blockquote == blockquote &&
        other.strong == strong &&
        other.emphasis == emphasis &&
        other.inlineCode == inlineCode &&
        other.strikethrough == strikethrough &&
        other.link == link;
  }

  @override
  int get hashCode {
    return Object.hash(
      headingLevel,
      codeBlock,
      blockquote,
      strong,
      emphasis,
      inlineCode,
      strikethrough,
      link,
    );
  }

  static bool _covers(FlarkSourceRange range, int start, int end) {
    return range.start <= start && range.end >= end;
  }

  static int? _headingLevel(FlarkRenderTextStyleToken token) {
    return switch (token) {
      FlarkRenderTextStyleToken.heading1 => 1,
      FlarkRenderTextStyleToken.heading2 => 2,
      FlarkRenderTextStyleToken.heading3 => 3,
      FlarkRenderTextStyleToken.heading4 => 4,
      FlarkRenderTextStyleToken.heading5 => 5,
      FlarkRenderTextStyleToken.heading6 => 6,
      _ => null,
    };
  }
}

final class _FlarkLiveRenderedEditableChrome extends StatelessWidget {
  const _FlarkLiveRenderedEditableChrome({
    required this.textController,
    required this.scrollController,
    required this.renderPlan,
    required this.displayText,
    required this.hasRenderPlan,
    required this.style,
    required this.child,
  });

  final TextEditingController textController;
  final ScrollController scrollController;
  final FlarkRenderPlan renderPlan;
  final String displayText;
  final bool hasRenderPlan;
  final TextStyle style;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final textSpan = textController.buildTextSpan(
      context: context,
      style: style,
      withComposing: false,
    );
    return Stack(
      fit: StackFit.passthrough,
      children: [
        Positioned.fill(
          child: IgnorePointer(
            child: RepaintBoundary(
              child: CustomPaint(
                key: const Key('FlarkLiveRenderedEditableChrome'),
                painter: _FlarkLiveRenderedBlockPainter(
                  renderPlan: renderPlan,
                  displayText: displayText,
                  textSpan: textSpan,
                  textDirection: Directionality.of(context),
                  textScaler: MediaQuery.textScalerOf(context),
                  scrollController: scrollController,
                  hasRenderPlan: hasRenderPlan,
                  theme: FlarkMarkdownTheme.of(context),
                ),
              ),
            ),
          ),
        ),
        child,
      ],
    );
  }
}

final class _FlarkLiveRenderedBlockPainter extends CustomPainter {
  _FlarkLiveRenderedBlockPainter({
    required this.renderPlan,
    required this.displayText,
    required this.textSpan,
    required this.textDirection,
    required this.textScaler,
    required this.scrollController,
    required this.hasRenderPlan,
    required this.theme,
  }) : super(repaint: scrollController);

  final FlarkRenderPlan renderPlan;
  final String displayText;
  final TextSpan textSpan;
  final TextDirection textDirection;
  final TextScaler textScaler;
  final ScrollController scrollController;
  final bool hasRenderPlan;
  final FlarkMarkdownThemeData theme;

  @override
  void paint(Canvas canvas, Size size) {
    if (!hasRenderPlan || displayText.isEmpty || size.isEmpty) {
      return;
    }

    final textPainter = TextPainter(
      text: textSpan,
      textDirection: textDirection,
      textScaler: textScaler,
    )..layout(maxWidth: size.width);

    canvas.save();
    if (scrollController.hasClients) {
      canvas.translate(0, -scrollController.offset);
    }

    for (final block in renderPlan.allBlocks) {
      if (block.kind == FlarkMarkdownBlockKind.blockquote) {
        _paintBlockquote(canvas, size, textPainter, block);
      }
    }
    for (final block in renderPlan.codeBlocks) {
      _paintCodeBlock(canvas, size, textPainter, block);
    }
    for (final block in renderPlan.allBlocks) {
      for (final run in block.inlineRuns) {
        if (run.styleToken != FlarkRenderTextStyleToken.inlineCode) continue;
        flarkPaintInlineCodeRunBackground(
          canvas: canvas,
          textPainter: textPainter,
          start: run.displayRange.start.clamp(0, displayText.length),
          end: run.displayRange.end.clamp(0, displayText.length),
          color: theme.inlineCodeBackgroundColor,
        );
      }
    }

    canvas.restore();
  }

  void _paintBlockquote(
    Canvas canvas,
    Size size,
    TextPainter textPainter,
    FlarkRenderBlock block,
  ) {
    final rect = _rectForBlock(textPainter, block.displayRange, size.width);
    if (rect == null) return;
    final expanded = Rect.fromLTRB(
      0,
      rect.top - 5,
      size.width,
      rect.bottom + 5,
    );
    final background = Paint()..color = theme.quoteBackgroundColor;
    canvas.drawRect(expanded, background);
    final rail = Paint()..color = theme.quoteRailColor;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTRB(0, expanded.top, 3, expanded.bottom),
        const Radius.circular(2),
      ),
      rail,
    );
  }

  void _paintCodeBlock(
    Canvas canvas,
    Size size,
    TextPainter textPainter,
    FlarkRenderBlock block,
  ) {
    final rect = _rectForBlock(textPainter, block.displayRange, size.width);
    if (rect == null) return;
    final expanded = Rect.fromLTRB(
      0,
      rect.top - 6,
      size.width,
      rect.bottom + 6,
    );
    final background = Paint()..color = theme.codeBlockBackgroundColor;
    final border = Paint()
      ..color = theme.borderColor
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    final shape = RRect.fromRectAndRadius(expanded, const Radius.circular(6));
    canvas.drawRRect(shape, background);
    canvas.drawRRect(shape, border);
  }

  Rect? _rectForBlock(
    TextPainter textPainter,
    FlarkSourceRange range,
    double width,
  ) {
    final start = range.start.clamp(0, displayText.length);
    final end = range.end.clamp(0, displayText.length);
    if (start >= end) return null;

    final boxes = textPainter.getBoxesForSelection(
      TextSelection(baseOffset: start, extentOffset: end),
      boxHeightStyle: BoxHeightStyle.max,
    );
    if (boxes.isEmpty) return null;

    var top = boxes.first.top;
    var bottom = boxes.first.bottom;
    for (final box in boxes.skip(1)) {
      if (box.top < top) top = box.top;
      if (box.bottom > bottom) bottom = box.bottom;
    }
    return Rect.fromLTRB(0, top, width, bottom);
  }

  @override
  bool shouldRepaint(_FlarkLiveRenderedBlockPainter oldDelegate) {
    return oldDelegate.renderPlan != renderPlan ||
        oldDelegate.displayText != displayText ||
        oldDelegate.textSpan != textSpan ||
        oldDelegate.textDirection != textDirection ||
        oldDelegate.textScaler != textScaler ||
        oldDelegate.hasRenderPlan != hasRenderPlan ||
        oldDelegate.scrollController != scrollController ||
        oldDelegate.theme != theme;
  }
}

/// Paints an inline-code run's background from layout selection boxes.
///
/// This underlay is the only painter of inline-code highlights. Flutter's
/// `TextStyle.backgroundColor` skips line-trailing whitespace (so a run
/// that currently ends in a space — mid-typing `` `multi word ` `` — would
/// lose its highlight on the last character) and uses different box
/// geometry than selection boxes, so mixing the two paints mismatched
/// heights. Selection boxes include trailing whitespace and give every part
/// of the run identical geometry.
void flarkPaintInlineCodeRunBackground({
  required Canvas canvas,
  required TextPainter textPainter,
  required int start,
  required int end,
  required Color color,
}) {
  if (start >= end) return;
  final boxes = textPainter.getBoxesForSelection(
    TextSelection(baseOffset: start, extentOffset: end),
  );
  if (boxes.isEmpty) return;
  final paint = Paint()..color = color;
  for (final box in boxes) {
    canvas.drawRect(box.toRect(), paint);
  }
}

/// Underlay for per-block editables that keeps inline-code highlights
/// contiguous over trailing whitespace (see
/// [flarkPaintInlineCodeRunBackground]).
final class _FlarkLiveBlockInlineCodeChrome extends StatelessWidget {
  const _FlarkLiveBlockInlineCodeChrome({
    required this.textController,
    required this.style,
    required this.child,
  });

  final _FlarkBlockTextController textController;
  final TextStyle style;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final block = textController.block;
    final text = textController.text;
    if (block == null || text.isEmpty) return child;
    final blockDisplayStart = flarkClampedDisplayRange(
      block,
      textController.displayText,
    ).start;
    final ranges = <TextRange>[
      for (final run in block.inlineRuns)
        if (run.styleToken == FlarkRenderTextStyleToken.inlineCode)
          TextRange(
            start: (run.displayRange.start - blockDisplayStart).clamp(
              0,
              text.length,
            ),
            end: (run.displayRange.end - blockDisplayStart).clamp(
              0,
              text.length,
            ),
          ),
    ];
    if (!ranges.any((range) => range.start < range.end)) return child;
    final textSpan = textController.buildTextSpan(
      context: context,
      style: style,
      withComposing: false,
    );
    return Stack(
      fit: StackFit.passthrough,
      children: [
        Positioned.fill(
          child: IgnorePointer(
            child: RepaintBoundary(
              child: CustomPaint(
                painter: _FlarkInlineCodeRunPainter(
                  textSpan: textSpan,
                  ranges: ranges,
                  textDirection: Directionality.of(context),
                  textScaler: MediaQuery.textScalerOf(context),
                  color: FlarkMarkdownTheme.of(
                    context,
                  ).inlineCodeBackgroundColor,
                ),
              ),
            ),
          ),
        ),
        child,
      ],
    );
  }
}

final class _FlarkInlineCodeRunPainter extends CustomPainter {
  const _FlarkInlineCodeRunPainter({
    required this.textSpan,
    required this.ranges,
    required this.textDirection,
    required this.textScaler,
    required this.color,
  });

  final TextSpan textSpan;
  final List<TextRange> ranges;
  final TextDirection textDirection;
  final TextScaler textScaler;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    if (size.isEmpty) return;
    final textPainter = TextPainter(
      text: textSpan,
      textDirection: textDirection,
      textScaler: textScaler,
    )..layout(maxWidth: size.width);
    for (final range in ranges) {
      flarkPaintInlineCodeRunBackground(
        canvas: canvas,
        textPainter: textPainter,
        start: range.start,
        end: range.end,
        color: color,
      );
    }
    textPainter.dispose();
  }

  @override
  bool shouldRepaint(_FlarkInlineCodeRunPainter oldDelegate) {
    return oldDelegate.textSpan != textSpan ||
        !listEquals(oldDelegate.ranges, ranges) ||
        oldDelegate.textDirection != textDirection ||
        oldDelegate.textScaler != textScaler ||
        oldDelegate.color != color;
  }
}
