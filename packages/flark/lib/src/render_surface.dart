import 'dart:async';
import 'dart:math' as math;

import 'package:flark_core/flark_core.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'controller.dart';

const _maximumNeutralPaintRows = 32;

/// One laid-out painter normally holds at most this many UTF-16 units. Keeping
/// the tile substantially shorter than the 2 KiB source-window cap prevents a
/// partly visible wrapped fragment from asking the raster thread to visit
/// thousands of offscreen glyphs. One indivisible grapheme may exceed it.
const _fragmentUtf16Budget = 256;

/// Rows starting below the viewport bottom plus this margin are not laid
/// out; their height is estimated until scrolling materializes them.
const _layoutOverscanPx = 400.0;

/// A test-only observation emitted synchronously after the render object paints
/// one visible frame. The text is the exact presentation plan visited by that
/// paint, bounded to visible rows rather than the complete document.
final class FlarkSurfacePaintObservation {
  const FlarkSurfacePaintObservation({
    required this.revision,
    required this.viewportPageIndex,
    required this.presentation,
    required this.renderPlanHash,
  });

  final int revision;
  final int viewportPageIndex;
  final String presentation;
  final int renderPlanHash;
}

enum FlarkSurfaceAction { toggleTaskChecked }

final class FlarkSurfaceHit {
  const FlarkSurfaceHit({
    required this.globalUtf16Offset,
    required this.ordinal,
    required this.affinity,
    this.row,
    this.neutralText,
    this.neutralUtf16Start,
    this.action,
  });

