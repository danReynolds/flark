/// Undo history: snapshots grouped so one logical action is one entry.
library;

import 'document.dart';

/// A style the next typed character will take, from a formatting command at
/// the caret or from an owner just emptied by a deletion. Part of the state
/// history restores: undo brings back the typing intent, not just the text.
final class PendingStyle {
  const PendingStyle(this.open, this.close, this.styles);
  final String open, close;
  final int styles;
}

final class HistoryEntry {
  const HistoryEntry(this.source, this.selection, this.pending, this.group);
  final String source;
  final FlarkSelection selection;
  final PendingStyle? pending;
  final int group;
}

final class History {
  History({this.coalesceWindow = const Duration(seconds: 1)});
  final Duration coalesceWindow;
  final List<HistoryEntry> _undo = [];
  final List<HistoryEntry> _redo = [];
  int _group = 0;
  Duration? _lastTypingAt;
  bool _lastWasTyping = false;

  bool get canUndo => _undo.isNotEmpty;

  /// Id of the group the next coalescing record would join.
  int get openGroup => _group;

  /// Group of the most recent undo entry, or -1.
  int get lastGroup => _undo.isEmpty ? -1 : _undo.last.group;
  bool get canRedo => _redo.isNotEmpty;

  /// Record the state before a change. Typing within the coalescing window
  /// joins the open group; anything else starts a new one.
  void record(FlarkDocument before, {required PendingStyle? pending, required bool typing, required Duration at, bool composition = false}) {
    final joins = (typing || composition) && _lastWasTyping && _lastTypingAt != null && at - _lastTypingAt! <= coalesceWindow;
    if (!joins) _group++;
    _undo.add(HistoryEntry(before.source, before.selection, pending, _group));
    _redo.clear();
    _lastWasTyping = typing || composition;
    _lastTypingAt = at;
  }

  /// Pop the whole most recent group; returns the state to restore.
  HistoryEntry? undo(FlarkDocument current, PendingStyle? pending) {
    if (_undo.isEmpty) return null;
    final group = _undo.last.group;
    HistoryEntry? target;
    while (_undo.isNotEmpty && _undo.last.group == group) { target = _undo.removeLast(); }
    _redo.add(HistoryEntry(current.source, current.selection, pending, group));
    return target;
  }

  HistoryEntry? redo(FlarkDocument current, PendingStyle? pending) {
    if (_redo.isEmpty) return null;
    final target = _redo.removeLast();
    _undo.add(HistoryEntry(current.source, current.selection, pending, target.group));
    return target;
  }
}
