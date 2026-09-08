import 'package:flark/flark.dart';
import 'package:flutter/material.dart';

/// Custom presentation retains Flark's guarded semantic actions.
typedef FlarkResourcePresenter =
    Future<void> Function(BuildContext context, FlarkResourceSession session);
typedef FlarkLinkPopoverBuilder =
    Widget Function(BuildContext context, FlarkLinkActions actions);

/// A bounded editing request created by the host. All methods become inert
/// when its target changes, the editor disappears, or the presentation closes.
class FlarkResourceSession {
  FlarkResourceSession({
    required this.image,
    required this.resource,
    required this.selectedText,
    required this._isActive,
    required this._apply,
    this._onOpen,
  });
  final bool image;
  final InlineResource? resource;
  final String selectedText;
  final bool Function() _isActive;
  final bool Function(FlarkCommand) _apply;
  final VoidCallback? _onOpen;
  VoidCallback? get onOpen => _onOpen == null ? null : open;
  bool _closed = false;
  bool get active => !_closed && _isActive();
  String get label => resource?.text ?? selectedText;
  String get destination => resource?.destination ?? '';
  String get title => resource?.title ?? '';

  bool save({
    required String destination,
    required String label,
    String title = '',
  }) {
    if (!active) return false;
    if (resource != null &&
        destination.trim() == this.destination &&
        label == this.label &&
        title == this.title) {
      close();
      return true;
    }
    if (destination.trim().isEmpty) return false;
    final text = label == this.label && !(image && resource == null)
        ? null
        : label;
    final command = image
        ? SetImage(destination.trim(), alt: text, title: title)
        : SetLink(destination.trim(), text: text, title: title);
    return _submit(command);
  }

  bool remove() =>
      active &&
      resource != null &&
      _submit(image ? const RemoveImage() : const RemoveLink());
  void open() {
    if (active) _onOpen?.call();
  }

  bool _submit(FlarkCommand command) {
    final result = _apply(command);
    if (result) close();
    return result;
  }

  /// End the request without changing the document.
  void close() {
    _closed = true;
  }
}

/// Resource facts and host-guarded actions, shared by all popover designs.
class FlarkLinkActions {
  const FlarkLinkActions({
    required this.resource,
    required this.dismiss,
    this.open,
    this.edit,
    this.remove,
  });
  final InlineResource resource;
  final VoidCallback dismiss;
  final VoidCallback? open, edit, remove;
}

/// The default content is public so consumers can compose or wrap it.
class FlarkLinkPopover extends StatelessWidget {
  const FlarkLinkPopover({super.key, required this.actions});
  final FlarkLinkActions actions;
  @override
  Widget build(BuildContext context) => Material(
    elevation: 6,
    color: Theme.of(context).colorScheme.surfaceContainer,
    borderRadius: BorderRadius.circular(10),
    child: Padding(
      padding: const EdgeInsets.all(10),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            actions.resource.destination,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.bodySmall,
          ),
          Wrap(
            children: [
              TextButton(onPressed: actions.open, child: const Text('Open')),
              if (actions.edit != null)
                TextButton(onPressed: actions.edit, child: const Text('Edit')),
              if (actions.remove != null)
                TextButton(
                  onPressed: actions.remove,
                  child: const Text('Remove'),
                ),
              IconButton(
                tooltip: 'Dismiss link actions',
                onPressed: actions.dismiss,
                icon: const Icon(Icons.close, size: 18),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}

/// Placement is evaluated during layout against current glyph geometry.
class LinkPopoverLayout extends SingleChildLayoutDelegate {
  LinkPopoverLayout(this.anchor, this.viewport);
  final Rect Function() anchor;
  final Rect viewport;
  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      BoxConstraints(
        maxWidth: (viewport.width - 16).clamp(0, 360),
        maxHeight: (viewport.height - 16).clamp(0, double.infinity),
      );
  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final rect = anchor();
    final bottom = rect.bottom + 6;
    final y = bottom + childSize.height <= viewport.bottom - 8
        ? bottom
        : rect.top - childSize.height - 6;
    return Offset(
      rect.left.clamp(
        viewport.left + 8,
        (viewport.right - childSize.width - 8).clamp(
          viewport.left + 8,
          double.infinity,
        ),
      ),
      y.clamp(
        viewport.top + 8,
        (viewport.bottom - childSize.height - 8).clamp(
          viewport.top + 8,
          double.infinity,
        ),
      ),
    );
  }

  @override
  bool shouldRelayout(LinkPopoverLayout oldDelegate) => true;
}
