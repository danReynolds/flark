/// Undo history: snapshots grouped so one logical action is one entry.
library;

import 'document.dart';

/// A style the next typed character will take, from a formatting command at
/// the caret or from an owner just emptied by a deletion. Part of the state
/// history restores: undo brings back the typing intent, not just the text.
final class PendingStyle {
  const PendingStyle(
    this.open,
    this.close,
    this.styles, {
    this.continueAcrossSpaces = false,
  });
  final String open, close;
  final int styles;

  /// A nonempty span moved its closing syntax before newly typed whitespace.
  /// Further spaces keep that typing intent; an emptied span still exits it.
  final bool continueAcrossSpaces;
}

final class HistoryEntry {
  const HistoryEntry(this.source, this.selection, this.pending, this.group);
  final String source;
  final FlarkSelection selection;
  final PendingStyle? pending;
  final int group;
}

final class History {
  History({
    this.coalesceWindow = const Duration(seconds: 1),
    this.maxEntries = 100,
    this.maxSourceCodeUnits = 4 * 1024 * 1024,
  }) {
    if (maxEntries <= 0) {
      throw ArgumentError.value(maxEntries, 'maxEntries', 'must be positive');
    }
    if (maxSourceCodeUnits < 0) {
      throw ArgumentError.value(
        maxSourceCodeUnits,
        'maxSourceCodeUnits',
        'must not be negative',
      );
    }
  }
  final Duration coalesceWindow;

  /// Shared cap across undo and redo snapshots.
  final int maxEntries;

  /// Shared cap for the source strings held by undo and redo snapshots.
  final int maxSourceCodeUnits;
  final List<HistoryEntry> _undo = [];
  final List<HistoryEntry> _redo = [];
  int _storedSourceCodeUnits = 0;
  int _group = 0;
  Duration? _lastTypingAt;
  bool _lastWasTyping = false;

  bool get canUndo => _undo.isNotEmpty;
  HistoryEntry? get undoTarget => _targetOf(_undo);

  /// Id of the group the next coalescing record would join.
  int get openGroup => _group;

  /// Group of the most recent undo entry, or -1.
  int get lastGroup => _undo.isEmpty ? -1 : _undo.last.group;
  bool get canRedo => _redo.isNotEmpty;
  HistoryEntry? get redoTarget => _targetOf(_redo);

  /// Close the current typing group. The next source change starts a new
  /// undo step even when it arrives inside [coalesceWindow].
  void breakCoalescing() {
    _lastWasTyping = false;
    _lastTypingAt = null;
  }

  /// Record the state before a change. Typing within the coalescing window
  /// joins the open group; anything else starts a new one. A joined change
  /// does not append another full-source snapshot: the group's first state
  /// is all undo needs.
  void record(
    FlarkDocument before, {
    required PendingStyle? pending,
    required bool typing,
    required Duration at,
    bool composition = false,
  }) => recordState(
    before.source,
    before.selection,
    pending: pending,
    typing: typing,
    at: at,
    composition: composition,
  );

  /// Record an editor state that may not own a parsed [FlarkDocument]. Source
  /// mode uses this path so history never parses merely to store an undo entry.
  void recordState(
    String source,
    FlarkSelection selection, {
    required PendingStyle? pending,
    required bool typing,
    required Duration at,
    bool composition = false,
  }) {
    final elapsed = _lastTypingAt == null ? null : at - _lastTypingAt!;
    final joins =
        (typing || composition) &&
        _lastWasTyping &&
        _undo.isNotEmpty &&
        _undo.last.group == _group &&
        elapsed != null &&
        !elapsed.isNegative &&
        elapsed <= coalesceWindow;
    if (!joins) {
      _group++;
      _add(_undo, HistoryEntry(source, selection, pending, _group));
    }
    _clear(_redo);
    _trim();
    _lastWasTyping = typing || composition;
    _lastTypingAt = at;
    if (_undo.isEmpty || _undo.last.group != _group) breakCoalescing();
  }

  /// Pop the whole most recent group; returns the state to restore.
  HistoryEntry? undo(FlarkDocument current, PendingStyle? pending) =>
      undoState(current.source, current.selection, pending);

  /// Undo from an editor state that may not own a parsed document.
  HistoryEntry? undoState(
    String source,
    FlarkSelection selection,
    PendingStyle? pending,
  ) {
    if (_undo.isEmpty) return null;
    breakCoalescing();
    final group = _undo.last.group;
    HistoryEntry? target;
    while (_undo.isNotEmpty && _undo.last.group == group) {
      target = _removeLast(_undo);
    }
    _add(_redo, HistoryEntry(source, selection, pending, group));
    _trim();
    return target;
  }

  HistoryEntry? redo(FlarkDocument current, PendingStyle? pending) =>
      redoState(current.source, current.selection, pending);

  /// Redo from an editor state that may not own a parsed document.
  HistoryEntry? redoState(
    String source,
    FlarkSelection selection,
    PendingStyle? pending,
  ) {
    if (_redo.isEmpty) return null;
    breakCoalescing();
    final target = _removeLast(_redo);
    _add(_undo, HistoryEntry(source, selection, pending, target.group));
    _trim();
    return target;
  }

  void _add(List<HistoryEntry> stack, HistoryEntry entry) {
    stack.add(entry);
    _storedSourceCodeUnits += entry.source.length;
  }

  static HistoryEntry? _targetOf(List<HistoryEntry> stack) {
    if (stack.isEmpty) return null;
    final group = stack.last.group;
    var index = stack.length - 1;
    while (index > 0 && stack[index - 1].group == group) {
      index--;
    }
    return stack[index];
  }

  HistoryEntry _removeLast(List<HistoryEntry> stack) {
    final entry = stack.removeLast();
    _storedSourceCodeUnits -= entry.source.length;
    return entry;
  }

  void _clear(List<HistoryEntry> stack) {
    for (final entry in stack) {
      _storedSourceCodeUnits -= entry.source.length;
    }
    stack.clear();
  }

  void _removeOldest(List<HistoryEntry> stack) {
    final entry = stack.removeAt(0);
    _storedSourceCodeUnits -= entry.source.length;
  }

  void _trim() {
    while (_undo.length + _redo.length > maxEntries ||
        _storedSourceCodeUnits > maxSourceCodeUnits) {
      // Preserve the immediately reachable state on each side when possible.
      if (_undo.length > 1) {
        _removeOldest(_undo);
      } else if (_redo.length > 1) {
        _removeOldest(_redo);
      } else if (_undo.isNotEmpty &&
          (_redo.isEmpty ||
              _undo.first.source.length >= _redo.first.source.length)) {
        _removeOldest(_undo);
      } else if (_redo.isNotEmpty) {
        _removeOldest(_redo);
      } else {
        break;
      }
    }
  }
}
