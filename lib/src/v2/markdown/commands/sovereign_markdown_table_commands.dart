import '../../core/command/sovereign_command.dart';
import '../../core/command/sovereign_command_registry.dart';
import '../../core/command/sovereign_command_result.dart';
import '../../core/document/sovereign_text_buffer.dart';
import '../../core/extension/sovereign_extension.dart';
import '../../core/selection/sovereign_selection.dart';
import '../../core/state/sovereign_editor_state.dart';
import '../../core/transaction/sovereign_source_operation.dart';
import '../../core/transaction/sovereign_source_range.dart';
import '../../core/transaction/sovereign_transaction.dart';
import '../../core/transaction/sovereign_transaction_metadata.dart';

abstract final class SovereignMarkdownTableCommands {
  static const insertTable = SovereignCommand<SovereignInsertTablePayload>(
    'markdown.insertTable',
  );

  static const insertRowBelow = SovereignCommand<SovereignTableMutationPayload>(
    'markdown.insertTableRowBelow',
  );

  static const deleteRow = SovereignCommand<SovereignTableMutationPayload>(
    'markdown.deleteTableRow',
  );

  static const insertColumnRight =
      SovereignCommand<SovereignTableMutationPayload>(
    'markdown.insertTableColumnRight',
  );

  static const deleteColumn = SovereignCommand<SovereignTableMutationPayload>(
    'markdown.deleteTableColumn',
  );
}

final class SovereignInsertTablePayload {
  const SovereignInsertTablePayload({
    this.columns = 2,
    this.bodyRows = 1,
    this.userEvent = 'command.insertTable',
  });

  final int columns;
  final int bodyRows;
  final String userEvent;
}

final class SovereignTableMutationPayload {
  const SovereignTableMutationPayload({
    this.userEvent = 'command.mutateTable',
  });

  final String userEvent;
}

final class SovereignMarkdownTableEditingExtension extends SovereignExtension {
  const SovereignMarkdownTableEditingExtension();

  @override
  String get id => 'markdown.tableEditing';

  @override
  SovereignCommandRegistry registerCommands(SovereignCommandRegistry registry) {
    return registry
        .register<SovereignInsertTablePayload>(
          SovereignMarkdownTableCommands.insertTable,
          _insertTable,
        )
        .register<SovereignTableMutationPayload>(
          SovereignMarkdownTableCommands.insertRowBelow,
          _insertRowBelow,
        )
        .register<SovereignTableMutationPayload>(
          SovereignMarkdownTableCommands.deleteRow,
          _deleteRow,
        )
        .register<SovereignTableMutationPayload>(
          SovereignMarkdownTableCommands.insertColumnRight,
          _insertColumnRight,
        )
        .register<SovereignTableMutationPayload>(
          SovereignMarkdownTableCommands.deleteColumn,
          _deleteColumn,
        );
  }

  SovereignCommandResult _insertTable(
    SovereignCommandContext<SovereignInsertTablePayload> context,
  ) {
    final state = context.state;
    final selection = state.selection;
    final text = state.markdown;
    final columns = context.payload.columns < 2 ? 2 : context.payload.columns;
    final bodyRows =
        context.payload.bodyRows < 1 ? 1 : context.payload.bodyRows;
    final before = text.substring(0, selection.start);
    final after = text.substring(selection.end);
    final prefix = _blockPrefix(before);
    final suffix = _blockSuffix(after);
    final template = _tableTemplate(columns: columns, bodyRows: bodyRows);
    final insertion = '$prefix$template\n$suffix';
    final updated =
        text.replaceRange(selection.start, selection.end, insertion);
    final firstBodyOffset = _firstBodyCellOffsetInTemplate(template);
    final caret = (selection.start + prefix.length + firstBodyOffset).clamp(
      0,
      updated.length,
    );
    final formatted = _formatEstablishedTableAroundCaret(updated, caret);
    final outputText = formatted?.text ?? updated;
    final headerLine = _lineAtOffset(
      _lineStarts(outputText),
      (selection.start + prefix.length).clamp(0, outputText.length),
    );
    final outputCaret = _preferredCaretForCell(
          outputText,
          targetLine: headerLine + 2,
          targetCell: 0,
        ) ??
        (formatted?.caret ?? caret).clamp(0, outputText.length);

    return _handledTextReplacement(
      state: state,
      updatedText: outputText,
      selectionAfter: SovereignSelection.collapsed(outputCaret),
      userEvent: context.payload.userEvent,
    );
  }

