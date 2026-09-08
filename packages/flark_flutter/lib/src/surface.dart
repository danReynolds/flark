import 'dart:math' as math;
import 'dart:ui' as ui;
import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'package:flark/render_model.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'controller.dart';
import 'code_font.dart';
import 'source_window.dart';

/// Emitted from actual paint, after all visible rows and the caret are drawn.
/// Tests can distinguish a current, styled frame from a settled text transcript.
class FlarkPaintObservation {
  const FlarkPaintObservation(
    this.revision,
    this.snapshot,
    this.rows,
    this.styles,
    this.caret,
    this.caretSource,
    this.selectionRects,
    this.resolvedStyles,
    this.frameNumber,
  );
  final int revision;
  final FlarkEditorSnapshot snapshot;
  final List<String> rows;
  final List<List<int>> styles;
  final List<List<TextStyle>> resolvedStyles;
  final int frameNumber;
  final Rect? caret;
  final int caretSource;
  final List<Rect> selectionRects;
}

class FlarkSurface extends LeafRenderObjectWidget {
  const FlarkSurface({
    super.key,
    required this.controller,
    required this.style,
    required this.focused,
    required this.viewportHeight,
    required this.scrollOffset,
    this.readOnly = false,
    this.onPaint,
    this.revealDuringLayout,
  });
  final FlarkController controller;
  final TextStyle style;
  final bool focused, readOnly;
  final double viewportHeight, scrollOffset;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  final double Function(Rect, double, double)? revealDuringLayout;
  @override
  RenderFlarkSurface createRenderObject(BuildContext context) =>
      RenderFlarkSurface(
        controller,
        style,
        focused,
        readOnly,
        viewportHeight,
        scrollOffset,
        onPaint,
        revealDuringLayout,
      );
  @override
  void updateRenderObject(
    BuildContext context,
    RenderFlarkSurface renderObject,
  ) => renderObject.update(
    controller,
    style,
    focused,
    readOnly,
    viewportHeight,
    scrollOffset,
    onPaint,
    revealDuringLayout,
  );
}

class _RowLayout {
  _RowLayout(
    this.row,
    this.sourceStart,
    this.text,
    this.styles,
    this.painter,
    this.width,
    this.codeInfo,
  );
  ProjectedRow? row;
  int sourceStart;
  final String text;
  final List<int> styles;
  final TextPainter painter;
  double width;
  final String codeInfo;
  Rect rect = Rect.zero;
  Offset origin = Offset.zero;
  double indent = 22;
}

/// Uses the same hit test as checkbox activation, including after scrolling or
/// a layout change underneath a stationary pointer.
class _TaskCursorTarget extends MouseTrackerAnnotation
    implements HitTestTarget {
  const _TaskCursorTarget() : super(cursor: SystemMouseCursors.click);

  @override
  void handleEvent(PointerEvent event, HitTestEntry entry) {}
}

