import '../../core/command/sovereign_command.dart';
import '../../core/command/sovereign_command_registry.dart';
import '../../core/command/sovereign_command_result.dart';
import '../../core/extension/sovereign_extension.dart';
import '../../core/selection/sovereign_selection.dart';
import '../../core/transaction/sovereign_source_operation.dart';
import '../../core/transaction/sovereign_source_range.dart';
import '../../core/transaction/sovereign_transaction.dart';
import '../../core/transaction/sovereign_transaction_metadata.dart';
import '../inline/sovereign_markdown_inline_style.dart';

abstract final class SovereignMarkdownInlineCommands {
  static const toggleInlineStyle =
      SovereignCommand<SovereignToggleInlineStylePayload>(
    'markdown.toggleInlineStyle',
  );
}

final class SovereignToggleInlineStylePayload {
  const SovereignToggleInlineStylePayload(
    this.style, {
    this.userEvent = 'command.toggleInlineStyle',
  });

  final SovereignMarkdownInlineStyle style;
  final String userEvent;
}

final class SovereignMarkdownInlineEditingExtension extends SovereignExtension {
  const SovereignMarkdownInlineEditingExtension();

  @override
  String get id => 'markdown.inlineEditing';

  @override
  SovereignCommandRegistry registerCommands(SovereignCommandRegistry registry) {
    return registry.register<SovereignToggleInlineStylePayload>(
      SovereignMarkdownInlineCommands.toggleInlineStyle,
      _toggleInlineStyle,
    );
  }

  SovereignCommandResult _toggleInlineStyle(
    SovereignCommandContext<SovereignToggleInlineStylePayload> context,
  ) {
    final selection = context.state.selection;
    if (selection.isCollapsed) {
      return SovereignCommandResult.rejected(
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
    final selectionEndsWithMarker = end >= markerLength &&
        _hasUnescapedMarkerAt(text, end - markerLength, marker);
    final hasLeadingMarker =
        markerStart >= 0 && _hasUnescapedMarkerAt(text, markerStart, marker);
    final hasTrailingMarker =
        markerEnd <= text.length && _hasUnescapedMarkerAt(text, end, marker);

    if (selectionStartsWithMarker != selectionEndsWithMarker) {
      return SovereignCommandResult.rejected(
        'Inline style toggling cannot partially overlap source markers.',
      );
    }

    if (hasLeadingMarker != hasTrailingMarker) {
      return SovereignCommandResult.rejected(
        'Inline style toggling cannot partially overlap source markers.',
      );
    }

    if (selectionStartsWithMarker && selectionEndsWithMarker) {
      final innerText = selectedText.substring(
        markerLength,
        selectedText.length - markerLength,
      );
      return SovereignCommandResult.handled(
        transaction: SovereignTransaction.single(
          SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(start, end),
            replacementText: innerText,
          ),
          selectionBefore: selection,
          selectionAfter: SovereignSelection(
            baseOffset: start,
            extentOffset: start + innerText.length,
          ),
          metadata: SovereignTransactionMetadata(
            intent: SovereignTransactionIntent.command,
            userEvent: context.payload.userEvent,
            parseInvalidationRange: SovereignSourceRange(start, end),
            projectionInvalidationRange: SovereignSourceRange(start, end),
          ),
        ),
      );
    }

    if (hasLeadingMarker && hasTrailingMarker) {
      return SovereignCommandResult.handled(
        transaction: SovereignTransaction.single(
          SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(markerStart, markerEnd),
            replacementText: selectedText,
          ),
          selectionBefore: selection,
          selectionAfter: SovereignSelection(
            baseOffset: markerStart,
            extentOffset: markerStart + selectedText.length,
          ),
          metadata: SovereignTransactionMetadata(
            intent: SovereignTransactionIntent.command,
            userEvent: context.payload.userEvent,
            parseInvalidationRange: SovereignSourceRange(
              markerStart,
              markerEnd,
            ),
            projectionInvalidationRange: SovereignSourceRange(
              markerStart,
              markerEnd,
            ),
          ),
        ),
      );
    }

    return SovereignCommandResult.handled(
      transaction: SovereignTransaction.single(
        SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(start, end),
          replacementText: '$marker$selectedText$marker',
        ),
        selectionBefore: selection,
        selectionAfter: SovereignSelection(
          baseOffset: start + markerLength,
          extentOffset: start + markerLength + selectedText.length,
        ),
        metadata: SovereignTransactionMetadata(
          intent: SovereignTransactionIntent.command,
          userEvent: context.payload.userEvent,
          parseInvalidationRange: SovereignSourceRange(start, end),
          projectionInvalidationRange: SovereignSourceRange(start, end),
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
