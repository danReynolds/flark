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

  /// Intent that inserts a `[label]()` link skeleton with the caret in the URL.
  static FlarkCommandIntent insertLink() {
    return FlarkCommandIntent(
      const FlarkTypedCommandInvocation(
        command: FlarkMarkdownLinkCommands.insertLink,
        payload: FlarkInsertLinkPayload(userEvent: 'shortcut.insertLink'),
      ),
    );
  }

  /// Intent that sets the current block's heading [level] (0 clears it).
  static FlarkCommandIntent setHeadingLevel(int level) {
    return FlarkCommandIntent(
      FlarkTypedCommandInvocation(
        command: FlarkMarkdownBlockCommands.setHeadingLevel,
        payload: FlarkSetHeadingLevelPayload(
          level,
          userEvent: 'shortcut.setHeadingLevel',
        ),
      ),
    );
  }

  /// Intent that toggles a `-` bullet list.
  static FlarkCommandIntent toggleBulletList() {
    return FlarkCommandIntent(
      const FlarkTypedCommandInvocation(
        command: FlarkMarkdownBlockCommands.toggleBulletList,
        payload: FlarkToggleBulletListPayload(
          userEvent: 'shortcut.toggleBulletList',
        ),
      ),
    );
  }

  /// Intent that toggles a `1.` ordered list.
  static FlarkCommandIntent toggleOrderedList() {
    return FlarkCommandIntent(
      const FlarkTypedCommandInvocation(
        command: FlarkMarkdownBlockCommands.toggleOrderedList,
        payload: FlarkToggleOrderedListPayload(
          userEvent: 'shortcut.toggleOrderedList',
        ),
      ),
    );
  }

  /// Intent that toggles a `>` block quote.
  static FlarkCommandIntent toggleQuote() {
    return FlarkCommandIntent(
      const FlarkTypedCommandInvocation(
        command: FlarkMarkdownBlockCommands.toggleQuote,
        payload: FlarkToggleQuotePayload(userEvent: 'shortcut.toggleQuote'),
      ),
    );
  }

  /// Intent that moves the line(s) under the selection up or, with [down], down.
  static FlarkMoveLinesIntent moveLines({bool down = false}) {
    return FlarkMoveLinesIntent(down: down);
  }

  /// Intent that duplicates the line(s) under the selection.
  static FlarkCommandIntent duplicateLines() {
    return FlarkCommandIntent(
      const FlarkTypedCommandInvocation(
        command: FlarkMarkdownInputCommands.duplicateLines,
        payload: FlarkDuplicateLinesPayload(userEvent: 'shortcut.duplicateLines'),
      ),
    );
  }

  /// Intent that deletes the line(s) under the selection.
  static FlarkCommandIntent deleteLines() {
    return FlarkCommandIntent(
      const FlarkTypedCommandInvocation(
        command: FlarkMarkdownInputCommands.deleteLines,
        payload: FlarkDeleteLinesPayload(userEvent: 'shortcut.deleteLines'),
      ),
    );
  }

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
    SingleActivator primary(
      LogicalKeyboardKey key, {
      bool shift = false,
      bool alt = false,
    }) {
      return SingleActivator(
        key,
        control: !isApple,
        meta: isApple,
        shift: shift,
        alt: alt,
      );
    }

    return <ShortcutActivator, Intent>{
      primary(LogicalKeyboardKey.keyB): toggleStrong(),
      primary(LogicalKeyboardKey.keyI): toggleEmphasis(),
      primary(LogicalKeyboardKey.keyE): toggleInlineCode(),
      primary(LogicalKeyboardKey.keyX, shift: true): toggleStrikethrough(),
      primary(LogicalKeyboardKey.keyK): insertLink(),
      // Headings: Cmd/Ctrl + Alt + 1..6, with 0 clearing back to body text.
      primary(LogicalKeyboardKey.digit1, alt: true): setHeadingLevel(1),
      primary(LogicalKeyboardKey.digit2, alt: true): setHeadingLevel(2),
      primary(LogicalKeyboardKey.digit3, alt: true): setHeadingLevel(3),
      primary(LogicalKeyboardKey.digit4, alt: true): setHeadingLevel(4),
      primary(LogicalKeyboardKey.digit5, alt: true): setHeadingLevel(5),
      primary(LogicalKeyboardKey.digit6, alt: true): setHeadingLevel(6),
      primary(LogicalKeyboardKey.digit0, alt: true): setHeadingLevel(0),
      // Lists and quote follow the Google Docs convention.
      primary(LogicalKeyboardKey.digit7, shift: true): toggleOrderedList(),
      primary(LogicalKeyboardKey.digit8, shift: true): toggleBulletList(),
      primary(LogicalKeyboardKey.digit9, shift: true): toggleQuote(),
      // Tab / Shift+Tab nest list items. The action is gated so it only fires
      // inside a list — Tab elsewhere keeps its normal behavior.
      const SingleActivator(LogicalKeyboardKey.tab): const FlarkIndentListIntent(),
      const SingleActivator(LogicalKeyboardKey.tab, shift: true):
          const FlarkIndentListIntent(outdent: true),
      // Alt+Up/Down move the current line(s); gated to fall through to caret
      // movement at the document boundary.
      const SingleActivator(LogicalKeyboardKey.arrowUp, alt: true): moveLines(),
      const SingleActivator(LogicalKeyboardKey.arrowDown, alt: true): moveLines(
        down: true,
      ),
      primary(LogicalKeyboardKey.keyD, shift: true): duplicateLines(),
      primary(LogicalKeyboardKey.keyK, shift: true): deleteLines(),
    };
  }
}
