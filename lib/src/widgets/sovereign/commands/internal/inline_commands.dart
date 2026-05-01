import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_command_result.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_inline_style.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

import 'command_context.dart';
import 'command_transaction.dart';

typedef _InlineMarkerPair = ({String prefix, String suffix});
typedef _InlineWrapperRange = ({
  int prefixStart,
  int contentStart,
  int suffixStart,
});

const Map<SovereignInlineStyle, _InlineMarkerPair> _inlineMarkers = {
  SovereignInlineStyle.bold: (prefix: '**', suffix: '**'),
  SovereignInlineStyle.italic: (prefix: '*', suffix: '*'),
  SovereignInlineStyle.inlineCode: (prefix: '`', suffix: '`'),
};

// Keep empty wrappers syntactically inert while preserving typing mode.
const String _emptyInlinePlaceholder = '\u2060';

extension on SovereignCommandResult {
  bool get isApplied => this is SovereignCommandApplied;
}

String _inlineToken(_InlineMarkerPair markers) =>
    '${markers.prefix}${markers.suffix}';

String _placeholderInlineToken(_InlineMarkerPair markers) =>
    '${markers.prefix}$_emptyInlinePlaceholder${markers.suffix}';

_InlineWrapperRange? _findEnclosingInlineWrapper({
  required String text,
  required int cursor,
  required _InlineMarkerPair markers,
}) {
  if (text.isEmpty) return null;

  var prefixIndex = text.lastIndexOf(markers.prefix, cursor);
  while (prefixIndex >= 0) {
    final contentStart = prefixIndex + markers.prefix.length;
    if (contentStart > text.length) break;

    final suffixIndex = text.indexOf(markers.suffix, contentStart);
    if (suffixIndex < 0) {
      if (prefixIndex == 0) break;
      prefixIndex = text.lastIndexOf(markers.prefix, prefixIndex - 1);
      continue;
    }

    final inside = cursor >= contentStart && cursor <= suffixIndex;
    if (inside) {
      return (
        prefixStart: prefixIndex,
        contentStart: contentStart,
        suffixStart: suffixIndex,
      );
    }

    if (prefixIndex == 0) break;
    prefixIndex = text.lastIndexOf(markers.prefix, prefixIndex - 1);
  }

  return null;
}

_InlineWrapperRange? _selectedPlaceholderWrapper({
  required SovereignCommandContext context,
  required _InlineMarkerPair markers,
}) {
  final selection = context.selection;
  if (selection.isCollapsed ||
      selection.start < 0 ||
      selection.end > context.text.length ||
      selection.start >= selection.end) {
    return null;
  }

  final selected = context.text.substring(selection.start, selection.end);
  if (selected != _placeholderInlineToken(markers)) return null;

  final contentStart = selection.start + markers.prefix.length;
  final suffixStart = contentStart + _emptyInlinePlaceholder.length;
  return (
    prefixStart: selection.start,
    contentStart: contentStart,
    suffixStart: suffixStart,
  );
}

SovereignInlineStyle? _selectedPlaceholderWrapperStyle(
  SovereignCommandContext context,
) {
  final selection = context.selection;
  if (selection.isCollapsed ||
      selection.start < 0 ||
      selection.end > context.text.length ||
      selection.start >= selection.end) {
    return null;
  }
  final selected = context.text.substring(selection.start, selection.end);
  for (final entry in _inlineMarkers.entries) {
    if (selected == _placeholderInlineToken(entry.value)) return entry.key;
  }
  return null;
}

_InlineWrapperRange? _findEmptyInlineWrapper({
  required SovereignCommandContext context,
  required _InlineMarkerPair markers,
}) {
  final text = context.text;
  if (text.isEmpty || !context.selection.isCollapsed) return null;
  final cursor = context.selection.extentOffset.clamp(0, text.length).toInt();

  final enclosed = _findEnclosingInlineWrapper(
    text: text,
    cursor: cursor,
    markers: markers,
  );
  if (enclosed != null) {
    final content = text.substring(enclosed.contentStart, enclosed.suffixStart);
    final isEffectivelyEmpty = enclosed.suffixStart == enclosed.contentStart ||
        content == _emptyInlinePlaceholder;
    if (!isEffectivelyEmpty) return null;
    return enclosed;
  }

  final token = _inlineToken(markers);
  var tokenIndex = text.lastIndexOf(token, cursor);
  if (tokenIndex < 0) tokenIndex = text.lastIndexOf(token);
  if (tokenIndex < 0) return null;

  final contentStart = tokenIndex + markers.prefix.length;
  return (
    prefixStart: tokenIndex,
    contentStart: contentStart,
    suffixStart: contentStart,
  );
}

