import '../../core/command/sovereign_command.dart';
import '../../core/command/sovereign_command_registry.dart';
import '../../core/command/sovereign_command_result.dart';
import '../../core/extension/sovereign_extension.dart';
import '../../core/selection/sovereign_selection.dart';
import '../../core/state/sovereign_editor_state.dart';
import '../../core/transaction/sovereign_source_operation.dart';
import '../../core/transaction/sovereign_source_range.dart';
import '../../core/transaction/sovereign_transaction.dart';
import '../../core/transaction/sovereign_transaction_metadata.dart';

final RegExp _linkPattern = RegExp(r'\[([^\]\n]*)\]\(([^)\n]*)\)');
final RegExp _exactLinkPattern = RegExp(r'^\[([^\]\n]*)\]\(([^)\n]*)\)$');

abstract final class SovereignMarkdownLinkCommands {
  static const insertLink = SovereignCommand<SovereignInsertLinkPayload>(
    'markdown.insertLink',
  );

  static const applyLinkEdit = SovereignCommand<SovereignApplyLinkEditPayload>(
    'markdown.applyLinkEdit',
  );

  static const removeLink = SovereignCommand<SovereignRemoveLinkPayload>(
    'markdown.removeLink',
  );

  static SovereignMarkdownLinkEditContext resolveLinkEditContext(
    SovereignEditorState state,
  ) {
    final text = state.markdown;
    final selection = state.selection;

    if (!selection.isCollapsed) {
      final selectedText = text.substring(selection.start, selection.end);
      final exact = _exactLinkPattern.firstMatch(selectedText);
      if (exact != null) {
        return SovereignMarkdownLinkEditContext(
          replaceRange: SovereignSourceRange(selection.start, selection.end),
          label: exact.group(1) ?? '',
          url: exact.group(2) ?? '',
          isExisting: true,
        );
      }
      return SovereignMarkdownLinkEditContext(
        replaceRange: SovereignSourceRange(selection.start, selection.end),
        label: selectedText,
        url: 'https://',
        isExisting: false,
      );
    }

    final cursor = selection.extentOffset.clamp(0, text.length);
    for (final match in _linkPattern.allMatches(text)) {
      if (cursor < match.start || cursor > match.end) continue;
      return SovereignMarkdownLinkEditContext(
        replaceRange: SovereignSourceRange(match.start, match.end),
        label: match.group(1) ?? '',
        url: match.group(2) ?? '',
        isExisting: true,
      );
    }

    return SovereignMarkdownLinkEditContext(
      replaceRange: SovereignSourceRange(cursor, cursor),
      label: '',
      url: 'https://',
      isExisting: false,
    );
  }
}

final class SovereignMarkdownLinkEditContext {
  const SovereignMarkdownLinkEditContext({
    required this.replaceRange,
    required this.label,
    required this.url,
    required this.isExisting,
  });

  final SovereignSourceRange replaceRange;
  final String label;
  final String url;
  final bool isExisting;
}

final class SovereignInsertLinkPayload {
  const SovereignInsertLinkPayload({
    this.userEvent = 'command.insertLink',
  });

  final String userEvent;
}

final class SovereignApplyLinkEditPayload {
  const SovereignApplyLinkEditPayload({
    required this.context,
    required this.label,
    required this.url,
    this.userEvent = 'command.applyLinkEdit',
  });

  final SovereignMarkdownLinkEditContext context;
  final String label;
  final String url;
  final String userEvent;
}

final class SovereignRemoveLinkPayload {
  const SovereignRemoveLinkPayload({
    required this.linkRange,
    this.userEvent = 'command.removeLink',
  });

  final SovereignSourceRange linkRange;
  final String userEvent;
}

final class SovereignMarkdownLinkEditingExtension extends SovereignExtension {
  const SovereignMarkdownLinkEditingExtension();

  @override
  String get id => 'markdown.linkEditing';

