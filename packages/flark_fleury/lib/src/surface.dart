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

  int _revision = -1;
  FlarkDocumentState? _editor;
  bool _focused = false;

  /// The editor and revision at which the caret was placed at the end of a
  /// visual line: by End, a click past the line's last character or a
  /// vertical move. Where a row wraps, that offset also starts the next
  /// visual line, so the caret would show there and the next End or Down
  /// would start from the wrong line. Any later change of the editor
  /// returns the caret to the next line, where typing puts the character.
  ({FlarkDocumentState editor, int revision})? _caretAtLineEnd;

  /// Records whether a placement left the caret at the end of its visual
  /// line. Every placement records it: one at the start of the next line
  /// has the same offset, so it changes nothing else the editor knows.
  void placedCaret(FlarkDocumentState editor, {required bool atLineEnd}) =>
      _caretAtLineEnd = atLineEnd
      ? (editor: editor, revision: editor.revision)
      : null;

  /// The caret's cell in [layout].
  CellOffset get caret {
    final layout = this.layout!, editor = layout.controller.editor;
    final placed = _caretAtLineEnd;
    return layout.caretPosition(
      lineEnd:
          placed != null &&
          identical(placed.editor, editor) &&
          placed.revision == editor.revision,
    );
  }

  CellSize prepare(
    CellConstraints constraints,
    FlarkCellController controller,
    FlarkCellTheme theme,
    CellWidthPolicy policy,
    FocusNode focus, {
    bool fullDocument = false,
  }) {
    final cols = constraints.maxCols ?? 80;
    final viewport = this;
    final cached = viewport.layout;
    final layout = viewport.layout =
        cached != null && cached.describes(controller, cols, theme, policy)
        ? cached
        : CellDocumentLayout(controller, cols, theme, policy, previous: cached);
    final result = constraints.constrain(
      CellSize(
        cols,
        fullDocument
            ? layout.lines.length
            : (constraints.maxRows ?? layout.lines.length.clamp(1, 24)),
      ),
    );
    final editor = controller.editor;
    if (!fullDocument &&
        (editor.revision != _revision ||
            !identical(layout, cached) ||
            result.rows != rows ||
            !identical(_editor, editor) ||
            (!_focused && focus.hasFocus))) {
      final caret = viewport.caret;
      if (caret.row < viewport.top) viewport.top = caret.row;
      if (caret.row >= viewport.top + result.rows) {
        viewport.top = caret.row - result.rows + 1;
      }
    }
    viewport.rows = result.rows;
    if (fullDocument) viewport.top = 0;
    viewport.scroll(0);
    _revision = editor.revision;
    _editor = editor;
    _focused = focus.hasFocus;
    return result;
  }

  /// The image preview or resource text painted at screen cell ([col], [row]),
  /// in a view that paints a [caret] (an editor with focus) or none.
  InlineResource? resourceAt(
    FlarkDocumentState editor,
    int col,
    int row, {
    required bool caret,
  }) {
    final layout = this.layout;
    if (layout == null || editor.sourceMode) return null;
    final index = row - origin.row + top;
    if (index < 0 || index >= layout.lines.length) return null;
    final x = col - origin.col;
    final line = layout.lines[index].cellAt(x);
    if (line.image != null) return line.image;
    if (!line.labelVisible(editor.selection, caret: caret)) return null;
    for (final glyph in line.glyphs) {
      if (x < glyph.col || x >= glyph.col + glyph.width) continue;
      final source = line.sourceAt(glyph.start);
      // A link in an image's alt text is the link, as in Flutter.
      InlineResource? image;
      for (final resource in editor.document.resources) {
        if (resource.contentStart <= source && source < resource.contentEnd) {
          if (!resource.isImage) return resource;
          image ??= resource;
        }
      }
      return image;
    }
    return null;
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
    this.policy, {
    this.fullDocument = false,
  });
  final bool fullDocument;
  final FlarkCellController controller;
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
    if (!widget.fullDocument) widget.focus.attachCaretHost(this);
  }
  _Surface widget;

  void update(_Surface value) {
    if (!identical(widget.focus, value.focus)) {
      widget.focus.detachCaretHost(this);
      if (!value.fullDocument) value.focus.attachCaretHost(this);
    }
    widget = value;
    // The new widget can carry a different document, viewport or focus.
    markNeedsLayout();
  }

  /// Whether this surface paints a caret: an editor's does while it has
  /// focus, the reader's never. A standalone image's label, which a caret at
  /// the image shows, goes with it.
  bool get _paintsCaret => !widget.fullDocument && widget.focus.hasFocus;

  @override
  CellRect? get localCaretRect {
    final layout = widget.viewport.layout;
    final selection = widget.controller.editor.selection;
    if (!_paintsCaret || layout == null || !selection.isCollapsed) {
      return null;
    }
    final caret = widget.viewport.caret;
    final row = caret.row - widget.viewport.top;
    if (row < 0 || row >= size.rows || caret.col >= size.cols) return null;
    return CellRect.fromLTWH(caret.col, row, 1, 1);
  }

  @override
  CellSize performLayout(CellConstraints constraints) {
    return widget.viewport.prepare(
      constraints,
      widget.controller,
      widget.theme,
      widget.policy,
      widget.focus,
      fullDocument: widget.fullDocument,
    );
  }

  @override
  void performPaint(CellBuffer buffer, CellOffset offset) {
    final viewport = widget.viewport;
    final layout = viewport.layout!;
    final geometry = screenGeometry();
    viewport.origin = geometry?.bounds.offset ?? offset;
    final selection = widget.controller.editor.selection;
    final selectionByRow = <ProjectedRow, (int, int)>{};
    final caret = viewport.caret;
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

    // Reverse-video terminal bands still need a visible caret and selection.
    CellStyle highlight(CellStyle base, CellStyle overlay) => base
        .merge(overlay)
        .copyWith(
          inverse: overlay.inverse ? !base.inverse : overlay.inverseOrNull,
        );

    for (var y = 0; y < size.rows; y++) {
      final visual = y + viewport.top < layout.lines.length
          ? layout.lines[y + viewport.top]
          : null;
      final background = visual?.row?.kind == RowKind.codeBlock
          ? widget.theme.body.merge(widget.theme.code)
          : visual?.row?.kind == RowKind.heading && !visual!.headingRule
          ? widget.theme.headingSurface(visual.row!.headingLevel)
          : widget.theme.body;
      for (var x = 0; x < size.cols; x++) {
        write(
          x,
          y,
          ' ',
          visual != null && (visual.codeEdgeTop != null || x < visual.blockLeft)
              ? widget.theme.body
              : background,
        );
      }
      if (visual == null) continue;
      if (visual.headingRule) {
        for (var x = 0; x < visual.blockLeft; x++) {
          write(
            x,
            y,
            visual.prefix[x],
            widget.theme.body.merge(widget.theme.marker),
          );
        }
        final glyph = layout.tableRail == '|' ? '-' : '─';
        for (var x = visual.blockLeft; x < size.cols; x++) {
          write(
            x,
            y,
            glyph,
            widget.theme.body.merge(widget.theme.headingDivider),
          );
        }
        continue;
      }
      if (visual.rule case final rule?) {
        var x = 0;
        for (final rune in rule.runes) {
          write(
            x++,
            y,
            String.fromCharCode(rune),
            background.merge(widget.theme.tableBorder),
          );
        }
        continue;
      }
      if (visual.cells != null) {
        for (var x = 0; x < visual.prefix.length; x++) {
          write(x, y, visual.prefix[x], background.merge(widget.theme.marker));
        }
        write(
          visual.prefix.length,
          y,
          layout.tableLeftRail,
          background.merge(widget.theme.tableBorder),
        );
        for (final cell in visual.fragments) {
          write(
            cell.right,
            y,
            identical(cell, visual.cells!.last)
                ? layout.tableRightRail
                : layout.tableRail,
            background.merge(widget.theme.tableBorder),
          );
        }
      }
      if (visual.codeEdgeTop case final top?) {
        final theme = widget.theme;
        final eighths = (theme.codePaddingRows * 8).round().clamp(0, 8);
        final codeColor = background.background;
        for (var x = 0; x < visual.blockLeft && x < visual.prefix.length; x++) {
          write(x, y, visual.prefix[x], theme.body.merge(theme.marker));
        }
        if (eighths > 0 && codeColor != null) {
          final full = eighths == 8 || theme.body.background == null;
          if (!layout.blockEdges) {
            // Two-cell blocks would evict each other: a whole cell of code
            // background stands in for an edge at least half painted.
            if (full || eighths >= 4) {
              for (var x = visual.blockLeft; x < size.cols; x++) {
                write(x, y, ' ', CellStyle(background: codeColor));
              }
            }
            continue;
          }
          final count = top || full ? eighths : 8 - eighths;
          final glyph = full ? '█' : '▁▂▃▄▅▆▇█'[count - 1];
          final style = CellStyle(
            foreground: top || full ? codeColor : theme.body.background,
            background: top || full ? theme.body.background : codeColor,
          );
          for (var x = visual.blockLeft; x < size.cols; x++) {
            write(x, y, glyph, style);
          }
        }
        continue;
      }
      for (final line in visual.fragments) {
        if (!line.labelVisible(selection, caret: _paintsCaret)) continue;
        if (line.image != null) {
          for (var x = 0; x < line.prefix.length; x++) {
            write(x, y, line.prefix[x], background.merge(widget.theme.marker));
          }
          continue;
        }
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
          write(
            line.left + x,
            y,
            line.prefix[x],
            (line.left + x < line.blockLeft ? widget.theme.body : background)
                .merge(widget.theme.marker),
          );
        }
        if (line.headingLabelColumn case final col?) {
          final label = 'H${line.row!.headingLevel}';
          final labelBackground = col < line.blockLeft
              ? widget.theme.body
              : background;
          for (var x = 0; x < label.length; x++) {
            write(
              col + x,
              y,
              label[x],
              labelBackground.merge(widget.theme.headingIndicator),
            );
          }
        }
        if (line.row?.kind == RowKind.thematicBreak) {
          final glyph = layout.tableRail == '|' ? '-' : '─';
          var style = background.merge(widget.theme.thematicBreak);
          if (!selection.isCollapsed &&
              selection.start < line.row!.sourceEnd &&
              selection.end > line.row!.sourceStart) {
            style = highlight(style, widget.theme.selection);
          }
          for (var x = line.prefix.length; x < size.cols; x++) {
            write(x, y, glyph, style);
          }
        }
        for (final glyph in line.glyphs) {
          var style = line.imageLabel == null ? glyph.style : widget.theme.body;
          final selected =
              !selection.isCollapsed &&
              selectedRange.$1 < glyph.end &&
              selectedRange.$2 > glyph.start;
          if (selected) style = highlight(style, widget.theme.selection);
          if (_paintsCaret &&
              selection.isCollapsed &&
              caret.row == y + viewport.top &&
              caret.col == glyph.col) {
            style = highlight(style, widget.theme.caret);
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
        if (_paintsCaret &&
            selection.isCollapsed &&
            caret.row == y + viewport.top) {
          if (caret.col == line.endColumn) {
            write(caret.col, y, ' ', highlight(background, widget.theme.caret));
          }
        }
      }
    }
  }
}

