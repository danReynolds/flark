part of 'editor.dart';

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
  /// A refusal/exception restores the original snapshot; composition captures
  /// the original source and virtual caret before any delimiters are created.
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
    final before = _snapshot, pending = _pending, goal = _goalColumn;
    final row = projection.rows[index];
    var first = index;
    while (projection.isMissingCell(first - 1)) {
      first--;
    }
    final count = row.column - projection.rows[first].column + 1;
    final at = row.sourceStart;
    final closingPipe = at < source.length && source[at] == '|';
    final prefix = '${closingPipe ? '' : ' '}${'| ' * count}';
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
    _cellOrigin = before;
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
      _cellOrigin = null;
      if (!applied) {
        _snapshot = before;
        _pending = pending;
        _goalColumn = goal;
      }
    }
  }
}
