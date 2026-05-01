import 'package:flutter/services.dart';
import 'package:sovereign_editor/widgets/sovereign/commands/models/sovereign_command_result.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

import 'command_context.dart';
import 'command_ranges.dart';
import 'command_selection.dart';
import 'command_transaction.dart';

final RegExp _headingPrefixPattern = RegExp(r'^(#{1,6})\s+');
final RegExp _leadingQuotePrefixPattern = RegExp(r'^((?:\s*> ?)+)(.*)$');

({String text, TextSelection selection}) _toggleLinePrefix(
  SovereignCommandContext context,
  String prefix,
) {
  final text = context.text;
  final selection = context.selection;
  final lineRange = selectedLineRange(text, selection);
  final segment = text.substring(lineRange.start, lineRange.end);
  final lines = segment.split('\n');
  if (lines.isEmpty) {
    return (text: text, selection: selection);
  }

  final shouldRemove = lines.every((line) => line.startsWith(prefix));
  final transformedLines = lines.map((line) {
    if (shouldRemove) {
      return line.startsWith(prefix) ? line.substring(prefix.length) : line;
    }
    return '$prefix$line';
  }).toList(growable: false);
  final transformedSegment = transformedLines.join('\n');
  final updated = text.replaceRange(
    lineRange.start,
    lineRange.end,
    transformedSegment,
  );

  if (selection.isCollapsed) {
    final oldLineStart = lineStart(text, selection.extentOffset);
    final oldLineEnd = lineEnd(text, selection.extentOffset);
    final oldLine = text.substring(oldLineStart, oldLineEnd);
    final oldColumn = selection.extentOffset - oldLineStart;
    final newLine = shouldRemove
        ? (oldLine.startsWith(prefix)
            ? oldLine.substring(prefix.length)
            : oldLine)
        : '$prefix$oldLine';
    final newColumn = shouldRemove
        ? (oldLine.startsWith(prefix)
            ? (oldColumn - prefix.length).clamp(0, newLine.length)
            : oldColumn.clamp(0, newLine.length))
        : (oldColumn + prefix.length).clamp(0, newLine.length);
    final offset = (oldLineStart + newColumn).clamp(0, updated.length);
    return (text: updated, selection: TextSelection.collapsed(offset: offset));
  }

  final end = (lineRange.start + transformedSegment.length).clamp(
    0,
    updated.length,
  );
  return (
    text: updated,
    selection: TextSelection(baseOffset: lineRange.start, extentOffset: end),
  );
}

({String quotePrefix, String body}) _splitLeadingQuotePrefix(String line) {
  final match = _leadingQuotePrefixPattern.firstMatch(line);
  if (match == null) {
    return (quotePrefix: '', body: line);
  }
  return (quotePrefix: match.group(1) ?? '', body: match.group(2) ?? '');
}

({String text, TextSelection selection}) _toggleListLinePrefix(
  SovereignCommandContext context,
  String prefix,
) {
  final text = context.text;
  final selection = context.selection;
  final lineRange = selectedLineRange(text, selection);
  final segment = text.substring(lineRange.start, lineRange.end);
  final lines = segment.split('\n');
  if (lines.isEmpty) {
    return (text: text, selection: selection);
  }

  final shouldRemove = lines.every((line) {
    final parts = _splitLeadingQuotePrefix(line);
    return parts.body.startsWith(prefix);
  });

  final transformedLines = lines.map((line) {
    final parts = _splitLeadingQuotePrefix(line);
    final body = parts.body;
    final transformedBody = shouldRemove
        ? (body.startsWith(prefix) ? body.substring(prefix.length) : body)
        : '$prefix$body';
    return '${parts.quotePrefix}$transformedBody';
  }).toList(growable: false);

  final transformedSegment = transformedLines.join('\n');
  final updated = text.replaceRange(
    lineRange.start,
    lineRange.end,
    transformedSegment,
  );

  if (selection.isCollapsed) {
    final oldLineStart = lineStart(text, selection.extentOffset);
    final oldLineEnd = lineEnd(text, selection.extentOffset);
    final oldLine = text.substring(oldLineStart, oldLineEnd);
    final oldColumn = selection.extentOffset - oldLineStart;
    final parts = _splitLeadingQuotePrefix(oldLine);
    final oldBodyColumn = (oldColumn - parts.quotePrefix.length).clamp(
      0,
      parts.body.length,
    );
    final transformedBody = shouldRemove
        ? (parts.body.startsWith(prefix)
            ? parts.body.substring(prefix.length)
            : parts.body)
        : '$prefix${parts.body}';
    final transformedBodyColumn = shouldRemove
        ? (parts.body.startsWith(prefix)
            ? (oldBodyColumn - prefix.length).clamp(0, transformedBody.length)
            : oldBodyColumn.clamp(0, transformedBody.length))
        : (oldBodyColumn + prefix.length).clamp(0, transformedBody.length);
    final newColumn = (parts.quotePrefix.length + transformedBodyColumn).clamp(
      0,
      (parts.quotePrefix.length + transformedBody.length),
    );
    final offset = (oldLineStart + newColumn).clamp(0, updated.length);
    return (text: updated, selection: TextSelection.collapsed(offset: offset));
  }

  final end = (lineRange.start + transformedSegment.length).clamp(
    0,
    updated.length,
  );
  return (
    text: updated,
    selection: TextSelection(baseOffset: lineRange.start, extentOffset: end),
  );
}