class RenderFlarkSurface extends RenderBox
    with RelayoutWhenSystemFontsChangeMixin {
  RenderFlarkSurface(
    this.controller,
    this.style,
    this.focused,
    this.readOnly,
    this.viewportHeight,
    this.scrollOffset,
    this.onPaint,
    this.revealDuringLayout,
  ) {
    _snapshot = controller.editor.snapshot;
    controller.addListener(_changed);
    controller.codeColors?.addListener(_colorsChanged);
  }
  FlarkController controller;
  TextStyle style;
  bool focused, readOnly;
  double viewportHeight, scrollOffset;
  ValueChanged<FlarkPaintObservation>? onPaint;
  double Function(Rect, double, double)? revealDuringLayout;
  bool _needsReveal = true;
  double _contentHeight = 0;
  List<_RowLayout> _rows = [];
  // Keep one layout per mode. A temporary Source excursion must not discard
  // every shaped live paragraph. Both lists remain bounded by admission/page
  // limits, and every reused row is checked against the current presentation.
  List<_RowLayout> _otherRows = [];
  bool _sourceLayout = false;
  int _otherColorRevision = -1;
  late FlarkEditorSnapshot _snapshot;
  int _revision = 0;
  double? _layoutWidth;
  Object? _layoutContent;
  int _colorRevision = -1;
  static const _padding = 16.0;
  static const _caretPrototype = Rect.fromLTWH(0, 0, 1.5, 22);

  void update(
    FlarkController next,
    TextStyle nextStyle,
    bool focus,
    bool readonly,
    double height,
    double scroll,
    ValueChanged<FlarkPaintObservation>? observer,
    double Function(Rect, double, double)? reveal,
  ) {
    if (next != controller) {
      controller.removeListener(_changed);
      controller.codeColors?.removeListener(_colorsChanged);
      _needsReveal = true;
      controller = next;
      controller.addListener(_changed);
      controller.codeColors?.addListener(_colorsChanged);
      _clearRows();
    }
    if (nextStyle != style) {
      style = nextStyle;
      _clearRows();
    }
    if ((!focused && focus) || viewportHeight != height) _needsReveal = true;
    focused = focus;
    readOnly = readonly;
    viewportHeight = height;
    scrollOffset = scroll;
    onPaint = observer;
    revealDuringLayout = reveal;
    markNeedsLayout();
    markNeedsSemanticsUpdate();
  }

  void _changed() {
    _needsReveal = true;
    markNeedsLayout();
    markNeedsSemanticsUpdate();
  }

  void _colorsChanged() {
    // Colors cannot move the caret, scroll, or mutate semantics/history.
    markNeedsLayout();
  }

  void _clearRows() {
    _layoutContent = null;
    for (final row in [..._rows, ..._otherRows]) {
      row.painter.dispose();
    }
    _rows = [];
    _otherRows = [];
    _otherColorRevision = -1;
  }

  @override
  void systemFontsDidChange() {
    super.systemFontsDidChange();
    _clearRows();
    _needsReveal = true;
    markNeedsSemanticsUpdate();
  }

  @override
  void dispose() {
    controller.removeListener(_changed);
    controller.codeColors?.removeListener(_colorsChanged);
    _clearRows();
    super.dispose();
  }

  TextStyle _rowStyle(ProjectedRow? row) {
    var s = style;
    if (row?.kind == RowKind.heading) {
      s = s.copyWith(
        fontSize:
            (style.fontSize ?? 17) * (1.75 - (row!.headingLevel - 1) * .12),
        fontWeight: FontWeight.w700,
        height: 1.35,
      );
    }
    if (row?.kind == RowKind.codeBlock || controller.editor.sourceMode) {
      s = s.copyWith(
        fontFamily: flarkCodeFontFamily,
        fontSize: (style.fontSize ?? 17) * .9,
      );
    }
    if (row?.header == true) s = s.copyWith(fontWeight: FontWeight.w700);
    return s;
  }

  TextStyle _inlineStyle(TextStyle base, int bits) => base.copyWith(
    fontWeight: bits & Style.strong != 0 ? FontWeight.w700 : base.fontWeight,
    fontStyle: bits & Style.emphasis != 0 ? FontStyle.italic : base.fontStyle,
    fontFamily: bits & Style.code != 0 ? flarkCodeFontFamily : base.fontFamily,
    backgroundColor: bits & Style.code != 0 ? const Color(0xffeceff4) : null,
    color: bits & Style.link != 0 ? const Color(0xff2864c7) : base.color,
    decoration: bits & Style.strikethrough != 0
        ? TextDecoration.lineThrough
        : bits & Style.link != 0
        ? TextDecoration.underline
        : null,
  );

  Color? _codeColor(String? kind) => switch (kind) {
    'keyword' || 'selector-tag' || 'meta' => const Color(0xff8250df),
    'string' ||
    'regexp' ||
    'attr' ||
    'selector-attr' => const Color(0xff116329),
    'number' ||
    'constant' ||
    'boolean' ||
    'literal' ||
    'built_in' => const Color(0xff0550ae),
    'comment' || 'doctag' => const Color(0xff6e7781),
    'title' ||
    'function' ||
    'constructor' ||
    'type' ||
    'class' => const Color(0xff953800),
    'variable' ||
    'property' ||
    'tag' ||
    'params' ||
    'attribute' ||
    'selector-class' => const Color(0xff9a6700),
    _ => null,
  };

  bool _samePresentation(ProjectedRow? before, ProjectedRow? after) {
    if (before == null || after == null) return before == after;
    if (before.kind != after.kind ||
        before.headingLevel != after.headingLevel ||
        before.header != after.header ||
        before.alignment != after.alignment ||
        before.text != after.text ||
        before.segments.length != after.segments.length) {
      return false;
    }
    for (var i = 0; i < before.segments.length; i++) {
      final a = before.segments[i], b = after.segments[i];
      if (a.displayStart != b.displayStart ||
          a.displayEnd != b.displayEnd ||
          a.styles != b.styles) {
        return false;
      }
    }
    return true;
  }

  @override
  void performLayout() {
    if (_layoutWidth != constraints.maxWidth) _needsReveal = true;
    _prepareRows();
    size = constraints.constrain(
      Size(constraints.maxWidth, math.max(viewportHeight, _contentHeight)),
    );
    if (_needsReveal && focused && !readOnly && revealDuringLayout != null) {
      scrollOffset = revealDuringLayout!(
        caretRect,
        _contentHeight,
        viewportHeight,
      );
      _needsReveal = false;
    }
    final visible = Rect.fromLTWH(0, scrollOffset, size.width, viewportHeight);
    controller.codeColors?.setVisibleRows([
      for (final layout in _rows)
        if (layout.row?.kind == RowKind.codeBlock &&
            layout.rect.overlaps(visible))
          layout.row!.index,
    ]);
  }

  // Input can arrive several times before Flutter lays out the next frame.
  // Geometry must use the current source and selection, including in that burst.
  void _prepareRows() {
    _snapshot = controller.editor.snapshot;
    _revision = controller.editor.revision;
    final sourceMode = _snapshot is FlarkSourceSnapshot;
    if (sourceMode != _sourceLayout) {
      final previousRows = _rows;
      _rows = _otherRows;
      _otherRows = previousRows;
      final previousColors = _colorRevision;
      _colorRevision = _otherColorRevision;
      _otherColorRevision = previousColors;
      _sourceLayout = sourceMode;
      _layoutContent = null;
    }
    final window = _snapshot is FlarkSourceSnapshot
        ? SourceWindow.at(_snapshot.source, _snapshot.selection.extent)
        : null;
    final content = _snapshot is FlarkLiveSnapshot
        ? (_snapshot as FlarkLiveSnapshot).projection
        : (_snapshot.source, window!.start, window.end);
    final colorRevision = controller.codeColors?.revision ?? 0;
    final colorsChanged = colorRevision != _colorRevision;
    if (content == _layoutContent &&
        !colorsChanged &&
        constraints.maxWidth == _layoutWidth &&
        _rows.isNotEmpty) {
      return;
    }
    _layoutContent = content;
    _colorRevision = colorRevision;
    _layoutWidth = constraints.maxWidth;
    final available = math.max(40.0, constraints.maxWidth - 2 * _padding);
    final old = _rows;
    final next = <_RowLayout>[];
    final sourceLines = _snapshot is FlarkSourceSnapshot
        ? _snapshot.source.substring(window!.start, window.end).split('\n')
        : null;
    final projected = _snapshot is FlarkLiveSnapshot
        ? (_snapshot as FlarkLiveSnapshot).projection.rows
        : null;
    var sourceOffset = window?.start ?? 0, y = _padding;
    var tableRow = -1, tableTop = 0.0, tableHeight = 0.0;
    for (var i = 0; i < (projected?.length ?? sourceLines!.length); i++) {
      final row = projected?[i];
      final text = row?.text ?? sourceLines![i].replaceAll('\r', '');
      final depth =
          row?.shells
              .where(
                (s) =>
                    s.kind == ShellKind.item || s.kind == ShellKind.blockQuote,
              )
              .length ??
          0;
      final indent = depth == 0
          ? 22.0
          : math.min(22.0, math.max(0.0, available - 40) / depth);
      var x = _padding + depth * indent;
      var width = math.max(24.0, available - depth * indent);
      if (row?.kind == RowKind.tableCell) {
        final model = (_snapshot as FlarkLiveSnapshot).document.model;
        final columns = model.block(row!.tableBlock, BlockField.attr0);
        width = math.max(24, available / columns - 16);
        x = _padding + row.column * (width + 16) + 8;
        if (tableRow != row.tableRowBlock) {
          y += tableHeight;
          tableTop = y;
          tableHeight = 0;
          tableRow = row.tableRowBlock;
        }
      } else if (tableRow != -1) {
        y += tableHeight;
        tableHeight = 0;
        tableRow = -1;
      }
      final base = _rowStyle(row);
      final codeInfo = row?.fenced == true
          ? _snapshot.source.substring(row!.codeInfoStart, row.codeInfoEnd)
          : '';
      _RowLayout layout;
      if (i < old.length &&
          !(colorsChanged && row?.kind == RowKind.codeBlock) &&
          old[i].text == text &&
          old[i].codeInfo == codeInfo &&
          _samePresentation(old[i].row, row)) {
        layout = old[i];
        layout.row = row;
        layout.sourceStart = row?.sourceStart ?? sourceOffset;
        if (layout.width != width) {
          // TextPainter can reflow its existing paragraph at a new width;
          // rebuilding all spans and shaping again makes a resize expensive.
          layout.painter.layout(maxWidth: width);
          layout.width = width;
        }
      } else {
        final bits = row?.segments.map((s) => s.styles).toList() ?? [0];
        final span = TextSpan(
          style: base,
          children: row?.kind == RowKind.codeBlock
              ? [
                  for (final token
                      in (controller.codeColors?.highlight(text, codeInfo) ??
                              controller.editor.codeEditing?.highlight(
                                text,
                                codeInfo,
                              ) ??
                              CodeHighlight(null, [
                                CodeToken(0, text.length, null),
                              ]))
                          .tokens)
                    TextSpan(
                      text: text.substring(token.start, token.end),
                      style: base.copyWith(color: _codeColor(token.kind)),
                    ),
                ]
              : row == null
              ? [TextSpan(text: text)]
              : [
                  for (final segment in row.segments)
                    TextSpan(
                      text: text.substring(
                        segment.displayStart,
                        segment.displayEnd,
                      ),
                      style: _inlineStyle(base, segment.styles),
                    ),
                ],
        );
        final painter = TextPainter(
          text: span,
          textDirection: TextDirection.ltr,
          textAlign: row?.alignment == 2
              ? TextAlign.center
              : row?.alignment == 3
              ? TextAlign.right
              : TextAlign.left,
        )..layout(maxWidth: width);
        layout = _RowLayout(
          row,
          row?.sourceStart ?? sourceOffset,
          text,
          bits,
          painter,
          width,
          codeInfo,
        );
        if (i < old.length) old[i].painter.dispose();
      }
      final height = math.max(
        base.fontSize! * (base.height ?? 1.4),
        layout.painter.height,
      );
      final top = row?.kind == RowKind.tableCell ? tableTop : y;
      layout.indent = indent;
      layout.origin = Offset(x, top + 4);
      layout.rect = Rect.fromLTWH(
        x - (row?.kind == RowKind.tableCell ? 8 : 0),
        top,
        width + (row?.kind == RowKind.tableCell ? 16 : 0),
        height + 8,
      );
      if (row?.kind == RowKind.tableCell) {
        tableHeight = math.max(tableHeight, height + 8);
      } else {
        y += height + 8;
      }
      next.add(layout);
      sourceOffset += (sourceLines?[i].length ?? 0) + 1;
    }
    for (var i = next.length; i < old.length; i++) {
      old[i].painter.dispose();
    }
    // A table row shares one height across its independently wrapped cells.
    for (var start = 0; start < next.length;) {
      final row = next[start].row;
      if (row?.kind != RowKind.tableCell) {
        start++;
        continue;
      }
      var end = start + 1, height = next[start].rect.height;
      while (end < next.length &&
          next[end].row?.tableRowBlock == row!.tableRowBlock) {
        height = math.max(height, next[end].rect.height);
        end++;
      }
      for (var i = start; i < end; i++) {
        final r = next[i].rect;
        next[i].rect = Rect.fromLTWH(r.left, r.top, r.width, height);
      }
      start = end;
    }
    _rows = next;
    _contentHeight = y + tableHeight + _padding;
  }

  (int, int) _display(int source) {
    if (_snapshot is FlarkLiveSnapshot) {
      final p = (_snapshot as FlarkLiveSnapshot).document.displayOf(source);
      return (p.row, p.offset);
    }
    for (var i = _rows.length - 1; i >= 0; i--) {
      if (source >= _rows[i].sourceStart) {
        return (
          i,
          (source - _rows[i].sourceStart).clamp(0, _rows[i].text.length),
        );
      }
    }
    return (0, 0);
  }

  Rect get caretRect {
    if (_layoutWidth == null) return Rect.zero;
    _prepareRows();
    final (index, offset) = _display(_snapshot.selection.extent);
    final row = _rows[index];
    final p = row.painter.getOffsetForCaret(
      TextPosition(offset: offset),
      _caretPrototype,
    );
    final height = math.max(
      style.fontSize! * 1.4,
      row.painter.getFullHeightForCaret(
        TextPosition(offset: offset),
        _caretPrototype,
      ),
    );
    return Rect.fromLTWH(
      row.origin.dx + p.dx,
      row.origin.dy + p.dy,
      1.5,
      height,
    );
  }

  int sourceAt(Offset position) {
    _prepareRows();
    final row = _rowAt(position);
    final relative = position - row.origin;
    final p = row.painter.getPositionForOffset(relative);
    if (row.row == null) return row.sourceStart + p.offset;
    final doc = (_snapshot as FlarkLiveSnapshot).document;
    final caret = row.painter.getOffsetForCaret(p, _caretPrototype);
    return doc.pointerAnchorAt(
      row.row!.index,
      p.offset,
      leadingHalf: relative.dx > caret.dx,
    );
  }

  _RowLayout _rowAt(Offset point) {
    for (final row in _rows) {
      if (row.rect.contains(point)) return row;
    }
    var nearest = _rows.first;
    var distance = double.infinity;
    for (final row in _rows) {
      final dy = point.dy < row.rect.top
          ? row.rect.top - point.dy
          : point.dy > row.rect.bottom
          ? point.dy - row.rect.bottom
          : 0.0;
      if (dy < distance) {
        nearest = row;
        distance = dy;
      }
    }
    return nearest;
  }

  int? _taskSourceAt(Offset point) {
    _prepareRows();
    if (_snapshot is! FlarkLiveSnapshot) return null;
    final layout = _rowAt(point),
        model = (_snapshot as FlarkLiveSnapshot).document.model;
    final row = layout.row;
    if (row == null) return null;
    var x = _padding;
    for (final shell in row.shells) {
      if (shell.kind == ShellKind.item) {
        if (shell.task &&
            row.firstLine == model.block(shell.block, BlockField.firstLine) &&
            Rect.fromLTWH(x, layout.origin.dy, 22, 24).contains(point)) {
          return row.sourceStart;
        }
        x += layout.indent;
      } else if (shell.kind == ShellKind.blockQuote) {
        x += layout.indent;
      }
    }
    return null;
  }

  bool toggleTaskAt(Offset point) {
    final source = _taskSourceAt(point);
    if (source == null) return false;
    controller.command(SetSelection.caret(source));
    return controller.command(const ToggleTask());
  }

  void place(Offset point, {bool extend = false}) {
    final target = sourceAt(point);
    controller.command(
      SetSelection(extend ? controller.editor.selection.base : target, target),
    );
  }

  void selectWordAt(Offset point) {
    _prepareRows();
    final row = _rowAt(point);
    final position = row.painter.getPositionForOffset(point - row.origin);
    final word = row.painter.getWordBoundary(position);
    final start =
        row.row?.sourceForDisplay(word.start, anchor: Anchor.after) ??
        row.sourceStart + word.start;
    final end =
        row.row?.sourceForDisplay(word.end, anchor: Anchor.before) ??
        row.sourceStart + word.end;
    controller.command(SetSelection(start, end));
  }

  bool vertical(bool down, {bool extend = false, double? goalX}) {
    final caret = caretRect;
    final (index, _) = _display(_snapshot.selection.extent);
    final row = _rows[index];
    final lines = row.painter.computeLineMetrics();
    final localY = caret.top - row.origin.dy;
    var line = -1, distance = double.infinity;
    for (var i = 0; i < lines.length; i++) {
      final delta = (localY - (lines[i].baseline - lines[i].ascent)).abs();
      if (delta < distance) {
        line = i;
        distance = delta;
      }
    }
    final adjacent = line + (down ? 1 : -1);
    final double y;
    if (adjacent >= 0 && adjacent < lines.length) {
      final next = lines[adjacent];
      y = row.origin.dy + next.baseline - next.ascent + next.height / 2;
    } else {
      // Cross the row's padding as well as its glyphs. A fixed caret-relative
      // step can hit the same row forever, especially for empty paragraphs.
      y = down ? row.rect.bottom + .5 : row.rect.top - .5;
    }
    final x = goalX ?? caret.left;
    final target = sourceAt(Offset(x, y));
    return controller.command(
      SetSelection(extend ? controller.editor.selection.base : target, target),
    );
  }

  bool lineEdge(bool end, {bool extend = false}) {
    _prepareRows();
    final (index, offset) = _display(_snapshot.selection.extent);
    final row = _rows[index];
    final line = row.painter.getLineBoundary(TextPosition(offset: offset));
    final at = end ? line.end : line.start;
    var target =
        row.row?.sourceForDisplay(
          at,
          anchor: end ? Anchor.after : Anchor.before,
        ) ??
        row.sourceStart + at;
    if (_snapshot is FlarkLiveSnapshot) {
      final anchors = (_snapshot as FlarkLiveSnapshot).document.anchorsAt(
        target,
      );
      target = end ? anchors.last : anchors.first;
    }
    return controller.command(
      SetSelection(extend ? controller.editor.selection.base : target, target),
    );
  }

  @override
  bool hitTestSelf(Offset position) => true;
  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) {
    if (!readOnly && _taskSourceAt(position) != null) {
      result.add(HitTestEntry(const _TaskCursorTarget()));
    }
    return false;
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    final canvas = context.canvas;
    final selected = _snapshot.selection;
    final (baseRow, baseOffset) = _display(selected.start);
    final (endRow, endOffset) = _display(selected.end);
    final visible = Rect.fromLTWH(0, scrollOffset, size.width, viewportHeight);
    final painted = <String>[],
        styles = <List<int>>[],
        selectionRects = <Rect>[];
    final resolvedStyles = <List<TextStyle>>[];
    for (var i = 0; i < _rows.length; i++) {
      final layout = _rows[i];
      if (!layout.rect.overlaps(visible)) continue;
      final row = layout.row;
      if (row?.kind == RowKind.codeBlock) {
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            layout.rect.shift(offset).inflate(4),
            const Radius.circular(5),
          ),
          Paint()..color = const Color(0xfff1f3f6),
        );
      }
      if (row?.kind == RowKind.tableCell) {
        canvas.drawRect(
          layout.rect.shift(offset),
          Paint()
            ..color = const Color(0xffd6dce5)
            ..style = PaintingStyle.stroke,
        );
      }
      if (row?.kind == RowKind.thematicBreak) {
        canvas.drawLine(
          offset + layout.rect.centerLeft,
          offset + layout.rect.centerRight,
          Paint()
            ..color = const Color(0xffabb5c3)
            ..strokeWidth = 1,
        );
      }
      if (row != null) {
        var indent = _padding;
        for (final shell in row.shells) {
          if (shell.kind == ShellKind.blockQuote) {
            canvas.drawLine(
              offset + Offset(indent + 5, layout.rect.top),
              offset + Offset(indent + 5, layout.rect.bottom),
              Paint()
                ..color = const Color(0xffb9c5d7)
                ..strokeWidth = 3,
            );
            indent += layout.indent;
          }
          if (shell.kind == ShellKind.item) {
            final model = (_snapshot as FlarkLiveSnapshot).document.model;
            if (row.firstLine ==
                model.block(shell.block, BlockField.firstLine)) {
              if (shell.task) {
                // Task state is UI geometry, independent of symbol-font fallback.
                final fontSize = style.fontSize ?? 17;
                final box = Rect.fromLTWH(
                  offset.dx + indent,
                  offset.dy + layout.origin.dy + fontSize * .3,
                  fontSize * .8,
                  fontSize * .8,
                );
                final pen = Paint()
                  ..color = style.color ?? const Color(0xff253047)
                  ..style = PaintingStyle.stroke
                  ..strokeWidth = 1.4
                  ..strokeCap = StrokeCap.round
                  ..strokeJoin = StrokeJoin.round;
                canvas.drawRRect(
                  RRect.fromRectAndRadius(box, const Radius.circular(2)),
                  pen,
                );
                if (shell.checked) {
                  canvas.drawPath(
                    Path()
                      ..moveTo(
                        box.left + box.width * .2,
                        box.top + box.height * .5,
                      )
                      ..lineTo(
                        box.left + box.width * .43,
                        box.top + box.height * .73,
                      )
                      ..lineTo(
                        box.left + box.width * .8,
                        box.top + box.height * .25,
                      ),
                    pen,
                  );
                }
              } else {
                final marker = shell.ordered
                    ? '${shell.start + shell.itemIndex}.'
                    : '•';
                final painter = TextPainter(
                  text: TextSpan(text: marker, style: style),
                  textDirection: TextDirection.ltr,
                )..layout();
                painter.paint(
                  canvas,
                  offset + Offset(indent, layout.origin.dy),
                );
                painter.dispose();
              }
            }
            indent += layout.indent;
          }
        }
      }
      if (!selected.isCollapsed && i >= baseRow && i <= endRow) {
        final boxes = layout.painter.getBoxesForSelection(
          TextSelection(
            baseOffset: i == baseRow ? baseOffset : 0,
            extentOffset: i == endRow ? endOffset : layout.text.length,
          ),
        );
        for (final box in boxes) {
          final rect = box.toRect().shift(layout.origin);
          selectionRects.add(rect);
          canvas.drawRect(
            rect.shift(offset),
            Paint()..color = const Color(0x553c82ed),
          );
        }
      }
      layout.painter.paint(canvas, offset + layout.origin);
      painted.add(layout.painter.text!.toPlainText());
      styles.add(List.unmodifiable(layout.styles));
      resolvedStyles.add(
        List.unmodifiable(
          ((layout.painter.text as TextSpan).children ?? [])
              .whereType<TextSpan>()
              .map(
                (s) =>
                    (layout.painter.text as TextSpan).style?.merge(s.style) ??
                    s.style ??
                    style,
              ),
        ),
      );
    }
    final candidateCaret = focused && !readOnly && selected.isCollapsed
        ? caretRect
        : null;
    final caret = candidateCaret != null && candidateCaret.overlaps(visible)
        ? candidateCaret
        : null;
    if (caret != null) {
      canvas.drawRect(
        caret.shift(offset),
        Paint()..color = const Color(0xff2864c7),
      );
    }
    onPaint?.call(
      FlarkPaintObservation(
        _revision,
        _snapshot,
        List.unmodifiable(painted),
        List.unmodifiable(styles),
        caret,
        selected.extent,
        List.unmodifiable(selectionRects),
        List.unmodifiable(resolvedStyles),
        ui.PlatformDispatcher.instance.frameData.frameNumber,
      ),
    );
  }

  @override
  void describeSemanticsConfiguration(SemanticsConfiguration config) {
    super.describeSemanticsConfiguration(config);
    config.isSemanticBoundary = true;
    config.isTextField = !readOnly;
    config.isReadOnly = readOnly;
    config.isFocused = focused;
    config.isMultiline = true;
    config.textDirection = TextDirection.ltr;
    final current = controller.editor.snapshot;
    final rows = current is FlarkLiveSnapshot ? current.projection.rows : null;
    final window = current is FlarkSourceSnapshot
        ? SourceWindow.at(current.source, current.selection.extent)
        : null;
    final texts =
        rows?.map((r) => r.text).toList() ??
        current.source.substring(window!.start, window.end).split('\n');
    config.value = texts.join('\n');
    int displayOffset(int source) {
      if (current is! FlarkLiveSnapshot) {
        return (source - window!.start).clamp(0, window.end - window.start);
      }
      final p = current.document.displayOf(source);
      return texts.take(p.row).fold(0, (sum, r) => sum + r.length + 1) +
          p.offset;
    }

    int sourceOffset(int display, {Anchor anchor = Anchor.after}) {
      if (rows == null) {
        return (window!.start + display).clamp(window.start, window.end);
      }
      var remaining = display;
      for (final row in rows) {
        if (remaining <= row.text.length) {
          return row.sourceForDisplay(remaining, anchor: anchor);
        }
        remaining -= row.text.length + 1;
      }
      return current.source.length;
    }

    config.textSelection = TextSelection(
      baseOffset: displayOffset(current.selection.base),
      extentOffset: displayOffset(current.selection.extent),
    );
    if (!readOnly) {
      config.onSetSelection = (selection) => controller.command(
        SetSelection(
          sourceOffset(
            selection.baseOffset,
            anchor: selection.baseOffset <= selection.extentOffset
                ? Anchor.after
                : Anchor.before,
          ),
          sourceOffset(
            selection.extentOffset,
            anchor: selection.baseOffset < selection.extentOffset
                ? Anchor.before
                : Anchor.after,
          ),
        ),
      );
      config.onSetText = (text) {
        // A pathological single grapheme may exceed a source page. Never let
        // an accessibility page replacement expand into its neighboring page.
        if (window != null &&
            (CharacterRange.at(current.source, window.start).isNotEmpty ||
                CharacterRange.at(current.source, window.end).isNotEmpty)) {
          return;
        }
        controller.sourceMode(true);
        controller.command(
          ReplaceRange(
            window?.start ?? 0,
            window?.end ?? controller.text.length,
            text,
          ),
        );
      };
    }
  }
}
