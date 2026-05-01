import 'package:sovereign_editor/src/widgets/sovereign/commands/internal/block_commands.dart';
import 'package:sovereign_editor/src/widgets/sovereign/commands/internal/fence_commands.dart';
import 'package:sovereign_editor/src/widgets/sovereign/commands/internal/inline_commands.dart';
import 'package:sovereign_editor/src/widgets/sovereign/commands/internal/link_commands.dart';

import '../controllers/sovereign_controller.dart';
import 'models/sovereign_command_capabilities.dart';
import 'models/sovereign_command_result.dart';
import 'models/sovereign_inline_style.dart';
import 'models/sovereign_link_edit_context.dart';

/// Toolbar-friendly command facade for a [SovereignController].
///
/// Commands mutate markdown text through the controller while preserving editor
/// invariants such as selection projection, IME composition safety, and undo
/// grouping.
class SovereignMarkdownCommands {
  /// Controller that receives command mutations.
  final SovereignController controller;

  /// Creates a command facade for [controller].
  const SovereignMarkdownCommands(this.controller);

  void _clearComposingForInlineMutation() {
    if (controller.value.composing.isValid) {
      controller.clearComposing();
    }
  }

  /// Returns the active inline style at the current selection, if any.
  SovereignInlineStyle? getInlineStyleAtSelection() =>
      SovereignInlineCommands.activeInlineStyleAtCursor(controller);

  /// Toggles [style] at the current selection.
  SovereignCommandResult toggleInlineStyle(SovereignInlineStyle style) {
    _clearComposingForInlineMutation();
    return SovereignInlineCommands.toggleInlineStyle(controller, style);
  }

  /// Removes the active inline style from the current selection or caret.
  SovereignCommandResult deactivateInlineStyle() {
    _clearComposingForInlineMutation();
    return SovereignInlineCommands.deactivateInlineStyle(controller);
  }

  /// Sets the current block heading level, or clears heading style with `null`.
  SovereignCommandResult setHeadingLevel(int? level) =>
      SovereignBlockCommands.setHeadingLevel(controller, level);

  /// Returns the current heading level at the selection, if any.
  int? getHeadingLevelAtSelection() =>
      SovereignBlockCommands.headingLevelAtCursor(controller);

  /// Toggles blockquote formatting for the current block or selection.
  SovereignCommandResult toggleQuote() =>
      SovereignBlockCommands.toggleQuote(controller);

  /// Returns whether the current selection is inside a blockquote.
  bool isQuoteActiveAtSelection() =>
      SovereignBlockCommands.isQuoteAtCursor(controller);

  /// Toggles bullet-list formatting for the current block or selection.
  SovereignCommandResult toggleBulletList() =>
      SovereignBlockCommands.toggleBulletList(controller);

  /// Toggles task-list formatting for the current block or selection.
  SovereignCommandResult toggleTaskList() =>
      SovereignBlockCommands.toggleTaskList(controller);

  /// Inserts a thematic break at the current selection.
  SovereignCommandResult insertHorizontalRule() =>
      SovereignBlockCommands.insertHorizontalRule(controller);

  /// Inserts a fenced code block using [language] as the info string.
  SovereignCommandResult insertFence({String language = 'plain'}) =>
      SovereignFenceCommands.insertFence(controller, language: language);

  /// Inserts a markdown link at the current selection.
  SovereignCommandResult insertLink() =>
      SovereignLinkCommands.insertLink(controller);

  /// Returns command enablement and active-style state for the current selection.
  SovereignCommandCapabilities capabilitiesAtSelection() {
    final isComposing = controller.value.composing.isValid;
    return SovereignCommandCapabilities(
      isComposing: isComposing,
      canMutate: !isComposing,
      activeInlineStyle: getInlineStyleAtSelection(),
      activeHeadingLevel: getHeadingLevelAtSelection(),
      quoteActive: isQuoteActiveAtSelection(),
    );
  }

  /// Runs multiple command calls as one undoable command transaction.
  T runInTransaction<T>(T Function(SovereignMarkdownCommands commands) action) {
    return controller.runInCommandTransaction(() => action(this));
  }

  /// Returns the editable link context at the current selection.
  SovereignLinkEditContext resolveLinkEditContext() =>
      SovereignLinkCommands.resolveLinkEditContext(controller);

  /// Applies an edited link target from [context].
  SovereignCommandResult applyLinkEdit({
    required SovereignLinkEditContext context,
    required String label,
    required String url,
  }) =>
      SovereignLinkCommands.applyLinkEdit(
        controller,
        context: context,
        label: label,
        url: url,
      );
}

/// Convenience access to [SovereignMarkdownCommands] from a controller.
extension SovereignMarkdownCommandControllerExtension on SovereignController {
  /// Creates a command facade bound to this controller.
  SovereignMarkdownCommands get commands => SovereignMarkdownCommands(this);
}