  SovereignCommandResult _insertRowBelow(
    SovereignCommandContext<SovereignTableMutationPayload> context,
  ) {
    final table = _establishedTableAtSelection(context.state);
    if (table == null) return const SovereignCommandResult.notHandled();

    final anchorIndex = table.currentRowIndex <= table.separatorRowIndex
        ? table.separatorRowIndex
        : table.currentRowIndex;
    final anchor = table.rows[anchorIndex];
    final prefix = anchor.hasLineBreak ? '' : '\n';
    final suffix = anchor.hasLineBreak ? '\n' : '';
    final insertion = '$prefix${_emptyRowTemplate(
      table.columnCount,
      indent: anchor.indent,
    )}$suffix';
    final updated = context.state.markdown.replaceRange(
      anchor.lineEndWithBreak,
      anchor.lineEndWithBreak,
      insertion,
    );
    final targetLine = anchor.line + 1;
    final targetCell = 0;
    final caret = _preferredCaretForCell(
          updated,
          targetLine: targetLine,
          targetCell: targetCell,
        ) ??
        anchor.lineEndWithBreak + prefix.length + anchor.indent.length + 2;

    return _formatAndCommit(
      state: context.state,
      text: updated,
      caret: caret,
      targetLine: targetLine,
      targetCell: targetCell,
      userEvent: context.payload.userEvent,
    );
  }

  SovereignCommandResult _deleteRow(
    SovereignCommandContext<SovereignTableMutationPayload> context,
  ) {
    final table = _establishedTableAtSelection(context.state);
    if (table == null || table.currentRowIndex <= table.separatorRowIndex) {
      return const SovereignCommandResult.notHandled();
    }

    final row = table.currentRow;
    final updated = context.state.markdown.replaceRange(
      row.lineStart,
      row.lineEndWithBreak,
      '',
    );
    final targetRow = _targetRowAfterDeletingCurrentRow(table);
    final targetLine =
        targetRow.line > row.line ? targetRow.line - 1 : targetRow.line;
    final targetCell = table.currentColumnIndex.clamp(0, table.columnCount - 1);
    final caret = _preferredCaretForCell(
          updated,
          targetLine: targetLine,
          targetCell: targetCell,
        ) ??
        row.lineStart.clamp(0, updated.length);

    return _formatAndCommit(
      state: context.state,
      text: updated,
      caret: caret,
      targetLine: targetLine,
      targetCell: targetCell,
      userEvent: context.payload.userEvent,
    );
  }

  SovereignCommandResult _insertColumnRight(
    SovereignCommandContext<SovereignTableMutationPayload> context,
  ) {
    final table = _establishedTableAtSelection(context.state);
    if (table == null) return const SovereignCommandResult.notHandled();

    final insertIndex = table.currentColumnIndex + 1;
    final rowTexts = <String>[];
    for (final row in table.rows) {
      final cells = _cellTexts(context.state.markdown, row);
      if (cells == null) return const SovereignCommandResult.notHandled();
      cells.insert(insertIndex, row.isSeparator ? '---' : '');
      rowTexts.add(_formatSourceRow(cells, indent: row.indent));
    }

    final updated =
        _replaceTableRows(context.state.markdown, table.rows, rowTexts);
    final caret = _preferredCaretForCell(
          updated,
          targetLine: table.currentRow.line,
          targetCell: insertIndex,
        ) ??
        table.currentRow.lineStart.clamp(0, updated.length);

    return _formatAndCommit(
      state: context.state,
      text: updated,
      caret: caret,
      targetLine: table.currentRow.line,
      targetCell: insertIndex,
      userEvent: context.payload.userEvent,
    );
  }

  SovereignCommandResult _deleteColumn(
    SovereignCommandContext<SovereignTableMutationPayload> context,
  ) {
    final table = _establishedTableAtSelection(context.state);
    if (table == null || table.columnCount <= 2) {
      return const SovereignCommandResult.notHandled();
    }

    final deleteIndex = table.currentColumnIndex;
    final rowTexts = <String>[];
    for (final row in table.rows) {
      final cells = _cellTexts(context.state.markdown, row);
      if (cells == null || deleteIndex >= cells.length) {
        return const SovereignCommandResult.notHandled();
      }
      cells.removeAt(deleteIndex);
      rowTexts.add(_formatSourceRow(cells, indent: row.indent));
    }

    final targetCell = deleteIndex.clamp(0, table.columnCount - 2);
    final updated =
        _replaceTableRows(context.state.markdown, table.rows, rowTexts);
    final caret = _preferredCaretForCell(
          updated,
          targetLine: table.currentRow.line,
          targetCell: targetCell,
        ) ??
        table.currentRow.lineStart.clamp(0, updated.length);

    return _formatAndCommit(
      state: context.state,
      text: updated,
      caret: caret,
      targetLine: table.currentRow.line,
      targetCell: targetCell,
      userEvent: context.payload.userEvent,
    );
  }
}

