import '../controllers/sovereign_controller.dart';
import 'internal/block_commands.dart';
import 'internal/fence_commands.dart';
import 'internal/inline_commands.dart';
import 'internal/link_commands.dart';
import 'models/sovereign_command_capabilities.dart';
import 'models/sovereign_command_result.dart';
import 'models/sovereign_inline_style.dart';
import 'models/sovereign_link_edit_context.dart';

class SovereignMarkdownCommands {
  final SovereignController controller;

  const SovereignMarkdownCommands(this.controller);

  void _clearComposingForInlineMutation() {
    if (controller.value.composing.isValid) {
      controller.clearComposing();
    }
  }

  SovereignInlineStyle? getInlineStyleAtSelection() =>
      SovereignInlineCommands.activeInlineStyleAtCursor(controller);

  SovereignCommandResult toggleInlineStyle(SovereignInlineStyle style) {
    _clearComposingForInlineMutation();
    return SovereignInlineCommands.toggleInlineStyle(controller, style);
  }

  SovereignCommandResult deactivateInlineStyle() {
    _clearComposingForInlineMutation();
    return SovereignInlineCommands.deactivateInlineStyle(controller);
  }

  SovereignCommandResult setHeadingLevel(int? level) =>
      SovereignBlockCommands.setHeadingLevel(controller, level);

  int? getHeadingLevelAtSelection() =>
      SovereignBlockCommands.headingLevelAtCursor(controller);

  SovereignCommandResult toggleQuote() =>
      SovereignBlockCommands.toggleQuote(controller);

  bool isQuoteActiveAtSelection() =>
      SovereignBlockCommands.isQuoteAtCursor(controller);

  SovereignCommandResult toggleBulletList() =>
      SovereignBlockCommands.toggleBulletList(controller);

  SovereignCommandResult toggleTaskList() =>
      SovereignBlockCommands.toggleTaskList(controller);

  SovereignCommandResult insertHorizontalRule() =>
      SovereignBlockCommands.insertHorizontalRule(controller);

  SovereignCommandResult insertFence({String language = 'plain'}) =>
      SovereignFenceCommands.insertFence(controller, language: language);

  SovereignCommandResult insertLink() =>
      SovereignLinkCommands.insertLink(controller);

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

  T runInTransaction<T>(T Function(SovereignMarkdownCommands commands) action) {
    return controller.runInCommandTransaction(() => action(this));
  }

  SovereignLinkEditContext resolveLinkEditContext() =>
      SovereignLinkCommands.resolveLinkEditContext(controller);

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

extension SovereignMarkdownCommandControllerExtension on SovereignController {
  SovereignMarkdownCommands get commands => SovereignMarkdownCommands(this);
}
