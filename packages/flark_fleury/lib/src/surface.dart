part of 'editor_view.dart';

final class _Viewport {
  _Viewport() {
    bounds.addListener(_publishBounds);
  }
  final bounds = BoundsNotifier();
  FocusNode? focus;
  CellDocumentLayout? layout;
  CellOffset origin = const CellOffset(0, 0);
  int top = 0, rows = 0;
  void _publishBounds() {
    // BoundsObserver also replays through cached/repositioned Fleury subtrees.
    // Pointer and IME geometry must move even when our paint is cached.
    final painted = bounds.bounds, visible = bounds.visibleBounds;
    if (painted != null) origin = painted.offset;
    final current = layout, node = focus;
    if (node == null) return;
    node.caretRect = null;
    if (current == null || visible == null || !node.hasFocus) return;
    final selection = current.controller.editor.selection;
    if (!selection.isCollapsed) return;
    final caret = current.positionFor(selection.extent);
    node.caretRect = CellRect.fromLTWH(
      origin.col + caret.col,
      origin.row + caret.row - top,
      1,
      1,
    ).intersect(visible);
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
  RenderObject createRenderObject(BuildContext context) => _RenderSurface(this);
  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderSurface renderObject,
  ) => renderObject.update(this);
}

class _RenderSurface extends RenderObject {
  _RenderSurface(this.widget);
  _Surface widget;
  int _revision = -1, _width = -1;
  FlarkEditor? _editor;
  bool _focused = false;

  void update(_Surface value) {
    widget = value;
    markNeedsLayout();
    markNeedsPaint();
  }

  @override
  CellSize performLayout(CellConstraints constraints) {
    final cols = constraints.maxCols ?? 80;
    final viewport = widget.viewport;
    viewport.focus = widget.focus;
    final layout = viewport.layout = CellDocumentLayout(
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
    return result;
  }

  @override
  void paint(
    CellBuffer buffer,
    CellOffset offset, {
    CellOffset? screenOffset,
    CellRect? clipRect,
  }) {
    final viewport = widget.viewport;
    final layout = viewport.layout!;
    final screen = viewport.origin = screenOffset ?? offset;
    final selection = widget.controller.editor.selection;
    final selectionByRow = <ProjectedRow, (int, int)>{};
    final caret = layout.positionFor(selection.extent);
    widget.focus.caretRect = null;
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
      if (clipRect != null &&
          (!clipRect.contains(CellOffset(screen.col + col, screen.row + row)) ||
              !clipRect.contains(
                CellOffset(screen.col + col + width - 1, screen.row + row),
              ))) {
        return;
      }
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
        final rect = CellRect.fromLTWH(
          screen.col + caret.col,
          screen.row + y,
          1,
          1,
        );
        widget.focus.caretRect = clipRect == null
            ? rect
            : rect.intersect(clipRect);
      }
    }
  }
}