int _trailingWhitespaceLength(String value) {
  var count = 0;
  for (var i = value.length - 1; i >= 0; i -= 1) {
    final unit = value.codeUnitAt(i);
    if (unit == 0x20 || unit == 0x09) {
      count += 1;
      continue;
    }
    break;
  }
  return count;
}

SovereignCommandResult _deactivateInlineWrapper(
  SovereignController controller, {
  required _InlineMarkerPair markers,
}) {
  final context = SovereignCommandContext.fromController(controller);
  final text = context.text;
  if (text.isEmpty) {
    return SovereignCommandNoOp.code(
      SovereignCommandReasonCode.noActiveInlineStyle,
    );
  }

  final cursor = context.selection.extentOffset.clamp(0, text.length).toInt();
  final wrapper = _findEnclosingInlineWrapper(
        text: text,
        cursor: cursor,
        markers: markers,
      ) ??
      _selectedPlaceholderWrapper(context: context, markers: markers);
  if (wrapper == null) {
    return SovereignCommandNoOp.code(
      SovereignCommandReasonCode.noActiveInlineStyle,
    );
  }

  final wrapperContent = text.substring(
    wrapper.contentStart,
    wrapper.suffixStart,
  );
  final isEffectivelyEmpty = wrapper.suffixStart == wrapper.contentStart ||
      wrapperContent == _emptyInlinePlaceholder;
  if (isEffectivelyEmpty) {
    final removeStart = wrapper.prefixStart;
    final removeEnd = wrapper.suffixStart + markers.suffix.length;
    final updated = text.replaceRange(removeStart, removeEnd, '');
    return commitCommandMutation(context, (
      text: updated,
      selection: TextSelection.collapsed(offset: removeStart),
      composing: TextRange.empty,
    ));
  }

  // Keep whitespace outside the wrapper when deactivating at the suffix
  // boundary to avoid adjacent `***` delimiter runs on style switching.
  if (cursor == wrapper.suffixStart) {
    final trailingWhitespace = _trailingWhitespaceLength(wrapperContent);
    if (trailingWhitespace > 0 && trailingWhitespace < wrapperContent.length) {
      final boundaryStart = wrapper.suffixStart - trailingWhitespace;
      final movedWhitespace = text.substring(
        boundaryStart,
        wrapper.suffixStart,
      );
      final suffixEnd = wrapper.suffixStart + markers.suffix.length;
      final updated = text.replaceRange(
        boundaryStart,
        suffixEnd,
        '${markers.suffix}$movedWhitespace',
      );
      final targetOffset =
          (boundaryStart + markers.suffix.length + trailingWhitespace).clamp(
        0,
        updated.length,
      );
      return commitCommandMutation(context, (
        text: updated,
        selection: TextSelection.collapsed(offset: targetOffset),
        composing: TextRange.empty,
      ));
    }
  }

  final targetOffset = (wrapper.suffixStart + markers.suffix.length).clamp(
    0,
    text.length,
  );
  return commitCommandMutation(context, (
    text: text,
    selection: TextSelection.collapsed(offset: targetOffset),
    composing: TextRange.empty,
  ));
}

SovereignCommandResult _replaceEmptyInlineWrapper(
  SovereignController controller, {
  required SovereignInlineStyle fromStyle,
  required SovereignInlineStyle toStyle,
}) {
  final fromMarkers = _inlineMarkers[fromStyle];
  final toMarkers = _inlineMarkers[toStyle];
  if (fromMarkers == null || toMarkers == null) {
    return SovereignCommandRejected.code(
      SovereignCommandReasonCode.unsupportedInlineStyle,
    );
  }

  final context = SovereignCommandContext.fromController(controller);
  final wrapper = _findEmptyInlineWrapper(
    context: context,
    markers: fromMarkers,
  );
  if (wrapper == null) {
    return SovereignCommandNoOp.code(SovereignCommandReasonCode.noEmptyWrapper);
  }

  final source = context.text;
  final replaceStart = wrapper.prefixStart;
  final replaceEnd = wrapper.suffixStart + fromMarkers.suffix.length;
  final replacement = _placeholderInlineToken(toMarkers);
  final updated = source.replaceRange(replaceStart, replaceEnd, replacement);
  final targetStart = (replaceStart + toMarkers.prefix.length).clamp(
    0,
    updated.length,
  );
  final targetOffset = (targetStart + _emptyInlinePlaceholder.length).clamp(
    0,
    updated.length,
  );
  return commitCommandMutation(context, (
    text: updated,
    selection: TextSelection.collapsed(offset: targetOffset),
    composing: TextRange.empty,
  ));
}

