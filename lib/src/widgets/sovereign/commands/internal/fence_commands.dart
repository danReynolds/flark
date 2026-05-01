import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_command_result.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

import 'command_context.dart';
import 'command_transaction.dart';

abstract final class SovereignFenceCommands {
  static SovereignCommandResult insertFence(
    SovereignController controller, {
    String language = 'plain',
  }) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final text = context.text;
    final selection = context.selection;
    final selectedText = text.substring(selection.start, selection.end);
    final normalizedLanguage = language.trim();
    final fenceLanguage = normalizedLanguage.isEmpty ||
            normalizedLanguage.toLowerCase() == 'plain'
        ? ''
        : normalizedLanguage;
    final fencePrefix = fenceLanguage.isEmpty ? '```\n' : '```$fenceLanguage\n';
    final hasSelection = selectedText.isNotEmpty;
    final replacement =
        hasSelection ? '$fencePrefix$selectedText\n```' : '$fencePrefix\n```';

    final updated = text.replaceRange(
      selection.start,
      selection.end,
      replacement,
    );
    final bodyStart = (selection.start + fencePrefix.length).clamp(
      0,
      updated.length,
    );
    final bodyEnd = hasSelection
        ? (bodyStart + selectedText.length).clamp(0, updated.length)
        : bodyStart;

    return commitCommandMutation(context, (
      text: updated,
      selection: hasSelection
          ? TextSelection(baseOffset: bodyStart, extentOffset: bodyEnd)
          : TextSelection.collapsed(offset: bodyStart),
      composing: TextRange.empty,
    ));
  }
}