SovereignCommandResult _formatAndCommit({
  required SovereignEditorState state,
  required String text,
  required int caret,
  required int targetLine,
  required int targetCell,
  required String userEvent,
}) {
  final formatted = _formatEstablishedTableAroundCaret(text, caret);
  final outputText = formatted?.text ?? text;
  final outputCaret = _preferredCaretForCell(
        outputText,
        targetLine: targetLine,
        targetCell: targetCell,
      ) ??
      (formatted?.caret ?? caret).clamp(0, outputText.length);
  return _handledTextReplacement(
    state: state,
    updatedText: outputText,
    selectionAfter: SovereignSelection.collapsed(outputCaret),
    userEvent: userEvent,
  );
}

SovereignCommandResult _handledTextReplacement({
  required SovereignEditorState state,
  required String updatedText,
  required SovereignSelection selectionAfter,
  required String userEvent,
}) {
  final original = state.markdown;
  if (updatedText == original) return SovereignCommandResult.handled();

  final edit = _minimalReplacement(original, updatedText);
  final invalidationRange = SovereignSourceRange(edit.start, edit.end);
  return SovereignCommandResult.handled(
    transaction: SovereignTransaction.single(
      SovereignSourceOperation.replace(
        replacedRange: invalidationRange,
        replacementText: edit.replacement,
      ),
      selectionBefore: state.selection,
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

_Replacement _minimalReplacement(String original, String updated) {
  var prefix = 0;
  while (prefix < original.length &&
      prefix < updated.length &&
      original.codeUnitAt(prefix) == updated.codeUnitAt(prefix)) {
    prefix += 1;
  }

  var suffix = 0;
  while (suffix < original.length - prefix &&
      suffix < updated.length - prefix &&
      original.codeUnitAt(original.length - suffix - 1) ==
          updated.codeUnitAt(updated.length - suffix - 1)) {
    suffix += 1;
  }

  final end = original.length - suffix;
  return _Replacement(
    start: prefix,
    end: end,
    replacement: updated.substring(prefix, updated.length - suffix),
  );
}

String _blockPrefix(String before) {
  if (before.isEmpty) return '';
  if (before.endsWith('\n\n')) return '';
  if (before.endsWith('\n')) return '\n';
  return '\n\n';
}

String _blockSuffix(String after) {
  if (after.isEmpty) return '';
  if (after.startsWith('\n')) return '';
  return '\n';
}

String _tableTemplate({required int columns, required int bodyRows}) {
  final headers = List<String>.generate(columns, (i) => 'Header ${i + 1}');
  final rows = <String>[
    _formatSourceRow(headers, indent: ''),
    _formatSourceRow(List<String>.filled(columns, '---'), indent: ''),
    for (var i = 0; i < bodyRows; i++)
      _formatSourceRow(List<String>.filled(columns, ''), indent: ''),
  ];
  return rows.join('\n');
}

int _firstBodyCellOffsetInTemplate(String template) {
  final firstBreak = template.indexOf('\n');
  if (firstBreak == -1) return 0;
  final secondBreak = template.indexOf('\n', firstBreak + 1);
  if (secondBreak == -1) return template.length;
  return (secondBreak + 3).clamp(0, template.length);
}

String _emptyRowTemplate(int columns, {required String indent}) {
  final safeColumns = columns < 1 ? 1 : columns;
  return '$indent| ${List<String>.filled(safeColumns, '').join(' | ')} |';
}

_EstablishedTable? _establishedTableAtSelection(SovereignEditorState state) {
  final selection = state.selection;
  if (!selection.isCollapsed) return null;
  final text = state.markdown;
  if (text.isEmpty) return null;

  final caret = selection.extentOffset.clamp(0, text.length);
  if (_isOffsetInsideFencedCode(text, caret)) return null;

  final buffer = state.document.buffer;
  final line = buffer.lineAtOffset(caret);
  final lineLookup = _DocumentLineLookup(buffer);
  final current = _parseTableLineAt(text, lineLookup, line);
  if (current == null) return null;
  final currentColumn = _tableCellIndexForCaret(current, caret);
  if (currentColumn == null) return null;

  var firstLine = line;
  while (firstLine > 0) {
    final previous = _parseTableLineAt(text, lineLookup, firstLine - 1);
    if (previous == null || previous.columnCount != current.columnCount) break;
    firstLine -= 1;
  }

  var endLineExclusive = line + 1;
  while (endLineExclusive < buffer.lineCount) {
    final next = _parseTableLineAt(text, lineLookup, endLineExclusive);
    if (next == null || next.columnCount != current.columnCount) break;
    endLineExclusive += 1;
  }

  final rows = <_ParsedTableLine>[];
  var separatorRowIndex = -1;
  var currentRowIndex = -1;
  for (var scan = firstLine; scan < endLineExclusive; scan += 1) {
    final row = _parseTableLineAt(text, lineLookup, scan);
    if (row == null || row.columnCount != current.columnCount) return null;
    if (row.isSeparator && separatorRowIndex == -1) {
      separatorRowIndex = rows.length;
    }
    if (scan == line) currentRowIndex = rows.length;
    rows.add(row);
  }

  if (separatorRowIndex == -1 || currentRowIndex == -1) return null;
  return _EstablishedTable(
    rows: rows,
    currentRowIndex: currentRowIndex,
    currentColumnIndex: currentColumn,
    separatorRowIndex: separatorRowIndex,
  );
}

_ParsedTableLine? _parseTableLineAt(
  String text,
  _LineLookup buffer,
  int line,
) {
  if (line < 0 || line >= buffer.lineCount) return null;
  final lineStart = buffer.lineStart(line);
  if (_isLineInsideFencedCode(text, lineStart)) return null;
  final lineEnd = buffer.lineEnd(line);
  final lineEndWithBreak =
      line + 1 < buffer.lineCount ? buffer.lineStart(line + 1) : text.length;
  final lineText = text.substring(lineStart, lineEnd);
  if (lineText.trim().isEmpty) return null;
  final indent = _leadingWhitespacePrefix(lineText);
  final body = lineText.substring(indent.length);
  if (body.isEmpty || body.startsWith('>')) return null;
  final cellTexts = _splitCellTexts(body);
  if (cellTexts == null || cellTexts.length < 2) return null;
  final isSeparator = cellTexts.every(_isSeparatorCell);
  final cells = _parseTableCellsFromBody(
    body,
    baseOffset: lineStart + indent.length,
  );
  if (cells == null || cells.length != cellTexts.length) return null;

  return _ParsedTableLine(
    line: line,
    lineStart: lineStart,
    lineEnd: lineEnd,
    lineEndWithBreak: lineEndWithBreak,
    indent: indent,
    columnCount: cells.length,
    isSeparator: isSeparator,
    cells: cells,
  );
}

int? _tableCellIndexForCaret(_ParsedTableLine row, int caret) {
  for (var i = 0; i < row.cells.length; i += 1) {
    final cell = row.cells[i];
    if (caret >= cell.rawStart && caret <= cell.rawEnd) return i;
  }
  if (caret < row.cells.first.rawStart) return 0;
  if (caret > row.cells.last.rawEnd) return row.cells.length - 1;
  return null;
}

List<_TableCell>? _parseTableCellsFromBody(
  String body, {
  required int baseOffset,
}) {
  final rawSegments = <_RawSegment>[];
  var sawPipe = false;
  var start = 0;
  var cursor = 0;
  while (cursor < body.length) {
    final codeUnit = body.codeUnitAt(cursor);
    if (codeUnit == 92) {
      cursor += 2;
      continue;
    }
    if (codeUnit == 124) {
      sawPipe = true;
      rawSegments.add(_RawSegment(start, cursor));
      start = cursor + 1;
    }
    cursor += 1;
  }
  if (!sawPipe) return null;
  rawSegments.add(_RawSegment(start, body.length));

  final hasLeadingPipe = body.trimLeft().startsWith('|');
  final hasTrailingPipe = body.trimRight().endsWith('|');
  var segments = List<_RawSegment>.from(rawSegments);
  if (hasLeadingPipe &&
      segments.isNotEmpty &&
      body.substring(segments.first.start, segments.first.end).trim().isEmpty) {
    segments = segments.sublist(1);
  }
  if (hasTrailingPipe &&
      segments.isNotEmpty &&
      body.substring(segments.last.start, segments.last.end).trim().isEmpty) {
    segments = segments.sublist(0, segments.length - 1);
  }
  if (segments.isEmpty) return null;

  return [
    for (final segment in segments)
      _TableCell(
        rawStart: baseOffset + segment.start,
        rawEnd: baseOffset + segment.end,
        preferredCaret: baseOffset + _preferredRelativeCaret(body, segment),
      ),
  ];
}

int _preferredRelativeCaret(String body, _RawSegment segment) {
  var preferred = segment.start;
  while (preferred < segment.end) {
    final codeUnit = body.codeUnitAt(preferred);
    if (codeUnit != 32 && codeUnit != 9) break;
    preferred += 1;
  }
  if (preferred >= segment.end && segment.start < segment.end) {
    preferred = segment.start;
  }
  if (preferred < segment.end && body.codeUnitAt(preferred) == 32) {
    preferred += 1;
  }
  return preferred.clamp(segment.start, segment.end);
}

List<String>? _cellTexts(String text, _ParsedTableLine row) {
  final cells = <String>[];
  for (final cell in row.cells) {
    if (cell.rawStart < 0 || cell.rawEnd > text.length) return null;
    cells.add(text.substring(cell.rawStart, cell.rawEnd).trim());
  }
  return cells;
}

String _replaceTableRows(
  String text,
  List<_ParsedTableLine> rows,
  List<String> rowTexts,
) {
  final regionStart = rows.first.lineStart;
  final regionEnd = rows.last.lineEndWithBreak;
  final preserveTrailingNewline = rows.last.hasLineBreak;
  var replacement = rowTexts.join('\n');
  if (preserveTrailingNewline) replacement = '$replacement\n';
  return text.replaceRange(regionStart, regionEnd, replacement);
}

String _formatSourceRow(List<String> cells, {required String indent}) {
  return '$indent| ${cells.join(' | ')} |';
}

_ParsedTableLine _targetRowAfterDeletingCurrentRow(_EstablishedTable table) {
  for (var scan = table.currentRowIndex + 1;
      scan < table.rows.length;
      scan += 1) {
    final row = table.rows[scan];
    if (!row.isSeparator) return row;
  }
  for (var scan = table.currentRowIndex - 1; scan >= 0; scan -= 1) {
    final row = table.rows[scan];
    if (!row.isSeparator) return row;
  }
  return table.rows.first;
}

_TableFormatResult? _formatEstablishedTableAroundCaret(String text, int caret) {
  if (text.isEmpty) return null;
  final lineStarts = _lineStarts(text);
  final targetLine = _lineAtOffset(lineStarts, caret.clamp(0, text.length));
  final target = _lineBounds(text, lineStarts, targetLine);
  if (target == null) return null;
  final targetShape = _matchRowShape(text, target.start, target.end);
  if (targetShape == null) return null;

  var startLine = targetLine;
  var endLineExclusive = targetLine + 1;
  while (startLine > 0) {
    final previous = _lineBounds(text, lineStarts, startLine - 1);
    if (previous == null) break;
    final shape = _matchRowShape(text, previous.start, previous.end);
    if (shape == null || shape.columnCount != targetShape.columnCount) break;
    startLine -= 1;
  }
  while (true) {
    final next = _lineBounds(text, lineStarts, endLineExclusive);
    if (next == null) break;
    final shape = _matchRowShape(text, next.start, next.end);
    if (shape == null || shape.columnCount != targetShape.columnCount) break;
    endLineExclusive += 1;
  }

  final rows = <_FormatRow>[];
  var hasSeparator = false;
  final widths = List<int>.filled(targetShape.columnCount, 3);

  for (var line = startLine; line < endLineExclusive; line += 1) {
    final bounds = _lineBounds(text, lineStarts, line);
    if (bounds == null) return null;
    final shape = _matchRowShape(text, bounds.start, bounds.end);
    if (shape == null || shape.columnCount != targetShape.columnCount) {
      return null;
    }
    final body = text.substring(bounds.start + shape.indent.length, bounds.end);
    final cells = _splitCellTexts(body);
    if (cells == null || cells.length != targetShape.columnCount) return null;
    if (shape.isSeparator) {
      hasSeparator = true;
    } else {
      for (var i = 0; i < cells.length; i += 1) {
        final width = cells[i].trim().length;
        if (width > widths[i]) widths[i] = width;
      }
    }
    rows.add(_FormatRow(bounds, shape, cells));
  }
  if (!hasSeparator) return null;

  final formattedLines = <String>[];
  for (final row in rows) {
    if (row.shape.isSeparator) {
      formattedLines.add(
        '${row.shape.indent}| ${[
          for (var i = 0; i < row.cells.length; i += 1)
            _formatSeparatorCell(widths[i], row.cells[i])
        ].join(' | ')} |',
      );
    } else {
      formattedLines.add(
        '${row.shape.indent}| ${[
          for (var i = 0; i < row.cells.length; i += 1)
            _formatBodyCell(widths[i], row.cells[i])
        ].join(' | ')} |',
      );
    }
  }

  final regionStart = rows.first.bounds.start;
  final regionEnd = rows.last.bounds.endWithBreak;
  final preserveTrailingNewline = regionEnd > rows.last.bounds.end &&
      regionEnd <= text.length &&
      text.codeUnitAt(regionEnd - 1) == 10;
  var regionText = formattedLines.join('\n');
  if (preserveTrailingNewline && !regionText.endsWith('\n')) {
    regionText = '$regionText\n';
  }
  final updated = text.replaceRange(regionStart, regionEnd, regionText);
  final mappedCaret = _mapCaretThroughReplacement(
    caret,
    regionStart,
    regionEnd,
    regionText.length,
  );
  return _TableFormatResult(text: updated, caret: mappedCaret);
}

