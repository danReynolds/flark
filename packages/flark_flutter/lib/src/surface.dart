import 'dart:math' as math;
import 'dart:ui' as ui;
import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'package:flark/render_model.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/foundation.dart' show mapEquals;
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'controller.dart';
import 'theme.dart';
import 'source_window.dart';
import 'image_previews.dart';

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
    this.frameNumber, {
    this.images = const [],
  });
  final int revision;
  final FlarkEditorSnapshot snapshot;
  final List<String> rows;
  final List<List<int>> styles;
  final List<List<TextStyle>> resolvedStyles;
  final int frameNumber;
  final Rect? caret;
  final int caretSource;
  final List<Rect> selectionRects;
  final List<FlarkImageObservation> images;
}

class FlarkImageObservation {
  const FlarkImageObservation(
    this.sourceStart,
    this.destination,
    this.rect,
    this.state,
  );
  final int sourceStart;
  final String destination, state;
  final Rect rect;
}

class FlarkSurface extends LeafRenderObjectWidget {
  const FlarkSurface({
    super.key,
    required this.controller,
    required this.theme,
    this.textScaler = TextScaler.noScaling,
    required this.focused,
    required this.viewportHeight,
    required this.scrollOffset,
    this.readOnly = false,
    this.onPaint,
    this.onFocus,
    this.revealDuringLayout,
    this.baseUri,
    this.imageProvider,
    this.showImagePreviews = true,
  });
  final Uri? baseUri;
  final FlarkImageProvider? imageProvider;
  final bool showImagePreviews;
  final FlarkController controller;
  final FlarkThemeData theme;
  final TextScaler textScaler;
  final bool focused, readOnly;
  final double viewportHeight, scrollOffset;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  final VoidCallback? onFocus;
  final double Function(Rect, double, double)? revealDuringLayout;
  @override
  RenderFlarkSurface createRenderObject(BuildContext context) =>
      RenderFlarkSurface(
        controller,
        theme,
        textScaler,
        focused,
        readOnly,
        viewportHeight,
        scrollOffset,
        onPaint,
        revealDuringLayout,
        onFocus: onFocus,
        baseUri: baseUri,
        imageProvider: imageProvider,
        showImagePreviews: showImagePreviews,
      );
  @override
  void updateRenderObject(
    BuildContext context,
    RenderFlarkSurface renderObject,
  ) => renderObject.update(
    controller,
    theme,
    textScaler,
    focused,
    readOnly,
    viewportHeight,
    scrollOffset,
    onPaint,
    revealDuringLayout,
    onFocus: onFocus,
    baseUri: baseUri,
    imageProvider: imageProvider,
    showImagePreviews: showImagePreviews,
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
  List<({InlineResource resource, Rect rect})> images = [];
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
    this.theme,
    this.textScaler,
    this.focused,
    this.readOnly,
    this.viewportHeight,
    this.scrollOffset,
    this.onPaint,
    this.revealDuringLayout, {
    this.onFocus,
    Uri? baseUri,
    FlarkImageProvider? imageProvider,
    this.showImagePreviews = true,
  }) {
    _images.configure(baseUri, imageProvider);
    _snapshot = controller.editor.snapshot;
    controller.addListener(_changed);
    controller.codeColors?.addListener(_colorsChanged);
  }
  late final _images = SurfaceImageCache(markNeedsPaint);
  bool showImagePreviews;
  FlarkController controller;
  FlarkThemeData theme;
  TextScaler textScaler;
  TextStyle get style => theme.styles[FlarkTextRole.body]!;
  double metric(FlarkMetric role) => theme.metrics[role]!;
  Color color(FlarkColorRole role) => theme.colors[role]!;
  VoidCallback? onFocus;
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
  double get _padding => metric(FlarkMetric.documentPadding);
  static const _caretPrototype = Rect.fromLTWH(0, 0, 1.5, 22);

