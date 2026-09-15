part of 'editor_view.dart';

/// The native textarea mirror cannot contain interactive descendants in HTML.
/// Keep painted checkbox/link targets beside that editor node in the graph.
class _InteractiveSemantics extends ProxyWidget {
  const _InteractiveSemantics({
    required this.controller,
    required this.viewport,
    required this.readOnly,
    required this.onActivate,
    required super.child,
  });
  final FlarkFleuryController controller;
  final _Viewport viewport;
  final bool readOnly;
  final void Function(int col, int row) onActivate;

  @override
  ComponentElement createElement() => _InteractiveSemanticsElement(this);
}

/// Publish the same painted targets used by pointer editing to accessibility
/// and browser cursor hit testing. No invisible overlay intercepts selection.
class _InteractiveSemanticsElement extends ComponentElement
    implements SemanticContributor, SemanticActionContributor {
  _InteractiveSemanticsElement(_InteractiveSemantics super.widget);

  @override
  _InteractiveSemantics get widget => super.widget as _InteractiveSemantics;

  @override
  Widget buildChild() => widget.child;

  @override
  void update(covariant _InteractiveSemantics newWidget) {
    super.update(newWidget);
    rebuild(force: true);
  }

  @override
  SemanticNode buildSemanticNode(List<SemanticNode> children) {
    final viewport = widget.viewport;
    final layout = viewport.layout;
    final editor = widget.controller.editor;
    final id = semanticAnchorOf(this) ?? 'element-$hashCode';
    final nodes = <SemanticNode>[];
    final visible = viewport.bounds.visibleBounds;
    if (layout != null && visible != null && !editor.sourceMode) {
      void add(
        String key,
        SemanticRole role,
        String label,
        int x,
        int y,
        int width, {
        bool? checked,
      }) {
        final bounds = CellRect.fromLTWH(
          viewport.origin.col + x,
          viewport.origin.row + y,
          width,
          1,
        ).intersect(visible);
        if (bounds == null || bounds.size.isEmpty) return;
        final enabled = role != SemanticRole.checkbox || !widget.readOnly;
        nodes.add(
          SemanticNode(
            id: SemanticNodeId('$id/${editor.revision}/$key'),
            role: role,
            label: label,
            checked: checked,
            bounds: bounds,
            enabled: enabled,
            actions: enabled ? const {SemanticAction.activate} : const {},
          ),
        );
      }

      for (var y = 0; y < viewport.rows; y++) {
        final index = viewport.top + y;
        if (index >= layout.lines.length) break;
        final line = layout.lines[index];
        final task = line.taskColumn;
        if (task >= 0) {
          add(
            'task/$index',
            SemanticRole.checkbox,
            line.row!.text,
            task,
            y,
            3,
            checked: line.row!.shells.lastWhere((s) => s.task).checked,
          );
        }
        for (final resource in editor.document.resources) {
          if (resource.isImage) continue;
          int? start, end;
          for (final glyph in line.glyphs) {
            final source = line.sourceAt(glyph.start);
            if (source >= resource.contentStart &&
                source < resource.contentEnd) {
              start ??= glyph.col;
              end = glyph.col + glyph.width;
            }
          }
          if (start != null) {
            add(
              'link/${resource.start}/$index',
              SemanticRole.link,
              resource.destination,
              start,
              y,
              end! - start,
            );
          }
        }
      }
    }
    return SemanticNode(
      id: SemanticNodeId('$id/targets'),
      role: SemanticRole.region,
      children: [...children, ...nodes],
    );
  }

  @override
  bool handleSemanticAction(SemanticNode target, SemanticAction action) {
    if (action != SemanticAction.activate) return false;
    // Revalidate against the current source and painted viewport before using
    // a stored target: edits, scrolling and read-only changes can invalidate it.
    for (final node in buildSemanticNode(const []).children) {
      if (node.id == target.id && node.enabled) {
        widget.onActivate(node.bounds!.left, node.bounds!.top);
        return true;
      }
    }
    return false;
  }
}
