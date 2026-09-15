part of 'editor_view.dart';

final class _Viewport {
  _Viewport() {
    bounds.addListener(_publishBounds);
  }
  final bounds = BoundsNotifier();
  CellDocumentLayout? layout;
  CellOffset origin = const CellOffset(0, 0);
  int top = 0, rows = 0;
  void _publishBounds() {
    // BoundsObserver also replays through cached/repositioned Fleury subtrees.
    // Pointer geometry must move even when our paint is cached. Fleury derives
    // IME geometry directly from the attached CaretHost.
    final painted = bounds.bounds;
    if (painted != null) origin = painted.offset;
  }

  void dispose() {
    bounds.removeListener(_publishBounds);
    bounds.dispose();
  }

  void scroll(int delta) => top = (top + delta).clamp(
    0,
    ((layout?.lines.length ?? 0) - rows).clamp(0, 1 << 30),
  );
}

class _Surface extends LeafRenderObjectWidget {
  const _Surface(
    this.controller,
    this.theme,
    this.focus,
    this.viewport,
    this.policy,
  );
  final FlarkFleuryController controller;
  final FlarkCellTheme theme;
  final FocusNode focus;
  final _Viewport viewport;
  final CellWidthPolicy policy;

  @override
  LeafRenderObjectElement createElement() => _SurfaceElement(this);

  @override
  RenderObject createRenderObject(BuildContext context) => _RenderSurface(this);
  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderSurface renderObject,
  ) => renderObject.update(this);
}

class _SurfaceElement extends LeafRenderObjectElement {
  _SurfaceElement(_Surface super.widget);
  @override
  void unmount() {
    final surface = maybeRenderObject as _RenderSurface?;
    surface?.widget.focus.detachCaretHost(surface);
    super.unmount();
  }
}

class _RenderSurface extends RenderObject implements CaretHost {
  _RenderSurface(this.widget) {
    widget.focus.attachCaretHost(this);
  }
  _Surface widget;
  int _revision = -1, _width = -1;
  FlarkEditor? _editor;
  bool _focused = false;

  void update(_Surface value) {
    if (!identical(widget.focus, value.focus)) {
      widget.focus.detachCaretHost(this);
      value.focus.attachCaretHost(this);
    }
    widget = value;
    markNeedsLayout();
    markNeedsPaint();
  }

  @override
  CellRect? get localCaretRect {
    final layout = widget.viewport.layout;
    final selection = widget.controller.editor.selection;
    if (layout == null || !widget.focus.hasFocus || !selection.isCollapsed) {
      return null;
    }
    final caret = layout.positionFor(selection.extent);
    final row = caret.row - widget.viewport.top;
    if (row < 0 || row >= size.rows || caret.col >= size.cols) return null;
    return CellRect.fromLTWH(caret.col, row, 1, 1);
  }

  @override
  CellSize performLayout(CellConstraints constraints) {
    final cols = constraints.maxCols ?? 80;
    final viewport = widget.viewport;
    final cached = viewport.layout;
    final layout = viewport.layout =
        cached != null &&
            cached.describes(
              widget.controller,
              cols,
              widget.theme,
              widget.policy,
            )
        ? cached
        : CellDocumentLayout(
            widget.controller,
            cols,
            widget.theme,
            widget.policy,
          );
    final result = constraints.constrain(
      CellSize(cols, constraints.maxRows ?? layout.lines.length.clamp(1, 24)),
    );
    viewport.rows = result.rows;
    final editor = widget.controller.editor;
    if (editor.revision != _revision ||
        cols != _width ||
        !identical(_editor, editor) ||
        (!_focused && widget.focus.hasFocus)) {
      final caret = layout.positionFor(editor.selection.extent);
      if (caret.row < viewport.top) viewport.top = caret.row;
      if (caret.row >= viewport.top + result.rows) {
        viewport.top = caret.row - result.rows + 1;
      }
    }
    viewport.scroll(0);
    _revision = editor.revision;
    _editor = editor;
    _width = cols;
    _focused = widget.focus.hasFocus;
    widget.controller.setVisibleRows({
      for (final line in layout.lines.skip(viewport.top).take(result.rows))
        if (line.row != null) line.row!.index,
    });
    return result;
  }

  @override
  void performPaint(CellBuffer buffer, CellOffset offset) {
    final viewport = widget.viewport;
    final layout = viewport.layout!;
    final geometry = screenGeometry();
    viewport.origin = geometry?.bounds.offset ?? offset;
    final selection = widget.controller.editor.selection;
    final selectionByRow = <ProjectedRow, (int, int)>{};
    final caret = layout.positionFor(selection.extent);
    void write(
      int col,
      int row,
      String text,
      CellStyle style, {
      int width = 1,
    }) {
      if (col < 0 || col + width > size.cols || row < 0 || row >= size.rows) {
        return;
      }
      // Paint the whole local surface into Fleury's buffer. Ancestor clipping
      // belongs to composition, not cached content: a later scroll can reveal
      // cells that were outside the screen when this cache was first painted.
      buffer.writeGrapheme(
        CellOffset(offset.col + col, offset.row + row),
        text,
        style: style,
        policy: widget.policy,
      );
    }

    for (var y = 0; y < size.rows; y++) {
      final line = y + viewport.top < layout.lines.length
          ? layout.lines[y + viewport.top]
          : null;
      final background = line?.row?.kind == RowKind.codeBlock
          ? widget.theme.body.merge(widget.theme.code)
          : widget.theme.body;
      for (var x = 0; x < size.cols; x++) {
        write(x, y, ' ', background);
      }
      if (line == null) continue;
      final selectedRange = line.row == null
          ? (
              selection.start - line.sourceStart,
              selection.end - line.sourceStart,
            )
          : selectionByRow.putIfAbsent(
              line.row!,
              () => (
                line.row!.displayForSource(selection.start).$1,
                line.row!.displayForSource(selection.end).$1,
              ),
            );
      for (var x = 0; x < line.prefix.length; x++) {
        write(x, y, line.prefix[x], background.merge(widget.theme.marker));
      }
      for (final glyph in line.glyphs) {
        var style = glyph.style;
        final selected =
            !selection.isCollapsed &&
            selectedRange.$1 < glyph.end &&
            selectedRange.$2 > glyph.start;
        if (selected) style = style.merge(widget.theme.selection);
        if (widget.focus.hasFocus &&
            selection.isCollapsed &&
            caret.row == y + viewport.top &&
            caret.col == glyph.col) {
          style = style.merge(widget.theme.caret);
        }
        write(
          glyph.col,
          y,
          glyph.text,
          style,
          width: glyph.text == ' ' ? 1 : glyph.width,
        );
        // Tabs occupy several cells but keep one source grapheme.
        if (glyph.text == ' ') {
          for (var dx = 1; dx < glyph.width; dx++) {
            write(glyph.col + dx, y, ' ', style);
          }
        }
      }
      if (widget.focus.hasFocus &&
          selection.isCollapsed &&
          caret.row == y + viewport.top) {
        if (caret.col == line.endColumn) {
          write(caret.col, y, ' ', background.merge(widget.theme.caret));
        }
      }
    }
  }
}
