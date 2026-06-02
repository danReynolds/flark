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
import '../source/sovereign_markdown_line_selection.dart';

abstract final class SovereignMarkdownBlockCommands {
  static const setHeadingLevel =
      SovereignCommand<SovereignSetHeadingLevelPayload>(
        'markdown.setHeadingLevel',
      );

  static const toggleQuote = SovereignCommand<SovereignToggleQuotePayload>(
    'markdown.toggleQuote',
  );

  static const toggleBulletList =
      SovereignCommand<SovereignToggleBulletListPayload>(
        'markdown.toggleBulletList',
      );

  static const toggleOrderedList =
      SovereignCommand<SovereignToggleOrderedListPayload>(
        'markdown.toggleOrderedList',
      );

  static const toggleTaskList =
      SovereignCommand<SovereignToggleTaskListPayload>(
        'markdown.toggleTaskList',
      );

  static const setTaskListChecked =
      SovereignCommand<SovereignSetTaskListCheckedPayload>(
        'markdown.setTaskListChecked',
      );

  static const insertThematicBreak =
      SovereignCommand<SovereignInsertThematicBreakPayload>(
        'markdown.insertThematicBreak',
      );

  static const insertFence = SovereignCommand<SovereignInsertFencePayload>(
    'markdown.insertFence',
  );

  static const setFenceLanguage =
      SovereignCommand<SovereignSetFenceLanguagePayload>(
        'markdown.setFenceLanguage',
      );
}

final class SovereignSetHeadingLevelPayload {
  const SovereignSetHeadingLevelPayload(
    this.level, {
    this.userEvent = 'command.setHeadingLevel',
  });

  final int level;
  final String userEvent;
}

final class SovereignToggleQuotePayload {
  const SovereignToggleQuotePayload({this.userEvent = 'command.toggleQuote'});

  final String userEvent;
}

final class SovereignToggleBulletListPayload {
  const SovereignToggleBulletListPayload({
    this.userEvent = 'command.toggleBulletList',
  });

  final String userEvent;
}

final class SovereignToggleOrderedListPayload {
  const SovereignToggleOrderedListPayload({
    this.startNumber = 1,
    this.userEvent = 'command.toggleOrderedList',
  }) : assert(startNumber > 0);

  final int startNumber;
  final String userEvent;
}

final class SovereignToggleTaskListPayload {
  const SovereignToggleTaskListPayload({
    this.userEvent = 'command.toggleTaskList',
  });

  final String userEvent;
}

final class SovereignSetTaskListCheckedPayload {
  const SovereignSetTaskListCheckedPayload({
    required this.taskItemRange,
    required this.checked,
    this.userEvent = 'command.setTaskListChecked',
  });

  final SovereignSourceRange taskItemRange;
  final bool checked;
  final String userEvent;
}

final class SovereignInsertThematicBreakPayload {
  const SovereignInsertThematicBreakPayload({
    this.userEvent = 'command.insertThematicBreak',
  });

  final String userEvent;
}

final class SovereignInsertFencePayload {
  const SovereignInsertFencePayload({
    this.language,
    this.userEvent = 'command.insertFence',
  });

  final String? language;
  final String userEvent;
}

final class SovereignSetFenceLanguagePayload {
  const SovereignSetFenceLanguagePayload({
    required this.codeBlockRange,
    required this.language,
    this.userEvent = 'command.setFenceLanguage',
  });

  final SovereignSourceRange codeBlockRange;
  final String? language;
  final String userEvent;
}

final class SovereignMarkdownBlockEditingExtension extends SovereignExtension {
  const SovereignMarkdownBlockEditingExtension();

  @override
  String get id => 'markdown.blockEditing';

