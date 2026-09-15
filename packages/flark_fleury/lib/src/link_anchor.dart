part of 'editor_view.dart';

/// A caret is inside the painted document, not a separate widget to wrap in
/// BoundsObserver. Resolve its anchor after the preceding editor has laid out
/// and painted, using Fleury's placement rules and the popover's measured size.
class _LinkAnchor extends SingleChildRenderObjectWidget {
  const _LinkAnchor({required this.viewport, required Widget super.child});
  final _Viewport viewport;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderLinkAnchor(viewport);

  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderLinkAnchor renderObject,
  ) {
    renderObject.viewport = viewport;
    renderObject.markNeedsLayout();
    renderObject.markNeedsPaint();
  }
}

class _RenderLinkAnchor extends RenderObject
    implements RenderObjectWithSingleChild {
  _RenderLinkAnchor(this.viewport);
  _Viewport viewport;
  RenderObject? _child;

  @override
  RenderObject? get child => _child;

  @override
  set child(RenderObject? value) {
    if (identical(_child, value)) return;
    if (_child != null) dropChild(_child!);
    _child = value;
    if (value != null) adoptChild(value);
  }

  @override
  CellSize performLayout(CellConstraints constraints) {
    final cols = constraints.maxCols ?? viewport.layout?.cols ?? 52;
    final rows = constraints.maxRows ?? viewport.rows;
    _child?.layout(CellConstraints(maxCols: cols.clamp(0, 52), maxRows: rows));
    return constraints.constrain(CellSize(cols, rows));
  }

  @override
  bool presentsChild(RenderObject child) {
    final layout = viewport.layout;
    if (layout == null || size.isEmpty) return false;
    final row =
        layout.positionFor(layout.controller.editor.selection.extent).row -
        viewport.top;
    return row >= 0 && row < size.rows;
  }

  @override
  CellRect childClipOf(RenderObject child) =>
      CellRect(offset: CellOffset.zero, size: size);

  @override
  CellOffset childOffsetOf(RenderObject child) {
    final layout = viewport.layout;
    if (layout == null) return CellOffset.zero;
    final caret = layout.positionFor(layout.controller.editor.selection.extent);
    return resolveAnchoredOffset(
      anchor: CellRect.fromLTWH(caret.col, caret.row - viewport.top, 1, 1),
      overlaySize: child.size,
      alignment: Alignment.bottomLeft,
      anchorAlignment: Alignment.topLeft,
      w: size.cols,
      h: size.rows,
    );
  }

  @override
  void performPaint(CellBuffer buffer, CellOffset offset) {
    final child = _child;
    if (child != null && presentsChild(child)) {
      child.paint(buffer, offset + childOffsetOf(child));
    }
  }
}