/// Lays its child out [rows] taller than itself and paints it that far up,
/// for an image preview whose top has scrolled above the viewport: the
/// preview keeps the fit of its whole slot and [_ClipViewport] cuts what is
/// above. A [Positioned] child cannot itself start above its stack.
class _RaisedPreview extends SingleChildRenderObjectWidget {
  const _RaisedPreview({required this.rows, required super.child});
  final int rows;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderRaisedPreview(rows);

  @override
  void updateRenderObject(
    BuildContext context,
    covariant _RenderRaisedPreview renderObject,
  ) => renderObject.rows = rows;
}

class _RenderRaisedPreview extends RenderObject
    implements RenderObjectWithSingleChild {
  _RenderRaisedPreview(this._rows);

  int _rows;
  set rows(int value) {
    if (_rows == value) return;
    _rows = value;
    markNeedsLayout();
  }

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
    final size = constraints.constrain(
      CellSize(
        constraints.maxCols ?? constraints.minCols,
        constraints.maxRows ?? constraints.minRows,
      ),
    );
    _child?.layout(
      CellConstraints.tight(CellSize(size.cols, size.rows + _rows)),
    );
    return size;
  }

  @override
  CellOffset childOffsetOf(RenderObject child) => CellOffset(0, -_rows);

  @override
  CellRect childClipOf(RenderObject child) =>
      CellRect(offset: CellOffset.zero, size: size);

  @override
  void performPaint(CellBuffer buffer, CellOffset offset) =>
      _child?.paint(buffer, offset + CellOffset(0, -_rows));
}

