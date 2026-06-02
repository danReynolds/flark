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

abstract final class FlarkMarkdownBlockCommands {
  static const setHeadingLevel = FlarkCommand<FlarkSetHeadingLevelPayload>(
    'markdown.setHeadingLevel',
  );

  static const toggleQuote = FlarkCommand<FlarkToggleQuotePayload>(
    'markdown.toggleQuote',
  );

  static const toggleBulletList = FlarkCommand<FlarkToggleBulletListPayload>(
    'markdown.toggleBulletList',
  );

  static const toggleOrderedList = FlarkCommand<FlarkToggleOrderedListPayload>(
    'markdown.toggleOrderedList',
  );

  static const toggleTaskList = FlarkCommand<FlarkToggleTaskListPayload>(
    'markdown.toggleTaskList',
  );

  static const setTaskListChecked =
      FlarkCommand<FlarkSetTaskListCheckedPayload>(
        'markdown.setTaskListChecked',
      );

  static const insertThematicBreak =
      FlarkCommand<FlarkInsertThematicBreakPayload>(
        'markdown.insertThematicBreak',
      );

  static const insertFence = FlarkCommand<FlarkInsertFencePayload>(
    'markdown.insertFence',
  );

  static const setFenceLanguage = FlarkCommand<FlarkSetFenceLanguagePayload>(
    'markdown.setFenceLanguage',
  );
}

final class FlarkSetHeadingLevelPayload {
  const FlarkSetHeadingLevelPayload(
    this.level, {
    this.userEvent = 'command.setHeadingLevel',
  });

  final int level;
  final String userEvent;
}

final class FlarkToggleQuotePayload {
  const FlarkToggleQuotePayload({this.userEvent = 'command.toggleQuote'});

  final String userEvent;
}

final class FlarkToggleBulletListPayload {
  const FlarkToggleBulletListPayload({
    this.userEvent = 'command.toggleBulletList',
  });

  final String userEvent;
}

final class FlarkToggleOrderedListPayload {
  const FlarkToggleOrderedListPayload({
    this.startNumber = 1,
    this.userEvent = 'command.toggleOrderedList',
  }) : assert(startNumber > 0);

  final int startNumber;
  final String userEvent;
}

final class FlarkToggleTaskListPayload {
  const FlarkToggleTaskListPayload({this.userEvent = 'command.toggleTaskList'});

  final String userEvent;
}

final class FlarkSetTaskListCheckedPayload {
  const FlarkSetTaskListCheckedPayload({
    required this.taskItemRange,
    required this.checked,
    this.userEvent = 'command.setTaskListChecked',
  });

  final FlarkSourceRange taskItemRange;
  final bool checked;
  final String userEvent;
}

final class FlarkInsertThematicBreakPayload {
  const FlarkInsertThematicBreakPayload({
    this.userEvent = 'command.insertThematicBreak',
  });

  final String userEvent;
}

final class FlarkInsertFencePayload {
  const FlarkInsertFencePayload({
    this.language,
    this.userEvent = 'command.insertFence',
  });

  final String? language;
  final String userEvent;
}

final class FlarkSetFenceLanguagePayload {
  const FlarkSetFenceLanguagePayload({
    required this.codeBlockRange,
    required this.language,
    this.userEvent = 'command.setFenceLanguage',
  });

  final FlarkSourceRange codeBlockRange;
  final String? language;
  final String userEvent;
}

final class FlarkMarkdownBlockEditingExtension extends FlarkExtension {
  const FlarkMarkdownBlockEditingExtension();

  @override
  String get id => 'markdown.blockEditing';

  @override
  FlarkCommandRegistry registerCommands(FlarkCommandRegistry registry) {
    return registry
        .register<FlarkSetHeadingLevelPayload>(
          FlarkMarkdownBlockCommands.setHeadingLevel,
          _setHeadingLevel,
        )
        .register<FlarkToggleQuotePayload>(
          FlarkMarkdownBlockCommands.toggleQuote,
          _toggleQuote,
        )
        .register<FlarkToggleBulletListPayload>(
          FlarkMarkdownBlockCommands.toggleBulletList,
          _toggleBulletList,
        )
        .register<FlarkToggleOrderedListPayload>(
          FlarkMarkdownBlockCommands.toggleOrderedList,
          _toggleOrderedList,
        )
        .register<FlarkToggleTaskListPayload>(
          FlarkMarkdownBlockCommands.toggleTaskList,
          _toggleTaskList,
        )
        .register<FlarkSetTaskListCheckedPayload>(
          FlarkMarkdownBlockCommands.setTaskListChecked,
          _setTaskListChecked,
        )
        .register<FlarkInsertThematicBreakPayload>(
          FlarkMarkdownBlockCommands.insertThematicBreak,
          _insertThematicBreak,
        )
        .register<FlarkInsertFencePayload>(
          FlarkMarkdownBlockCommands.insertFence,
          _insertFence,
        )
        .register<FlarkSetFenceLanguagePayload>(
          FlarkMarkdownBlockCommands.setFenceLanguage,
          _setFenceLanguage,
        );
  }

