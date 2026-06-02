import '../../core/command/flark_command.dart';
import '../../core/command/flark_command_registry.dart';
import '../../core/command/flark_command_result.dart';
import '../../core/extension/flark_extension.dart';
import '../../core/selection/flark_selection.dart';
import '../../core/state/flark_editor_state.dart';
import '../../core/transaction/flark_source_operation.dart';
import '../../core/transaction/flark_source_range.dart';
import '../../core/transaction/flark_transaction.dart';
import '../../core/transaction/flark_transaction_metadata.dart';

final RegExp _linkPattern = RegExp(r'\[([^\]\n]*)\]\(([^)\n]*)\)');
final RegExp _exactLinkPattern = RegExp(r'^\[([^\]\n]*)\]\(([^)\n]*)\)$');

abstract final class FlarkMarkdownLinkCommands {
  static const insertLink = FlarkCommand<FlarkInsertLinkPayload>(
    'markdown.insertLink',
  );

  static const applyLinkEdit = FlarkCommand<FlarkApplyLinkEditPayload>(
    'markdown.applyLinkEdit',
  );

  static const removeLink = FlarkCommand<FlarkRemoveLinkPayload>(
    'markdown.removeLink',
  );

  static FlarkMarkdownLinkEditContext resolveLinkEditContext(
    FlarkEditorState state,
  ) {
    final text = state.markdown;
    final selection = state.selection;

    if (!selection.isCollapsed) {
      final selectedText = text.substring(selection.start, selection.end);
      final exact = _exactLinkPattern.firstMatch(selectedText);
      if (exact != null) {
        return FlarkMarkdownLinkEditContext(
          replaceRange: FlarkSourceRange(selection.start, selection.end),
          label: exact.group(1) ?? '',
          url: exact.group(2) ?? '',
          isExisting: true,
        );
      }
      return FlarkMarkdownLinkEditContext(
        replaceRange: FlarkSourceRange(selection.start, selection.end),
        label: selectedText,
        url: 'https://',
        isExisting: false,
      );
    }

    final cursor = selection.extentOffset.clamp(0, text.length);
    for (final match in _linkPattern.allMatches(text)) {
      if (cursor < match.start || cursor > match.end) continue;
      return FlarkMarkdownLinkEditContext(
        replaceRange: FlarkSourceRange(match.start, match.end),
        label: match.group(1) ?? '',
        url: match.group(2) ?? '',
        isExisting: true,
      );
    }

    return FlarkMarkdownLinkEditContext(
      replaceRange: FlarkSourceRange(cursor, cursor),
      label: '',
      url: 'https://',
      isExisting: false,
    );
  }
}

final class FlarkMarkdownLinkEditContext {
  const FlarkMarkdownLinkEditContext({
    required this.replaceRange,
    required this.label,
    required this.url,
    required this.isExisting,
  });

  final FlarkSourceRange replaceRange;
  final String label;
  final String url;
  final bool isExisting;
}

final class FlarkInsertLinkPayload {
  const FlarkInsertLinkPayload({this.userEvent = 'command.insertLink'});

  final String userEvent;
}

final class FlarkApplyLinkEditPayload {
  const FlarkApplyLinkEditPayload({
    required this.context,
    required this.label,
    required this.url,
    this.userEvent = 'command.applyLinkEdit',
  });

  final FlarkMarkdownLinkEditContext context;
  final String label;
  final String url;
  final String userEvent;
}

final class FlarkRemoveLinkPayload {
  const FlarkRemoveLinkPayload({
    required this.linkRange,
    this.userEvent = 'command.removeLink',
  });

  final FlarkSourceRange linkRange;
  final String userEvent;
}

final class FlarkMarkdownLinkEditingExtension extends FlarkExtension {
  const FlarkMarkdownLinkEditingExtension();

  @override
  String get id => 'markdown.linkEditing';

  @override
  FlarkCommandRegistry registerCommands(FlarkCommandRegistry registry) {
    return registry
        .register<FlarkInsertLinkPayload>(
          FlarkMarkdownLinkCommands.insertLink,
          _insertLink,
        )
        .register<FlarkApplyLinkEditPayload>(
          FlarkMarkdownLinkCommands.applyLinkEdit,
          _applyLinkEdit,
        )
        .register<FlarkRemoveLinkPayload>(
          FlarkMarkdownLinkCommands.removeLink,
          _removeLink,
        );
  }

  FlarkCommandResult _insertLink(
    FlarkCommandContext<FlarkInsertLinkPayload> context,
  ) {
    final linkContext = FlarkMarkdownLinkCommands.resolveLinkEditContext(
      context.state,
    );
    final label = linkContext.label.trim().isEmpty
        ? 'link text'
        : linkContext.label;
    final replacement = '[$label]()';
    final selectionOffset = linkContext.replaceRange.start + replacement.length;
    return _replaceLinkRange(
      state: context.state,
      range: linkContext.replaceRange,
      replacement: replacement,
      selectionAfter: FlarkSelection.collapsed(selectionOffset),
      userEvent: context.payload.userEvent,
    );
  }

  FlarkCommandResult _applyLinkEdit(
    FlarkCommandContext<FlarkApplyLinkEditPayload> context,
  ) {
    final cleanUrl = context.payload.url.trim();
    if (cleanUrl.isEmpty) {
      return FlarkCommandResult.rejected('Link URL cannot be empty.');
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
      selectionAfter: FlarkSelection.collapsed(
        range.start + replacement.length,
      ),
      userEvent: context.payload.userEvent,
    );
  }

  FlarkCommandResult _removeLink(
    FlarkCommandContext<FlarkRemoveLinkPayload> context,
  ) {
    final range = _clampedRange(
      context.payload.linkRange,
      context.state.markdown.length,
    );
    final source = context.state.markdown.substring(range.start, range.end);
    final match = _exactLinkPattern.firstMatch(source);
    if (match == null) {
      return FlarkCommandResult.rejected(
        'Link range does not contain a markdown link.',
      );
    }

    final label = match.group(1) ?? '';
    return _replaceLinkRange(
      state: context.state,
      range: range,
      replacement: label,
      selectionAfter: FlarkSelection.collapsed(range.start + label.length),
      userEvent: context.payload.userEvent,
    );
  }
}

FlarkCommandResult _replaceLinkRange({
  required FlarkEditorState state,
  required FlarkSourceRange range,
  required String replacement,
  required FlarkSelection selectionAfter,
  required String userEvent,
}) {
  final safeRange = _clampedRange(range, state.markdown.length);
  return FlarkCommandResult.handled(
    transaction: FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: safeRange,
        replacementText: replacement,
      ),
      selectionBefore: state.selection,
      selectionAfter: selectionAfter.validate(
        state.markdown.length - safeRange.length + replacement.length,
      ),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.command,
        userEvent: userEvent,
        parseInvalidationRange: safeRange,
        projectionInvalidationRange: safeRange,
      ),
    ),
  );
}

FlarkSourceRange _clampedRange(FlarkSourceRange range, int textLength) {
  final start = range.start.clamp(0, textLength);
  final end = range.end.clamp(start, textLength);
  return FlarkSourceRange(start, end);
}