SovereignCommandResult _wrapSelectionWithInlineMarkers(
  SovereignController controller, {
  required _InlineMarkerPair markers,
}) {
  final context = SovereignCommandContext.fromController(controller);
  final selection = context.selection;
  final selectedText = context.text.substring(selection.start, selection.end);
  final hasSelection = selectedText.isNotEmpty;
  final content = hasSelection ? selectedText : _emptyInlinePlaceholder;
  final replacement = '${markers.prefix}$content${markers.suffix}';
  final updated = context.text.replaceRange(
    selection.start,
    selection.end,
    replacement,
  );
  final selectionStart = (selection.start + markers.prefix.length).clamp(
    0,
    updated.length,
  );
  final targetSelection = hasSelection
      ? TextSelection(
          baseOffset: selectionStart,
          extentOffset: (selectionStart + content.length).clamp(
            0,
            updated.length,
          ),
        )
      : TextSelection.collapsed(
          offset: (selectionStart + _emptyInlinePlaceholder.length).clamp(
            0,
            updated.length,
          ),
        );
  return commitCommandMutation(context, (
    text: updated,
    selection: targetSelection,
    composing: TextRange.empty,
  ));
}

abstract final class SovereignInlineCommands {
  static SovereignInlineStyle? activeInlineStyleAtCursor(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    final text = context.text;
    if (text.isEmpty) return null;

    final cursor = context.selection.extentOffset.clamp(0, text.length).toInt();
    ({SovereignInlineStyle style, int span})? best;
    for (final entry in _inlineMarkers.entries) {
      final wrapper = _findEnclosingInlineWrapper(
        text: text,
        cursor: cursor,
        markers: entry.value,
      );
      if (wrapper == null) continue;
      final span = wrapper.suffixStart - wrapper.contentStart;
      if (best == null || span < best.span) {
        best = (style: entry.key, span: span);
      }
    }
    if (best != null) return best.style;
    return _selectedPlaceholderWrapperStyle(context);
  }

  static SovereignCommandResult toggleInlineStyle(
    SovereignController controller,
    SovereignInlineStyle style,
  ) {
    final markers = _inlineMarkers[style];
    if (markers == null) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.unsupportedInlineStyle,
      );
    }

    final context = SovereignCommandContext.fromController(controller);
    final active = activeInlineStyleAtCursor(controller) ??
        _selectedPlaceholderWrapperStyle(context);
    if (active == style) {
      return _deactivateInlineWrapper(controller, markers: markers);
    }

    if (active != null) {
      final switchResult = _replaceEmptyInlineWrapper(
        controller,
        fromStyle: active,
        toStyle: style,
      );
      if (switchResult.isApplied) return switchResult;

      final activeMarkers = _inlineMarkers[active];
      if (activeMarkers != null) {
        final deactivateResult = _deactivateInlineWrapper(
          controller,
          markers: activeMarkers,
        );
        if (deactivateResult is SovereignCommandRejected) {
          return deactivateResult;
        }
      }
    }

    return _wrapSelectionWithInlineMarkers(controller, markers: markers);
  }

  static SovereignCommandResult deactivateInlineStyle(
    SovereignController controller,
  ) {
    final active = activeInlineStyleAtCursor(controller);
    if (active == null) {
      return SovereignCommandNoOp.code(
        SovereignCommandReasonCode.noActiveInlineStyle,
      );
    }
    final markers = _inlineMarkers[active];
    if (markers == null) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.unsupportedInlineStyle,
      );
    }
    return _deactivateInlineWrapper(controller, markers: markers);
  }
}
