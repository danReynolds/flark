import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../render_plan/render_plan.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_controller_commands.dart';
import 'flark_markdown_interactions.dart';
import 'flark_markdown_theme.dart';

/// The image the caret is resting in, handed to every [FlarkImageAction].
///
/// Mirrors [FlarkLinkActionContext]: it carries the image's parts plus the
/// [controller]/[interactions] an action needs, and a [dismiss] to close the
/// popover. Mutating actions should call [dismiss]; read-only ones like copy
/// can leave it open.
final class FlarkImageActionContext {
  const FlarkImageActionContext({
    required this.context,
    required this.url,
    required this.alt,
    required this.range,
    required this.controller,
    required this.interactions,
    required this.dismiss,
  });

  /// The build context the popover is shown in — passed to integrator
  /// callbacks (e.g. to anchor an edit dialog).
  final BuildContext context;

  /// The image source (`https://…`).
  final String url;

  /// The image's alt text.
  final String alt;

  /// The image's full source span, including its `![]()` markers.
  final FlarkSourceRange range;

  final FlarkFlutterController controller;
  final FlarkMarkdownInteractions interactions;

  /// Closes the popover.
  final VoidCallback dismiss;

  /// Whether the surface is editable (false in read-only preview): edit/remove
  /// actions should hide or no-op when this is false.
  bool get editable => interactions.editable;
}

/// One button in the inline image popover.
///
/// Customize the popover by passing a list of these — reorder, drop, or add
/// app-specific actions. The built-in set is [FlarkImageAction.defaults]; build
/// on it with `[...FlarkImageAction.defaults, myAction]` or replace it.
final class FlarkImageAction {
  const FlarkImageAction({
    required this.id,
    required this.label,
    required this.onInvoke,
    this.icon,
    this.isDestructive = false,
    this.isAvailable,
  });

  /// Stable identifier (used as a [ValueKey] and for semantics).
  final String id;

  /// Tooltip / accessibility label.
  final String label;

  /// Optional leading icon. When null, [label] is shown as text.
  final IconData? icon;

  /// Tints the action with the theme's destructive color (e.g. Remove).
  final bool isDestructive;

  /// Whether to show this action for a given image. Defaults to always.
  final bool Function(FlarkImageActionContext image)? isAvailable;

  /// Invoked on tap.
  final void Function(FlarkImageActionContext image) onInvoke;

  /// The built-in actions: Open, Edit, Copy, Remove.
  ///
  /// Open and Copy always apply; Edit and Remove apply only on an editable
  /// surface.
  static const List<FlarkImageAction> defaults = [
    FlarkImageAction(
      id: 'open',
      label: 'Open image',
      icon: Icons.open_in_new,
      onInvoke: _open,
    ),
    FlarkImageAction(
      id: 'edit',
      label: 'Edit image',
      icon: Icons.edit_outlined,
      isAvailable: _whenEditable,
      onInvoke: _edit,
    ),
    FlarkImageAction(
      id: 'copy',
      label: 'Copy image URL',
      icon: Icons.copy_outlined,
      onInvoke: _copy,
    ),
    FlarkImageAction(
      id: 'remove',
      label: 'Remove image',
      icon: Icons.hide_image_outlined,
      isDestructive: true,
      isAvailable: _whenEditable,
      onInvoke: _remove,
    ),
  ];

  static bool _whenEditable(FlarkImageActionContext image) => image.editable;

  static void _open(FlarkImageActionContext image) {
    image.interactions.config.onOpenImage?.call(image.url);
  }

  static void _edit(FlarkImageActionContext image) {
    final target = FlarkRenderOverlayTarget(
      kind: FlarkRenderOverlayKind.image,
      sourceRange: image.range,
      displayRange: image.range,
      action: FlarkRenderInlineActionDescriptor(
        kind: FlarkRenderInlineActionKind.image,
        destination: image.url,
        label: image.alt,
      ),
    );
    image.dismiss();
    // Selects the image's range and invokes the app's onEditImage callback.
    image.interactions.editImage(image.context, target);
  }

  static void _copy(FlarkImageActionContext image) {
    Clipboard.setData(ClipboardData(text: image.url));
  }

  static void _remove(FlarkImageActionContext image) {
    image.controller.commands.removeImage(imageRange: image.range);
    image.dismiss();
  }
}

/// The default inline image popover: a themed card showing the image source and
/// a row of [actions], themed via [FlarkMarkdownTheme].
final class FlarkImagePopover extends StatelessWidget {
  const FlarkImagePopover({
    super.key,
    required this.image,
    required this.actions,
  });

  final FlarkImageActionContext image;
  final List<FlarkImageAction> actions;

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    final baseStyle = DefaultTextStyle.of(context).style;
    final fontSize = (baseStyle.fontSize ?? 14) - 1;
    final visible = [
      for (final action in actions)
        if (action.isAvailable?.call(image) ?? true) action,
    ];

    return Material(
      type: MaterialType.transparency,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: theme.menuBackgroundColor,
          border: Border.all(color: theme.borderColor),
          borderRadius: const BorderRadius.all(Radius.circular(8)),
          boxShadow: [
            BoxShadow(
              color: theme.menuShadowColor,
              blurRadius: 12,
              offset: const Offset(0, 4),
            ),
          ],
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(10, 6, 6, 6),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.image_outlined,
                size: fontSize + 4,
                color: theme.chromeLabelColor,
              ),
              const SizedBox(width: 6),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 280),
                child: Text(
                  image.url,
                  key: const Key('FlarkImagePopoverDestination'),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: baseStyle.copyWith(
                    fontSize: fontSize,
                    color: theme.linkColor,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              for (final action in visible)
                _ImageActionButton(
                  action: action,
                  image: image,
                  fontSize: fontSize,
                  color: action.isDestructive
                      ? theme.checkboxBorderColor
                      : theme.chromeLabelColor,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

final class _ImageActionButton extends StatelessWidget {
  const _ImageActionButton({
    required this.action,
    required this.image,
    required this.fontSize,
    required this.color,
  });

  final FlarkImageAction action;
  final FlarkImageActionContext image;
  final double fontSize;
  final Color color;

  @override
  Widget build(BuildContext context) {
    final icon = action.icon;
    final child = icon != null
        ? Icon(icon, size: fontSize + 4, color: color)
        : Text(
            action.label,
            style: DefaultTextStyle.of(context).style.copyWith(
              fontSize: fontSize,
              color: color,
              fontWeight: FontWeight.w600,
            ),
          );
    return Semantics(
      button: true,
      label: action.label,
      child: Tooltip(
        message: action.label,
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          child: GestureDetector(
            key: ValueKey('FlarkImageAction.${action.id}'),
            behavior: HitTestBehavior.opaque,
            onTap: () => action.onInvoke(image),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
              child: child,
            ),
          ),
        ),
      ),
    );
  }
}
