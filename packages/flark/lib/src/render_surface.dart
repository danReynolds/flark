import 'dart:async';
import 'dart:math' as math;

import 'package:flark_core/flark_core.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';

const _maximumNeutralPaintRows = 32;

final class FlarkSurfaceHit {
  const FlarkSurfaceHit({
    required this.globalUtf16Offset,
    required this.ordinal,
    this.row,
    this.neutralText,
    this.neutralUtf16Start,
  });

  final int globalUtf16Offset;
  final int ordinal;
  final FlarkViewportRow? row;
  final String? neutralText;
  final int? neutralUtf16Start;
}

final class _PaintedRow {
  const _PaintedRow({
    required this.top,
    required this.height,
    required this.painter,
    required this.presentation,
    required this.ordinal,
    this.row,
    this.neutralText,
    this.neutralUtf16Start,
  });

  final double top;
  final double height;
  final TextPainter painter;
  final FlarkSurfaceRow presentation;
  final int ordinal;
  final FlarkViewportRow? row;
  final String? neutralText;
  final int? neutralUtf16Start;
}

final class FlarkRenderSurfaceWidget extends LeafRenderObjectWidget {
  const FlarkRenderSurfaceWidget({
    required this.controller,
    required this.textStyle,
    required this.padding,
    required this.caretColor,
    required this.selectionColor,
    super.key,
  });

  final FlarkEditorController controller;
  final TextStyle textStyle;
  final EdgeInsets padding;
  final Color caretColor;
  final Color selectionColor;

  @override
  RenderFlarkSurface createRenderObject(BuildContext context) =>
      RenderFlarkSurface(
        controller: controller,
        textStyle: textStyle,
        padding: padding,
        caretColor: caretColor,
        selectionColor: selectionColor,
        textDirection: Directionality.of(context),
      );

  @override
  void updateRenderObject(
    BuildContext context,
    RenderFlarkSurface renderObject,
  ) {
    renderObject
      ..controller = controller
      ..textStyle = textStyle
      ..padding = padding
      ..caretColor = caretColor
      ..selectionColor = selectionColor
      ..textDirection = Directionality.of(context);
  }
}

final class RenderFlarkSurface extends RenderBox {
  RenderFlarkSurface({
    required FlarkEditorController controller,
    required TextStyle textStyle,
    required EdgeInsets padding,
    required Color caretColor,
    required Color selectionColor,
    required TextDirection textDirection,
  }) : _controller = controller,
       _textStyle = textStyle,
       _padding = padding,
       _caretColor = caretColor,
       _selectionColor = selectionColor,
       _textDirection = textDirection;

  FlarkEditorController _controller;
  TextStyle _textStyle;
  EdgeInsets _padding;
  Color _caretColor;
  Color _selectionColor;
  TextDirection _textDirection;
  final List<_PaintedRow> _paintedRows = [];
  double _scrollOffset = 0;
  double _contentHeight = 0;
  int _laidOutPageIndex = 0;

  double get scrollOffset => _scrollOffset;

  FlarkEditorController get controller => _controller;
  set controller(FlarkEditorController value) {
    if (identical(value, _controller)) return;
    if (attached) _controller.removeListener(_changed);
    _controller = value;
    if (attached) _controller.addListener(_changed);
    markNeedsLayout();
  }

  TextStyle get textStyle => _textStyle;
  set textStyle(TextStyle value) {
    if (value == _textStyle) return;
    _textStyle = value;
    markNeedsLayout();
  }

  EdgeInsets get padding => _padding;
  set padding(EdgeInsets value) {
    if (value == _padding) return;
    _padding = value;
    markNeedsLayout();
  }

  Color get caretColor => _caretColor;
  set caretColor(Color value) {
    if (value == _caretColor) return;
    _caretColor = value;
    markNeedsPaint();
  }

  Color get selectionColor => _selectionColor;
  set selectionColor(Color value) {
    if (value == _selectionColor) return;
    _selectionColor = value;
    markNeedsPaint();
  }

  TextDirection get textDirection => _textDirection;
  set textDirection(TextDirection value) {
    if (value == _textDirection) return;
    _textDirection = value;
    markNeedsLayout();
  }

  @override
  void attach(PipelineOwner owner) {
    super.attach(owner);
    _controller.addListener(_changed);
  }

  @override
  void detach() {
    _controller.removeListener(_changed);
    super.detach();
  }

  void _changed() => markNeedsLayout();

  @override
  bool hitTestSelf(Offset position) => true;

  @override
  Size computeDryLayout(BoxConstraints constraints) => constraints.constrain(
    Size(
      constraints.hasBoundedWidth ? constraints.maxWidth : 640,
      constraints.hasBoundedHeight ? constraints.maxHeight : 480,
    ),
  );