  @override
  SovereignCommandRegistry registerCommands(SovereignCommandRegistry registry) {
    return registry
        .register<SovereignSetHeadingLevelPayload>(
          SovereignMarkdownBlockCommands.setHeadingLevel,
          _setHeadingLevel,
        )
        .register<SovereignToggleQuotePayload>(
          SovereignMarkdownBlockCommands.toggleQuote,
          _toggleQuote,
        )
        .register<SovereignToggleBulletListPayload>(
          SovereignMarkdownBlockCommands.toggleBulletList,
          _toggleBulletList,
        )
        .register<SovereignToggleOrderedListPayload>(
          SovereignMarkdownBlockCommands.toggleOrderedList,
          _toggleOrderedList,
        )
        .register<SovereignToggleTaskListPayload>(
          SovereignMarkdownBlockCommands.toggleTaskList,
          _toggleTaskList,
        )
        .register<SovereignSetTaskListCheckedPayload>(
          SovereignMarkdownBlockCommands.setTaskListChecked,
          _setTaskListChecked,
        )
        .register<SovereignInsertThematicBreakPayload>(
          SovereignMarkdownBlockCommands.insertThematicBreak,
          _insertThematicBreak,
        )
        .register<SovereignInsertFencePayload>(
          SovereignMarkdownBlockCommands.insertFence,
          _insertFence,
        )
        .register<SovereignSetFenceLanguagePayload>(
          SovereignMarkdownBlockCommands.setFenceLanguage,
          _setFenceLanguage,
        );
  }

  SovereignCommandResult _setHeadingLevel(
    SovereignCommandContext<SovereignSetHeadingLevelPayload> context,
  ) {
    final level = context.payload.level;
    if (level < 0 || level > 6) {
      return SovereignCommandResult.rejected(
        'Heading level must be between 0 and 6.',
      );
    }

    final marker = level == 0 ? '' : '${List.filled(level, '#').join()} ';
    final operations = <SovereignSourceOperation>[];
    final lines = selectedMarkdownLines(context.state);

    for (final line in lines) {
      final headingMarker = _headingMarker(line);
      operations.add(
        SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(
            line.start,
            line.start + headingMarker.length,
          ),
          replacementText: marker,
        ),
      );
    }