/// Clips glyphs and pixel placements to the same viewport. Partly scrolled
/// images keep their original fit, independent of the visible slice.
class _ClipViewport extends SingleChildRenderObjectWidget {
  const _ClipViewport({required super.child});
  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderClipViewport();
}

class _RenderClipViewport extends RenderObject
    implements RenderObjectWithSingleChild {
  RenderObject? _child;
  CellBuffer? _scratch;
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
  CellSize performLayout(CellConstraints constraints) =>
      _child?.layout(constraints) ?? constraints.constrain(CellSize.zero);
  @override
  CellRect childClipOf(RenderObject child) =>
      CellRect(offset: CellOffset.zero, size: size);
  @override
  void performPaint(CellBuffer buffer, CellOffset offset) {
    final scratch = _scratch = CellBuffer.acquire(_scratch, size);
    _child?.paint(scratch, CellOffset.zero);
    for (var y = 0; y < size.rows; y++) {
      for (var x = 0; x < size.cols; x++) {
        final tx = offset.col + x, ty = offset.row + y;
        if (tx >= 0 &&
            ty >= 0 &&
            tx < buffer.size.cols &&
            ty < buffer.size.rows) {
          buffer.replayCellFrom(scratch, x, y, tx, ty);
        }
      }
    }
    buffer.compositeImageRectFrom(
      scratch,
      CellRect(offset: CellOffset.zero, size: size),
      offset,
    );
  }
}
