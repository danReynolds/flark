import '../../core/command/flark_command.dart';
import '../../core/command/flark_command_registry.dart';
import '../../core/command/flark_command_result.dart';
import '../../core/extension/flark_extension.dart';
import '../../core/selection/flark_selection.dart';
import '../../core/transaction/flark_source_operation.dart';
import '../../core/transaction/flark_source_range.dart';
import '../../core/transaction/flark_transaction.dart';
import '../../core/transaction/flark_transaction_metadata.dart';
import '../inline/flark_inline_delimiter_placement.dart';
import '../inline/flark_inline_flanking.dart';
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

    // Toggling a style off must recognize whichever equivalent delimiter the
    // source actually uses, not just the canonical one: `_text_` unwraps under
    // emphasis exactly like `*text*`, `__x__` under strong like `**x**`, and
    // `~x~` under strikethrough like `~~x~~`. Probe every equivalent form
    // (canonical first) and unwrap the first that brackets the selection — with
    // that form's own marker length. A form whose markers only partially
    // overlap the selection is a malformed target and is rejected; when no form
    // brackets the selection the style is applied fresh with the canonical
    // marker below.
    var partialOverlap = false;
    for (final marker in context.payload.style.equivalentMarkers) {
      final markerLength = marker.length;
      final markerStart = start - markerLength;
      final markerEnd = end + markerLength;
      final selectionStartsWithMarker =
          end - start >= markerLength &&
          _isToggleableMarkerRun(text, start, marker);
      final selectionEndsWithMarker =
          end - start >= markerLength &&
          _isToggleableMarkerRun(text, end - markerLength, marker);
      final hasLeadingMarker =
          markerStart >= 0 && _isToggleableMarkerRun(text, markerStart, marker);
      final hasTrailingMarker =
          markerEnd <= text.length && _isToggleableMarkerRun(text, end, marker);

      if (selectionStartsWithMarker != selectionEndsWithMarker ||
          hasLeadingMarker != hasTrailingMarker) {
        partialOverlap = true;
        continue;
      }

      if (selectionStartsWithMarker &&
          selectionEndsWithMarker &&
          end - start >= 2 * markerLength) {
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
    }

    if (partialOverlap) {
      return FlarkCommandResult.rejected(
        'Inline style toggling cannot partially overlap source markers.',
      );
    }

    // No equivalent form brackets the selection: apply the style fresh with the
    // canonical marker (`equivalentMarkers.first`), wrapping each PARAGRAPH in
    // the selection independently. A single delimiter pair cannot span a blank
    // line — `**alpha\n\nbeta**` is literal text, not one bold run — so a
    // multi-paragraph selection wrapped as a whole would commit invalid
    // markdown through the public toggle API and break the always-valid-inline
    // invariant. Per the "valid subset" contract, a paragraph whose wrap would
    // collide with an interior same-marker run that closes the delimiter early
    // (`**a**b**`) is left unstyled instead.
    final marker = context.payload.style.marker;
    final markerLength = marker.length;
    final isCode = context.payload.style == FlarkMarkdownInlineStyle.inlineCode;

    final buffer = StringBuffer();
    int? firstInnerStart;
    int? lastInnerEnd;
    // A paragraph break is a newline followed by one or more blank lines; a
    // single soft line break stays within one paragraph (emphasis may
    // soft-wrap across it), so only `\n\n`+ splits.
    final paragraphBreak = RegExp(r'\n(?:[ \t]*\n)+');
    selectedText.splitMapJoin(
      paragraphBreak,
      onMatch: (match) {
        buffer.write(match[0]);
        return '';
      },
      onNonMatch: (segment) {
        final base = start + buffer.length;
        final wrap = _wrapFreshParagraph(segment, marker, markerLength, isCode);
        if (wrap == null) {
          buffer.write(segment);
        } else {
          buffer.write(wrap.text);
          final innerStart = base + wrap.innerOffset;
          firstInnerStart ??= innerStart;
          lastInnerEnd = innerStart + wrap.innerLength;
        }
        return '';
      },
    );

    if (firstInnerStart == null || lastInnerEnd == null) {
      return FlarkCommandResult.rejected(
        'Inline style toggling requires non-whitespace content.',
      );
    }

    return FlarkCommandResult.handled(
      transaction: FlarkTransaction.single(
        FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(start, end),
          replacementText: buffer.toString(),
        ),
        selectionBefore: selection,
        selectionAfter: FlarkSelection(
          baseOffset: firstInnerStart!,
          extentOffset: lastInnerEnd!,
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

  /// Wraps a single paragraph [segment] in [marker], hugging its non-whitespace
  /// core (CommonMark refuses a delimiter stranded against whitespace, so edge
  /// whitespace stays outside the markers). Returns null — leaving the segment
  /// unstyled — when there is nothing to style (whitespace-only) or when the
  /// wrap would collide with an interior same-marker run that closes the
  /// delimiter early. Inline code is verbatim: backticks may hug whitespace and
  /// interior markers are literal, so it is wrapped as-is.
  ({String text, int innerOffset, int innerLength})? _wrapFreshParagraph(
    String segment,
    String marker,
    int markerLength,
    bool isCode,
  ) {
    if (isCode) {
      if (segment.isEmpty) return null;
      return (
        text: '$marker$segment$marker',
        innerOffset: markerLength,
        innerLength: segment.length,
      );
    }
    final split = FlarkInlineDelimiterPlacement.splitEdgeWhitespace(segment);
    if (split.core.isEmpty) return null;
    final text = '${split.leading}$marker${split.core}$marker${split.trailing}';
    final coreStart = split.leading.length + markerLength;
    final coreEnd = coreStart + split.core.length;
    if (_coreClosesDelimiterEarly(text, coreStart, coreEnd, markerLength)) {
      return null;
    }
    return (
      text: text,
      innerOffset: coreStart,
      innerLength: split.core.length,
    );
  }

  /// Whether the wrapped [text]'s core (`[coreStart, coreEnd)`) contains an
  /// unescaped marker run of at least [markerLength] that can close the opening
  /// delimiter — which would make `marker + core + marker` misparse (the inner
  /// run closes the run early and the trailing marker leaks as literal text).
  ///
  /// Length-aware so a strong wrap (`**`) is not tripped by a nested emphasis
  /// `*b*`, and flanking-aware so a space-flanked interior marker (`2 * 3`,
  /// which is literal) does not block the wrap.
  bool _coreClosesDelimiterEarly(
    String text,
    int coreStart,
    int coreEnd,
    int markerLength,
  ) {
    final markerChar = text.codeUnitAt(coreStart - 1);
    var index = coreStart;
    while (index < coreEnd) {
      if (text.codeUnitAt(index) != markerChar ||
          FlarkInlineFlanking.isEscaped(text, index)) {
        index += 1;
        continue;
      }
      var runEnd = index;
      while (runEnd < coreEnd && text.codeUnitAt(runEnd) == markerChar) {
        runEnd += 1;
      }
      if (runEnd - index >= markerLength &&
          FlarkInlineFlanking.canClose(text, index, runEnd)) {
        return true;
      }
      index = runEnd;
    }
    return false;
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
    final validRun = marker.length == 1 ? runLength.isOdd : runLength >= 2;
    if (!validRun) return false;
    // An emphasis-family delimiter must also be flanking-valid to act as a
    // marker here: an intraword `_`/`__`/`~` (the `_` in `my_variable`) is not
    // a real emphasis delimiter, so a selection merely abutting one applies the
    // style fresh instead of misreading it as a partial wrap and rejecting.
    // Backticks carry no flanking rule (code spans pair by run) and are exempt.
    if (markerChar == 0x60) return true;
    final markerEnd = candidateStart + marker.length;
    return FlarkInlineFlanking.canOpen(text, candidateStart, markerEnd) ||
        FlarkInlineFlanking.canClose(text, candidateStart, markerEnd);
  }

  bool _hasUnescapedMarkerAt(String text, int offset, String marker) {
    if (offset < 0 || offset + marker.length > text.length) return false;
    if (text.substring(offset, offset + marker.length) != marker) return false;
    return !FlarkInlineFlanking.isEscaped(text, offset);
  }
}