    return _handledBlockTransaction(
      context.state.selection,
      operations,
      context.payload.userEvent,
    );
  }

  SovereignCommandResult _setTaskListChecked(
    SovereignCommandContext<SovereignSetTaskListCheckedPayload> context,
  ) {
    final line = _lineAtRangeStart(
      context.state,
      context.payload.taskItemRange,
    );
    if (line == null) {
      return SovereignCommandResult.rejected(
        'Task item range does not start on a valid source line.',
      );
    }
    final taskMarker = _taskMarker(line);
    if (taskMarker == null) {
      return SovereignCommandResult.rejected(
        'Task item range does not contain a task marker.',
      );
    }

    return _handledBlockTransaction(context.state.selection, [
      SovereignSourceOperation.replace(
        replacedRange: SovereignSourceRange(
          line.start + taskMarker.checkStart,
          line.start + taskMarker.checkEnd,
        ),
        replacementText: context.payload.checked ? 'x' : ' ',
      ),
    ], context.payload.userEvent);
  }

  SovereignCommandResult _toggleQuote(
    SovereignCommandContext<SovereignToggleQuotePayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final shouldRemove = lines.every((line) => _quoteMarker(line).isNotEmpty);
    final operations = <SovereignSourceOperation>[];

    for (final line in lines) {
      final marker = _quoteMarker(line);
      if (shouldRemove) {
        operations.add(
          SovereignSourceOperation.delete(
            line.start,
            line.start + marker.length,
          ),
        );
      } else if (marker.isEmpty) {
        operations.add(SovereignSourceOperation.insert(line.start, '> '));
      }
    }

    return _handledBlockTransaction(
      context.state.selection,
      operations,
      context.payload.userEvent,
    );
  }

  SovereignCommandResult _toggleBulletList(
    SovereignCommandContext<SovereignToggleBulletListPayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final shouldRemove = lines.every((line) => _bulletMarker(line) != null);
    final operations = <SovereignSourceOperation>[];

    for (final line in lines) {
      final marker = _bulletMarker(line);
      if (shouldRemove) {
        final marker = _bulletMarker(line);
        if (marker == null) continue;
        operations.add(
          SovereignSourceOperation.delete(
            line.start + marker.start,
            line.start + marker.end,
          ),
        );
      } else if (marker == null) {
        operations.add(
          SovereignSourceOperation.insert(
            line.start + _quotePrefixLength(line.text),
            '- ',
          ),
        );
      }
    }

    return _handledBlockTransaction(
      context.state.selection,
      operations,
      context.payload.userEvent,
    );
  }

  SovereignCommandResult _toggleOrderedList(
    SovereignCommandContext<SovereignToggleOrderedListPayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final shouldRemove = lines.every((line) => _orderedMarker(line) != null);
    final operations = <SovereignSourceOperation>[];

    for (final (index, line) in lines.indexed) {
      final marker = _orderedMarker(line);
      if (shouldRemove) {
        if (marker == null) continue;
        operations.add(
          SovereignSourceOperation.delete(
            line.start + marker.start,
            line.start + marker.end,
          ),
        );
      } else if (marker == null) {
        operations.add(
          SovereignSourceOperation.insert(
            line.start + _quotePrefixLength(line.text),
            '${context.payload.startNumber + index}. ',
          ),
        );
      }
    }

    return _handledBlockTransaction(
      context.state.selection,
      operations,
      context.payload.userEvent,
    );
  }

  SovereignCommandResult _toggleTaskList(
    SovereignCommandContext<SovereignToggleTaskListPayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final operations = <SovereignSourceOperation>[];

    for (final line in lines) {
      final taskMarker = _taskMarker(line);
      if (taskMarker != null) {
        operations.add(
          SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(
              line.start + taskMarker.checkStart,
              line.start + taskMarker.checkEnd,
            ),
            replacementText: taskMarker.isChecked ? ' ' : 'x',
          ),
        );
        continue;
      }

      final bulletMarker = _bulletMarker(line);
      if (bulletMarker != null) {
        operations.add(
          SovereignSourceOperation.insert(
            line.start + bulletMarker.end,
            '[ ] ',
          ),
        );
        continue;
      }

      operations.add(
        SovereignSourceOperation.insert(
          line.start + _quotePrefixLength(line.text),
          '- [ ] ',
        ),
      );
    }

    return _handledBlockTransaction(
      context.state.selection,
      operations,
      context.payload.userEvent,
    );
  }

  SovereignCommandResult _insertThematicBreak(
    SovereignCommandContext<SovereignInsertThematicBreakPayload> context,
  ) {
    final selection = context.state.selection;
    final lineIndex = context.state.document.buffer.lineAtOffset(
      selection.start,
    );
    final lineStart = context.state.document.buffer.lineStart(lineIndex);
    final insertText = selection.start == lineStart ? '---\n' : '\n\n---\n';
    final insertOffset = selection.start == lineStart
        ? lineStart
        : selection.end;

    return _handledBlockTransaction(
      selection,
      [SovereignSourceOperation.insert(insertOffset, insertText)],
      context.payload.userEvent,
      selectionAfter: SovereignSelection.collapsed(
        insertOffset + insertText.length,
      ),
    );
  }

  SovereignCommandResult _insertFence(
    SovereignCommandContext<SovereignInsertFencePayload> context,
  ) {
    final selection = context.state.selection;
    final text = context.state.markdown;
    final info = context.payload.language?.trim() ?? '';
    final opener = info.isEmpty ? '```' : '```$info';
    final before = text.substring(0, selection.start);
    final after = text.substring(selection.end);
    final prefix = _blockInsertionPrefix(before);
    final suffix = _blockInsertionSuffix(after);
    final selectedText = text.substring(selection.start, selection.end);

    if (selection.isCollapsed) {
      final fenceText = '$prefix$opener\n\n```$suffix';
      return _handledBlockTransaction(
        selection,
        [SovereignSourceOperation.insert(selection.start, fenceText)],
        context.payload.userEvent,
        selectionAfter: SovereignSelection.collapsed(
          selection.start + prefix.length + opener.length + 1,
        ),
      );
    }

    final fenceText = '$prefix$opener\n$selectedText\n```$suffix';
    return _handledBlockTransaction(
      selection,
      [
        SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(selection.start, selection.end),
          replacementText: fenceText,
        ),
      ],
      context.payload.userEvent,
      selectionAfter: SovereignSelection(
        baseOffset: selection.start + prefix.length + opener.length + 1,
        extentOffset:
            selection.start +
            prefix.length +
            opener.length +
            1 +
            selectedText.length,
      ),
    );
  }

  SovereignCommandResult _setFenceLanguage(
    SovereignCommandContext<SovereignSetFenceLanguagePayload> context,
  ) {
    final opener = _fenceOpeningLine(
      context.state,
      context.payload.codeBlockRange,
    );
    if (opener == null) {
      return SovereignCommandResult.rejected(
        'Code block range does not start with a fenced code opener.',
      );
    }

    final language = context.payload.language?.trim() ?? '';
    if (language.contains('\n') || language.contains('\r')) {
      return SovereignCommandResult.rejected(
        'Fence language cannot contain line breaks.',
      );
    }
    if (opener.marker.startsWith('`') && language.contains('`')) {
      return SovereignCommandResult.rejected(
        'Backtick fence language cannot contain backticks.',
      );
    }

    final replacement =
        '${opener.indent}${opener.marker}${language.isEmpty ? '' : language}';
    return _handledBlockTransaction(context.state.selection, [
      SovereignSourceOperation.replace(
        replacedRange: SovereignSourceRange(opener.start, opener.end),
        replacementText: replacement,
      ),
    ], context.payload.userEvent);
  }

  SovereignCommandResult _handledBlockTransaction(
    SovereignSelection selection,
    List<SovereignSourceOperation> operations,
    String userEvent, {
    SovereignSelection? selectionAfter,
  }) {
    if (operations.isEmpty) {
      return SovereignCommandResult.handled();
    }

    final invalidationRange = _invalidationRange(operations);
    return SovereignCommandResult.handled(
      transaction: SovereignTransaction(
        operations: operations,
        selectionBefore: selection,
        selectionAfter: selectionAfter,
        metadata: SovereignTransactionMetadata(
          intent: SovereignTransactionIntent.command,
          userEvent: userEvent,
          parseInvalidationRange: invalidationRange,
          projectionInvalidationRange: invalidationRange,
        ),
      ),
    );
  }

  SovereignSourceRange _invalidationRange(
    List<SovereignSourceOperation> operations,
  ) {
    var start = operations.first.replacedRange.start;
    var end = operations.first.replacedRange.end;
    for (final operation in operations.skip(1)) {
      if (operation.replacedRange.start < start) {
        start = operation.replacedRange.start;
      }
      if (operation.replacedRange.end > end) {
        end = operation.replacedRange.end;
      }
    }
    return SovereignSourceRange(start, end);
  }

  String _headingMarker(SovereignSelectedLine line) {
    final match = RegExp(r'^(#{1,6})(?:\s+|$)').firstMatch(line.text);
    return match?.group(0) ?? '';
  }

  String _quoteMarker(SovereignSelectedLine line) {
    final match = RegExp(r'^>\s?').firstMatch(line.text);
    return match?.group(0) ?? '';
  }

  _MarkerRange? _bulletMarker(SovereignSelectedLine line) {
    final prefixLength = _quotePrefixLength(line.text);
    final match = RegExp(
      r'^[-+*]\s+',
    ).firstMatch(line.text.substring(prefixLength));
    if (match == null) return null;
    return _MarkerRange(prefixLength + match.start, prefixLength + match.end);
  }

  _MarkerRange? _orderedMarker(SovereignSelectedLine line) {
    final prefixLength = _quotePrefixLength(line.text);
    final match = RegExp(
      r'^\d{1,9}[.)]\s+',
    ).firstMatch(line.text.substring(prefixLength));
    if (match == null) return null;
    return _MarkerRange(prefixLength + match.start, prefixLength + match.end);
  }
}

