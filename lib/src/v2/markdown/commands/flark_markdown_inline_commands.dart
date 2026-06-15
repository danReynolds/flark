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
      // Toggling a style off with a collapsed caret inside its run exits the
      // run — the caret steps past the hidden closing marker so future typing
      // is unstyled — rather than unwrapping the text already written. At the
      // run's trailing edge the closing marker is zero-width, so this is an
      // invisible step (matches Google Docs / Word "turn off, keep typing").
      // (Arming a new style on a collapsed caret is handled one layer up, on
      // the controller, before reaching here.)
      final exit = _collapsedExitRun(
        text,
        selection.extentOffset,
        marker,
        context.payload.userEvent,
      );
      if (exit != null) return exit;
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

  /// Exits the run of [marker] enclosing the collapsed [caret] by stepping the
  /// caret just past the closing marker, or null when the caret is not inside
  /// such a run.
  ///
  /// The text already written stays styled; only the caret moves, so the next
  /// character typed lands outside the run as unstyled text. This is a
  /// selection-only move (no document edit, not recorded in history).
  FlarkCommandResult? _collapsedExitRun(
    String text,
    int caret,
    String marker,
    String userEvent,
  ) {
    final run = _enclosingMarkerRun(text, caret, marker);
    if (run == null) return null;
    return FlarkCommandResult.handled(
      transaction: FlarkTransaction(
        operations: const [],
        selectionBefore: FlarkSelection.collapsed(caret),
        selectionAfter: FlarkSelection.collapsed(run.closeEnd),
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.selection,
          userEvent: userEvent,
          addToHistory: false,
        ),
      ),
    );
  }

  /// Finds the innermost run of [marker] whose content encloses [caret].
  ///
  /// The opener is the nearest toggleable marker run starting at or before the
  /// caret's content; the closer is the nearest toggleable run starting at or
  /// after the caret. Returns null when either side is missing.
  _MarkerRun? _enclosingMarkerRun(String text, int caret, String marker) {
    final length = marker.length;

    var openStart = -1;
    var probe = caret - length;
    while (probe >= 0) {
      final index = text.lastIndexOf(marker, probe);
      if (index < 0) break;
      if (_isToggleableMarkerRun(text, index, marker)) {
        openStart = index;
        break;
      }
      probe = index - 1;
    }
    if (openStart < 0 || openStart + length > caret) return null;
    final contentStart = openStart + length;

    var closeStart = -1;
    var search = caret;
    while (search <= text.length - length) {
      final index = text.indexOf(marker, search);
      if (index < 0) break;
      if (index >= caret && _isToggleableMarkerRun(text, index, marker)) {
        closeStart = index;
        break;
      }
      search = index + length;
    }
    if (closeStart < contentStart) return null;

    return _MarkerRun(
      openStart: openStart,
      contentStart: contentStart,
      contentEnd: closeStart,
      closeEnd: closeStart + length,
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

/// The source span of an inline run: its marker boundaries and content range.
final class _MarkerRun {
  const _MarkerRun({
    required this.openStart,
    required this.contentStart,
    required this.contentEnd,
    required this.closeEnd,
  });

  final int openStart;
  final int contentStart;
  final int contentEnd;
  final int closeEnd;
}
