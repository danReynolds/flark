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
    final text = context.state.markdown;
    final marker = context.payload.style.marker;

    if (selection.isCollapsed) {
      // A collapsed caret carries no range to wrap or unwrap. Arming a style on
      // or off for a collapsed caret is handled one layer up, on the controller
      // (pending / muted), so the command itself rejects.
      return FlarkCommandResult.rejected(
        'Inline style toggling requires a selected source range.',
      );
    }
    final start = selection.start;
    final end = selection.end;
    final selectedText = text.substring(start, end);
    final markerLength = marker.length;
    final markerStart = start - markerLength;
    final markerEnd = end + markerLength;
    final selectionStartsWithMarker =
        _isToggleableMarkerRun(text, start, marker) &&
        end - start >= markerLength;
    final selectionEndsWithMarker =
        end >= markerLength &&
        _isToggleableMarkerRun(text, end - markerLength, marker);
    final hasLeadingMarker =
        markerStart >= 0 && _isToggleableMarkerRun(text, markerStart, marker);
    final hasTrailingMarker =
        markerEnd <= text.length && _isToggleableMarkerRun(text, end, marker);

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


  /// Whether the marker candidate at [candidateStart] can act as one side of
  /// a toggle-off pair under CommonMark delimiter-run semantics.
  ///
  /// The candidate must exist unescaped, and the full contiguous run of the
  /// marker character containing it must actually carry the requested style:
  /// an odd-length run carries emphasis (`*`, `***`), a run of two or more
  /// carries strong. Without the run check, the inner `*` of `**bold**`
  /// passes as an emphasis pair and toggling italic strips one layer of the
  /// strong markers instead of nesting.
  bool _isToggleableMarkerRun(String text, int candidateStart, String marker) {
    if (!_hasUnescapedMarkerAt(text, candidateStart, marker)) return false;
    final markerChar = marker.codeUnitAt(0);
    var runStart = candidateStart;
    while (runStart > 0 && text.codeUnitAt(runStart - 1) == markerChar) {
      runStart -= 1;
    }
    var runEnd = candidateStart + marker.length;
    while (runEnd < text.length && text.codeUnitAt(runEnd) == markerChar) {
      runEnd += 1;
    }
    final runLength = runEnd - runStart;
    return marker.length == 1 ? runLength.isOdd : runLength >= 2;
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