final class _FenceOpeningLine {
  const _FenceOpeningLine({
    required this.start,
    required this.end,
    required this.indent,
    required this.marker,
  });

  final int start;
  final int end;
  final String indent;
  final String marker;
}

_FenceOpeningLine? _fenceOpeningLine(
  SovereignEditorState state,
  SovereignSourceRange codeBlockRange,
) {
  if (state.markdown.isEmpty ||
      codeBlockRange.start < 0 ||
      codeBlockRange.start >= state.markdown.length ||
      codeBlockRange.start > codeBlockRange.end) {
    return null;
  }
  final lineIndex = state.document.buffer.lineAtOffset(codeBlockRange.start);
  final lineStart = state.document.buffer.lineStart(lineIndex);
  final lineEnd = state.document.buffer.lineEnd(lineIndex);
  if (lineStart != codeBlockRange.start) return null;
  final line = state.markdown.substring(lineStart, lineEnd);
  final match = RegExp(r'^([ \t]{0,3})(`{3,}|~{3,})(.*)$').firstMatch(line);
  if (match == null) return null;
  return _FenceOpeningLine(
    start: lineStart,
    end: lineEnd,
    indent: match.group(1) ?? '',
    marker: match.group(2) ?? '',
  );
}

SovereignSelectedLine? _lineAtRangeStart(
  SovereignEditorState state,
  SovereignSourceRange range,
) {
  if (state.markdown.isEmpty ||
      range.start < 0 ||
      range.start >= state.markdown.length ||
      range.start > range.end) {
    return null;
  }
  final lineIndex = state.document.buffer.lineAtOffset(range.start);
  final lineStart = state.document.buffer.lineStart(lineIndex);
  final lineEnd = state.document.buffer.lineEnd(lineIndex);
  if (lineStart != range.start) return null;
  return SovereignSelectedLine(
    index: lineIndex,
    start: lineStart,
    end: lineEnd,
    text: state.markdown.substring(lineStart, lineEnd),
  );
}