  FlarkCommandResult _setHeadingLevel(
    FlarkCommandContext<FlarkSetHeadingLevelPayload> context,
  ) {
    final level = context.payload.level;
    if (level < 0 || level > 6) {
      return FlarkCommandResult.rejected(
        'Heading level must be between 0 and 6.',
      );
    }

    final marker = level == 0 ? '' : '${List.filled(level, '#').join()} ';
    final operations = <FlarkSourceOperation>[];
    final lines = selectedMarkdownLines(context.state);

    for (final line in lines) {
      final headingMarker = _headingMarker(line);
      operations.add(
        FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(
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

  FlarkCommandResult _setTaskListChecked(
    FlarkCommandContext<FlarkSetTaskListCheckedPayload> context,
  ) {
    final line = _lineAtRangeStart(
      context.state,
      context.payload.taskItemRange,
    );
    if (line == null) {
      return FlarkCommandResult.rejected(
        'Task item range does not start on a valid source line.',
      );
    }
    final taskMarker = _taskMarker(line);
    if (taskMarker == null) {
      return FlarkCommandResult.rejected(
        'Task item range does not contain a task marker.',
      );
    }

    return _handledBlockTransaction(context.state.selection, [
      FlarkSourceOperation.replace(
        replacedRange: FlarkSourceRange(
          line.start + taskMarker.checkStart,
          line.start + taskMarker.checkEnd,
        ),
        replacementText: context.payload.checked ? 'x' : ' ',
      ),
    ], context.payload.userEvent);
  }

  FlarkCommandResult _toggleQuote(
    FlarkCommandContext<FlarkToggleQuotePayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final shouldRemove = lines.every((line) => _quoteMarker(line).isNotEmpty);
    final operations = <FlarkSourceOperation>[];

    for (final line in lines) {
      final marker = _quoteMarker(line);
      if (shouldRemove) {
        operations.add(
          FlarkSourceOperation.delete(line.start, line.start + marker.length),
        );
      } else if (marker.isEmpty) {
        operations.add(FlarkSourceOperation.insert(line.start, '> '));
      }
    }

    return _handledBlockTransaction(
      context.state.selection,
      operations,
      context.payload.userEvent,
    );
  }

  FlarkCommandResult _toggleBulletList(
    FlarkCommandContext<FlarkToggleBulletListPayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final shouldRemove = lines.every((line) => _bulletMarker(line) != null);
    final operations = <FlarkSourceOperation>[];

    for (final line in lines) {
      final marker = _bulletMarker(line);
      if (shouldRemove) {
        final marker = _bulletMarker(line);
        if (marker == null) continue;
        operations.add(
          FlarkSourceOperation.delete(
            line.start + marker.start,
            line.start + marker.end,
          ),
        );
      } else if (marker == null) {
        operations.add(
          FlarkSourceOperation.insert(
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

  FlarkCommandResult _toggleOrderedList(
    FlarkCommandContext<FlarkToggleOrderedListPayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final shouldRemove = lines.every((line) => _orderedMarker(line) != null);
    final operations = <FlarkSourceOperation>[];

    for (final (index, line) in lines.indexed) {
      final marker = _orderedMarker(line);
      if (shouldRemove) {
        if (marker == null) continue;
        operations.add(
          FlarkSourceOperation.delete(
            line.start + marker.start,
            line.start + marker.end,
          ),
        );
      } else if (marker == null) {
        operations.add(
          FlarkSourceOperation.insert(
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

  FlarkCommandResult _toggleTaskList(
    FlarkCommandContext<FlarkToggleTaskListPayload> context,
  ) {
    final lines = selectedMarkdownLines(context.state);
    final operations = <FlarkSourceOperation>[];

    for (final line in lines) {
      final taskMarker = _taskMarker(line);
      if (taskMarker != null) {
        operations.add(
          FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(
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
          FlarkSourceOperation.insert(line.start + bulletMarker.end, '[ ] '),
        );
        continue;
      }

      operations.add(
        FlarkSourceOperation.insert(
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

  FlarkCommandResult _insertThematicBreak(
    FlarkCommandContext<FlarkInsertThematicBreakPayload> context,
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
      [FlarkSourceOperation.insert(insertOffset, insertText)],
      context.payload.userEvent,
      selectionAfter: FlarkSelection.collapsed(
        insertOffset + insertText.length,
      ),
    );
  }

  FlarkCommandResult _insertFence(
    FlarkCommandContext<FlarkInsertFencePayload> context,
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
        [FlarkSourceOperation.insert(selection.start, fenceText)],
        context.payload.userEvent,
        selectionAfter: FlarkSelection.collapsed(
          selection.start + prefix.length + opener.length + 1,
        ),
      );
    }

    final fenceText = '$prefix$opener\n$selectedText\n```$suffix';
    return _handledBlockTransaction(
      selection,
      [
        FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(selection.start, selection.end),
          replacementText: fenceText,
        ),
      ],
      context.payload.userEvent,
      selectionAfter: FlarkSelection(
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

  FlarkCommandResult _setFenceLanguage(
    FlarkCommandContext<FlarkSetFenceLanguagePayload> context,
  ) {
    final opener = _fenceOpeningLine(
      context.state,
      context.payload.codeBlockRange,
    );
    if (opener == null) {
      return FlarkCommandResult.rejected(
        'Code block range does not start with a fenced code opener.',
      );
    }

    final language = context.payload.language?.trim() ?? '';
    if (language.contains('\n') || language.contains('\r')) {
      return FlarkCommandResult.rejected(
        'Fence language cannot contain line breaks.',
      );
    }
    if (opener.marker.startsWith('`') && language.contains('`')) {
      return FlarkCommandResult.rejected(
        'Backtick fence language cannot contain backticks.',
      );
    }

    final replacement =
        '${opener.indent}${opener.marker}${language.isEmpty ? '' : language}';
    return _handledBlockTransaction(context.state.selection, [
      FlarkSourceOperation.replace(
        replacedRange: FlarkSourceRange(opener.start, opener.end),
        replacementText: replacement,
      ),
    ], context.payload.userEvent);
  }

  FlarkCommandResult _handledBlockTransaction(
    FlarkSelection selection,
    List<FlarkSourceOperation> operations,
    String userEvent, {
    FlarkSelection? selectionAfter,
  }) {
    if (operations.isEmpty) {
      return FlarkCommandResult.handled();
    }

    final invalidationRange = _invalidationRange(operations);
    return FlarkCommandResult.handled(
      transaction: FlarkTransaction(
        operations: operations,
        selectionBefore: selection,
        selectionAfter: selectionAfter,
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.command,
          userEvent: userEvent,
          parseInvalidationRange: invalidationRange,
          projectionInvalidationRange: invalidationRange,
        ),
      ),
    );
  }

  FlarkSourceRange _invalidationRange(List<FlarkSourceOperation> operations) {
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
    return FlarkSourceRange(start, end);
  }

  String _headingMarker(FlarkSelectedLine line) {
    final match = RegExp(r'^(#{1,6})(?:\s+|$)').firstMatch(line.text);
    return match?.group(0) ?? '';
  }

  String _quoteMarker(FlarkSelectedLine line) {
    final match = RegExp(r'^>\s?').firstMatch(line.text);
    return match?.group(0) ?? '';
  }

  _MarkerRange? _bulletMarker(FlarkSelectedLine line) {
    final prefixLength = _quotePrefixLength(line.text);
    final match = RegExp(
      r'^[-+*]\s+',
    ).firstMatch(line.text.substring(prefixLength));
    if (match == null) return null;
    return _MarkerRange(prefixLength + match.start, prefixLength + match.end);
  }

  _MarkerRange? _orderedMarker(FlarkSelectedLine line) {
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
  FlarkEditorState state,
  FlarkSourceRange codeBlockRange,
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

FlarkSelectedLine? _lineAtRangeStart(
  FlarkEditorState state,
  FlarkSourceRange range,
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
  return FlarkSelectedLine(
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

_TaskMarkerRange? _taskMarker(FlarkSelectedLine line) {
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
