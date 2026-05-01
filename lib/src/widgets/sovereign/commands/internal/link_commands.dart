import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_command_result.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_link_edit_context.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

import 'command_context.dart';
import 'command_transaction.dart';

final RegExp _linkPattern = RegExp(r'\[([^\]\n]*)\]\(([^)\n]*)\)');
final RegExp _exactLinkPattern = RegExp(r'^\[([^\]\n]*)\]\(([^)\n]*)\)$');

abstract final class SovereignLinkCommands {
  static SovereignLinkEditContext resolveLinkEditContext(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    final text = context.text;
    final selection = context.selection;

    if (!selection.isCollapsed) {
      final selectedText = text.substring(selection.start, selection.end);
      final exact = _exactLinkPattern.firstMatch(selectedText);
      if (exact != null) {
        return SovereignLinkEditContext(
          replaceRange: TextRange(start: selection.start, end: selection.end),
          label: exact.group(1) ?? '',
          url: exact.group(2) ?? '',
          isExisting: true,
        );
      }
      return SovereignLinkEditContext(
        replaceRange: TextRange(start: selection.start, end: selection.end),
        label: selectedText,
        url: 'https://',
        isExisting: false,
      );
    }

    final cursor = selection.extentOffset.clamp(0, text.length);
    for (final match in _linkPattern.allMatches(text)) {
      if (cursor < match.start || cursor > match.end) {
        continue;
      }
      return SovereignLinkEditContext(
        replaceRange: TextRange(start: match.start, end: match.end),
        label: match.group(1) ?? '',
        url: match.group(2) ?? '',
        isExisting: true,
      );
    }

    return SovereignLinkEditContext(
      replaceRange: TextRange.collapsed(cursor),
      label: '',
      url: 'https://',
      isExisting: false,
    );
  }

  static SovereignCommandResult applyLinkEdit(
    SovereignController controller, {
    required SovereignLinkEditContext context,
    required String label,
    required String url,
  }) {
    final commandContext = SovereignCommandContext.fromController(controller);
    if (commandContext.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final cleanUrl = url.trim();
    if (cleanUrl.isEmpty) {
      return SovereignCommandNoOp.code(SovereignCommandReasonCode.emptyUrl);
    }

    final cleanLabel = label.trim().isEmpty ? 'link' : label.trim();
    final replacement = '[$cleanLabel]($cleanUrl)';
    final start = context.replaceRange.start.clamp(
      0,
      commandContext.text.length,
    );
    final end = context.replaceRange.end.clamp(
      start,
      commandContext.text.length,
    );
    final updated = commandContext.text.replaceRange(start, end, replacement);
    final caret = (start + replacement.length).clamp(0, updated.length);

    return commitCommandMutation(commandContext, (
      text: updated,
      selection: TextSelection.collapsed(offset: caret),
      composing: TextRange.empty,
    ));
  }

  static SovereignCommandResult insertLink(SovereignController controller) {
    final commandContext = SovereignCommandContext.fromController(controller);
    if (commandContext.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final context = resolveLinkEditContext(controller);
    final label = context.label.trim().isEmpty ? 'link text' : context.label;
    const url = '';
    final replacement = '[$label]($url)';
    final start = context.replaceRange.start.clamp(
      0,
      commandContext.text.length,
    );
    final end = context.replaceRange.end.clamp(
      start,
      commandContext.text.length,
    );
    final updated = commandContext.text.replaceRange(start, end, replacement);
    final selectionStart = (start + label.length + 3).clamp(0, updated.length);
    final selectionEnd = (selectionStart + url.length).clamp(0, updated.length);

    return commitCommandMutation(commandContext, (
      text: updated,
      selection: TextSelection(
        baseOffset: selectionStart,
        extentOffset: selectionEnd,
      ),
      composing: TextRange.empty,
    ));
  }
}