  final int globalUtf16Offset;
  final int ordinal;
  final TextAffinity affinity;
  final FlarkViewportRow? row;
  final String? neutralText;
  final int? neutralUtf16Start;
  final FlarkSurfaceAction? action;
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
    this.includeEditingState = true,
    this.debugPaintObserver,
    super.key,
  });

  final FlarkEditorController controller;
  final TextStyle textStyle;
  final EdgeInsets padding;
  final Color caretColor;
  final Color selectionColor;
  final bool includeEditingState;
  final ValueChanged<FlarkSurfacePaintObservation>? debugPaintObserver;

  @override
  RenderFlarkSurface createRenderObject(BuildContext context) =>
      RenderFlarkSurface(
        controller: controller,
        textStyle: textStyle,
        padding: padding,
        caretColor: caretColor,
        selectionColor: selectionColor,
        includeEditingState: includeEditingState,
        debugPaintObserver: debugPaintObserver,
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
      ..includeEditingState = includeEditingState
      ..debugPaintObserver = debugPaintObserver
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
    required bool includeEditingState,
    this.debugPaintObserver,
    required TextDirection textDirection,
  }) : _controller = controller,
       _textStyle = textStyle,
       _padding = padding,
       _caretColor = caretColor,
       _selectionColor = selectionColor,
       _includeEditingState = includeEditingState,
       _textDirection = textDirection;

  FlarkEditorController _controller;
  TextStyle _textStyle;
  EdgeInsets _padding;
  Color _caretColor;
  Color _selectionColor;
  bool _includeEditingState;
  ValueChanged<FlarkSurfacePaintObservation>? debugPaintObserver;
  TextDirection _textDirection;
  final List<_PaintedRow> _paintedRows = [];
  double _scrollOffset = 0;
  double _contentHeight = 0;
  int _laidOutPageIndex = 0;
  int _laidOutRowCount = 0;
  int _skippedRowCount = 0;
  int _skippedFragmentCount = 0;
  double _skippedFragmentEstimate = 0;
  Map<int, SemanticsNode> _semanticRowNodes = <int, SemanticsNode>{};

  double get scrollOffset => _scrollOffset;
  double get debugContentHeight => _contentHeight;
  Size get debugSurfaceSize => size;

  /// Rows fully laid out in the last pass; below-fold rows are skipped.
  int get debugLaidOutRowCount => _laidOutRowCount;

  /// Rows whose layout was skipped as below the overscan budget.
  int get debugSkippedRowCount => _skippedRowCount;

  int get debugPaintedFragmentCount => _paintedRows.length;

  List<({int ordinal, bool neutral, int sourceStart, String text, bool active})>
  get debugPaintedPlan => _paintedRows
      .map(
        (row) => (
          ordinal: row.ordinal,
          neutral: row.row == null,
          sourceStart:
              row.neutralUtf16Start ?? row.presentation.globalUtf16Start,
          text: row.presentation.text,
          active: row.presentation.active,
        ),
      )
      .toList(growable: false);

  int get debugFragmentBudget => _fragmentUtf16Budget;

  /// Fragments of a laid-out row whose layout was skipped as below-fold.
  int get debugSkippedFragmentCount => _skippedFragmentCount;

  /// Start offsets of every laid-out fragment, in presentation-text units.
  List<int> get debugFragmentBoundaries =>
      _paintedRows.map((row) => row.fragmentStart).toList();

  /// The largest fragment any single painter holds, in UTF-16 units.
  int get debugMaxFragmentUnits => _paintedRows.fold(
    0,
    (maximum, row) => math.max(maximum, row.fragmentEnd - row.fragmentStart),
  );

  /// Content/layout identity excluding editor-only caret and selection paint.
  int get debugRenderPlanHash => Object.hashAll(
    _paintedRows.map(
      (painted) => Object.hash(
        painted.ordinal,
        painted.fragmentStart,
        painted.fragmentEnd,
        painted.leadingLength,
        painted.presentation.leadingText,
        painted.presentation.text,
        painted.presentation.kind,
        painted.presentation.headingLevel,
        painted.presentation.blockQuoteDepth,
        painted.presentation.thematicBreak,
        Object.hashAll(
          painted.presentation.runs.map(
            (run) => Object.hash(
              run.text,
              run.sourceUtf16Start,
              run.sourceUtf16End,
              run.sourceExact,
              Object.hashAllUnordered(run.styles),
            ),
          ),
        ),
        painted.painter.width,
        painted.painter.height,
      ),
    ),
  );

  FlarkEditorController get controller => _controller;
  set controller(FlarkEditorController value) {
    if (identical(value, _controller)) return;
    if (attached) _controller.removeListener(_changed);
    _controller = value;
    if (attached) _controller.addListener(_changed);
    markNeedsLayout();
    markNeedsSemanticsUpdate();
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

  bool get includeEditingState => _includeEditingState;
  set includeEditingState(bool value) {
    if (value == _includeEditingState) return;
    _includeEditingState = value;
    markNeedsLayout();
    markNeedsSemanticsUpdate();
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

  void _changed() {
    markNeedsLayout();
    markNeedsSemanticsUpdate();
  }

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
      markNeedsSemanticsUpdate();
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
      var sourceCursor = _controller.visibleUtf16Start;
      for (final row in rows) {
        final sourceRange = _controller.surfaceSourceRange(row);
        if (sourceRange.start > sourceCursor) {
          top = _emitNeutralGap(
            globalStart: sourceCursor,
            globalEnd: sourceRange.start,
            hasPrecedingRow: sourceCursor > _controller.visibleUtf16Start,
            hasFollowingRow: true,
            top: top,
            maxWidth: maxWidth,
          );
        }
        if (top > _layoutBudgetBottom) {
          _skippedRowCount += 1;
          skippedEstimate += _estimatedRowHeight + 6;
          sourceCursor = math.max(sourceCursor, sourceRange.end);
          continue;
        }
        final presentations = _controller.surfaceRowsFor(
          row,
          includeEditingState: _includeEditingState,
        );
        for (final presentation in presentations) {
          if (top > _layoutBudgetBottom) {
            _skippedRowCount += 1;
            skippedEstimate += _estimatedRowHeight + 6;
            continue;
          }
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
        sourceCursor = math.max(sourceCursor, sourceRange.end);
      }
      final visibleEnd =
          _controller.visibleUtf16Start + _controller.visibleSource.length;
      if (sourceCursor < visibleEnd) {
        top = _emitNeutralGap(
          globalStart: sourceCursor,
          globalEnd: visibleEnd,
          hasPrecedingRow: true,
          hasFollowingRow: false,
          top: top,
          maxWidth: maxWidth,
        );
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
        includeEditingState: _includeEditingState,
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

  double _emitNeutralGap({
    required int globalStart,
    required int globalEnd,
    required bool hasPrecedingRow,
    required bool hasFollowingRow,
    required double top,
    required double maxWidth,
  }) {
    final visibleStart = _controller.visibleUtf16Start;
    final source = _controller.visibleSource;
    final localStart = (globalStart - visibleStart).clamp(0, source.length);
    final localEnd = (globalEnd - visibleStart).clamp(
      localStart,
      source.length,
    );
    final lines = <({int start, int end})>[];
    var cursor = localStart;
    while (cursor < localEnd) {
      final newline = source.indexOf('\n', cursor);
      final end = newline == -1 || newline >= localEnd ? localEnd : newline + 1;
      lines.add((start: cursor, end: end));
      cursor = end;
    }

    // The outer blank lines are Markdown separators owned by the surrounding
    // semantic rows. Interior lines are editor-owned empty blocks and need a
    // caret-bearing row even though the parser intentionally omits them.
    final firstEmitted = hasPrecedingRow ? 1 : 0;
    final endEmitted = math.max(
      firstEmitted,
      lines.length - (hasFollowingRow ? 1 : 0),
    );
    for (var index = firstEmitted; index < endEmitted; index += 1) {
      final line = lines[index];
      if (top > _layoutBudgetBottom) {
        _skippedRowCount += 1;
        continue;
      }
      var ordinal = 0;
      for (var offset = 0; offset < line.start; offset += 1) {
        if (source.codeUnitAt(offset) == 0x0a) ordinal += 1;
      }
      final text = source.substring(line.start, line.end);
      final presentation = _controller.neutralSurfaceRow(
        globalUtf16Start: visibleStart + line.start,
        text: text,
        ordinal: ordinal,
        includeEditingState: _includeEditingState,
      );
      top = _emitFragments(
        presentation: presentation,
        ordinal: ordinal,
        top: top,
        maxWidth: maxWidth,
        neutralText: text,
        neutralUtf16Start: visibleStart + line.start,
      );
      _laidOutRowCount += 1;
    }
    return top;
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
      var fragmentEnd = math.min(
        text.length,
        fragmentStart + _fragmentUtf16Budget,
      );
      if (fragmentEnd < text.length) {
        // Cut on an extended-grapheme-cluster boundary, not merely between
        // surrogates: splitting a ZWJ sequence or a combining mark would
        // render one cluster as two. Policy lives in the core.
        final snapped = FlarkCoreGraphemePolicy.clusterBoundaryAtOrBefore(
          text,
          fragmentEnd,
        );
        if (snapped > fragmentStart) {
          fragmentEnd = snapped;
        } else {
          // One cluster can exceed the ordinary fragment target. Keep it
          // intact; the visible-source cap remains the hard outer bound.
          fragmentEnd = FlarkCoreGraphemePolicy.clusterBoundaryAtOrAfter(
            text,
            fragmentEnd,
          );
        }
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

  FlarkSurfaceHit? positionForOffset(
    Offset offset, {
    double minimumActionExtent = 24,
  }) {
    if (_paintedRows.isEmpty) return null;
    final contentOffset = offset + Offset(0, _scrollOffset);
    final row = _paintedRows.firstWhere(
      (candidate) => contentOffset.dy <= candidate.top + candidate.height,
      orElse: () => _paintedRows.last,
    );
    final painterPoint = Offset(
      (contentOffset.dx - _padding.left).clamp(0, row.painter.width),
      (contentOffset.dy - row.top).clamp(0, row.height),
    );
    final position = row.painter.getPositionForOffset(painterPoint);
    final local = (position.offset - row.leadingLength + row.fragmentStart)
        .clamp(row.fragmentStart, row.fragmentEnd)
        .clamp(0, row.presentation.text.length);
    final taskAction = _taskActionHitBox(row, minimumActionExtent);
    return _hitForTextOffset(
      row,
      local,
      affinity: position.affinity,
      action: taskAction?.contains(painterPoint) == true
          ? FlarkSurfaceAction.toggleTaskChecked
          : null,
    );
  }

  FlarkSurfaceHit _hitForTextOffset(
    _PaintedRow row,
    int textOffset, {
    required TextAffinity affinity,
    FlarkSurfaceAction? action,
  }) => FlarkSurfaceHit(
    globalUtf16Offset: row.presentation.sourceOffsetForTextOffset(
      textOffset,
      affinity: affinity,
    ),
    ordinal: row.ordinal,
    affinity: affinity,
    row: row.row,
    neutralText: row.neutralText,
    neutralUtf16Start: row.neutralUtf16Start,
    action: action,
  );

  Iterable<_PaintedRow> get _logicalRows sync* {
    int? previousOrdinal;
    for (final row in _paintedRows) {
      if (row.ordinal == previousOrdinal) continue;
      previousOrdinal = row.ordinal;
      yield row;
    }
  }

  ({int start, int end}) _sourceBounds(_PaintedRow row) {
    final runs = row.presentation.runs;
    if (runs.isNotEmpty) {
      return (
        start: runs.first.sourceUtf16Start,
        end: runs.last.sourceUtf16End,
      );
    }
    return (
      start: row.presentation.globalUtf16Start,
      end: row.presentation.globalUtf16Start + row.presentation.text.length,
    );
  }

  _PaintedRow? _logicalRowForSourceUtf16(int offset) {
    _PaintedRow? boundary;
    for (final row in _logicalRows) {
      final bounds = _sourceBounds(row);
      if (bounds.start < offset && offset < bounds.end) return row;
      if (offset == bounds.start || offset == bounds.end) {
        if (row.presentation.active) return row;
        boundary ??= row;
      }
    }
    return boundary;
  }

  FlarkSurfaceHit? hitForSourceUtf16(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final row = _logicalRowForSourceUtf16(offset);
    if (row == null) return null;
    return _hitForTextOffset(
      row,
      row.presentation.textOffsetForSourceOffset(offset, affinity: affinity),
      affinity: affinity,
    );
  }

  /// Moves by one rendered grapheme, never through a hidden Markdown marker.
  /// Core remains authoritative for the resulting source selection.
  FlarkSurfaceHit? adjacentCharacterHit(int offset, {required bool forward}) {
    final rows = _logicalRows.toList(growable: false);
    final current = _logicalRowForSourceUtf16(offset);
    if (current == null) return null;
    final rowIndex = rows.indexWhere((row) => row.ordinal == current.ordinal);
    if (rowIndex < 0) return null;
    final text = current.presentation.text;
    final textOffset = current.presentation.textOffsetForSourceOffset(
      offset,
      affinity: forward ? TextAffinity.downstream : TextAffinity.upstream,
    );
    if (forward && textOffset < text.length) {
      final range = FlarkCoreGraphemePolicy.nextClusterRange(text, textOffset);
      if (range == null) return null;
      return _hitForTextOffset(
        current,
        range.$2,
        affinity: TextAffinity.downstream,
      );
    }
    if (!forward && textOffset > 0) {
      final range = FlarkCoreGraphemePolicy.previousClusterRange(
        text,
        textOffset,
      );
      if (range == null) return null;
      return _hitForTextOffset(
        current,
        range.$1,
        affinity: TextAffinity.upstream,
      );
    }
    final adjacentIndex = rowIndex + (forward ? 1 : -1);
    if (adjacentIndex < 0 || adjacentIndex >= rows.length) return null;
    final adjacent = rows[adjacentIndex];
    return _hitForTextOffset(
      adjacent,
      forward ? 0 : adjacent.presentation.text.length,
      affinity: forward ? TextAffinity.downstream : TextAffinity.upstream,
    );
  }

  FlarkSurfaceHit? verticalHit(
    int offset, {
    required bool forward,
    double? preferredX,
  }) {
    final current = _localPositionForSourceUtf16(offset);
    final row = _logicalRowForSourceUtf16(offset);
    if (current == null || row == null) return null;
    return positionForOffset(
      Offset(
        preferredX ?? current.dx,
        current.dy + (forward ? 1 : -1) * row.painter.preferredLineHeight,
      ),
    );
  }

  double? localXForSourceUtf16(int offset) =>
      _localPositionForSourceUtf16(offset)?.dx;

  Rect? _taskActionBox(_PaintedRow row) {
    if (row.fragmentStart != 0 ||
        row.leadingLength == 0 ||
        row.row?.listItem?.taskChecked == null) {
      return null;
    }
    final leading = row.presentation.leadingText;
    var marker = leading.indexOf('☐');
    if (marker < 0) marker = leading.indexOf('☑');
    if (marker < 0) return null;
    final boxes = row.painter.getBoxesForSelection(
      TextSelection(baseOffset: marker, extentOffset: marker + 1),
    );
    return boxes.isEmpty ? null : boxes.first.toRect();
  }

  Rect? _taskActionHitBox(_PaintedRow row, double minimumExtent) {
    final box = _taskActionBox(row);
    if (box == null) return null;
    return Rect.fromCenter(
      center: box.center,
      width: math.max(box.width, minimumExtent),
      height: math.max(box.height, minimumExtent),
    );
  }

  Offset? debugLocalPositionForTaskCheckbox(int ordinal) {
    for (final row in _paintedRows) {
      if (row.ordinal != ordinal) continue;
      final box = _taskActionBox(row);
      if (box == null) return null;
      final result = Offset(
        _padding.left + box.center.dx,
        row.top - _scrollOffset + box.center.dy,
      );
      if (result.dx < 0 ||
          result.dy < 0 ||
          result.dx > size.width ||
          result.dy > size.height) {
        return null;
      }
      return result;
    }
    return null;
  }

  /// Returns a visible local point that hit-tests to [sourceUtf16Offset].
  ///
  /// This is a debug/integration-test inverse of [positionForOffset]. It is
  /// intentionally bounded to rows laid out by the current viewport; callers
  /// must page/scroll before asking for an offscreen source position.
  Offset? _localPositionForSourceUtf16(int sourceUtf16Offset) {
    for (final row in _paintedRows) {
      final sourceStart = row.presentation.runs.isEmpty
          ? row.presentation.globalUtf16Start
          : row.presentation.runs.first.sourceUtf16Start;
      final sourceEnd = row.presentation.runs.isEmpty
          ? row.presentation.globalUtf16Start + row.presentation.text.length
          : row.presentation.runs.last.sourceUtf16End;
      if (sourceUtf16Offset < sourceStart || sourceUtf16Offset > sourceEnd) {
        continue;
      }
      final textOffset = row.presentation.textOffsetForSourceOffset(
        sourceUtf16Offset,
      );
      final ownsOffset =
          row.fragmentStart <= textOffset &&
          (textOffset < row.fragmentEnd ||
              (textOffset == row.fragmentEnd &&
                  row.fragmentEnd == row.presentation.text.length));
      if (!ownsOffset) continue;
      final painterOffset = textOffset - row.fragmentStart + row.leadingLength;
      final caret = row.painter.getOffsetForCaret(
        TextPosition(offset: painterOffset),
        Rect.zero,
      );
      final result = Offset(
        _padding.left + caret.dx,
        row.top -
            _scrollOffset +
            caret.dy +
            row.painter.preferredLineHeight / 2,
      );
      if (result.dx < 0 ||
          result.dy < 0 ||
          result.dx > size.width ||
          result.dy > size.height) {
        return null;
      }
      return result;
    }
    return null;
  }

  Offset? debugLocalPositionForSourceUtf16(int sourceUtf16Offset) =>
      _localPositionForSourceUtf16(sourceUtf16Offset);

  @override
  void describeSemanticsConfiguration(SemanticsConfiguration config) {
    super.describeSemanticsConfiguration(config);
    config
      ..isSemanticBoundary = true
      ..explicitChildNodes = true
      ..identifier = _includeEditingState
          ? 'flark-markdown-editor'
          : 'flark-markdown-view';
  }

  String _semanticLabel(_PaintedRow row) {
    final text = row.presentation.text.trim().replaceAll('\n', ' ');
    if (row.presentation.thematicBreak) return 'Horizontal rule';
    if (row.presentation.headingLevel case final level?) {
      return text.isEmpty ? 'Heading level $level' : text;
    }
    return text.isEmpty ? 'Blank line' : text;
  }

  @override
  void assembleSemanticsNode(
    SemanticsNode node,
    SemanticsConfiguration config,
    Iterable<SemanticsNode> children,
  ) {
    final available = Map<int, SemanticsNode>.of(_semanticRowNodes);
    final next = <int, SemanticsNode>{};
    final semanticChildren = <SemanticsNode>[];
    var ordinal = 0.0;
    for (final row in _logicalRows) {
      final top = row.top - _scrollOffset;
      final bottom = top + row.height;
      if (bottom <= 0 || top >= size.height) continue;
      final rowConfig = SemanticsConfiguration()
        ..sortKey = OrdinalSortKey(ordinal++)
        ..textDirection = _textDirection
        ..label = _semanticLabel(row);
      if (row.presentation.headingLevel != null) rowConfig.isHeader = true;
      final task = row.row?.listItem?.taskChecked;
      if (task != null) {
        final checked = row.presentation.leadingText.contains('☑');
        rowConfig.isChecked = checked;
        final taskRow = row.row;
        if (_includeEditingState && taskRow != null) {
          rowConfig.onTap = () =>
              unawaited(_controller.toggleTaskChecked(taskRow));
          rowConfig.hint = checked
              ? 'Mark task incomplete'
              : 'Mark task complete';
        }
      }
      final child =
          available.remove(row.ordinal) ??
          SemanticsNode(key: ValueKey(('flark-row', row.ordinal)));
      child
        ..rect = Rect.fromLTRB(
          _padding.left,
          math.max(0, top),
          math.max(_padding.left, size.width - _padding.right),
          math.min(size.height, bottom),
        )
        ..updateWith(config: rowConfig);
      next[row.ordinal] = child;
      semanticChildren.add(child);
    }
    _semanticRowNodes = next;
    node.updateWith(
      config: config,
      childrenInInversePaintOrder: semanticChildren,
    );
  }

  @override
  void clearSemantics() {
    super.clearSemantics();
    _semanticRowNodes = <int, SemanticsNode>{};
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    final canvas = context.canvas;
    final observedRows = <String>[];
    final observedKeys = <Object>{};
    canvas.save();
    canvas.clipRect(offset & size);
    for (final row in _paintedRows) {
      final paintedTop = row.top - _scrollOffset;
      if (paintedTop + row.height < 0 || paintedTop > size.height) continue;
      final observationKey = row.row != null
          ? (
              'row',
              row.row!.ordinal,
              row.presentation.globalUtf16Start,
              row.presentation.blockQuoteDepth,
            )
          : ('neutral', row.ordinal, row.neutralUtf16Start, row.neutralText);
      if (observedKeys.add(observationKey)) {
        observedRows.add(
          '${row.presentation.leadingText}${row.presentation.text}',
        );
      }
      final origin = offset + Offset(_padding.left, paintedTop);
      if (row.presentation.thematicBreak) {
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
              affinity: selection.affinity,
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
    debugPaintObserver?.call(
      FlarkSurfacePaintObservation(
        revision: _controller.revision,
        viewportPageIndex: _controller.viewportPageIndex,
        presentation: observedRows.isEmpty
            ? '<empty>'
            : observedRows.join('\n'),
        renderPlanHash: debugRenderPlanHash,
      ),
    );
  }
}