({String text, TextSelection selection}) _setHeadingLevel(
  SovereignCommandContext context,
  int? level,
) {
  final text = context.text;
  final selection = context.selection;
  final lineRange = selectedLineRange(text, selection);
  final segment = text.substring(lineRange.start, lineRange.end);
  final lines = segment.split('\n');
  if (lines.isEmpty) {
    return (text: text, selection: selection);
  }

  final headingPrefix = level == null ? '' : '${'#' * level} ';
  final transformedLines = lines.map((line) {
    final stripped = line.replaceFirst(_headingPrefixPattern, '');
    return '$headingPrefix$stripped';
  }).toList(growable: false);
  final transformedSegment = transformedLines.join('\n');
  final updated = text.replaceRange(
    lineRange.start,
    lineRange.end,
    transformedSegment,
  );

  if (selection.isCollapsed) {
    final oldLineStart = lineStart(text, selection.extentOffset);
    final oldLineEnd = lineEnd(text, selection.extentOffset);
    final oldLine = text.substring(oldLineStart, oldLineEnd);
    final oldPrefixMatch = _headingPrefixPattern.firstMatch(oldLine);
    final oldPrefixLength = oldPrefixMatch?.group(0)?.length ?? 0;
    final oldContentColumn =
        (selection.extentOffset - oldLineStart - oldPrefixLength).clamp(
      0,
      oldLine.length,
    );
    final newLine =
        '$headingPrefix${oldLine.replaceFirst(_headingPrefixPattern, '')}';
    final newColumn = (headingPrefix.length + oldContentColumn).clamp(
      0,
      newLine.length,
    );
    final offset = (oldLineStart + newColumn).clamp(0, updated.length);
    return (text: updated, selection: TextSelection.collapsed(offset: offset));
  }

  final end = (lineRange.start + transformedSegment.length).clamp(
    0,
    updated.length,
  );
  return (
    text: updated,
    selection: TextSelection(baseOffset: lineRange.start, extentOffset: end),
  );
}

abstract final class SovereignBlockCommands {
  static SovereignCommandResult setHeadingLevel(
    SovereignController controller,
    int? level,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }
    final safeLevel = level?.clamp(1, 6);
    final mutation = _setHeadingLevel(context, safeLevel);
    return commitCommandMutation(context, (
      text: mutation.text,
      selection: mutation.selection,
      composing: TextRange.empty,
    ));
  }

  static SovereignCommandResult toggleQuote(SovereignController controller) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }
    final mutation = _toggleLinePrefix(context, '> ');
    return commitCommandMutation(context, (
      text: mutation.text,
      selection: mutation.selection,
      composing: TextRange.empty,
    ));
  }

  static SovereignCommandResult toggleBulletList(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }
    final mutation = _toggleListLinePrefix(context, '- ');
    return commitCommandMutation(context, (
      text: mutation.text,
      selection: mutation.selection,
      composing: TextRange.empty,
    ));
  }

  static SovereignCommandResult toggleTaskList(SovereignController controller) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    // Preserve existing editor-native task checkbox semantics when inside a
    // list context (toggle [ ] <-> [x] or insert on list marker).
    if (controller.toggleTaskCheckboxAtSelection()) {
      return SovereignCommandApplied(controller.selection);
    }

    final fallbackMutation = _toggleListLinePrefix(context, '- [ ] ');
    return commitCommandMutation(context, (
      text: fallbackMutation.text,
      selection: fallbackMutation.selection,
      composing: TextRange.empty,
    ));
  }

  static SovereignCommandResult insertHorizontalRule(
    SovereignController controller,
  ) {
    final context = SovereignCommandContext.fromController(controller);
    if (context.isComposing) {
      return SovereignCommandRejected.code(
        SovereignCommandReasonCode.imeComposing,
      );
    }

    final text = context.text;
    final selection = context.selection;
    final updated = text.replaceRange(
      selection.start,
      selection.end,
      '\n---\n',
    );
    final newSelection = TextSelection.collapsed(offset: selection.start + 5);

    return commitCommandMutation(context, (
      text: updated,
      selection: newSelection,
      composing: TextRange.empty,
    ));
  }

  static bool isQuoteAtCursor(SovereignController controller) {
    final prefix = _linePrefixAtCursor(controller);
    return prefix?.startsWith('> ') ?? false;
  }

  static int? headingLevelAtCursor(SovereignController controller) {
    final prefix = _linePrefixAtCursor(controller);
    if (prefix == null || prefix.isEmpty) return null;
    final match = _headingPrefixPattern.firstMatch(prefix);
    if (match == null) return null;
    return match.group(1)?.length;
  }

  static String? _linePrefixAtCursor(SovereignController controller) {
    final value = controller.value;
    final text = value.text;
    if (text.isEmpty) return null;

    final safeSelection = safeSelectionForText(value.selection, text.length);
    final start = lineStart(text, safeSelection.extentOffset);
    final end = lineEnd(text, safeSelection.extentOffset);
    final line = text.substring(start, end);
    final headingMatch = _headingPrefixPattern.firstMatch(line);
    if (headingMatch != null) {
      return headingMatch.group(0);
    }
    if (line.startsWith('> ')) {
      return '> ';
    }
    return null;
  }
}
