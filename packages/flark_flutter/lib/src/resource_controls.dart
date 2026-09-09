import 'package:flark/resources.dart';
export 'package:flark/resources.dart';
import 'package:flutter/material.dart';

/// Custom presentation retains Flark's guarded semantic actions.
typedef FlarkResourcePresenter =
    Future<void> Function(BuildContext context, FlarkResourceSession session);
typedef FlarkLinkPopoverBuilder =
    Widget Function(BuildContext context, FlarkLinkActions actions);

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
