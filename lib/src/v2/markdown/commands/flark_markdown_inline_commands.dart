import '../../core/command/flark_command.dart';
import '../../core/command/flark_command_registry.dart';
import '../../core/command/flark_command_result.dart';
import '../../core/extension/flark_extension.dart';
import '../../core/selection/flark_selection.dart';
import '../../core/transaction/flark_source_operation.dart';
import '../../core/transaction/flark_source_range.dart';
import '../../core/transaction/flark_transaction.dart';
import '../../core/transaction/flark_transaction_metadata.dart';
import '../inline/flark_markdown_inline_style.dart';

abstract final class FlarkMarkdownInlineCommands {
  static const toggleInlineStyle = FlarkCommand<FlarkToggleInlineStylePayload>(
    'markdown.toggleInlineStyle',
  );
}

final class FlarkToggleInlineStylePayload {
  const FlarkToggleInlineStylePayload(
    this.style, {
    this.userEvent = 'command.toggleInlineStyle',
  });

  final FlarkMarkdownInlineStyle style;
  final String userEvent;
}

final class FlarkMarkdownInlineEditingExtension extends FlarkExtension {
  const FlarkMarkdownInlineEditingExtension();

  @override
  String get id => 'markdown.inlineEditing';

  @override
  FlarkCommandRegistry registerCommands(FlarkCommandRegistry registry) {
    return registry.register<FlarkToggleInlineStylePayload>(
      FlarkMarkdownInlineCommands.toggleInlineStyle,
      _toggleInlineStyle,
    );
  }

  FlarkCommandResult _toggleInlineStyle(
    FlarkCommandContext<FlarkToggleInlineStylePayload> context,
  ) {
    final selection = context.state.selection;
    if (selection.isCollapsed) {
      return FlarkCommandResult.rejected(
        'Inline style toggling requires a selected source range.',
      );
    }

    final text = context.state.markdown;
    final marker = context.payload.style.marker;
    final start = selection.start;
    final end = selection.end;
    final selectedText = text.substring(start, end);
    final markerLength = marker.length;
    final markerStart = start - markerLength;
    final markerEnd = end + markerLength;
    final selectionStartsWithMarker =
        _hasUnescapedMarkerAt(text, start, marker) &&
        end - start >= markerLength;
    final selectionEndsWithMarker =
        end >= markerLength &&
        _hasUnescapedMarkerAt(text, end - markerLength, marker);
    final hasLeadingMarker =
        markerStart >= 0 && _hasUnescapedMarkerAt(text, markerStart, marker);
    final hasTrailingMarker =
        markerEnd <= text.length && _hasUnescapedMarkerAt(text, end, marker);

    if (selectionStartsWithMarker != selectionEndsWithMarker) {
      return FlarkCommandResult.rejected(
        'Inline style toggling cannot partially overlap source markers.',
      );
    }

    if (hasLeadingMarker != hasTrailingMarker) {
      return FlarkCommandResult.rejected(
        'Inline style toggling cannot partially overlap source markers.',
      );
    }

    if (selectionStartsWithMarker && selectionEndsWithMarker) {
      final innerText = selectedText.substring(
        markerLength,
        selectedText.length - markerLength,
      );
      return FlarkCommandResult.handled(
        transaction: FlarkTransaction.single(
          FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(start, end),
            replacementText: innerText,
          ),
          selectionBefore: selection,
          selectionAfter: FlarkSelection(
            baseOffset: start,
            extentOffset: start + innerText.length,
          ),
          metadata: FlarkTransactionMetadata(
            intent: FlarkTransactionIntent.command,
            userEvent: context.payload.userEvent,
            parseInvalidationRange: FlarkSourceRange(start, end),
            projectionInvalidationRange: FlarkSourceRange(start, end),
          ),
        ),
      );
    }

    if (hasLeadingMarker && hasTrailingMarker) {
      return FlarkCommandResult.handled(
        transaction: FlarkTransaction.single(
          FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(markerStart, markerEnd),
            replacementText: selectedText,
          ),
          selectionBefore: selection,
          selectionAfter: FlarkSelection(
            baseOffset: markerStart,
            extentOffset: markerStart + selectedText.length,
          ),
          metadata: FlarkTransactionMetadata(
            intent: FlarkTransactionIntent.command,
            userEvent: context.payload.userEvent,
            parseInvalidationRange: FlarkSourceRange(markerStart, markerEnd),
            projectionInvalidationRange: FlarkSourceRange(
              markerStart,
              markerEnd,
            ),
          ),
        ),
      );
    }

    return FlarkCommandResult.handled(
      transaction: FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(start, end),
          replacementText: '$marker$selectedText$marker',
        ),
        selectionBefore: selection,
        selectionAfter: FlarkSelection(
          baseOffset: start + markerLength,
          extentOffset: start + markerLength + selectedText.length,
        ),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.command,
          userEvent: context.payload.userEvent,
          parseInvalidationRange: FlarkSourceRange(start, end),
          projectionInvalidationRange: FlarkSourceRange(start, end),
        ),
      ),
    );
  }

  bool _hasUnescapedMarkerAt(String text, int offset, String marker) {
    if (offset < 0 || offset + marker.length > text.length) return false;
    if (text.substring(offset, offset + marker.length) != marker) return false;

    var backslashCount = 0;
    var cursor = offset - 1;
    while (cursor >= 0 && text.codeUnitAt(cursor) == 0x5C) {
      backslashCount += 1;
      cursor -= 1;
    }
    return backslashCount.isEven;
  }
}