  @override
  void performLayout() {
    size = computeDryLayout(constraints);
    final previousPage = _laidOutPageIndex;
    _buildVisibleLayouts();
    _laidOutPageIndex = _controller.viewportPageIndex;
    if (_laidOutPageIndex > previousPage) {
      _scrollOffset = 0;
    } else if (_laidOutPageIndex < previousPage) {
      _scrollOffset = _maximumScrollOffset;
    } else {
      _scrollOffset = _scrollOffset.clamp(0, _maximumScrollOffset);
    }
  }

  double get _maximumScrollOffset =>
      math.max(0, _contentHeight - size.height + _padding.bottom);

  void scrollBy(double delta) {
    if (delta == 0 || !hasSize) return;
    final previous = _scrollOffset;
    _scrollOffset = (_scrollOffset + delta).clamp(0, _maximumScrollOffset);
    if (_scrollOffset != previous) markNeedsPaint();
    if (delta > 0 && _scrollOffset >= _maximumScrollOffset) {
      unawaited(_controller.nextViewportPage());
    } else if (delta < 0 && _scrollOffset <= 0) {
      unawaited(_controller.previousViewportPage());
    }
  }

  void _buildVisibleLayouts() {
    _paintedRows.clear();
    final maxWidth = math.max(0.0, size.width - _padding.horizontal);
    var top = _padding.top;
    final rows = _controller.rows;
    if (rows.isNotEmpty) {
      for (final row in rows) {
        final presentation = _controller.surfaceRow(row);
        final painter = _layoutText(presentation, maxWidth);
        final height = math.max(painter.height, painter.preferredLineHeight);
        _paintedRows.add(
          _PaintedRow(
            top: top,
            height: height,
            painter: painter,
            presentation: presentation,
            ordinal: row.ordinal,
            row: row,
          ),
        );
        top += height + 6;
      }
      _contentHeight = top + _padding.bottom;
      return;
    }

    final source = _controller.visibleSource;
    final ranges = <({int start, int end})>[];
    var sourceOffset = 0;
    while (sourceOffset <= source.length) {
      final newline = source.indexOf('\n', sourceOffset);
      final end = newline == -1 ? source.length : newline + 1;
      ranges.add((start: sourceOffset, end: end));
      if (newline == -1) break;
      sourceOffset = end;
    }
    final caret =
        (_controller.globalCaretOffset - _controller.visibleUtf16Start).clamp(
          0,
          source.length,
        );
    var activeLine = ranges.length - 1;
    for (var index = 0; index < ranges.length; index += 1) {
      if (caret < ranges[index].end ||
          (caret == source.length && index == ranges.length - 1)) {
        activeLine = index;
        break;
      }
    }
    final maximumStart = math.max(0, ranges.length - _maximumNeutralPaintRows);
    final firstLine = (activeLine - _maximumNeutralPaintRows ~/ 4).clamp(
      0,
      maximumStart,
    );
    final lastLine = math.min(
      ranges.length,
      firstLine + _maximumNeutralPaintRows,
    );
    for (var ordinal = firstLine; ordinal < lastLine; ordinal += 1) {
      final range = ranges[ordinal];
      sourceOffset = range.start;
      final end = range.end;
      final text = source.substring(sourceOffset, end);
      final presentation = _controller.neutralSurfaceRow(
        globalUtf16Start: _controller.visibleUtf16Start + sourceOffset,
        text: text,
        ordinal: ordinal,
      );
      final painter = _layoutText(presentation, maxWidth);
      final height = math.max(painter.height, painter.preferredLineHeight);
      _paintedRows.add(
        _PaintedRow(
          top: top,
          height: height,
          painter: painter,
          presentation: presentation,
          ordinal: ordinal,
          neutralText: text,
          neutralUtf16Start: _controller.visibleUtf16Start + sourceOffset,
        ),
      );
      top += height;
    }
    _contentHeight = top + _padding.bottom;
  }