  @override
  SovereignCommandRegistry registerCommands(SovereignCommandRegistry registry) {
    return registry
        .register<SovereignInsertLinkPayload>(
          SovereignMarkdownLinkCommands.insertLink,
          _insertLink,
        )
        .register<SovereignApplyLinkEditPayload>(
          SovereignMarkdownLinkCommands.applyLinkEdit,
          _applyLinkEdit,
        )
        .register<SovereignRemoveLinkPayload>(
          SovereignMarkdownLinkCommands.removeLink,
          _removeLink,
        );
  }

  SovereignCommandResult _insertLink(
    SovereignCommandContext<SovereignInsertLinkPayload> context,
  ) {
    final linkContext =
        SovereignMarkdownLinkCommands.resolveLinkEditContext(context.state);
    final label =
        linkContext.label.trim().isEmpty ? 'link text' : linkContext.label;
    final replacement = '[$label]()';
    final selectionOffset = linkContext.replaceRange.start + replacement.length;
    return _replaceLinkRange(
      state: context.state,
      range: linkContext.replaceRange,
      replacement: replacement,
      selectionAfter: SovereignSelection.collapsed(selectionOffset),
      userEvent: context.payload.userEvent,
    );
  }

  SovereignCommandResult _applyLinkEdit(
    SovereignCommandContext<SovereignApplyLinkEditPayload> context,
  ) {
    final cleanUrl = context.payload.url.trim();
    if (cleanUrl.isEmpty) {
      return SovereignCommandResult.rejected('Link URL cannot be empty.');
    }

    final cleanLabel = context.payload.label.trim().isEmpty
        ? 'link'
        : context.payload.label.trim();
    final replacement = '[$cleanLabel]($cleanUrl)';
    final range = _clampedRange(
      context.payload.context.replaceRange,
      context.state.markdown.length,
    );

    return _replaceLinkRange(
      state: context.state,
      range: range,
      replacement: replacement,
      selectionAfter: SovereignSelection.collapsed(
        range.start + replacement.length,
      ),
      userEvent: context.payload.userEvent,
    );
  }

  SovereignCommandResult _removeLink(
    SovereignCommandContext<SovereignRemoveLinkPayload> context,
  ) {
    final range = _clampedRange(
      context.payload.linkRange,
      context.state.markdown.length,
    );
    final source = context.state.markdown.substring(range.start, range.end);
    final match = _exactLinkPattern.firstMatch(source);
    if (match == null) {
      return SovereignCommandResult.rejected(
        'Link range does not contain a markdown link.',
      );
    }

    final label = match.group(1) ?? '';
    return _replaceLinkRange(
      state: context.state,
      range: range,
      replacement: label,
      selectionAfter: SovereignSelection.collapsed(range.start + label.length),
      userEvent: context.payload.userEvent,
    );
  }
}

SovereignCommandResult _replaceLinkRange({
  required SovereignEditorState state,
  required SovereignSourceRange range,
  required String replacement,
  required SovereignSelection selectionAfter,
  required String userEvent,
}) {
  final safeRange = _clampedRange(range, state.markdown.length);
  return SovereignCommandResult.handled(
    transaction: SovereignTransaction.single(
      SovereignSourceOperation.replace(
        replacedRange: safeRange,
        replacementText: replacement,
      ),
      selectionBefore: state.selection,
      selectionAfter: selectionAfter.validate(
        state.markdown.length - safeRange.length + replacement.length,
      ),
      metadata: SovereignTransactionMetadata(
        intent: SovereignTransactionIntent.command,
        userEvent: userEvent,
        parseInvalidationRange: safeRange,
        projectionInvalidationRange: safeRange,
      ),
    ),
  );
}

SovereignSourceRange _clampedRange(SovereignSourceRange range, int textLength) {
  final start = range.start.clamp(0, textLength);
  final end = range.end.clamp(start, textLength);
  return SovereignSourceRange(start, end);
}
