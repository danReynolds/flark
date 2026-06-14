import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../markdown/markdown.dart';
import 'flark_command_actions.dart';

/// Keyboard shortcut helpers for Markdown editing commands.
///
/// These build [FlarkCommandIntent]s for the same commands exposed as
/// `FlarkFlutterController` helper methods (e.g.
/// `controller.commands.toggleStrong()`),
/// so toolbar buttons and keyboard shortcuts drive one command surface. Pass
/// the result to `MarkdownEditor.shortcuts`, or rely on
/// `MarkdownEditor.useDefaultShortcuts` to install [defaults] automatically.
abstract final class FlarkMarkdownShortcuts {
  /// Intent that toggles an inline [style] (bold, italic, code, strikethrough).
  static FlarkCommandIntent toggleInlineStyle(
    FlarkMarkdownInlineStyle style, {
    String userEvent = 'shortcut.toggleInlineStyle',
  }) {
    return FlarkCommandIntent(
      FlarkTypedCommandInvocation(
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: FlarkToggleInlineStylePayload(style, userEvent: userEvent),
      ),
    );
  }

  /// Intent that toggles `**strong**` emphasis.
  static FlarkCommandIntent toggleStrong() => toggleInlineStyle(
    FlarkMarkdownInlineStyle.strong,
    userEvent: 'shortcut.toggleStrong',
  );

  /// Intent that toggles `*emphasis*`.
  static FlarkCommandIntent toggleEmphasis() => toggleInlineStyle(
    FlarkMarkdownInlineStyle.emphasis,
    userEvent: 'shortcut.toggleEmphasis',
  );

  /// Intent that toggles `` `inline code` ``.
  static FlarkCommandIntent toggleInlineCode() => toggleInlineStyle(
    FlarkMarkdownInlineStyle.inlineCode,
    userEvent: 'shortcut.toggleInlineCode',
  );

  /// Intent that toggles `~~strikethrough~~`.
  static FlarkCommandIntent toggleStrikethrough() => toggleInlineStyle(
    FlarkMarkdownInlineStyle.strikethrough,
    userEvent: 'shortcut.toggleStrikethrough',
  );

  /// Default inline-formatting shortcut map.
  ///
  /// Uses the Command modifier on Apple platforms and Control elsewhere:
  ///
  /// - Bold: Cmd/Ctrl + B
  /// - Italic: Cmd/Ctrl + I
  /// - Inline code: Cmd/Ctrl + E
  /// - Strikethrough: Cmd/Ctrl + Shift + X
  ///
  /// [platform] defaults to [defaultTargetPlatform].
  static Map<ShortcutActivator, Intent> defaults({TargetPlatform? platform}) {
    final isApple =
        (platform ?? defaultTargetPlatform) == TargetPlatform.macOS ||
        (platform ?? defaultTargetPlatform) == TargetPlatform.iOS;
    SingleActivator primary(LogicalKeyboardKey key, {bool shift = false}) {
      return SingleActivator(
        key,
        control: !isApple,
        meta: isApple,
        shift: shift,
      );
    }

    return <ShortcutActivator, Intent>{
      primary(LogicalKeyboardKey.keyB): toggleStrong(),
      primary(LogicalKeyboardKey.keyI): toggleEmphasis(),
      primary(LogicalKeyboardKey.keyE): toggleInlineCode(),
      primary(LogicalKeyboardKey.keyX, shift: true): toggleStrikethrough(),
      // Tab / Shift+Tab nest list items. The action is gated so it only fires
      // inside a list — Tab elsewhere keeps its normal behavior.
      const SingleActivator(LogicalKeyboardKey.tab): const FlarkIndentListIntent(),
      const SingleActivator(LogicalKeyboardKey.tab, shift: true):
          const FlarkIndentListIntent(outdent: true),
    };
  }
}