  void update(
    FlarkController next,
    FlarkThemeData nextTheme,
    TextScaler nextScaler,
    bool focus,
    bool readonly,
    double height,
    double scroll,
    ValueChanged<FlarkPaintObservation>? observer,
    double Function(Rect, double, double)? reveal, {
    VoidCallback? onFocus,
    Uri? baseUri,
    FlarkImageProvider? imageProvider,
    bool showImagePreviews = true,
  }) {
    this.onFocus = onFocus;
    _images.configure(baseUri, imageProvider);
    if (this.showImagePreviews != showImagePreviews) {
      this.showImagePreviews = showImagePreviews;
      _clearRows();
    }
    if (next != controller) {
      controller.removeListener(_changed);
      controller.codeColors?.removeListener(_colorsChanged);
      _needsReveal = true;
      _images.clear();
      controller = next;
      controller.addListener(_changed);
      controller.codeColors?.addListener(_colorsChanged);
      _clearRows();
    }
    if (nextTheme != theme || nextScaler != textScaler) {
      final geometryChanged =
          nextScaler != textScaler ||
          !mapEquals(nextTheme.metrics, theme.metrics) ||
          {...theme.styles.keys, ...nextTheme.styles.keys}.any(
            (role) =>
                (theme.styles[role] ?? const TextStyle()).compareTo(
                  nextTheme.styles[role] ?? const TextStyle(),
                ) ==
                RenderComparison.layout,
          );
      theme = nextTheme;
      textScaler = nextScaler;
      _clearRows();
      if (geometryChanged) _needsReveal = true;
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
    _images.clear();
    super.dispose();
  }

  TextStyle _rowStyle(ProjectedRow? row) {
    var s = style;
    if (row?.shells.any((s) => s.kind == ShellKind.blockQuote) == true) {
      s = s.merge(theme.styles[FlarkTextRole.quote]);
    }
    if (row?.kind == RowKind.heading) {
      s = s
          .copyWith(
            fontSize: style.fontSize! * (1.75 - (row!.headingLevel - 1) * .12),
            fontWeight: FontWeight.w700,
            height: 1.35,
          )
          .merge(
            theme.styles[FlarkTextRole.values[FlarkTextRole.heading1.index +
                row.headingLevel -
                1]],
          );
    }
    if (row?.kind == RowKind.codeBlock || controller.editor.sourceMode) {
      s = s
          .copyWith(fontSize: style.fontSize! * .9)
          .merge(theme.styles[FlarkTextRole.codeBlock]);
    }
    if (row?.header == true) {
      s = s.merge(theme.styles[FlarkTextRole.tableHeader]);
    }
    return s;
  }

  TextStyle _inlineStyle(TextStyle base, int bits) {
    var result = base;
    for (final (bit, role) in const [
      (Style.strong, FlarkTextRole.strong),
      (Style.emphasis, FlarkTextRole.emphasis),
      (Style.code, FlarkTextRole.inlineCode),
      (Style.strikethrough, FlarkTextRole.strikethrough),
      (Style.link, FlarkTextRole.link),
    ]) {
      if (bits & bit == 0) continue;
      final addition = theme.styles[role];
      final decorations = [
        if (result.decoration != null) result.decoration!,
        if (addition?.decoration != null) addition!.decoration!,
      ];
      result = result.merge(addition);
      if (decorations.isNotEmpty) {
        result = result.copyWith(
          decoration: TextDecoration.combine(decorations),
        );
      }
    }
    return result;
  }

  Color? _codeColor(String? kind) => switch (codeSyntaxRole(kind)) {
    // The scope-to-role table is shared with every other host; only the
    // colours are this one's.
    CodeSyntaxRole.keyword => theme.syntaxColors[FlarkSyntaxRole.keyword],
    CodeSyntaxRole.string => theme.syntaxColors[FlarkSyntaxRole.string],
    CodeSyntaxRole.number => theme.syntaxColors[FlarkSyntaxRole.number],
    CodeSyntaxRole.comment => theme.syntaxColors[FlarkSyntaxRole.comment],
    CodeSyntaxRole.function => theme.syntaxColors[FlarkSyntaxRole.function],
    CodeSyntaxRole.variable => theme.syntaxColors[FlarkSyntaxRole.variable],
    null => null,
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
    final imageRows = <int, List<InlineResource>>{};
    if (showImagePreviews && _snapshot is FlarkLiveSnapshot) {
      final doc = (_snapshot as FlarkLiveSnapshot).document;
      for (final resource in doc.images) {
        if (resource.isImage) {
          (imageRows[doc.displayOf(resource.contentStart).row] ??= []).add(
            resource,
          );
        }
      }
    }
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
          ? metric(FlarkMetric.listIndent)
          : math.min(
              metric(FlarkMetric.listIndent),
              math.max(0.0, available - 40) / depth,
            );
      final tablePadding = math.min(
        metric(FlarkMetric.tablePadding),
        available / 4,
      );
      final codePadding = row?.kind == RowKind.codeBlock
          ? math.min(metric(FlarkMetric.codePadding), available / 4)
          : 0.0;
      var x = _padding + depth * indent;
      var width = math.max(24.0, available - depth * indent);
      if (row?.kind == RowKind.tableCell) {
        final model = (_snapshot as FlarkLiveSnapshot).document.model;
        final columns = model.block(row!.tableBlock, BlockField.attr0);
        width = math.max(24, available / columns - 2 * tablePadding);
        x = _padding + row.column * (width + 2 * tablePadding) + tablePadding;
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
      x += codePadding;
      width = math.max(24.0, width - 2 * codePadding);
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
          textScaler: textScaler,
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
        textScaler.scale(base.fontSize!) * (base.height ?? 1.4),
        layout.painter.height,
      );
      final top = row?.kind == RowKind.tableCell ? tableTop : y;
      final images = imageRows[i] ?? const <InlineResource>[];
      // Stable slots keep async decoding out of caret/layout transactions.
      layout.images = [
        for (var n = 0; n < images.length; n++)
          (
            resource: images[n],
            rect: Rect.fromLTWH(
              x,
              top +
                  height +
                  2 * codePadding +
                  metric(FlarkMetric.imageSpacing) +
                  n *
                      (metric(FlarkMetric.imageHeight) +
                          metric(FlarkMetric.imageSpacing)),
              math.min(width, metric(FlarkMetric.imageMaxWidth)),
              metric(FlarkMetric.imageHeight),
            ),
          ),
      ];
      final spacing = metric(
        row?.kind == RowKind.heading
            ? FlarkMetric.headingSpacing
            : FlarkMetric.rowSpacing,
      );
      final totalHeight =
          height +
          spacing +
          2 * codePadding +
          images.length *
              (metric(FlarkMetric.imageHeight) +
                  metric(FlarkMetric.imageSpacing));

      layout.indent = indent;
      layout.origin = Offset(
        x,
        top + math.min(metric(FlarkMetric.rowInset), spacing / 2) + codePadding,
      );
      layout.rect = Rect.fromLTWH(
        x - (row?.kind == RowKind.tableCell ? tablePadding : codePadding),
        top,
        width +
            2 * (row?.kind == RowKind.tableCell ? tablePadding : codePadding),
        totalHeight,
      );
      if (row?.kind == RowKind.tableCell) {
        tableHeight = math.max(tableHeight, totalHeight);
      } else {
        y += totalHeight;
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
      textScaler.scale(style.fontSize!) * 1.4,
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
            Rect.fromLTWH(
              x,
              layout.origin.dy,
              math.max(layout.indent, textScaler.scale(style.fontSize!)),
              math.max(24, textScaler.scale(style.fontSize!) * 1.4),
            ).contains(point)) {
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

  InlineResource? imageAt(Offset point) {
    _prepareRows();
    for (final layout in _rows) {
      for (final image in layout.images) {
        if (image.rect.contains(point)) return image.resource;
      }
    }
    return null;
  }

  InlineResource? linkAt(Offset point) {
    _prepareRows();
    if (_snapshot is! FlarkLiveSnapshot) return null;
    final doc = (_snapshot as FlarkLiveSnapshot).document;
    final layout = _rowAt(point);
    final index = layout.row!.index;
    for (final resource in doc.resources.reversed) {
      if (resource.isImage) continue;
      final start = doc.displayOf(resource.contentStart);
      final end = doc.displayOf(resource.contentEnd);
      if (index < start.row || index > end.row) continue;
      final boxes = layout.painter.getBoxesForSelection(
        TextSelection(
          baseOffset: index == start.row ? start.offset : 0,
          extentOffset: index == end.row ? end.offset : layout.text.length,
        ),
      );
      if (boxes.any(
        (box) => box.toRect().shift(layout.origin).contains(point),
      )) {
        return resource;
      }
      if (layout.images.any(
        (image) =>
            resource.start <= image.resource.start &&
            image.resource.end <= resource.end &&
            image.rect.contains(point),
      )) {
        return resource;
      }
    }
    return null;
  }

  Rect? linkRect(InlineResource resource, {required int nearSource}) {
    _prepareRows();
    if (_snapshot is! FlarkLiveSnapshot) return null;
    final doc = (_snapshot as FlarkLiveSnapshot).document;
    final start = doc.displayOf(resource.contentStart),
        end = doc.displayOf(resource.contentEnd);
    final near = doc.displayOf(nearSource);
    final rowIndex = near.row.clamp(start.row, end.row);
    final row = _rows[rowIndex];
    final boxes = row.painter.getBoxesForSelection(
      TextSelection(
        baseOffset: rowIndex == start.row ? start.offset : 0,
        extentOffset: rowIndex == end.row ? end.offset : row.text.length,
      ),
    );
    if (boxes.isEmpty) return null;
    final caret = caretRect;
    return boxes
        .map((box) => box.toRect().shift(row.origin))
        .reduce(
          (a, b) =>
              (a.center.dy - caret.center.dy).abs() <=
                  (b.center.dy - caret.center.dy).abs()
              ? a
              : b,
        );
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
    if (!readOnly &&
            (_taskSourceAt(position) != null || imageAt(position) != null) ||
        readOnly && linkAt(position) != null) {
      result.add(HitTestEntry(const _TaskCursorTarget()));
    }
    return false;
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    final canvas = context.canvas;
    final selected = _snapshot.selection;
    canvas.drawRect(
      Rect.fromLTWH(
        offset.dx,
        offset.dy + scrollOffset,
        size.width,
        viewportHeight,
      ),
      Paint()..color = color(FlarkColorRole.canvas),
    );
    final (baseRow, baseOffset) = _display(selected.start);
    final (endRow, endOffset) = _display(selected.end);
    final visible = Rect.fromLTWH(0, scrollOffset, size.width, viewportHeight);
    _images.visible([
      for (final layout in _rows)
        for (final image in layout.images)
          if (image.rect.overlaps(visible)) image.resource.destination,
    ]);
    final painted = <String>[],
        styles = <List<int>>[],
        selectionRects = <Rect>[];
    final resolvedStyles = <List<TextStyle>>[];
    final paintedImages = <FlarkImageObservation>[];
    for (var i = 0; i < _rows.length; i++) {
      final layout = _rows[i];
      if (!layout.rect.overlaps(visible)) continue;
      if (layout.row?.shells.any((s) => s.kind == ShellKind.blockQuote) ==
          true) {
        canvas.drawRect(
          layout.rect.shift(offset),
          Paint()..color = color(FlarkColorRole.quoteBackground),
        );
      }
      for (final image in layout.images) {
        if (!image.rect.overlaps(visible)) continue;
        final rect = image.rect.shift(offset);
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            rect,
            Radius.circular(metric(FlarkMetric.imageRadius)),
          ),
          Paint()..color = color(FlarkColorRole.imageBackground),
        );
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            rect,
            Radius.circular(metric(FlarkMetric.imageRadius)),
          ),
          Paint()
            ..color = color(FlarkColorRole.imageBorder)
            ..style = PaintingStyle.stroke,
        );
        final entry = _images.get(image.resource.destination);
        paintedImages.add(
          FlarkImageObservation(
            image.resource.start,
            image.resource.destination,
            image.rect,
            entry?.info != null
                ? 'loaded'
                : entry == null || entry.failed
                ? 'failed'
                : 'loading',
          ),
        );
        if (entry?.info != null) {
          paintImage(
            canvas: canvas,
            rect: rect.deflate(6),
            image: entry!.info!.image,
            fit: BoxFit.contain,
            filterQuality: FilterQuality.medium,
          );
        } else {
          final label = TextPainter(
            text: TextSpan(
              text: entry == null || entry.failed
                  ? 'Image unavailable'
                  : 'Loading image…',
              style: style.merge(theme.styles[FlarkTextRole.imageLabel]),
            ),
            textDirection: TextDirection.ltr,
          )..layout(maxWidth: math.max(1, rect.width - 16));
          label.paint(
            canvas,
            rect.center - Offset(label.width / 2, label.height / 2),
          );
          label.dispose();
        }
      }
      final row = layout.row;
      if (row?.kind == RowKind.codeBlock) {
        canvas.drawRRect(
          RRect.fromRectAndRadius(
            layout.rect.shift(offset),
            Radius.circular(metric(FlarkMetric.codeRadius)),
          ),
          Paint()..color = color(FlarkColorRole.codeBackground),
        );
      }
      if (row?.kind == RowKind.tableCell) {
        if (row!.header) {
          canvas.drawRect(
            layout.rect.shift(offset),
            Paint()..color = color(FlarkColorRole.tableHeaderBackground),
          );
        }
        canvas.drawRect(
          layout.rect.shift(offset),
          Paint()
            ..color = color(FlarkColorRole.tableBorder)
            ..style = PaintingStyle.stroke,
        );
      }
      if (row?.kind == RowKind.thematicBreak) {
        canvas.drawLine(
          offset + layout.rect.centerLeft,
          offset + layout.rect.centerRight,
          Paint()
            ..color = color(FlarkColorRole.rule)
            ..strokeWidth = metric(FlarkMetric.ruleWidth),
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
                ..color = color(FlarkColorRole.quoteRail)
                ..strokeWidth = metric(FlarkMetric.quoteRailWidth),
            );
            indent += layout.indent;
          }
          if (shell.kind == ShellKind.item) {
            final model = (_snapshot as FlarkLiveSnapshot).document.model;
            if (row.firstLine ==
                model.block(shell.block, BlockField.firstLine)) {
              if (shell.task) {
                // Task state is UI geometry, independent of symbol-font fallback.
                final fontSize = textScaler.scale(style.fontSize!);
                final box = Rect.fromLTWH(
                  offset.dx + indent,
                  offset.dy + layout.origin.dy + fontSize * .3,
                  fontSize * .8,
                  fontSize * .8,
                );
                final pen = Paint()
                  ..color = color(FlarkColorRole.taskBorder)
                  ..style = PaintingStyle.stroke
                  ..strokeWidth = 1.4
                  ..strokeCap = StrokeCap.round
                  ..strokeJoin = StrokeJoin.round;
                canvas.drawRRect(
                  RRect.fromRectAndRadius(box, const Radius.circular(2)),
                  Paint()..color = color(FlarkColorRole.taskFill),
                );
                canvas.drawRRect(
                  RRect.fromRectAndRadius(box, const Radius.circular(2)),
                  pen,
                );
                if (shell.checked) {
                  pen.color = color(FlarkColorRole.taskCheck);
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
                  text: TextSpan(
                    text: marker,
                    style: style.merge(theme.styles[FlarkTextRole.listMarker]),
                  ),
                  textScaler: textScaler,
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
            Paint()..color = color(FlarkColorRole.selection),
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
        Paint()..color = color(FlarkColorRole.caret),
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
        images: List.unmodifiable(paintedImages),
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
    if (!readOnly) {
      config.isEnabled = true;
      config.onFocus = onFocus;
    }
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