int? _preferredCaretForCell(
  String text, {
  required int targetLine,
  required int targetCell,
}) {
  if (text.isEmpty) return null;
  final buffer = _SimpleLineBuffer(text);
  final line = targetLine.clamp(0, buffer.lineCount - 1);
  final row = _parseTableLineAt(text, buffer, line);
  if (row == null || row.cells.isEmpty) return null;
  final cell = targetCell.clamp(0, row.cells.length - 1);
  return row.cells[cell].preferredCaret.clamp(0, text.length);
}

_TableLineShape? _matchRowShape(String text, int lineStart, int lineEnd) {
  if (lineEnd <= lineStart || lineStart < 0 || lineEnd > text.length) {
    return null;
  }
  final line = text.substring(lineStart, lineEnd);
  if (line.trim().isEmpty) return null;
  final indent = _leadingWhitespacePrefix(line);
  final body = line.substring(indent.length);
  if (body.isEmpty || body.startsWith('>')) return null;
  final cells = _splitCellTexts(body);
  if (cells == null || cells.length < 2) return null;
  return _TableLineShape(
    columnCount: cells.length,
    isSeparator: cells.every(_isSeparatorCell),
    indent: indent,
  );
}

List<String>? _splitCellTexts(String body) {
  final cells = <String>[];
  var sawPipe = false;
  var start = 0;
  var cursor = 0;
  while (cursor < body.length) {
    final codeUnit = body.codeUnitAt(cursor);
    if (codeUnit == 92) {
      cursor += 2;
      continue;
    }
    if (codeUnit == 124) {
      sawPipe = true;
      cells.add(body.substring(start, cursor));
      start = cursor + 1;
    }
    cursor += 1;
  }
  if (!sawPipe) return null;
  cells.add(body.substring(start));

  final hasLeadingPipe = body.trimLeft().startsWith('|');
  final hasTrailingPipe = body.trimRight().endsWith('|');
  var normalized = List<String>.from(cells);
  if (hasLeadingPipe &&
      normalized.isNotEmpty &&
      normalized.first.trim().isEmpty) {
    normalized = normalized.sublist(1);
  }
  if (hasTrailingPipe &&
      normalized.isNotEmpty &&
      normalized.last.trim().isEmpty) {
    normalized = normalized.sublist(0, normalized.length - 1);
  }
  return normalized.isEmpty ? null : normalized;
}