  TextPainter _layoutText(FlarkSurfaceRow presentation, double maxWidth) {
    var style = switch (presentation.kind) {
      12 => _textStyle.copyWith(
        fontSize:
            (_textStyle.fontSize ?? 16) *
            switch (presentation.headingLevel) {
              1 => 1.65,
              2 => 1.45,
              3 => 1.30,
              4 => 1.18,
              5 => 1.08,
              _ => 1.0,
            },
        fontWeight:
            presentation.headingLevel != null && presentation.headingLevel! <= 3
            ? FontWeight.w700
            : FontWeight.w600,
        height: 1.25,
      ),
      6 || 7 => _textStyle.copyWith(fontFamily: 'Menlo', height: 1.45),
      _ => _textStyle,
    };
    if (presentation.blockQuoteDepth != null) {
      style = style.copyWith(fontStyle: FontStyle.italic, height: 1.4);
    }
    final children = <InlineSpan>[];
    if (presentation.leadingText.isNotEmpty) {
      children.add(TextSpan(text: presentation.leadingText));
    }
    if (presentation.runs.isNotEmpty) {
      for (final run in presentation.runs) {
        children.add(
          TextSpan(text: run.text, style: _inlineStyle(style, run.styles)),
        );
      }
    } else if (presentation.text.isNotEmpty) {
      children.add(TextSpan(text: presentation.text));
    }
    if (presentation.leadingText.isEmpty && presentation.text.isEmpty) {
      children.add(const TextSpan(text: ' '));
    }
    return TextPainter(
      text: TextSpan(style: style, children: children),
      textDirection: _textDirection,
    )..layout(maxWidth: maxWidth);
  }

  TextStyle _inlineStyle(TextStyle base, Set<FlarkSurfaceInlineStyle> styles) {
    var result = base;
    if (styles.contains(FlarkSurfaceInlineStyle.emphasis)) {
      result = result.copyWith(fontStyle: FontStyle.italic);
    }
    if (styles.contains(FlarkSurfaceInlineStyle.strong)) {
      result = result.copyWith(fontWeight: FontWeight.w700);
    }
    if (styles.contains(FlarkSurfaceInlineStyle.code)) {
      result = result.copyWith(fontFamily: 'Menlo');
    }
    final decorations = <TextDecoration>[
      if (styles.contains(FlarkSurfaceInlineStyle.strikethrough))
        TextDecoration.lineThrough,
      if (styles.contains(FlarkSurfaceInlineStyle.link))
        TextDecoration.underline,
    ];
    if (decorations.isNotEmpty) {
      result = result.copyWith(decoration: TextDecoration.combine(decorations));
    }
    return result;
  }

  FlarkSurfaceHit? positionForOffset(Offset offset) {
    if (_paintedRows.isEmpty) return null;
    final contentOffset = offset + Offset(0, _scrollOffset);
    final row = _paintedRows.firstWhere(
      (candidate) => contentOffset.dy <= candidate.top + candidate.height,
      orElse: () => _paintedRows.last,
    );
    final position = row.painter.getPositionForOffset(
      Offset(
        (contentOffset.dx - _padding.left).clamp(0, row.painter.width),
        (contentOffset.dy - row.top).clamp(0, row.height),
      ),
    );
    final local = (position.offset - row.presentation.leadingText.length).clamp(
      0,
      row.presentation.text.length,
    );
    return FlarkSurfaceHit(
      globalUtf16Offset: row.presentation.sourceOffsetForTextOffset(local),
      ordinal: row.ordinal,
      row: row.row,
      neutralText: row.neutralText,
      neutralUtf16Start: row.neutralUtf16Start,
    );
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    final canvas = context.canvas;
    canvas.save();
    canvas.clipRect(offset & size);
    for (final row in _paintedRows) {
      final paintedTop = row.top - _scrollOffset;
      if (paintedTop + row.height < 0 || paintedTop > size.height) continue;
      final origin = offset + Offset(_padding.left, paintedTop);
      if (row.presentation.thematicBreak && !row.presentation.active) {
        final lineY = origin.dy + row.height / 2;
        canvas.drawLine(
          Offset(origin.dx, lineY),
          Offset(offset.dx + size.width - _padding.right, lineY),
          Paint()
            ..color = _textStyle.color ?? const Color(0xff808080)
            ..strokeWidth = 1,
        );
        continue;
      }
      final selection = row.presentation.selection;
      if (selection != null && selection.isValid && !selection.isCollapsed) {
        final paint = Paint()..color = _selectionColor;
        final leadingLength = row.presentation.leadingText.length;
        final paintedSelection = TextSelection(
          baseOffset: selection.baseOffset + leadingLength,
          extentOffset: selection.extentOffset + leadingLength,
          affinity: selection.affinity,
          isDirectional: selection.isDirectional,
        );
        for (final box in row.painter.getBoxesForSelection(paintedSelection)) {
          canvas.drawRect(box.toRect().shift(origin), paint);
        }
      }
      row.painter.paint(canvas, origin);
      if (row.presentation.active && selection != null && selection.isValid) {
        final caret = row.painter.getOffsetForCaret(
          TextPosition(
            offset:
                selection.extentOffset + row.presentation.leadingText.length,
          ),
          Rect.zero,
        );
        canvas.drawRect(
          Rect.fromLTWH(
            origin.dx + caret.dx,
            origin.dy + caret.dy,
            1.5,
            row.painter.preferredLineHeight,
          ),
          Paint()..color = _caretColor,
        );
      }
    }
    canvas.restore();
  }
}
