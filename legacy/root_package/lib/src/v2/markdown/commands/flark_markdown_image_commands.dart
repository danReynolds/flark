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

final RegExp _imagePattern = RegExp(r'!\[([^\]\n]*)\]\(([^)\n]*)\)');
final RegExp _exactImagePattern = RegExp(r'^!\[([^\]\n]*)\]\(([^)\n]*)\)$');

/// Commands for editing and removing inline images (`![alt](src)`), mirroring
/// the link command surface so an image popover can act on them.
abstract final class FlarkMarkdownImageCommands {
  static const applyImageEdit = FlarkCommand<FlarkApplyImageEditPayload>(
    'markdown.applyImageEdit',
  );

  static const removeImage = FlarkCommand<FlarkRemoveImagePayload>(
    'markdown.removeImage',
  );

  /// The image under the caret (or covered by a selection), with its source
  /// range, alt text, and source URL. `isExisting` is false when the caret is
  /// not on an image.
  static FlarkMarkdownImageEditContext resolveImageEditContext(
    FlarkEditorState state,
  ) {
    final text = state.markdown;
    final selection = state.selection;

    if (!selection.isCollapsed) {
      final selectedText = text.substring(selection.start, selection.end);
      final exact = _exactImagePattern.firstMatch(selectedText);
      if (exact != null) {
        return FlarkMarkdownImageEditContext(
          replaceRange: FlarkSourceRange(selection.start, selection.end),
          alt: exact.group(1) ?? '',
          url: exact.group(2) ?? '',
          isExisting: true,
        );
      }
      return FlarkMarkdownImageEditContext(
        replaceRange: FlarkSourceRange(selection.start, selection.end),
        alt: selectedText,
        url: 'https://',
        isExisting: false,
      );
    }

    final cursor = selection.extentOffset.clamp(0, text.length);
    for (final match in _imagePattern.allMatches(text)) {
      if (cursor < match.start || cursor > match.end) continue;
      return FlarkMarkdownImageEditContext(
        replaceRange: FlarkSourceRange(match.start, match.end),
        alt: match.group(1) ?? '',
        url: match.group(2) ?? '',
        isExisting: true,
      );
    }

    return FlarkMarkdownImageEditContext(
      replaceRange: FlarkSourceRange(cursor, cursor),
      alt: '',
      url: 'https://',
      isExisting: false,
    );
  }
}

final class FlarkMarkdownImageEditContext {
  const FlarkMarkdownImageEditContext({
    required this.replaceRange,
    required this.alt,
    required this.url,
    required this.isExisting,
  });

  final FlarkSourceRange replaceRange;
  final String alt;
  final String url;
  final bool isExisting;
}

final class FlarkApplyImageEditPayload {
  const FlarkApplyImageEditPayload({
    required this.context,
    required this.alt,
    required this.url,
    this.userEvent = 'command.applyImageEdit',
  });

  final FlarkMarkdownImageEditContext context;
  final String alt;
  final String url;
  final String userEvent;
}

final class FlarkRemoveImagePayload {
  const FlarkRemoveImagePayload({
    required this.imageRange,
    this.userEvent = 'command.removeImage',
  });

  final FlarkSourceRange imageRange;
  final String userEvent;
}

final class FlarkMarkdownImageEditingExtension extends FlarkExtension {
  const FlarkMarkdownImageEditingExtension();

  @override
  String get id => 'markdown.imageEditing';

  @override
  FlarkCommandRegistry registerCommands(FlarkCommandRegistry registry) {
    return registry
        .register<FlarkApplyImageEditPayload>(
          FlarkMarkdownImageCommands.applyImageEdit,
          _applyImageEdit,
        )
        .register<FlarkRemoveImagePayload>(
          FlarkMarkdownImageCommands.removeImage,
          _removeImage,
        );
  }

  FlarkCommandResult _applyImageEdit(
    FlarkCommandContext<FlarkApplyImageEditPayload> context,
  ) {
    final cleanUrl = context.payload.url.trim();
    if (cleanUrl.isEmpty) {
      return FlarkCommandResult.rejected('Image URL cannot be empty.');
    }
    final replacement = '![${context.payload.alt.trim()}]($cleanUrl)';
    final range = _clampedRange(
      context.payload.context.replaceRange,
      context.state.markdown.length,
    );
    return _replaceImageRange(
      state: context.state,
      range: range,
      replacement: replacement,
      selectionAfter: FlarkSelection.collapsed(range.start + replacement.length),
      userEvent: context.payload.userEvent,
    );
  }

  FlarkCommandResult _removeImage(
    FlarkCommandContext<FlarkRemoveImagePayload> context,
  ) {
    final range = _clampedRange(
      context.payload.imageRange,
      context.state.markdown.length,
    );
    final source = context.state.markdown.substring(range.start, range.end);
    if (!_exactImagePattern.hasMatch(source)) {
      return FlarkCommandResult.rejected(
        'Image range does not contain a markdown image.',
      );
    }
    // Removing an image deletes it outright — its alt text is metadata, not
    // body content to keep (unlike unlinking, which preserves the link text).
    return _replaceImageRange(
      state: context.state,
      range: range,
      replacement: '',
      selectionAfter: FlarkSelection.collapsed(range.start),
      userEvent: context.payload.userEvent,
    );
  }
}

FlarkCommandResult _replaceImageRange({
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