bool _isSeparatorCell(String rawCell) {
  var cell = rawCell.trim();
  if (cell.isEmpty) return false;
  if (cell.startsWith(':')) cell = cell.substring(1);
  if (cell.endsWith(':')) cell = cell.substring(0, cell.length - 1);
  if (cell.isEmpty) return false;
  for (var i = 0; i < cell.length; i += 1) {
    if (cell.codeUnitAt(i) != 45) return false;
  }
  return true;
}

String _formatSeparatorCell(int width, String rawCell) {
  final trimmed = rawCell.trim();
  final left = trimmed.startsWith(':');
  final right = trimmed.endsWith(':');
  final dashCount = width < 3 ? 3 : width;
  final core = '-' * dashCount;
  if (left && right) return ':$core:';
  if (left) return ':$core';
  if (right) return '$core:';
  return core;
}

String _formatBodyCell(int width, String rawCell) {
  final trimmed = rawCell.trim();
  final pad = width - trimmed.length;
  return pad > 0 ? '$trimmed${' ' * pad}' : trimmed;
}

int _mapCaretThroughReplacement(
  int caret,
  int regionStart,
  int regionEnd,
  int replacementLength,
) {
  if (caret <= regionStart) return caret;
  if (caret >= regionEnd) {
    return caret + replacementLength - (regionEnd - regionStart);
  }
  final relative = caret - regionStart;
  return regionStart + relative.clamp(0, replacementLength);
}

