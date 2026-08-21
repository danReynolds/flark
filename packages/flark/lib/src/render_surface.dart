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

/// A fragment target may be extended to the end of one visual line. This
/// probe ceiling keeps that correction bounded even on an unusually wide
/// surface; one indivisible grapheme may still exceed it.
const _fragmentVisualLineProbeLimit = 2048;

/// Rows starting below the viewport bottom plus this margin are not laid
/// out; their height is estimated until scrolling materializes them.
const _layoutOverscanPx = 400.0;

/// A test-only observation emitted synchronously after the render object paints
/// one visible frame. The text is the exact presentation plan visited by that
/// paint, bounded to visible rows rather than the complete document.
final class FlarkSurfacePaintObservation {
  const FlarkSurfacePaintObservation({
    required this.revision,
    required this.sourceGeneration,
    required this.viewportPageIndex,
    required this.visibleUtf16Start,
    required this.visibleUtf16Length,
    required this.scrollOffset,
    required this.presentation,
    required this.renderPlanHash,
    required this.visualStateHash,
    required this.rows,
    required this.selectionRects,
    required this.caretRect,
    required this.caretSourceUtf16,
    required this.caretDisplayUtf16,
    required this.visibleSource,
    required this.canonicalSelectionBaseUtf16,
    required this.canonicalSelectionExtentUtf16,
    required this.canonicalSelectionAffinity,
    required this.canonicalSelectionIsDirectional,
    required this.composingSourceUtf16Start,
    required this.composingSourceUtf16End,
  });

  final int revision;

  /// Controller source generation represented by this exact paint.
  ///
  /// Unlike [revision], this advances synchronously for an optimistic edit
  /// before the native actor acknowledges its corresponding source revision.
  final int sourceGeneration;
  final int viewportPageIndex;
  final int visibleUtf16Start;
  final int visibleUtf16Length;
  final double scrollOffset;
  final String presentation;
  final int renderPlanHash;
  final int visualStateHash;
  final List<FlarkSurfacePaintRowObservation> rows;
  final List<Rect> selectionRects;
  final Rect? caretRect;

  /// The authoritative source offset represented by [caretRect]. This makes
  /// caret ownership testable without inferring it from coincident geometry.
  final int? caretSourceUtf16;
  final int? caretDisplayUtf16;
  final String visibleSource;
  final int canonicalSelectionBaseUtf16;
  final int canonicalSelectionExtentUtf16;
  final TextAffinity canonicalSelectionAffinity;
  final bool canonicalSelectionIsDirectional;
  final int? composingSourceUtf16Start;
  final int? composingSourceUtf16End;
}

/// Bounded geometry for one fragment visited by an actual surface paint.
final class FlarkSurfacePaintRowObservation {
  const FlarkSurfacePaintRowObservation({
    required this.ordinal,
    required this.neutral,
    required this.kind,
    required this.headingLevel,
    required this.blockQuoteDepth,
    required this.leadingText,
    required this.sourceUtf16Start,
    required this.fragmentStart,
    required this.fragmentEnd,
    required this.text,
    required this.runs,
    required this.resolvedBlockStyle,
    required this.active,
    required this.rect,
  });

  final int ordinal;

  /// Whether this paint used the exact-source fallback (`kind == 0`).
  final bool neutral;
  final int kind;
  final int? headingLevel;
  final int? blockQuoteDepth;
  final String leadingText;
  final int sourceUtf16Start;
  final int fragmentStart;
  final int fragmentEnd;
  final String text;
  final List<FlarkSurfacePaintRunObservation> runs;
  final TextStyle resolvedBlockStyle;
  final bool active;
  final Rect rect;
}

/// The exact source mapping and inline styles visited for one painted row run.
///
/// Text-only paint receipts cannot distinguish a correctly projected Strong
/// run from a marker-free but accidentally unstyled fallback. Keeping this
/// bounded run plan makes the rendered-result north star directly testable.
final class FlarkSurfacePaintRunObservation {
  const FlarkSurfacePaintRunObservation({
    required this.text,
    required this.sourceUtf16Start,
    required this.sourceUtf16End,
    required this.sourceExact,
    required this.styles,
    required this.resolvedStyle,
  });

  final String text;
  final int sourceUtf16Start;
  final int sourceUtf16End;
  final bool sourceExact;
  final Set<FlarkSurfaceInlineStyle> styles;
  final TextStyle resolvedStyle;
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
    this.semanticTargetFact,
  });

  final int globalUtf16Offset;
  final int ordinal;
  final TextAffinity affinity;
  final FlarkViewportRow? row;
  final String? neutralText;
  final int? neutralUtf16Start;
  final FlarkSurfaceAction? action;
  final FlarkInlineFact? semanticTargetFact;
}

final class FlarkSurfaceSelection {
  const FlarkSurfaceSelection({required this.base, required this.extent});

  final FlarkSurfaceHit base;
  final FlarkSurfaceHit extent;
}

typedef FlarkSurfaceSemanticsSelectionCallback =
    void Function(FlarkViewportRow row, int baseUtf16, int extentUtf16);

typedef FlarkSurfaceSemanticsCursorCallback =
    void Function({
      required bool forward,
      required bool byWord,
      required bool extendSelection,
    });

