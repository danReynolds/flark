import 'dart:async';
import 'dart:math' as math;

import 'package:flark_core/flark_core.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';

const _maximumNeutralPaintRows = 32;

/// One laid-out painter never holds more than this many UTF-16 units, so a
/// giant physical line cannot force full-block layout on the frame path.
const _fragmentUtf16Budget = 2048;

/// Rows starting below the viewport bottom plus this margin are not laid
/// out; their height is estimated until scrolling materializes them.
const _layoutOverscanPx = 400.0;

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
    required this.fragmentStart,
    required this.fragmentEnd,
    required this.leadingLength,
    this.row,
    this.neutralText,
    this.neutralUtf16Start,
  });

  final double top;
  final double height;
  final TextPainter painter;
  final FlarkSurfaceRow presentation;
  final int ordinal;

  /// The half-open range of `presentation.text` this fragment lays out.
  final int fragmentStart;
  final int fragmentEnd;

  /// Length of the leading text painted before the fragment body; nonzero
  /// only on a row's first fragment.
  final int leadingLength;

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
  int _laidOutRowCount = 0;
  int _skippedRowCount = 0;
  int _skippedFragmentCount = 0;
  double _skippedFragmentEstimate = 0;

  double get scrollOffset => _scrollOffset;

  /// Rows fully laid out in the last pass; below-fold rows are skipped.
  int get debugLaidOutRowCount => _laidOutRowCount;

  /// Rows whose layout was skipped as below the overscan budget.
  int get debugSkippedRowCount => _skippedRowCount;

  int get debugPaintedFragmentCount => _paintedRows.length;

  /// Fragments of a laid-out row whose layout was skipped as below-fold.
  int get debugSkippedFragmentCount => _skippedFragmentCount;

  /// The largest fragment any single painter holds, in UTF-16 units.
  int get debugMaxFragmentUnits => _paintedRows.fold(
    0,
    (maximum, row) => math.max(maximum, row.fragmentEnd - row.fragmentStart),
  );

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
    if (_scrollOffset != previous) {
      markNeedsPaint();
      // Scrolling toward estimated, un-laid-out rows materializes them.
      if (_skippedRowCount > 0 && _scrollOffset > previous) {
        markNeedsLayout();
      }
    }
    if (delta > 0 && _scrollOffset >= _maximumScrollOffset) {
      unawaited(_controller.nextViewportPage());
    } else if (delta < 0 && _scrollOffset <= 0) {
      unawaited(_controller.previousViewportPage());
    }
  }

  /// Rows fully below this content-space line are not laid out this pass.
  double get _layoutBudgetBottom =>
      _scrollOffset + (hasSize ? size.height : 480) + _layoutOverscanPx;

  /// The tallest reasonable estimate for one un-laid-out row: enough that
  /// the scroll range never undershoots badly, cheap enough to be a guess.
  double get _estimatedRowHeight {
    final fontSize = _textStyle.fontSize ?? 16;
    return fontSize * (_textStyle.height ?? 1.4);
  }

  void _buildVisibleLayouts() {
    _paintedRows.clear();
    _laidOutRowCount = 0;
    _skippedRowCount = 0;
    _skippedFragmentCount = 0;
    _skippedFragmentEstimate = 0;
    final maxWidth = math.max(0.0, size.width - _padding.horizontal);
    var top = _padding.top;
    final rows = _controller.rows;
    if (rows.isNotEmpty) {
      var skippedEstimate = 0.0;
      for (final row in rows) {
        if (top > _layoutBudgetBottom) {
          _skippedRowCount += 1;
          skippedEstimate += _estimatedRowHeight + 6;
          continue;
        }
        final presentation = _controller.surfaceRow(row);
        top = _emitFragments(
          presentation: presentation,
          ordinal: row.ordinal,
          top: top,
          maxWidth: maxWidth,
          row: row,
        );
        top += 6;
        _laidOutRowCount += 1;
      }
      _contentHeight =
          top + skippedEstimate + _skippedFragmentEstimate + _padding.bottom;
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
    var skippedEstimate = 0.0;
    for (var ordinal = firstLine; ordinal < lastLine; ordinal += 1) {
      final range = ranges[ordinal];
      if (top > _layoutBudgetBottom) {
        _skippedRowCount += 1;
        skippedEstimate += _estimatedRowHeight;
        continue;
      }
      sourceOffset = range.start;
      final end = range.end;
      final text = source.substring(sourceOffset, end);
      final presentation = _controller.neutralSurfaceRow(
        globalUtf16Start: _controller.visibleUtf16Start + sourceOffset,
        text: text,
        ordinal: ordinal,
      );
      top = _emitFragments(
        presentation: presentation,
        ordinal: ordinal,
        top: top,
        maxWidth: maxWidth,
        neutralText: text,
        neutralUtf16Start: _controller.visibleUtf16Start + sourceOffset,
      );
      _laidOutRowCount += 1;
    }
    _contentHeight =
        top + skippedEstimate + _skippedFragmentEstimate + _padding.bottom;
  }

  /// Lays out one presentation as one or more bounded fragments and returns
  /// the content-space bottom. A fragment boundary never splits a surrogate
  /// pair, so every painter holds valid UTF-16.
  double _emitFragments({
    required FlarkSurfaceRow presentation,
    required int ordinal,
    required double top,
    required double maxWidth,
    FlarkViewportRow? row,
    String? neutralText,
    int? neutralUtf16Start,
  }) {
    final text = presentation.text;
    var fragmentStart = 0;
    var first = true;
    while (first || fragmentStart < text.length) {
      // The layout budget applies per fragment, not only per row: one giant
      // physical line is a single row, so a row-level check alone would lay
      // out its entire length every frame.
      if (!first && top > _layoutBudgetBottom) {
        final remaining = text.length - fragmentStart;
        final skippedFragments =
            (remaining + _fragmentUtf16Budget - 1) ~/ _fragmentUtf16Budget;
        _skippedFragmentCount += skippedFragments;
        _skippedFragmentEstimate += skippedFragments * _estimatedRowHeight;
        break;
      }
      var fragmentEnd = math.min(text.length, fragmentStart + _fragmentUtf16Budget);
      if (fragmentEnd < text.length) {
        final unit = text.codeUnitAt(fragmentEnd);
        if (unit >= 0xdc00 && unit <= 0xdfff) fragmentEnd -= 1;
      }
      final painter = _layoutText(
        presentation,
        maxWidth,
        fragmentStart: fragmentStart,
        fragmentEnd: fragmentEnd,
        includeLeading: first,
      );
      final height = math.max(painter.height, painter.preferredLineHeight);
      _paintedRows.add(
        _PaintedRow(
          top: top,
          height: height,
          painter: painter,
          presentation: presentation,
          ordinal: ordinal,
          fragmentStart: fragmentStart,
          fragmentEnd: fragmentEnd,
          leadingLength: first ? presentation.leadingText.length : 0,
          row: row,
          neutralText: neutralText,
          neutralUtf16Start: neutralUtf16Start,
        ),
      );
      top += height;
      fragmentStart = fragmentEnd;
      first = false;
      if (text.isEmpty) break;
    }
    return top;
  }

  TextPainter _layoutText(
    FlarkSurfaceRow presentation,
    double maxWidth, {
    int? fragmentStart,
    int? fragmentEnd,
    bool includeLeading = true,
  }) {
    final start = fragmentStart ?? 0;
    final end = fragmentEnd ?? presentation.text.length;
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
    if (includeLeading && presentation.leadingText.isNotEmpty) {
      children.add(TextSpan(text: presentation.leadingText));
    }
    if (presentation.runs.isNotEmpty) {
      var cursor = 0;
      for (final run in presentation.runs) {
        final runEnd = cursor + run.text.length;
        final sliceStart = math.max(start, cursor);
        final sliceEnd = math.min(end, runEnd);
        if (sliceEnd > sliceStart) {
          children.add(
            TextSpan(
              text: run.text.substring(sliceStart - cursor, sliceEnd - cursor),
              style: _inlineStyle(style, run.styles),
            ),
          );
        }
        cursor = runEnd;
        if (cursor >= end) break;
      }
    } else if (end > start) {
      children.add(TextSpan(text: presentation.text.substring(start, end)));
    }
    if (children.isEmpty) {
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
    final local = (position.offset - row.leadingLength + row.fragmentStart)
        .clamp(row.fragmentStart, row.fragmentEnd)
        .clamp(0, row.presentation.text.length);
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
        final selectionStart = math.min(
          selection.baseOffset,
          selection.extentOffset,
        );
        final selectionEnd = math.max(
          selection.baseOffset,
          selection.extentOffset,
        );
        final fragmentSelectionStart = math.max(
          selectionStart,
          row.fragmentStart,
        );
        final fragmentSelectionEnd = math.min(selectionEnd, row.fragmentEnd);
        if (fragmentSelectionEnd > fragmentSelectionStart) {
          final paint = Paint()..color = _selectionColor;
          final paintedSelection = TextSelection(
            baseOffset:
                fragmentSelectionStart - row.fragmentStart + row.leadingLength,
            extentOffset:
                fragmentSelectionEnd - row.fragmentStart + row.leadingLength,
            affinity: selection.affinity,
            isDirectional: selection.isDirectional,
          );
          for (final box in row.painter.getBoxesForSelection(
            paintedSelection,
          )) {
            canvas.drawRect(box.toRect().shift(origin), paint);
          }
        }
      }
      row.painter.paint(canvas, origin);
      if (row.presentation.active && selection != null && selection.isValid) {
        final extent = selection.extentOffset;
        final caretInFragment =
            (extent >= row.fragmentStart && extent < row.fragmentEnd) ||
            (extent == row.fragmentEnd &&
                row.fragmentEnd == row.presentation.text.length);
        if (caretInFragment) {
          final caret = row.painter.getOffsetForCaret(
            TextPosition(
              offset: extent - row.fragmentStart + row.leadingLength,
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
    }
    canvas.restore();
  }
}