List<int> _lineStarts(String text) {
  final starts = <int>[0];
  for (var i = 0; i < text.length; i += 1) {
    if (text.codeUnitAt(i) == 10) starts.add(i + 1);
  }
  return starts;
}

int _lineAtOffset(List<int> starts, int offset) {
  var low = 0;
  var high = starts.length - 1;
  while (low <= high) {
    final middle = low + ((high - low) >> 1);
    final start = starts[middle];
    if (start == offset) return middle;
    if (start < offset) {
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  return high < 0 ? 0 : high;
}

_LineBounds? _lineBounds(String text, List<int> starts, int line) {
  if (line < 0 || line >= starts.length) return null;
  final start = starts[line];
  final endWithBreak =
      line + 1 < starts.length ? starts[line + 1] : text.length;
  var end = endWithBreak;
  if (end > start && text.codeUnitAt(end - 1) == 10) {
    end -= 1;
  }
  return _LineBounds(start: start, end: end, endWithBreak: endWithBreak);
}

String _leadingWhitespacePrefix(String text) {
  var cursor = 0;
  while (cursor < text.length) {
    final codeUnit = text.codeUnitAt(cursor);
    if (codeUnit != 32 && codeUnit != 9) break;
    cursor += 1;
  }
  return text.substring(0, cursor);
}

bool _isOffsetInsideFencedCode(String text, int offset) {
  final starts = _lineStarts(text);
  final line = _lineAtOffset(starts, offset.clamp(0, text.length));
  final lineStart = starts[line];
  return _isLineInsideFencedCode(text, lineStart);
}

bool _isLineInsideFencedCode(String text, int lineStart) {
  final starts = _lineStarts(text);
  var openMarker = '';
  var openLength = 0;
  for (var line = 0; line < starts.length; line += 1) {
    final bounds = _lineBounds(text, starts, line);
    if (bounds == null || bounds.start >= lineStart) break;
    final marker = _fenceMarker(text.substring(bounds.start, bounds.end));
    if (marker == null) continue;
    if (openMarker.isEmpty) {
      openMarker = marker.marker;
      openLength = marker.length;
    } else if (marker.marker == openMarker && marker.length >= openLength) {
      openMarker = '';
      openLength = 0;
    }
  }
  return openMarker.isNotEmpty;
}

({String marker, int length})? _fenceMarker(String line) {
  final trimmed = line.trimLeft();
  if (line.length - trimmed.length > 3 || trimmed.length < 3) return null;
  final first = trimmed.codeUnitAt(0);
  if (first != 96 && first != 126) return null;
  var length = 0;
  while (length < trimmed.length && trimmed.codeUnitAt(length) == first) {
    length += 1;
  }
  if (length < 3) return null;
  return (marker: String.fromCharCode(first), length: length);
}

final class _Replacement {
  const _Replacement({
    required this.start,
    required this.end,
    required this.replacement,
  });

  final int start;
  final int end;
  final String replacement;
}

final class _EstablishedTable {
  const _EstablishedTable({
    required this.rows,
    required this.currentRowIndex,
    required this.currentColumnIndex,
    required this.separatorRowIndex,
  });

  final List<_ParsedTableLine> rows;
  final int currentRowIndex;
  final int currentColumnIndex;
  final int separatorRowIndex;

  _ParsedTableLine get currentRow => rows[currentRowIndex];
  int get columnCount => currentRow.columnCount;
}

final class _ParsedTableLine {
  const _ParsedTableLine({
    required this.line,
    required this.lineStart,
    required this.lineEnd,
    required this.lineEndWithBreak,
    required this.indent,
    required this.columnCount,
    required this.isSeparator,
    required this.cells,
  });

  final int line;
  final int lineStart;
  final int lineEnd;
  final int lineEndWithBreak;
  final String indent;
  final int columnCount;
  final bool isSeparator;
  final List<_TableCell> cells;

  bool get hasLineBreak => lineEndWithBreak > lineEnd;
}

final class _TableCell {
  const _TableCell({
    required this.rawStart,
    required this.rawEnd,
    required this.preferredCaret,
  });

  final int rawStart;
  final int rawEnd;
  final int preferredCaret;
}

final class _RawSegment {
  const _RawSegment(this.start, this.end);

  final int start;
  final int end;
}

final class _TableFormatResult {
  const _TableFormatResult({required this.text, required this.caret});

  final String text;
  final int caret;
}

final class _TableLineShape {
  const _TableLineShape({
    required this.columnCount,
    required this.isSeparator,
    required this.indent,
  });

  final int columnCount;
  final bool isSeparator;
  final String indent;
}

final class _LineBounds {
  const _LineBounds({
    required this.start,
    required this.end,
    required this.endWithBreak,
  });

  final int start;
  final int end;
  final int endWithBreak;
}

final class _FormatRow {
  const _FormatRow(this.bounds, this.shape, this.cells);

  final _LineBounds bounds;
  final _TableLineShape shape;
  final List<String> cells;
}

abstract interface class _LineLookup {
  int get lineCount;

  int lineStart(int line);

  int lineEnd(int line);
}

final class _DocumentLineLookup implements _LineLookup {
  const _DocumentLineLookup(this.buffer);

  final SovereignTextBuffer buffer;

  @override
  int get lineCount => buffer.lineCount;

  @override
  int lineStart(int line) => buffer.lineStart(line);

  @override
  int lineEnd(int line) => buffer.lineEnd(line);
}

final class _SimpleLineBuffer implements _LineLookup {
  _SimpleLineBuffer(this.text) : starts = _lineStarts(text);

  final String text;
  final List<int> starts;

  @override
  int get lineCount => starts.length;

  @override
  int lineStart(int line) => starts[line];

  @override
  int lineEnd(int line) {
    final bounds = _lineBounds(text, starts, line);
    return bounds?.end ?? text.length;
  }
}
