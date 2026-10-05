import 'dart:math' as math;
import 'dart:ui' as ui;
import 'package:flark/flark.dart';
import 'package:flark/code.dart';

import 'package:flutter/rendering.dart';
import 'package:flutter/foundation.dart' show kIsWeb, mapEquals;
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
    this.selectable = false,
    this.onCopy,
    this.onPaint,
    this.onFocus,
    this.revealDuringLayout,
    this.baseUri,
    this.imageProvider,
    this.showImagePreviews = true,
    this.handles,
  });
  final Uri? baseUri;
  final FlarkImageProvider? imageProvider;
  final bool showImagePreviews;
  final FlarkSurfaceController controller;
  final FlarkThemeData theme;
  final TextScaler textScaler;
  final bool focused, readOnly, selectable;
  final VoidCallback? onCopy;
  final double viewportHeight, scrollOffset;
  final ValueChanged<FlarkPaintObservation>? onPaint;
  final VoidCallback? onFocus;
  final double Function(Rect, double, double)? revealDuringLayout;

  /// Anchors for touch selection handles at the selection's start and end.
  final (LayerLink, LayerLink)? handles;
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
        selectable: selectable,
        onCopy: onCopy,
        baseUri: baseUri,
        imageProvider: imageProvider,
        showImagePreviews: showImagePreviews,
        handles: handles,
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
    selectable: selectable,
    onCopy: onCopy,
    baseUri: baseUri,
    imageProvider: imageProvider,
    showImagePreviews: showImagePreviews,
    handles: handles,
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
    this.base,
  );
  ProjectedRow? row;
  int sourceStart;
  final String text;
  final List<int> styles;
  final TextPainter painter;
  double width;
  final String codeInfo;

  /// The row style the painter was shaped with. Reuse requires the same style
  /// inputs, so a reused row does not merge its text styles again.
  final TextStyle base;
  Rect rect = Rect.zero;
  Rect blockRect = Rect.zero;
  Offset origin = Offset.zero;
  List<({Shell shell, double left, double width})> containers = const [];
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
    this.selectable = false,
    this.onCopy,
    Uri? baseUri,
    FlarkImageProvider? imageProvider,
    this.showImagePreviews = true,
    this.handles,
  }) {
    _images.configure(baseUri, imageProvider);
    _snapshot = controller.editor.snapshot;
    controller.addListener(_changed);
  }

  /// Rows shaped by surfaces in this isolate. Reuse regressions assert that an
  /// edit shapes only the rows whose presentation changed.
  static int shapedRows = 0;

  late final _images = SurfaceImageCache(markNeedsPaint);
  bool showImagePreviews;
  FlarkSurfaceController controller;
  FlarkThemeData theme;
  TextScaler textScaler;
  TextStyle get style => theme.styles[FlarkTextRole.body]!;
  double metric(FlarkMetric role) => theme.metrics[role]!;
  Color color(FlarkColorRole role) => theme.colors[role]!;
  VoidCallback? onFocus;
  bool focused, readOnly, selectable;
  VoidCallback? onCopy;
  double viewportHeight, scrollOffset;
  ValueChanged<FlarkPaintObservation>? onPaint;
  double Function(Rect, double, double)? revealDuringLayout;
  bool _needsReveal = true;
  double _contentHeight = 0;

  /// Leaders for the selection handles, painted at the bottom of the
  /// selection's first and last carets while a touch selection shows them.
  (LayerLink, LayerLink)? handles;
  @override
  bool get alwaysNeedsCompositing => handles != null;
  List<_RowLayout> _rows = [];
  // Keep one layout per mode. A temporary Source excursion must not discard
  // every shaped live paragraph. Both lists remain bounded by admission/page
  // limits, and every reused row is checked against the current presentation.
  List<_RowLayout> _otherRows = [];
  bool _sourceLayout = false;
  // List marker painters from the last paint. Their style and text scale only
  // change through _clearRows, which disposes them.
  Map<String, TextPainter> _markers = {};
  late FlarkEditorSnapshot _snapshot;
  int _revision = 0;
  double? _layoutWidth;
  Object? _layoutContent;
  double get _visibleHeight =>
      viewportHeight > 0 ? viewportHeight : size.height;

  double get _padding => metric(FlarkMetric.documentPadding);
  static const _caretPrototype = Rect.fromLTWH(0, 0, 1.5, 22);

  void update(
    FlarkSurfaceController next,
    FlarkThemeData nextTheme,
    TextScaler nextScaler,
    bool focus,
    bool readonly,
    double height,
    double scroll,
    ValueChanged<FlarkPaintObservation>? observer,
    double Function(Rect, double, double)? reveal, {
    VoidCallback? onFocus,
    bool selectable = false,
    VoidCallback? onCopy,
    Uri? baseUri,
    FlarkImageProvider? imageProvider,
    bool showImagePreviews = true,
    (LayerLink, LayerLink)? handles,
  }) {
    // The editor rebuilds for every edit, focus change and toolbar update.
    // Edits already reach this object through its controller listener, so only
    // mark the work that a changed argument requires. Invalidating layout and
    // semantics unconditionally rebuilt the document's semantic text each time.
    var layout = false, paint = false, semantics = false;
    if (this.onFocus != onFocus) {
      this.onFocus = onFocus;
      semantics = true;
    }
    if (this.selectable != selectable || this.onCopy != onCopy) {
      this.selectable = selectable;
      this.onCopy = onCopy;
      semantics = true;
    }
    if (_images.configure(baseUri, imageProvider)) paint = true;
    if (this.showImagePreviews != showImagePreviews) {
      this.showImagePreviews = showImagePreviews;
      _clearRows();
      layout = true;
    }
    if (this.handles != handles) {
      this.handles = handles;
      markNeedsCompositingBitsUpdate();
      paint = true;
    }
    if (next != controller) {
      controller.removeListener(_changed);
      _described = null;
      _needsReveal = true;
      _images.clear();
      controller = next;
      controller.addListener(_changed);
      _clearRows();
      layout = semantics = true;
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
      layout = true;
    }
    if ((!focused && focus) || viewportHeight != height) _needsReveal = true;
    if (focused != focus || readOnly != readonly || viewportHeight != height) {
      layout = semantics = true;
    }
    focused = focus;
    readOnly = readonly;
    viewportHeight = height;
    if (onPaint != observer) {
      onPaint = observer;
      paint = true;
    }
    revealDuringLayout = reveal;
    if (layout) {
      scrollOffset = scroll;
      markNeedsLayout();
    } else {
      scrolled(scroll);
      if (paint) markNeedsPaint();
    }
    if (semantics) markNeedsSemanticsUpdate();
  }

  /// The enclosing viewport moved. Only the painted rows and the fences that
  /// deserve colors change; layout and semantics do not.
  void scrolled(double offset) {
    if (offset == scrollOffset) return;
    scrollOffset = offset;
    markNeedsPaint();
  }

  void _changed() {
    _needsReveal = true;
    markNeedsLayout();
    markNeedsSemanticsUpdate();
  }

  /// The snapshot the semantics last described. Flutter's web engine moves
  /// DOM focus to a focused text field's semantics element whenever that
  /// node changes, and a browser ends its composition when the input element
  /// loses focus. With accessibility on, every composed update was committed
  /// and the next one appended ("nににほ日本" for 日本). While the platform
  /// composes on the web, the semantics keep describing the document as it
  /// was before, so that the node does not change until the composition ends.
  FlarkEditorSnapshot? _described;

  bool get _composingOnWeb => kIsWeb && controller.composing;

  void _clearRows() {
    _layoutContent = null;
    for (final row in [..._rows, ..._otherRows]) {
      row.painter.dispose();
    }
    for (final painter in _markers.values) {
      painter.dispose();
    }
    _rows = [];
    _otherRows = [];
    _markers = {};
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
    _clearRows();
    _images.clear();
    super.dispose();
  }

  static bool _quoted(ProjectedRow row) =>
      row.shells.any((s) => s.kind == ShellKind.blockQuote);

  TextStyle _rowStyle(ProjectedRow? row) {
    var s = style;
    if (row != null && _quoted(row)) {
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

  /// Whether a layout shaped for [before] can paint [after]: the same text,
  /// segment styles and every input of [_rowStyle].
  bool _samePresentation(ProjectedRow? before, ProjectedRow? after) {
    if (before == null || after == null) return before == after;
    if (before.kind != after.kind ||
        _quoted(before) != _quoted(after) ||
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
      _sourceLayout = sourceMode;
      _layoutContent = null;
    }
    final window = _snapshot is FlarkSourceSnapshot
        ? SourceWindow.at(_snapshot.source, _snapshot.selection.extent)
        : null;
    final content = _snapshot is FlarkLiveSnapshot
        ? (_snapshot as FlarkLiveSnapshot).projection
        : (_snapshot.source, window!.start, window.end);
    if (content == _layoutContent &&
        constraints.maxWidth == _layoutWidth &&
        _rows.isNotEmpty) {
      return;
    }
    _layoutContent = content;
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
    final count = projected?.length ?? sourceLines!.length;
    String textAt(int i) =>
        projected?[i].text ?? sourceLines![i].replaceAll('\r', '');
    String codeInfoOf(ProjectedRow? row) => row?.fenced == true
        ? _snapshot.source.substring(row!.codeInfoStart, row.codeInfoEnd)
        : '';
    // A code row's colors follow from its text and info string alone.
    bool reusable(int previous, int i) {
      final layout = old[previous], row = projected?[i];
      return layout.text == textAt(i) &&
          layout.codeInfo == codeInfoOf(row) &&
          _samePresentation(layout.row, row);
    }

    // An edit replaces one contiguous run of rows; the rest only move. Match
    // unchanged rows from both ends so a row inserted or removed near the top
    // does not misalign, and reshape, every layout after it.
    var head = 0, tail = 0;
    while (head < count && head < old.length && reusable(head, head)) {
      head++;
    }
    while (tail < count - head &&
        tail < old.length - head &&
        reusable(old.length - 1 - tail, count - 1 - tail)) {
      tail++;
    }
    final kept = List<bool>.filled(old.length, false);
    var sourceOffset = window?.start ?? 0, y = _padding;
    var tableRow = -1, tableTop = 0.0, tableHeight = 0.0;
    for (var i = 0; i < count; i++) {
      final row = projected?[i];

      final shells =
          row?.shells
              .where(
                (s) =>
                    s.kind == ShellKind.item || s.kind == ShellKind.blockQuote,
              )
              .toList() ??
          <Shell>[];
      double inset(Shell shell) => metric(
        shell.kind == ShellKind.blockQuote
            ? FlarkMetric.quoteIndent
            : FlarkMetric.listIndent,
      );
      final requested = shells.fold(0.0, (sum, shell) => sum + inset(shell));
      final scale = requested == 0
          ? 1.0
          : math.min(1.0, math.max(0.0, available - 40) / requested);
      final containers = <({Shell shell, double left, double width})>[];
      var contentLeft = _padding;
      for (final shell in shells) {
        final width = inset(shell) * scale;
        containers.add((shell: shell, left: contentLeft, width: width));
        contentLeft += width;
      }
      final tablePadding = math.min(
        metric(FlarkMetric.tablePadding),
        available / 4,
      );
      final codePadding = row?.kind == RowKind.codeBlock
          ? math.min(metric(FlarkMetric.codePadding), available / 4)
          : 0.0;
      var x = contentLeft;
      var width = math.max(24.0, available - (contentLeft - _padding));
      if (row?.kind == RowKind.tableCell) {
        final model = (_snapshot as FlarkLiveSnapshot).document.model;
        final columns = model.blockAttr(row!.tableBlock);
        width = math.max(24, width / columns - 2 * tablePadding);
        x =
            contentLeft +
            row.column * (width + 2 * tablePadding) +
            tablePadding;
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
      final previous = i < head
          ? i
          : i >= count - tail
          ? i - count + old.length
          : i < old.length - tail && reusable(i, i)
          ? i
          : -1;
      _RowLayout layout;
      if (previous >= 0) {
        kept[previous] = true;
        layout = old[previous];
        layout.row = row;
        layout.sourceStart = row?.sourceStart ?? sourceOffset;
        if (layout.width != width) {
          // TextPainter can reflow its existing paragraph at a new width;
          // rebuilding all spans and shaping again makes a resize expensive.
          layout.painter.layout(maxWidth: width);
          layout.width = width;
        }
      } else {
        final text = textAt(i), codeInfo = codeInfoOf(row);
        final base = _rowStyle(row);
        final bits = row?.segments.map((s) => s.styles).toList() ?? [0];
        final span = TextSpan(
          style: base,
          children: row?.kind == RowKind.codeBlock
              ? [
                  for (final token
                      in (controller.editor.codeEditing?.highlight(
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
        shapedRows++;
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
          base,
        );
      }
      final base = layout.base;
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

      layout.containers = containers;
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
      layout.blockRect = row?.kind == RowKind.codeBlock
          ? Rect.fromLTWH(
              layout.rect.left,
              top + math.min(metric(FlarkMetric.rowInset), spacing / 2),
              layout.rect.width,
              height + 2 * codePadding,
            )
          : layout.rect;
      if (row?.kind == RowKind.tableCell) {
        tableHeight = math.max(tableHeight, totalHeight);
      } else {
        y += totalHeight;
      }
      next.add(layout);
      sourceOffset += (sourceLines?[i].length ?? 0) + 1;
    }
    for (var i = 0; i < old.length; i++) {
      if (!kept[i]) old[i].painter.dispose();
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

  (int, int) _display(int source, {int? tableCell}) {
    if (_snapshot is FlarkLiveSnapshot) {
      final p = (_snapshot as FlarkLiveSnapshot).projection.displayForSource(
        source,
        tableCell: tableCell,
      )!;
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
    // Preparing rows catches the snapshot up with unpainted input.
    _prepareRows();
    final selection = _snapshot.selection;
    return caretRectAt(selection.extent, tableCell: selection.tableCell);
  }

  /// The visible part of the document, in this surface's coordinates.
  Rect get visibleRect =>
      Rect.fromLTWH(0, scrollOffset, size.width, _visibleHeight);

  /// The line holding a caret at [source]: its row's width at the caret's
  /// height.
  Rect lineRectAt(int source) {
    final caret = caretRectAt(source);
    if (_layoutWidth == null) return caret;
    final row = _rows[_display(source).$1].rect;
    return Rect.fromLTRB(row.left, caret.top, row.right, caret.bottom);
  }

  /// The selection endpoint drawn upstream, and the snapshot it belongs to:
  /// at the end of a soft-wrapped line, where End or a hit past that end put
  /// it, rather than at the start of the next line, which has the same
  /// offset. The kernel keeps no affinity, so every later snapshot is drawn
  /// downstream again unless a placement records it anew.
  ({FlarkEditorSnapshot snapshot, int offset})? _upstream;

  TextAffinity _affinityAt(int source) {
    final upstream = _upstream;
    return upstream != null &&
            identical(upstream.snapshot, _snapshot) &&
            source == upstream.offset
        ? TextAffinity.upstream
        : TextAffinity.downstream;
  }

  /// Records whether [offset], a selection endpoint of the snapshot the last
  /// command produced, is drawn upstream.
  void _placedAt(int offset, {required bool upstream}) {
    final snapshot = controller.editor.snapshot;
    final current = _upstream;
    if (upstream
        ? current != null &&
              identical(current.snapshot, snapshot) &&
              current.offset == offset
        : current == null) {
      return;
    }
    _upstream = upstream ? (snapshot: snapshot, offset: offset) : null;
    // The endpoint may not have moved, only the line it is drawn on.
    markNeedsPaint();
  }

  /// Records whether the selection a placement just produced ends upstream.
  void _placed({required bool upstream}) =>
      _placedAt(controller.editor.selection.extent, upstream: upstream);

  /// Whether [offset] in [row] ends one visual line and starts the next.
  static bool _wrapsAt(_RowLayout row, int offset) {
    if (offset <= 0 || offset >= row.text.length) return false;
    final upstream = row.painter.getOffsetForCaret(
      TextPosition(offset: offset, affinity: TextAffinity.upstream),
      _caretPrototype,
    );
    final downstream = row.painter.getOffsetForCaret(
      TextPosition(offset: offset),
      _caretPrototype,
    );
    return upstream.dy != downstream.dy;
  }

  Rect caretRectAt(int source, {int? tableCell}) {
    if (_layoutWidth == null) return Rect.zero;
    _prepareRows();
    final (index, offset) = _display(source, tableCell: tableCell);
    final row = _rows[index];
    final position = TextPosition(
      offset: offset,
      affinity: _affinityAt(source),
    );
    final p = row.painter.getOffsetForCaret(position, _caretPrototype);
    final height = math.max(
      textScaler.scale(style.fontSize!) * 1.4,
      row.painter.getFullHeightForCaret(position, _caretPrototype),
    );
    return Rect.fromLTWH(
      row.origin.dx + p.dx,
      row.origin.dy + p.dy,
      1.5,
      height,
    );
  }

  int sourceAt(Offset position) => hitAt(position).source;

  /// The source offset a pointer at [position] selects, and whether it lies
  /// past the end of a soft-wrapped line, where an extent placed at it stays
  /// drawn ([extentPlaced]).
  ({int source, bool lineEnd}) hitAt(Offset position) {
    _prepareRows();
    final row = _rowAt(position);
    final relative = position - row.origin;
    final p = row.painter.getPositionForOffset(relative);
    final lineEnd =
        p.affinity == TextAffinity.upstream && _wrapsAt(row, p.offset);
    if (row.row == null) {
      return (source: row.sourceStart + p.offset, lineEnd: lineEnd);
    }
    final doc = (_snapshot as FlarkLiveSnapshot).document;
    final caret = row.painter.getOffsetForCaret(p, _caretPrototype);
    return (
      source: doc.pointerAnchorAt(
        row.row!.index,
        p.offset,
        leadingHalf: relative.dx > caret.dx,
      ),
      lineEnd: lineEnd,
    );
  }

  /// Whether [offset] is drawn at the end of a soft-wrapped line.
  bool drawnAtLineEnd(int offset) =>
      _affinityAt(offset) == TextAffinity.upstream;

  /// Records whether [offset], a selection endpoint the last command placed,
  /// is drawn at the end of a soft-wrapped line: where a [hitAt] past that
  /// end put it, or where it was drawn before.
  void placedAt(int offset, {required bool lineEnd}) =>
      _placedAt(offset, upstream: lineEnd);

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
    for (final container in layout.containers) {
      final shell = container.shell;
      if (shell.kind == ShellKind.item) {
        if (shell.task &&
            row.firstLine == model.blockFirstLine(shell.block) &&
            Rect.fromLTWH(
              container.left,
              layout.origin.dy,
              container.width,
              math.max(24, textScaler.scale(style.fontSize!) * 1.4),
            ).contains(point)) {
          return row.sourceStart;
        }
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

  bool place(Offset point, {bool extend = false}) {
    _prepareRows();
    final layout = _rowAt(point), row = layout.row;
    final relative = point - layout.origin;
    final position = layout.painter.getPositionForOffset(relative);
    final bool changed;
    if (row != null) {
      final caret = layout.painter.getOffsetForCaret(position, _caretPrototype);
      changed = controller.command(
        PlaceCaret(
          row.index,
          position.offset,
          leadingHalf: relative.dx > caret.dx,
          extend: extend,
        ),
      );
    } else {
      final target = layout.sourceStart + position.offset;
      changed = controller.command(
        SetSelection(
          extend ? controller.editor.selection.base : target,
          target,
        ),
      );
    }
    // A hit past the end of a soft-wrapped line leaves the caret drawn there.
    _placed(
      upstream:
          position.affinity == TextAffinity.upstream &&
          _wrapsAt(layout, position.offset),
    );
    return changed;
  }

  void selectWordAt(Offset point) {
    final (start, end) = wordAt(point);
    controller.command(SetSelection(start, end));
  }

  /// The source range of the word drawn at [point].
  (int, int) wordAt(Offset point) {
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
    return (start, end);
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
    final (index, _) = _display(
      _snapshot.selection.extent,
      tableCell: _snapshot.selection.tableCell,
    );
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
    // Up on the first line or Down on the last has nowhere to go. It is no
    // press: a pending style and the typing's undo step outlast it, as they
    // do Left at the document's start.
    if (!extend &&
        (adjacent < 0 || adjacent >= lines.length) &&
        (down ? index == _rows.length - 1 : index == 0)) {
      return false;
    }
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
    return place(Offset(x, y), extend: extend);
  }

  /// The start or end of the visual line the selection's extent is drawn on:
  /// its outermost source anchor, as row edges take, with the row and the
  /// display offset. Null for a caret in a missing table cell, which has no
  /// source of its own to move within.
  ({int target, _RowLayout row, int at})? _lineEdge(bool end) {
    _prepareRows();
    final selection = _snapshot.selection;
    if (selection.tableCell != null) return null;
    final (index, offset) = _display(selection.extent);
    final row = _rows[index];
    final line = row.painter.getLineBoundary(
      TextPosition(offset: offset, affinity: _affinityAt(selection.extent)),
    );
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
    return (target: target, row: row, at: at);
  }

  /// The source offset [lineEdge] moves the extent to, without moving it.
  int? lineEdgeTarget(bool end) => _lineEdge(end)?.target;

  bool lineEdge(bool end, {bool extend = false}) {
    final edge = _lineEdge(end);
    if (edge == null) return false;
    final changed = controller.command(
      SetSelection(
        extend ? controller.editor.selection.base : edge.target,
        edge.target,
      ),
    );
    // The end of a soft-wrapped line is also the start of the next one. The
    // caret End reaches stays drawn on the line it was on.
    _placed(upstream: end && _wrapsAt(edge.row, edge.at));
    return changed;
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
        _visibleHeight,
      ),
      Paint()..color = color(FlarkColorRole.canvas),
    );
    final (baseRow, baseOffset) = _display(selected.start);
    final (endRow, endOffset) = _display(selected.end);
    final visible = Rect.fromLTWH(0, scrollOffset, size.width, _visibleHeight);
    _images.visible([
      for (final layout in _rows)
        for (final image in layout.images)
          if (image.rect.overlaps(visible)) image.resource.destination,
    ]);
    // Observations exist for tests and qualification probes. Without an
    // observer, do not rebuild plain text and merged styles on every frame.
    final observed = onPaint != null;
    final painted = <String>[],
        styles = <List<int>>[],
        selectionRects = <Rect>[];
    final resolvedStyles = <List<TextStyle>>[];
    final paintedImages = <FlarkImageObservation>[];
    final markers = <String, TextPainter>{};
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
        if (observed) {
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
        }
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
            layout.blockRect.shift(offset),
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
          layout.rect.shift(offset).deflate(.5),
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
        for (final container in layout.containers) {
          final shell = container.shell;
          if (shell.kind == ShellKind.blockQuote) {
            final railWidth = math.min(
              metric(FlarkMetric.quoteRailWidth),
              container.width,
            );
            canvas.drawLine(
              offset + Offset(container.left + railWidth / 2, layout.rect.top),
              offset +
                  Offset(container.left + railWidth / 2, layout.rect.bottom),
              Paint()
                ..color = color(FlarkColorRole.quoteRail)
                ..strokeWidth = railWidth,
            );
          }
          if (shell.kind == ShellKind.item) {
            final model = (_snapshot as FlarkLiveSnapshot).document.model;
            if (row.firstLine == model.blockFirstLine(shell.block)) {
              if (shell.task) {
                // Task state is UI geometry, independent of symbol-font fallback.
                final fontSize = textScaler.scale(style.fontSize!);
                final gutter = math.max(
                  0.0,
                  container.width - metric(FlarkMetric.listMarkerGap),
                );
                final boxSize = math.min(fontSize * .8, gutter);
                final box = Rect.fromLTWH(
                  offset.dx + container.left + (gutter - boxSize) / 2,
                  offset.dy + layout.origin.dy + fontSize * .3,
                  boxSize,
                  boxSize,
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
                // Markers visible in the previous paint keep their shaped
                // painters; typing or scrolling reshaped every one per frame.
                final painter = markers[marker] ??=
                    _markers.remove(marker) ??
                    (TextPainter(
                      text: TextSpan(
                        text: marker,
                        style: style.merge(
                          theme.styles[FlarkTextRole.listMarker],
                        ),
                      ),
                      textScaler: textScaler,
                      textDirection: TextDirection.ltr,
                    )..layout());
                final gutter = math.max(
                  0.0,
                  container.width - metric(FlarkMetric.listMarkerGap),
                );
                painter.paint(
                  canvas,
                  offset +
                      Offset(
                        container.left +
                            math.max(
                              0.0,
                              shell.ordered
                                  ? gutter - painter.width
                                  : (gutter - painter.width) / 2,
                            ),
                        layout.origin.dy,
                      ),
                );
              }
            }
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
          if (observed) selectionRects.add(rect);
          canvas.drawRect(
            rect.shift(offset),
            Paint()..color = color(FlarkColorRole.selection),
          );
        }
      }
      layout.painter.paint(canvas, offset + layout.origin);
      if (!observed) continue;
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
    for (final painter in _markers.values) {
      painter.dispose();
    }
    _markers = markers;
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
    final handles = this.handles;
    if (handles != null && !selected.isCollapsed) {
      for (final (link, source) in [
        (handles.$1, selected.start),
        (handles.$2, selected.end),
      ]) {
        // An end scrolled out of view has no leader, so its handle hides.
        final end = caretRectAt(source).bottomLeft;
        if (end.dy < visible.top - .5 || end.dy > visible.bottom + .5) continue;
        context.pushLayer(
          LeaderLayer(link: link, offset: offset + end),
          (_, _) {},
          Offset.zero,
        );
      }
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
    if (!readOnly || selectable) {
      config.isEnabled = true;
      config.onFocus = onFocus;
    }
    config.isMultiline = true;
    config.textDirection = TextDirection.ltr;
    final current = _composingOnWeb
        ? _described ?? controller.editor.snapshot
        : controller.editor.snapshot;
    _described = current;
    final rows = current is FlarkLiveSnapshot ? current.projection.rows : null;
    final window = current is FlarkSourceSnapshot
        ? SourceWindow.at(current.source, current.selection.extent)
        : null;
    final texts =
        rows?.map((r) => r.text).toList() ??
        current.source.substring(window!.start, window.end).split('\n');
    final value = texts.join('\n');
    config.value = value;
    int displayOffset(int source, {int? tableCell}) {
      if (current is! FlarkLiveSnapshot) {
        return (source - window!.start).clamp(0, window.end - window.start);
      }
      final p = current.projection.displayForSource(
        source,
        tableCell: tableCell,
      )!;
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

    // The row holding a visible offset, and the offset within it.
    (ProjectedRow, int) rowAt(int display) {
      var remaining = display;
      for (final row in rows!) {
        if (remaining <= row.text.length) return (row, remaining);
        remaining -= row.text.length + 1;
      }
      return (rows.last, rows.last.text.length);
    }

    final caret = displayOffset(
      current.selection.extent,
      tableCell: current.selection.tableCell,
    );
    config.textSelection = TextSelection(
      baseOffset: displayOffset(
        current.selection.base,
        tableCell: current.selection.tableCell,
      ),
      extentOffset: caret,
    );
    if (!readOnly || selectable) {
      if (onCopy != null) config.onCopy = onCopy!;
      config.onSetSelection = (selection) {
        if (rows != null && selection.isCollapsed) {
          var offset = selection.extentOffset;
          for (final row in rows) {
            if (offset <= row.text.length) {
              controller.command(PlaceCaret(row.index, offset));
              return;
            }
            offset -= row.text.length + 1;
          }
        }
        controller.command(
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
      };
    }
    if (!readOnly) {
      config.onSetText = (text) {
        // The edited text is this snapshot's value. Applied after a later
        // edit, it would quietly revert that edit.
        if (!identical(controller.editor.snapshot, current)) return;
        if (window != null) {
          // A pathological single grapheme may exceed a source page. Never
          // let a page replacement expand into its neighboring page.
          if (CharacterRange.at(current.source, window.start).isNotEmpty ||
              CharacterRange.at(current.source, window.end).isNotEmpty) {
            return;
          }
          controller.command(ReplaceRange(window.start, window.end, text));
          return;
        }
        // Rendered text omits hidden Markdown, so it cannot replace the
        // source. Android Voice Access sends the whole edited value for
        // "replace X with Y" or dictation: apply only the visible range that
        // changed. An edit the kernel cannot express is rejected with the
        // controller's notice and leaves the document as it was.
        final edit = _visibleEdit(value, text, caret: caret);
        if (edit == null) return;
        if (edit.start == edit.end) {
          // Inserted text goes where typing it at that visible offset would:
          // a caret is placed there as a pointer places one, then the text is
          // typed, so a list continues and a missing table cell fills. At the
          // caret it is typed there, in the caret's context: a press would
          // drop a style chosen for the next text.
          if (!current.selection.isCollapsed || edit.start != caret) {
            final (row, offset) = rowAt(edit.start);
            controller.command(
              PlaceCaret(row.index, offset, leadingHalf: false),
            );
          }
          controller.command(
            edit.text == '\n' ? const Newline() : InsertText(edit.text),
          );
          return;
        }
        // A replaced range takes the outermost source offsets at its edges, so
        // formatting it reaches into is covered whole. The kernel narrows a
        // range that lies within one span to that span's content.
        final document = (current as FlarkLiveSnapshot).document;
        controller.command(
          ReplaceRange(
            document
                .anchorsAt(sourceOffset(edit.start, anchor: Anchor.before))
                .first,
            document
                .anchorsAt(sourceOffset(edit.end, anchor: Anchor.after))
                .last,
            edit.text,
          ),
        );
      };
    }
  }
}

/// The visible range of [before] that [after] replaces, and its replacement,
/// or null when they are equal. An insertion at [caret] is read there when it
/// explains the change: a diff alone cannot tell which of two equal
/// neighbors, two line breaks say, is the new one. Otherwise the range is
/// widened over the unchanged text on either side until both strings break
/// graphemes at its edges, so a surrogate pair or combining sequence is never
/// split.
({int start, int end, String text})? _visibleEdit(
  String before,
  String after, {
  int? caret,
}) {
  if (before == after) return null;
  final added = after.length - before.length;
  if (caret != null &&
      added > 0 &&
      caret >= 0 &&
      caret <= before.length &&
      after.startsWith(before.substring(0, caret)) &&
      after.endsWith(before.substring(caret)) &&
      CharacterRange.at(after, caret).isEmpty &&
      CharacterRange.at(after, caret + added).isEmpty) {
    return (
      start: caret,
      end: caret,
      text: after.substring(caret, caret + added),
    );
  }
  var start = 0;
  final shorter = math.min(before.length, after.length);
  while (start < shorter &&
      before.codeUnitAt(start) == after.codeUnitAt(start)) {
    start++;
  }
  var end = before.length, afterEnd = after.length;
  while (end > start &&
      afterEnd > start &&
      before.codeUnitAt(end - 1) == after.codeUnitAt(afterEnd - 1)) {
    end--;
    afterEnd--;
  }
  // Inside the unchanged prefix and suffix both strings break graphemes at
  // the same places, so the outer of their nearest breaks is one in both.
  int breakBefore(String text, int offset) {
    final range = CharacterRange.at(text, offset);
    return range.isEmpty ? offset : range.stringBeforeLength;
  }

  int breakAfter(String text, int offset) {
    final range = CharacterRange.at(text, offset);
    return range.isEmpty ? offset : text.length - range.stringAfterLength;
  }

  start = math.min(breakBefore(before, start), breakBefore(after, start));
  final widened = math.max(
    breakAfter(before, end),
    end + breakAfter(after, afterEnd) - afterEnd,
  );
  afterEnd += widened - end;
  end = widened;
  return (start: start, end: end, text: after.substring(start, afterEnd));
}
