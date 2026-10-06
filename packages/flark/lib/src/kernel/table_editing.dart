part of 'editor.dart';

/// The displayed text of each cell of [cell]'s table row, in column order,
/// including the cells the parser supplies for a short row. A table row's
/// cells are consecutive rows of the projection.
List<String> _tableRowCells(Projection projection, ProjectedRow cell) {
  final rows = projection.rows, block = cell.tableRowBlock;
  bool inRow(int i) =>
      i >= 0 &&
      i < rows.length &&
      rows[i].kind == RowKind.tableCell &&
      rows[i].tableRowBlock == block;
  var first = cell.index;
  while (inRow(first - 1)) {
    first--;
  }
  return [for (var i = first; inRow(i); i++) rows[i].text];
}

/// Whether [next] displays [cells] as the table row holding [offset], with
/// [offset] in [column]. GFM drops a row's surplus cells, so comparing the
/// cell count alone cannot notice a cell split by an edit. An [edited] cell
/// in [column] may show any text.
bool _showsTableRow(
  FlarkDocument next,
  int offset,
  int column,
  List<String> cells, {
  bool edited = false,
}) {
  final row = next.rowAt(offset);
  if (row.kind != RowKind.tableCell || row.column != column) return false;
  final now = _tableRowCells(next.projection, row);
  if (now.length != cells.length) return false;
  for (var i = 0; i < now.length; i++) {
    if ((!edited || i != column) && now[i] != cells[i]) return false;
  }
  return true;
}

extension _TableEditing on FlarkEditor {
  bool _moveTableCell(bool backward) {
    final row = _doc.caretRow;
    if (row.kind != RowKind.tableCell) return false;
    final index = row.index + (backward ? -1 : 1);
    if (index >= 0 &&
        index < projection.rows.length &&
        projection.rows[index].tableBlock == row.tableBlock) {
      return _place(index, 0, true, false);
    }
    return !backward && _returnFromTable(row);
  }

  /// Prepare an unwritten cell privately, then use the ordinary editing path.
  /// The preparation and final edit parse privately; only the final edit
  /// publishes and enters history.
  /// A refusal or exception puts the editing state back whole
  /// ([_EditState]); composition captures the original source and virtual
  /// caret before any delimiters are created.
  bool _withMissingCell(FlarkCommand command) {
    final index = selection.tableCell;
    if (index == null) return _applyLive(command);
    if (command is DeleteBackward ||
        command is DeleteForward ||
        command is RemoveLink ||
        command is RemoveImage) {
      return false;
    }
    final replacement =
        command is ReplaceRange &&
        command.start == selection.extent &&
        command.end == selection.extent;
    if (command is! InsertText &&
        command is! Paste &&
        command is! SetLink &&
        command is! SetImage &&
        !replacement) {
      return _applyLive(command);
    }
    final before = _editState;
    final row = projection.rows[index];
    var first = index;
    while (projection.isMissingCell(first - 1)) {
      first--;
    }
    final count = row.column - projection.rows[first].column + 1;
    final at = row.sourceStart;
    final closingPipe = at < source.length && source[at] == '|';
    // A row without its closing pipe gets one right after its last cell,
    // which then shows no new trailing space, unless that cell ends with a
    // backslash: GFM reads any backslash before a pipe as escaping it.
    final escapes = !closingPipe && at > 0 && source[at - 1] == r'\';
    final prefix = '${escapes ? ' ' : ''}${'| ' * count}';
    final materialized = source.replaceRange(
      at,
      at,
      '$prefix${closingPipe ? '' : '|'}',
    );
    if (!_withinLiveByteLimit(materialized, sourceLimit)) {
      _lastRejection = FlarkRejection.sourceLimit;
      return false;
    }
    var applied = false;
    _cellOrigin = before.snapshot;
    try {
      // Only delimiters for this already-admitted table are added here. Keep
      // the private preparation live so formatting uses the same semantics as
      // an explicitly written empty cell. Admission belongs to the final edit,
      // not this intermediate prefix; it can cross the live byte/line limit.
      _snapshot = FlarkLiveSnapshot._(
        FlarkDocument.load(
          materialized,
          _backend,
          caret: at + prefix.length,
          options: _options,
        ),
      );
      final mapped = replacement
          ? ReplaceRange(selection.extent, selection.extent, command.text)
          : command;
      applied = _applyLive(mapped);
      return applied;
    } on FlarkParseException catch (error) {
      if (error.code != FlarkParseException.extractionDeviationCode) rethrow;
      _lastRejection = FlarkRejection.extractionDeviation;
      return false;
    } finally {
      if (applied) {
        _cellOrigin = null;
      } else {
        _editState = before;
      }
    }
  }
}