int _quotePrefixLength(String text) {
  final match = RegExp(r'^(?:>\s?)+').firstMatch(text);
  return match?.group(0)?.length ?? 0;
}

String _blockInsertionPrefix(String before) {
  if (before.isEmpty || before.endsWith('\n')) return '';
  return '\n\n';
}

String _blockInsertionSuffix(String after) {
  if (after.isEmpty || after.startsWith('\n')) return '';
  return '\n\n';
}

_TaskMarkerRange? _taskMarker(SovereignSelectedLine line) {
  final prefixLength = _quotePrefixLength(line.text);
  final match = RegExp(
    r'^[-+*]\s+\[([ xX])\]\s+',
  ).firstMatch(line.text.substring(prefixLength));
  if (match == null) return null;
  final checkStart =
      prefixLength + match.start + match.group(0)!.indexOf('[') + 1;
  final check = match.group(1) ?? ' ';
  return _TaskMarkerRange(
    checkStart: checkStart,
    checkEnd: checkStart + 1,
    isChecked: check.toLowerCase() == 'x',
  );
}

final class _MarkerRange {
  const _MarkerRange(this.start, this.end);

  final int start;
  final int end;
}

final class _TaskMarkerRange {
  const _TaskMarkerRange({
    required this.checkStart,
    required this.checkEnd,
    required this.isChecked,
  });

  final int checkStart;
  final int checkEnd;
  final bool isChecked;
}
