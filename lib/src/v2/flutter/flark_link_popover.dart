import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../render_plan/render_plan.dart';
import 'flark_flutter_controller.dart';
import 'flark_markdown_controller_commands.dart';
import 'flark_markdown_interactions.dart';
import 'flark_markdown_theme.dart';

/// The link the caret is resting in, handed to every [FlarkLinkAction].
///
/// Carries the link's parts plus the [controller]/[interactions] an action
/// needs to act on it, and a [dismiss] to close the popover (e.g. after a
/// destructive action). Actions that mutate the document should call [dismiss]
/// themselves; read-only actions like copy can leave the popover open.
final class FlarkLinkActionContext {
  const FlarkLinkActionContext({
    required this.context,
    required this.url,
    required this.label,
    required this.range,
    required this.controller,
    required this.interactions,
    required this.dismiss,
  });

  /// The build context the popover is shown in — passed to integrator
  /// callbacks (e.g. to anchor an edit dialog).
  final BuildContext context;

  /// The link destination (`https://…`).
  final String url;

  /// The visible link text.
  final String label;

  /// The link's full source span, including its `[]()` markers.
  final FlarkSourceRange range;

  final FlarkFlutterController controller;
  final FlarkMarkdownInteractions interactions;

  /// Closes the popover.
  final VoidCallback dismiss;

  /// Whether the surface is editable (false in read-only preview): edit/remove
  /// actions should hide or no-op when this is false.
  bool get editable => interactions.editable;
}

/// One button in the inline link popover.
///
/// Provide a list of these to customize the popover — reorder, drop, or add
/// app-specific actions (e.g. "Open in app", "Unfurl"). The built-in set is
/// [FlarkLinkAction.defaults]; build on top of it with
/// `[...FlarkLinkAction.defaults, myAction]` or replace it entirely.
final class FlarkLinkAction {
  const FlarkLinkAction({
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

  /// Whether to show this action for a given link. Defaults to always.
  final bool Function(FlarkLinkActionContext link)? isAvailable;

  /// Invoked on tap.
  final void Function(FlarkLinkActionContext link) onInvoke;

  /// The built-in actions: Open, Edit, Copy, Remove.
  ///
  /// Open and Copy always apply; Edit and Remove apply only on an editable
  /// surface. Compose with these rather than rebuilding them:
  /// `linkActions: [...FlarkLinkAction.defaults, myAction]`.
  static const List<FlarkLinkAction> defaults = [
    FlarkLinkAction(
      id: 'open',
      label: 'Open link',
      icon: Icons.open_in_new,
      onInvoke: _open,
    ),
    FlarkLinkAction(
      id: 'edit',
      label: 'Edit link',
      icon: Icons.edit_outlined,
      isAvailable: _whenEditable,
      onInvoke: _edit,
    ),
    FlarkLinkAction(
      id: 'copy',
      label: 'Copy link',
      icon: Icons.copy_outlined,
      onInvoke: _copy,
    ),
    FlarkLinkAction(
      id: 'remove',
      label: 'Remove link',
      icon: Icons.link_off,
      isDestructive: true,
      isAvailable: _whenEditable,
      onInvoke: _remove,
    ),
  ];

  static bool _whenEditable(FlarkLinkActionContext link) => link.editable;

  static void _open(FlarkLinkActionContext link) {
    link.interactions.config.onOpenLink?.call(link.url);
  }

  static void _edit(FlarkLinkActionContext link) {
    final target = FlarkRenderOverlayTarget(
      kind: FlarkRenderOverlayKind.link,
      sourceRange: link.range,
      displayRange: link.range,
      action: FlarkRenderInlineActionDescriptor(
        kind: FlarkRenderInlineActionKind.link,
        destination: link.url,
        label: link.label,
      ),
    );
    link.dismiss();
    // Selects the link's range and invokes the app's onEditLink callback.
    link.interactions.editLink(link.context, target);
  }

  static void _copy(FlarkLinkActionContext link) {
    Clipboard.setData(ClipboardData(text: link.url));
  }

  static void _remove(FlarkLinkActionContext link) {
    link.controller.commands.removeLink(linkRange: link.range);
    link.dismiss();
  }
}

/// The default inline link popover: a themed card showing the destination and a
/// row of [actions].
///
/// Reads colors from [FlarkMarkdownTheme] (menu background/shadow/border, link
/// and label colors), so it themes with the rest of the editor. The editor
/// shows this automatically when the caret enters a link; apps that want a
/// different surface can build their own using [FlarkLinkActionContext].
final class FlarkLinkPopover extends StatelessWidget {
  const FlarkLinkPopover({
    super.key,
    required this.link,
    required this.actions,
  });

  final FlarkLinkActionContext link;
  final List<FlarkLinkAction> actions;

  @override
  Widget build(BuildContext context) {
    final theme = FlarkMarkdownTheme.of(context);
    final baseStyle = DefaultTextStyle.of(context).style;
    final fontSize = (baseStyle.fontSize ?? 14) - 1;
    final visible = [
      for (final action in actions)
        if (action.isAvailable?.call(link) ?? true) action,
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
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 280),
                child: Text(
                  link.url,
                  key: const Key('FlarkLinkPopoverDestination'),
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
                _LinkActionButton(
                  action: action,
                  link: link,
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

final class _LinkActionButton extends StatelessWidget {
  const _LinkActionButton({
    required this.action,
    required this.link,
    required this.fontSize,
    required this.color,
  });

  final FlarkLinkAction action;
  final FlarkLinkActionContext link;
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
            key: ValueKey('FlarkLinkAction.${action.id}'),
            behavior: HitTestBehavior.opaque,
            onTap: () => action.onInvoke(link),
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