/// Editing actions exposed by the Flutter host to the bounded semantics tree.
/// The render object owns only current geometry and source mapping; clipboard,
/// focus, and canonical selection adoption remain host adapter policy.
final class FlarkSurfaceSemanticsActions {
  const FlarkSurfaceSemanticsActions({
    required this.onSetSelection,
    required this.onMoveCursor,
    required this.onCopy,
    required this.onCut,
    required this.onPaste,
    required this.onShowToolbar,
  });

  final FlarkSurfaceSemanticsSelectionCallback onSetSelection;
  final FlarkSurfaceSemanticsCursorCallback onMoveCursor;
  final VoidCallback onCopy;
  final VoidCallback onCut;
  final VoidCallback onPaste;
  final VoidCallback onShowToolbar;
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
    required this.layoutMaxWidth,
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

  /// Width constraint used to lay out [painter]. TextPainter's public width
  /// is the resulting content width, not the max-width constraint, so retain
  /// the latter explicitly for safe cross-frame reuse.
  final double layoutMaxWidth;

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
    this.semanticsActions,
    this.debugPaintObserver,
    super.key,
  });

  final FlarkEditorController controller;
  final TextStyle textStyle;
  final EdgeInsets padding;
  final Color caretColor;
  final Color selectionColor;
  final bool includeEditingState;
  final FlarkSurfaceSemanticsActions? semanticsActions;
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
        semanticsActions: semanticsActions,
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
      ..semanticsActions = semanticsActions
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
    required FlarkSurfaceSemanticsActions? semanticsActions,
    this.debugPaintObserver,
    required TextDirection textDirection,
  }) : _controller = controller,
       _textStyle = textStyle,
       _padding = padding,
       _caretColor = caretColor,
       _selectionColor = selectionColor,
       _includeEditingState = includeEditingState,
       _semanticsActions = semanticsActions,
       _textDirection = textDirection;

  FlarkEditorController _controller;
  TextStyle _textStyle;
  EdgeInsets _padding;
  Color _caretColor;
  Color _selectionColor;
  bool _includeEditingState;
  FlarkSurfaceSemanticsActions? _semanticsActions;
  ValueChanged<FlarkSurfacePaintObservation>? debugPaintObserver;
  TextDirection _textDirection;
  final List<_PaintedRow> _paintedRows = [];
  final List<_PaintedRow> _reusablePaintedRows = [];
  double _scrollOffset = 0;
  double _contentHeight = 0;
  int _laidOutPageIndex = 0;
  bool _layOutThroughPageEnd = false;
  int _laidOutRowCount = 0;
  int _skippedRowCount = 0;
  int _skippedFragmentCount = 0;
  double _skippedFragmentEstimate = 0;
  int _reusedPainterCount = 0;
  Map<int, SemanticsNode> _semanticRowNodes = <int, SemanticsNode>{};

  double get scrollOffset => _scrollOffset;
  double get debugContentHeight => _contentHeight;
  Size get debugSurfaceSize => size;

  /// Rows fully laid out in the last pass; below-fold rows are skipped.
  int get debugLaidOutRowCount => _laidOutRowCount;

  /// Rows whose layout was skipped as below the overscan budget.
  int get debugSkippedRowCount => _skippedRowCount;

  int get debugPaintedFragmentCount => _paintedRows.length;

  /// Text layouts reused from the immediately preceding frame.
  int get debugReusedPainterCount => _reusedPainterCount;

  List<
    ({
      int ordinal,
      bool neutral,
      int sourceStart,
      String text,
      bool active,
      double top,
      double height,
    })
  >
  get debugPaintedPlan => _paintedRows
      .map(
        (row) => (
          ordinal: row.ordinal,
          neutral: row.row == null,
          sourceStart:
              row.neutralUtf16Start ?? row.presentation.globalUtf16Start,
          text: row.presentation.text,
          active: row.presentation.active,
          top: row.top - _scrollOffset,
          height: row.height,
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

  /// Caret/selection ownership layered over [debugRenderPlanHash]. Keeping it
  /// separate distinguishes a content/layout flash from a transient loss of
  /// editor visual state.
  int get debugVisualStateHash => Object.hash(
    debugRenderPlanHash,
    Object.hashAll(
      _paintedRows.map(
        (painted) => Object.hash(
          painted.presentation.active,
          painted.presentation.selection?.baseOffset,
          painted.presentation.selection?.extentOffset,
          painted.presentation.selection?.affinity,
          painted.presentation.selection?.isDirectional,
        ),
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

  FlarkSurfaceSemanticsActions? get semanticsActions => _semanticsActions;
  set semanticsActions(FlarkSurfaceSemanticsActions? value) {
    if (identical(value, _semanticsActions)) return;
    _semanticsActions = value;
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

  @override
  void dispose() {
    for (final row in _paintedRows) {
      row.painter.dispose();
    }
    _paintedRows.clear();
    for (final row in _reusablePaintedRows) {
      row.painter.dispose();
    }
    _reusablePaintedRows.clear();
    super.dispose();
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
    final nextPage = _controller.viewportPageIndex;
    if (nextPage > previousPage) {
      _scrollOffset = 0;
    }
    _layOutThroughPageEnd = nextPage < previousPage;
    try {
      _buildVisibleLayouts();
    } finally {
      _layOutThroughPageEnd = false;
    }
    _laidOutPageIndex = nextPage;
    if (nextPage < previousPage) {
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
  double get _layoutBudgetBottom => _layOutThroughPageEnd
      ? double.infinity
      : _scrollOffset + (hasSize ? size.height : 480) + _layoutOverscanPx;

  /// The tallest reasonable estimate for one un-laid-out row: enough that
  /// the scroll range never undershoots badly, cheap enough to be a guess.
  double get _estimatedRowHeight {
    final fontSize = _textStyle.fontSize ?? 16;
    return fontSize * (_textStyle.height ?? 1.4);
  }

  void _buildVisibleLayouts() {
    assert(_reusablePaintedRows.isEmpty);
    _reusablePaintedRows.addAll(_paintedRows);
    _paintedRows.clear();
    try {
      _buildVisibleLayoutsBody();
    } finally {
      for (final row in _reusablePaintedRows) {
        row.painter.dispose();
      }
      _reusablePaintedRows.clear();
    }
  }

  void _buildVisibleLayoutsBody() {
    _laidOutRowCount = 0;
    _skippedRowCount = 0;
    _skippedFragmentCount = 0;
    _skippedFragmentEstimate = 0;
    _reusedPainterCount = 0;
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
      final fragmentEnd = _visualLineAlignedFragmentEnd(
        presentation,
        maxWidth,
        fragmentStart,
        includeLeading: first,
      );
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
          layoutMaxWidth: maxWidth,
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

  /// Returns a bounded fragment cut that is also a real visual-line boundary.
  ///
  /// Independent [TextPainter]s are stacked vertically. Cutting at an
  /// arbitrary 256-unit offset would therefore turn that implementation tile
  /// into a visible newline. We probe only enough text to find the visual line
  /// containing the target and move the cut to one of its boundaries.
  int _visualLineAlignedFragmentEnd(
    FlarkSurfaceRow presentation,
    double maxWidth,
    int fragmentStart, {
    required bool includeLeading,
  }) {
    final text = presentation.text;
    final remaining = text.length - fragmentStart;
    if (remaining <= _fragmentUtf16Budget) return text.length;

    var probeUnits = math.min(remaining, _fragmentUtf16Budget * 2);
    while (true) {
      var probeEnd = fragmentStart + probeUnits;
      if (probeEnd < text.length) {
        final snapped = FlarkCoreGraphemePolicy.clusterBoundaryAtOrBefore(
          text,
          probeEnd,
        );
        probeEnd = snapped > fragmentStart
            ? snapped
            : FlarkCoreGraphemePolicy.clusterBoundaryAtOrAfter(text, probeEnd);
      }
      final bodyUnits = probeEnd - fragmentStart;
      final leadingLength = includeLeading
          ? presentation.leadingText.length
          : 0;
      final probe = _layoutText(
        presentation,
        maxWidth,
        fragmentStart: fragmentStart,
        fragmentEnd: probeEnd,
        includeLeading: includeLeading,
        allowReuse: false,
      );
      try {
        final targetLocal =
            leadingLength +
            math.min(_fragmentUtf16Budget - 1, bodyUnits - 1).toInt();
        final line = probe.getLineBoundary(
          TextPosition(offset: targetLocal, affinity: TextAffinity.downstream),
        );
        final lineStart = (line.start - leadingLength)
            .clamp(0, bodyUnits)
            .toInt();
        final lineEnd = (line.end - leadingLength).clamp(0, bodyUnits).toInt();
        int? cutUnits;
        if (lineStart > 0) {
          // Move the partial final line into the next tile.
          cutUnits = lineStart;
        } else if (lineEnd < bodyUnits || probeEnd == text.length) {
          // On a very wide surface the target can still be on the first line;
          // retain that whole visual line as the smallest correct fragment.
          cutUnits = lineEnd;
        }
        if (cutUnits != null && cutUnits > 0) {
          final candidate = fragmentStart + cutUnits;
          final snapped = FlarkCoreGraphemePolicy.clusterBoundaryAtOrBefore(
            text,
            candidate,
          );
          if (snapped > fragmentStart) return snapped;
        }
      } finally {
        probe.dispose();
      }

      if (probeEnd == text.length ||
          probeUnits >= _fragmentVisualLineProbeLimit) {
        // A single visual line can be wider than the probe ceiling. The cap
        // remains deterministic; an oversized grapheme is kept intact.
        return probeEnd;
      }
      probeUnits = math.min(
        remaining,
        math.min(_fragmentVisualLineProbeLimit, probeUnits * 2),
      );
    }
  }

  TextPainter _layoutText(
    FlarkSurfaceRow presentation,
    double maxWidth, {
    int? fragmentStart,
    int? fragmentEnd,
    bool includeLeading = true,
    bool allowReuse = true,
  }) {
    final start = fragmentStart ?? 0;
    final end = fragmentEnd ?? presentation.text.length;
    final style = _blockTextStyle(presentation);
    final children = <InlineSpan>[];
    final visualEmptyLine =
        presentation.kind == 0 &&
        start == 0 &&
        end == presentation.text.length &&
        (presentation.text == '\n' || presentation.text == '\r\n');
    if (includeLeading && presentation.leadingText.isNotEmpty) {
      children.add(TextSpan(text: presentation.leadingText));
    }
    if (visualEmptyLine) {
      // A source newline owns two caret boundaries but only one editor line.
      // Laying the literal newline paints an empty line before and after it,
      // doubling blank-block height. A single placeholder preserves the same
      // two TextPainter offsets while keeping source ownership in the model.
      children.add(TextSpan(text: ''.padLeft(presentation.text.length)));
    } else if (presentation.runs.isNotEmpty) {
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
    final span = TextSpan(style: style, children: children);
    if (allowReuse) {
      for (var index = 0; index < _reusablePaintedRows.length; index += 1) {
        final candidate = _reusablePaintedRows[index];
        final previousSpan = candidate.painter.text;
        if (candidate.layoutMaxWidth != maxWidth ||
            candidate.painter.textDirection != _textDirection ||
            previousSpan == null ||
            previousSpan.compareTo(span) != RenderComparison.identical) {
          continue;
        }
        _reusablePaintedRows.removeAt(index);
        _reusedPainterCount += 1;
        return candidate.painter;
      }
    }
    return TextPainter(text: span, textDirection: _textDirection)
      ..layout(maxWidth: maxWidth);
  }

  TextStyle _blockTextStyle(FlarkSurfaceRow presentation) {
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
    return style;
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
    final resolved = _textPositionForOffset(offset);
    if (resolved == null) return null;
    final (:row, :painterPoint, :position) = resolved;
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

  ({_PaintedRow row, Offset painterPoint, TextPosition position})?
  _textPositionForOffset(Offset offset) {
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
    return (
      row: row,
      painterPoint: painterPoint,
      position: row.painter.getPositionForOffset(painterPoint),
    );
  }

  FlarkSurfaceHit _hitForTextOffset(
    _PaintedRow row,
    int textOffset, {
    required TextAffinity affinity,
    FlarkSurfaceAction? action,
  }) {
    final globalUtf16Offset = row.presentation.sourceOffsetForTextOffset(
      textOffset,
      affinity: affinity,
    );
    final semanticTargetFact = _semanticTargetFactFor(
      row.row,
      globalUtf16Offset,
    );
    return FlarkSurfaceHit(
      globalUtf16Offset: globalUtf16Offset,
      ordinal: row.ordinal,
      affinity: affinity,
      row: row.row,
      neutralText: row.neutralText,
      neutralUtf16Start: row.neutralUtf16Start,
      action: action,
      semanticTargetFact: semanticTargetFact,
    );
  }

  static FlarkInlineFact? _semanticTargetFactFor(
    FlarkViewportRow? row,
    int globalUtf16Offset,
  ) {
    for (final fact in row?.inlineFacts ?? const <FlarkInlineFact>[]) {
      if (_isSemanticTargetFact(fact.kind) &&
          globalUtf16Offset >= fact.contentUtf16.start &&
          globalUtf16Offset < fact.contentUtf16.end) {
        return fact;
      }
    }
    return null;
  }

  static bool _isSemanticTargetFact(FlarkInlineFactKind kind) => switch (kind) {
    FlarkInlineFactKind.autolinkUri ||
    FlarkInlineFactKind.autolinkEmail ||
    FlarkInlineFactKind.directLink ||
    FlarkInlineFactKind.directImage ||
    FlarkInlineFactKind.referenceLink ||
    FlarkInlineFactKind.referenceImage => true,
    _ => false,
  };

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

  /// Global anchors for the platform-adaptive selection toolbar. The render
  /// surface owns geometry; clipboard and edit policy stay in the editor
  /// adapter. For an off-page base, the active extent remains a valid bounded
  /// anchor instead of forcing document-wide layout.
  TextSelectionToolbarAnchors? selectionToolbarAnchors(int base, int extent) {
    final extentLocal = _localPositionForSourceUtf16(extent);
    if (extentLocal == null) return null;
    final baseLocal = _localPositionForSourceUtf16(base) ?? extentLocal;
    final primaryLocal = Offset(
      (baseLocal.dx + extentLocal.dx) / 2,
      math.min(baseLocal.dy, extentLocal.dy),
    );
    final secondaryLocal = Offset(
      (baseLocal.dx + extentLocal.dx) / 2,
      math.max(baseLocal.dy, extentLocal.dy) +
          (_paintedRows.isEmpty
              ? 0
              : _paintedRows.first.painter.preferredLineHeight),
    );
    return TextSelectionToolbarAnchors(
      primaryAnchor: localToGlobal(primaryLocal),
      secondaryAnchor: localToGlobal(secondaryLocal),
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

  _PaintedRow? _fragmentForSourceUtf16(int offset) {
    for (final row in _paintedRows) {
      final bounds = _sourceBounds(row);
      if (offset < bounds.start || offset > bounds.end) continue;
      final textOffset = row.presentation.textOffsetForSourceOffset(offset);
      if (row.fragmentStart <= textOffset &&
          (textOffset < row.fragmentEnd ||
              (textOffset == row.fragmentEnd &&
                  row.fragmentEnd == row.presentation.text.length))) {
        return row;
      }
    }
    return null;
  }

  FlarkSurfaceHit? lineBoundaryHit(int offset, {required bool forward}) {
    final row = _fragmentForSourceUtf16(offset);
    if (row == null) return null;
    final textOffset = row.presentation.textOffsetForSourceOffset(
      offset,
      affinity: forward ? TextAffinity.downstream : TextAffinity.upstream,
    );
    final painterOffset = textOffset - row.fragmentStart + row.leadingLength;
    final boundary = row.painter.getLineBoundary(
      TextPosition(
        offset: painterOffset,
        affinity: forward ? TextAffinity.downstream : TextAffinity.upstream,
      ),
    );
    final target =
        ((forward ? boundary.end : boundary.start) -
                row.leadingLength +
                row.fragmentStart)
            .clamp(row.fragmentStart, row.fragmentEnd)
            .clamp(0, row.presentation.text.length);
    return _hitForTextOffset(
      row,
      target,
      affinity: forward ? TextAffinity.upstream : TextAffinity.downstream,
    );
  }

  /// Resolves the boundary of the complete rendered block rather than the
  /// current wrapped visual line. Hidden Markdown prefixes/suffixes stay
  /// outside the caret path through the row's source mapping.
  FlarkSurfaceHit? paragraphBoundaryHit(int offset, {required bool forward}) {
    final row = _logicalRowForSourceUtf16(offset);
    if (row == null) return null;
    return _hitForTextOffset(
      row,
      forward ? row.presentation.text.length : 0,
      affinity: forward ? TextAffinity.upstream : TextAffinity.downstream,
    );
  }

  FlarkSurfaceHit? wordBoundaryHit(int offset, {required bool forward}) {
    final rows = _logicalRows.toList(growable: false);
    final current = _logicalRowForSourceUtf16(offset);
    if (current == null) return null;
    var rowIndex = rows.indexWhere((row) => row.ordinal == current.ordinal);
    if (rowIndex < 0) return null;
    var row = rows[rowIndex];
    var textOffset = row.presentation.textOffsetForSourceOffset(
      offset,
      affinity: forward ? TextAffinity.downstream : TextAffinity.upstream,
    );
    while (true) {
      final text = row.presentation.text;
      if ((forward && textOffset < text.length) ||
          (!forward && textOffset > 0)) {
        // Word navigation is a user action, so doing one bounded (<= 2 KiB)
        // layout here is preferable to treating internal 256-unit paint
        // fragments as semantic word boundaries.
        final navigationPainter = TextPainter(
          text: TextSpan(text: text),
          textDirection: _textDirection,
        )..layout();
        try {
          final boundaries =
              navigationPainter.wordBoundaries.moveByWordBoundary;
          final target = forward
              ? boundaries.getTrailingTextBoundaryAt(textOffset)
              : boundaries.getLeadingTextBoundaryAt(textOffset - 1);
          if (target != null) {
            return _hitForTextOffset(
              row,
              target.clamp(0, text.length),
              affinity: forward
                  ? TextAffinity.downstream
                  : TextAffinity.upstream,
            );
          }
        } finally {
          navigationPainter.dispose();
        }
      }
      rowIndex += forward ? 1 : -1;
      if (rowIndex < 0 || rowIndex >= rows.length) return null;
      row = rows[rowIndex];
      textOffset = forward ? 0 : row.presentation.text.length;
    }
  }

  /// Whether [offset] belongs to a parser-authored table cell on the current
  /// painted page. This is layout navigation only: table shape and source
  /// ownership come from Core, while Flutter chooses the visible caret stop.
  bool isTableCellPosition(int offset) => _tableCellPosition(offset) != null;

  /// Moves to the beginning of the next or previous real table cell.
  /// Autocompleted cells have no source-backed editing position and are
  /// intentionally skipped. A null result at the first/last cell still leaves
  /// Tab owned by the table rather than leaking into widget focus traversal.
  FlarkSurfaceHit? adjacentTableCellHit(int offset, {required bool forward}) {
    final position = _tableCellPosition(offset);
    if (position == null) return null;
    final (:row, :cells, index: currentIndex) = position;
    final targetIndex = currentIndex + (forward ? 1 : -1);
    if (targetIndex < 0 || targetIndex >= cells.length) return null;
    return _hitForTextOffset(
      row,
      row.presentation.textOffsetForSourceOffset(
        cells[targetIndex].contentUtf16.start,
        affinity: TextAffinity.downstream,
      ),
      affinity: TextAffinity.downstream,
    );
  }

  ({_PaintedRow row, List<FlarkTableCellPresentation> cells, int index})?
  _tableCellPosition(int offset) {
    if (_controller.pendingTableNavigationLocked) return null;
    final row = _logicalRowForSourceUtf16(offset);
    if (row == null || row.presentation.kind == 0) return null;
    final table = row.row?.table;
    if (table == null) return null;
    final cells = table.rows
        .expand((cells) => cells)
        .where((cell) => !cell.autocompleted)
        .toList(growable: false);
    for (var index = 0; index < cells.length; index += 1) {
      final range = cells[index].contentUtf16;
      if (range.start <= offset && offset <= range.end) {
        return (row: row, cells: cells, index: index);
      }
    }
    return null;
  }

  /// Selects one rendered Unicode word without placing either source endpoint
  /// inside hidden Markdown syntax.
  FlarkSurfaceSelection? wordSelectionForOffset(Offset offset) {
    final resolved = _textPositionForOffset(offset);
    if (resolved == null) return null;
    final (:row, :position, painterPoint: _) = resolved;
    final text = row.presentation.text;
    final textOffset = (position.offset - row.leadingLength + row.fragmentStart)
        .clamp(row.fragmentStart, row.fragmentEnd)
        .clamp(0, text.length);
    if (textOffset == text.length) {
      final hit = _hitForTextOffset(
        row,
        textOffset,
        affinity: TextAffinity.upstream,
      );
      return FlarkSurfaceSelection(base: hit, extent: hit);
    }
    final navigationPainter = TextPainter(
      text: TextSpan(text: text),
      textDirection: _textDirection,
    )..layout();
    try {
      final word = navigationPainter.getWordBoundary(
        TextPosition(offset: textOffset, affinity: position.affinity),
      );
      return FlarkSurfaceSelection(
        base: _hitForTextOffset(
          row,
          word.start,
          affinity: TextAffinity.downstream,
        ),
        extent: _hitForTextOffset(
          row,
          word.end,
          affinity: TextAffinity.upstream,
        ),
      );
    } finally {
      navigationPainter.dispose();
    }
  }

  FlarkSurfaceHit? verticalHit(
    int offset, {
    required bool forward,
    double? preferredX,
  }) {
    final current = _localPositionForSourceUtf16(offset);
    final row = _logicalRowForSourceUtf16(offset);
    if (current == null || row == null) return null;
    final target = Offset(
      preferredX ?? current.dx,
      current.dy + (forward ? 1 : -1) * row.painter.preferredLineHeight,
    );
    final firstTop = _paintedRows.first.top - _scrollOffset;
    final lastBottom =
        _paintedRows.last.top + _paintedRows.last.height - _scrollOffset;
    if (target.dy < firstTop || target.dy > lastBottom) return null;
    return positionForOffset(target);
  }

  /// Whether a vertical move from [offset] has exhausted this bounded page.
  ///
  /// The editor uses this only after [verticalHit] finds no painted target.
  /// It deliberately refuses to skip rows that have not yet been laid out.
  bool isAtViewportPageEdge(int offset, {required bool forward}) {
    final current = _logicalRowForSourceUtf16(offset);
    if (current == null) return false;
    final logicalRows = _logicalRows.toList(growable: false);
    if (logicalRows.isEmpty) return false;
    if (forward) {
      return _skippedRowCount == 0 &&
          current.ordinal == logicalRows.last.ordinal;
    }
    return current.ordinal == logicalRows.first.ordinal;
  }

  /// Resolves the first or last rendered caret stop after a page transition.
  FlarkSurfaceHit? verticalPageEdgeHit({
    required bool forward,
    required double preferredX,
  }) {
    if (_paintedRows.isEmpty) return null;
    final row = forward ? _paintedRows.first : _paintedRows.last;
    final point = Offset(
      (preferredX - _padding.left).clamp(0, row.painter.width),
      forward
          ? math.min(row.height / 2, row.painter.preferredLineHeight / 2)
          : math.max(0, row.height - row.painter.preferredLineHeight / 2),
    );
    final position = row.painter.getPositionForOffset(point);
    final local = (position.offset - row.leadingLength + row.fragmentStart)
        .clamp(row.fragmentStart, row.fragmentEnd)
        .clamp(0, row.presentation.text.length);
    return _hitForTextOffset(row, local, affinity: position.affinity);
  }

  double? localXForSourceUtf16(int offset) =>
      _localPositionForSourceUtf16(offset)?.dx;

  /// Scrolls an already laid-out source caret into the visible surface.
  ///
  /// This never changes viewport pages or source selection. The editor calls
  /// it only after geometry has selected the next caret stop; any rows exposed
  /// below the overscan boundary are materialized by the following layout.
  void ensureSourceUtf16Visible(int offset) {
    if (!hasSize) return;
    final row = _fragmentForSourceUtf16(offset);
    if (row == null) return;
    final viewportTop = _scrollOffset;
    final viewportBottom = viewportTop + size.height;
    var next = viewportTop;
    if (row.top < viewportTop) {
      next = row.top;
    } else if (row.top + row.height > viewportBottom) {
      next = row.top + row.height - size.height;
    }
    next = next.clamp(0, _maximumScrollOffset);
    if (next == _scrollOffset) return;
    final movedForward = next > _scrollOffset;
    _scrollOffset = next;
    markNeedsPaint();
    markNeedsSemanticsUpdate();
    if (movedForward && _skippedRowCount > 0) markNeedsLayout();
  }

  Rect? _taskActionBox(_PaintedRow row) {
    if (row.presentation.kind == 0 ||
        row.fragmentStart != 0 ||
        row.leadingLength == 0 ||
        row.row?.listItem?.taskChecked == null ||
        !_controller.canToggleTaskChecked(row.row!)) {
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
    final maximumScrollOffset = hasSize ? _maximumScrollOffset : 0.0;
    config
      ..isSemanticBoundary = true
      ..explicitChildNodes = true
      ..hasImplicitScrolling = true
      ..scrollPosition = _scrollOffset
      ..scrollExtentMin = 0
      ..scrollExtentMax = maximumScrollOffset
      ..identifier = _includeEditingState
          ? 'flark-markdown-editor'
          : 'flark-markdown-view';
    if (maximumScrollOffset > 0 || _controller.canPageForward) {
      config.onScrollUp = () {
        if (hasSize) scrollBy(size.height * 0.8);
      };
    }
    if (_scrollOffset > 0 || _controller.canPageBackward) {
      config.onScrollDown = () {
        if (hasSize) scrollBy(-size.height * 0.8);
      };
    }
  }

  String _semanticLabel(_PaintedRow row) {
    final text = row.presentation.text.trim().replaceAll('\n', ' ');
    if (row.presentation.thematicBreak) return 'Horizontal rule';
    if (row.presentation.headingLevel case final level?) {
      return text.isEmpty ? 'Heading level $level' : text;
    }
    return text.isEmpty ? 'Blank line' : text;
  }

  void _setSemanticSelection(
    _PaintedRow row,
    TextSelection selection,
    FlarkSurfaceSemanticsActions actions,
  ) {
    final sourceRow = row.row;
    if (sourceRow == null) return;
    final textLength = row.presentation.text.length;
    final base = selection.baseOffset.clamp(0, textLength);
    final extent = selection.extentOffset.clamp(0, textLength);
    final collapsed = base == extent;
    final baseSource = row.presentation.sourceOffsetForTextOffset(
      base,
      affinity: TextAffinity.downstream,
    );
    final extentSource = row.presentation.sourceOffsetForTextOffset(
      extent,
      affinity: collapsed || extent < base
          ? TextAffinity.downstream
          : TextAffinity.upstream,
    );
    actions.onSetSelection(sourceRow, baseSource, extentSource);
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
        ..textDirection = _textDirection;
      final actions = _includeEditingState ? _semanticsActions : null;
      if (actions != null && row.row != null) {
        rowConfig
          ..value = row.presentation.text
          ..isTextField = true
          ..isReadOnly = false
          ..isMultiline = row.presentation.text.contains('\n')
          ..isFocused = row.presentation.active;
        rowConfig.onSetSelection = (selection) =>
            _setSemanticSelection(row, selection, actions);
        rowConfig.onMoveCursorForwardByCharacter = (extend) =>
            actions.onMoveCursor(
              forward: true,
              byWord: false,
              extendSelection: extend,
            );
        rowConfig.onMoveCursorBackwardByCharacter = (extend) =>
            actions.onMoveCursor(
              forward: false,
              byWord: false,
              extendSelection: extend,
            );
        rowConfig.onMoveCursorForwardByWord = (extend) => actions.onMoveCursor(
          forward: true,
          byWord: true,
          extendSelection: extend,
        );
        rowConfig.onMoveCursorBackwardByWord = (extend) => actions.onMoveCursor(
          forward: false,
          byWord: true,
          extendSelection: extend,
        );
        rowConfig
          ..onPaste = actions.onPaste
          ..onLongPress = actions.onShowToolbar;
        if (row.presentation.selection case final selection?) {
          if (selection.isValid &&
              selection.start >= 0 &&
              selection.end <= row.presentation.text.length) {
            rowConfig.textSelection = selection;
          }
        }
        if (_controller.globalSelectionBase !=
            _controller.globalSelectionExtent) {
          rowConfig
            ..onCopy = actions.onCopy
            ..onCut = actions.onCut;
        }
      } else {
        rowConfig.label = _semanticLabel(row);
      }
      if (row.presentation.headingLevel != null) rowConfig.isHeader = true;
      final taskRow = row.row;
      final task =
          row.presentation.kind == 0 ||
              taskRow == null ||
              !_controller.canToggleTaskChecked(taskRow)
          ? null
          : taskRow.listItem?.taskChecked;
      if (task != null) {
        final checked = row.presentation.leadingText.contains('☑');
        rowConfig.isChecked = checked;
        if (_includeEditingState) {
          rowConfig.onTap = () =>
              unawaited(_controller.toggleTaskChecked(taskRow!));
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
    final paintObserver = debugPaintObserver;
    final observedRows = paintObserver == null ? null : <String>[];
    final observedKeys = paintObserver == null ? null : <Object>{};
    final observedGeometry = paintObserver == null
        ? null
        : <FlarkSurfacePaintRowObservation>[];
    final observedSelectionRects = paintObserver == null ? null : <Rect>[];
    Rect? observedCaretRect;
    int? observedCaretSourceUtf16;
    int? observedCaretDisplayUtf16;
    canvas.save();
    canvas.clipRect(offset & size);
    for (final row in _paintedRows) {
      final paintedTop = row.top - _scrollOffset;
      if (paintedTop + row.height < 0 || paintedTop > size.height) continue;
      if (observedKeys != null) {
        final observationKey = row.row != null
            ? (
                'row',
                row.row!.ordinal,
                row.presentation.globalUtf16Start,
                row.presentation.blockQuoteDepth,
              )
            : ('neutral', row.ordinal, row.neutralUtf16Start, row.neutralText);
        if (observedKeys.add(observationKey)) {
          observedRows!.add(
            '${row.presentation.leadingText}${row.presentation.text}',
          );
        }
      }
      final origin = offset + Offset(_padding.left, paintedTop);
      observedGeometry?.add(
        FlarkSurfacePaintRowObservation(
          ordinal: row.ordinal,
          neutral: row.presentation.kind == 0,
          kind: row.presentation.kind,
          headingLevel: row.presentation.headingLevel,
          blockQuoteDepth: row.presentation.blockQuoteDepth,
          leadingText: row.presentation.leadingText,
          sourceUtf16Start:
              row.neutralUtf16Start ?? row.presentation.globalUtf16Start,
          fragmentStart: row.fragmentStart,
          fragmentEnd: row.fragmentEnd,
          text: row.presentation.text,
          runs: List.unmodifiable(_paintedRunObservations(row)),
          resolvedBlockStyle: _blockTextStyle(row.presentation),
          active: row.presentation.active,
          rect: Rect.fromLTWH(
            origin.dx,
            origin.dy,
            row.painter.width,
            row.height,
          ),
        ),
      );
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
            final rect = box.toRect().shift(origin);
            observedSelectionRects?.add(rect);
            canvas.drawRect(rect, paint);
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
          final rect = Rect.fromLTWH(
            origin.dx + caret.dx,
            origin.dy + caret.dy,
            1.5,
            row.painter.preferredLineHeight,
          );
          if (paintObserver != null) {
            observedCaretRect = rect;
            observedCaretDisplayUtf16 =
                extent - row.fragmentStart + row.leadingLength;
            // A projected display position can represent a range of hidden
            // source offsets (for example source offsets 0..2 all paint before
            // the first visible character of an ATX heading). Preserve the
            // authoritative source identity when the painted display position
            // is exactly its projection; only reverse-map when the painted
            // caret is genuinely somewhere else.
            final canonicalSource = _controller.globalSelectionExtent;
            final canonicalDisplay = row.presentation.textOffsetForSourceOffset(
              canonicalSource,
              affinity: selection.affinity,
            );
            observedCaretSourceUtf16 = canonicalDisplay == extent
                ? canonicalSource
                : row.presentation.sourceOffsetForTextOffset(
                    extent,
                    affinity: selection.affinity,
                  );
          }
          canvas.drawRect(rect, Paint()..color = _caretColor);
        }
      }
    }
    canvas.restore();
    if (paintObserver == null) return;
    final inputValue = _controller.inputValue;
    final composing = inputValue.composing;
    final composingStart = composing.isValid && !composing.isCollapsed
        ? _controller.inputWindowShadow.globalUtf16Start + composing.start
        : null;
    final composingEnd = composing.isValid && !composing.isCollapsed
        ? _controller.inputWindowShadow.globalUtf16Start + composing.end
        : null;
    paintObserver(
      FlarkSurfacePaintObservation(
        revision: _controller.revision,
        sourceGeneration: _controller.sourceGeneration,
        viewportPageIndex: _controller.viewportPageIndex,
        visibleUtf16Start: _controller.visibleUtf16Start,
        visibleUtf16Length: _controller.visibleSource.length,
        scrollOffset: _scrollOffset,
        presentation: observedRows!.isEmpty
            ? '<empty>'
            : observedRows.join('\n'),
        renderPlanHash: debugRenderPlanHash,
        visualStateHash: debugVisualStateHash,
        rows: List.unmodifiable(observedGeometry!),
        selectionRects: List.unmodifiable(observedSelectionRects!),
        caretRect: observedCaretRect,
        caretSourceUtf16: observedCaretSourceUtf16,
        caretDisplayUtf16: observedCaretDisplayUtf16,
        visibleSource: _controller.visibleSource,
        canonicalSelectionBaseUtf16: _controller.globalSelectionBase,
        canonicalSelectionExtentUtf16: _controller.globalSelectionExtent,
        canonicalSelectionAffinity: inputValue.selection.affinity,
        canonicalSelectionIsDirectional: inputValue.selection.isDirectional,
        composingSourceUtf16Start: composingStart,
        composingSourceUtf16End: composingEnd,
      ),
    );
  }

  Iterable<FlarkSurfacePaintRunObservation> _paintedRunObservations(
    _PaintedRow row,
  ) sync* {
    final blockStyle = _blockTextStyle(row.presentation);
    var cursor = 0;
    for (final run in row.presentation.runs) {
      final runEnd = cursor + run.text.length;
      final sliceStart = math.max(row.fragmentStart, cursor);
      final sliceEnd = math.min(row.fragmentEnd, runEnd);
      if (sliceEnd > sliceStart) {
        final sourceStart = run.sourceExact
            ? run.sourceUtf16Start + sliceStart - cursor
            : row.presentation.sourceOffsetForTextOffset(
                sliceStart,
                affinity: TextAffinity.downstream,
              );
        final sourceEnd = run.sourceExact
            ? run.sourceUtf16Start + sliceEnd - cursor
            : row.presentation.sourceOffsetForTextOffset(
                sliceEnd,
                affinity: TextAffinity.upstream,
              );
        yield FlarkSurfacePaintRunObservation(
          text: run.text.substring(sliceStart - cursor, sliceEnd - cursor),
          sourceUtf16Start: sourceStart,
          sourceUtf16End: sourceEnd,
          sourceExact: run.sourceExact,
          styles: run.styles,
          resolvedStyle: _inlineStyle(blockStyle, run.styles),
        );
      }
      cursor = runEnd;
      if (cursor >= row.fragmentEnd) break;
    }
  }
}
