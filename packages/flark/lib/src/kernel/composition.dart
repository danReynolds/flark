part of 'editor.dart';

/// Input-method composition. The platform holds the text it composes, and
/// each preedit, and the commit, edits the text it holds. Composed text goes
/// into the source as it is, so the two stay the same; what typing makes of
/// text (a pending style's delimiters, an escaped pipe in a table cell, a
/// completed fence or setext separation, the wrappers of a replaced
/// selection) would change the platform's text under it, and is made once,
/// when the composition commits. Where typing refuses the text, so does the
/// composition.
extension _Composition on FlarkEditor {
  /// [text] replaces [start]..[end] of the source, as the platform replaced
  /// it, and the caret follows it. The pending style stays the composition's
  /// intent. Composed text in code is literal code, as typing the code
  /// delegate declines is. Text composed past the text composed so far goes
  /// in where typing it there would.
  bool _compose(int start, int end, String text) {
    final length = source.length;
    var from = start.clamp(0, length), to = end.clamp(0, length);
    if (to < from) (from, to) = (to, from);
    if (from == to && text.isEmpty) return false;
    final range = _composed;
    final composed = range != null && identical(range.$3, source)
        ? range
        : null;
    final within = composed != null && composed.$1 <= from && to <= composed.$2;
    if (!within && !_wholeRange(from, to)) {
      final code = _pasteCode(text, from: from, to: to);
      if (code != null) return code;
      // Typing the text is only asked whether it would refuse it.
      final composing = _editState;
      final current = _snapshot as FlarkLiveSnapshot;
      final refused = _typeComposed(
        from == selection.start && to == selection.end
            ? current
            : FlarkLiveSnapshot._(_doc.withSelection(FlarkSelection(from, to))),
        _pending,
        text,
      );
      _editState = composing;
      if (refused != null) {
        _lastRejection = refused;
        return false;
      }
    }
    if (!_commit(
      source.replaceRange(from, to, text),
      FlarkSelection.collapsed(from + text.length),
      coalesce: false,
      pending: _pending,
      acceptSourceMode: true,
    )) {
      return false;
    }
    final grown = text.length - (to - from);
    _composed = composed == null
        ? (from, from + text.length, source, range != null)
        : (
            from < composed.$1 ? from : composed.$1,
            (to > composed.$2 ? to : composed.$2) + grown,
            source,
            composed.$4 || !within,
          );
    return true;
  }

  /// Makes of [before]'s composed text what typing it would have made: from
  /// the state the composition began in, the text typed over the range it
  /// replaced (or that range deleted, when it composed nothing), through the
  /// typing path, which the parser validates. Text in fenced code stays
  /// literal code, which the code delegate never sees. Typed as it was
  /// composed, it keeps the selection the platform left in it and takes
  /// typing's intent; refused, it is withdrawn, as a cancelled composition
  /// is, with typing's reason. True when the source, selection or typing
  /// intent changed.
  bool _retypeComposition(HistoryEntry before) {
    final began = before.source, now = source;
    if (sourceMode || began == now) return false;
    // Text composed in more than one place (an input method correcting a
    // word beside the one it composes) stays as composed: typed as one
    // span, the text between the places (a cell's pipe, a span's
    // delimiters) would be typed over as text.
    final range = _composed;
    if (range != null && identical(range.$3, now) && range.$4) return false;
    // The composed text and the range of [began] it replaced: the smallest
    // difference that still holds the selection the composition began with
    // and the text it composed, however the text around them repeats.
    final at = before.selection;
    var head = at.start, tail = began.length - at.end;
    if (range != null && identical(range.$3, now)) {
      if (range.$1 < head) head = range.$1;
      if (now.length - range.$2 < tail) tail = now.length - range.$2;
    }
    var a = 0, b = 0;
    while (a < head &&
        a < now.length &&
        began.codeUnitAt(a) == now.codeUnitAt(a)) {
      a++;
    }
    // Neither end may pass the other in either text: a composition beside
    // other edits can leave [began] shorter than [now] around it.
    while (b < tail &&
        b < now.length - a &&
        b < began.length - a &&
        began.codeUnitAt(began.length - 1 - b) ==
            now.codeUnitAt(now.length - 1 - b)) {
      b++;
    }
    // Whole code points: a pair's halves go together.
    bool low(String text, int i) =>
        i < text.length && text.codeUnitAt(i) & 0xFC00 == 0xDC00;
    if (a > 0 && (low(began, a) || low(now, a))) a--;
    if (b > 0 && (low(began, began.length - b) || low(now, now.length - b))) {
      b--;
    }
    final text = now.substring(a, now.length - b);
    var over = a == at.start && began.length - b == at.end
        ? at
        : FlarkSelection(a, began.length - b);
    var restored = _buildSnapshot(began, over, previous: _liveDocument);
    if (restored is FlarkLiveSnapshot && restored.selection != over) {
      // A span that ends inside the space before a cell's closing pipe (the
      // pipe's separator) leaves that space be, as typing does: the text is
      // typed at the end of the cell's text, which reads the same.
      final document = restored.document;
      var end = over.end;
      while (end > over.start &&
          !document.isLegal(end) &&
          (began.codeUnitAt(end - 1) == 0x20 ||
              began.codeUnitAt(end - 1) == 0x09)) {
        end--;
      }
      if (end != over.end &&
          (end > over.start || text.isNotEmpty) &&
          document.isLegal(end) &&
          document.rowAt(end).kind == RowKind.tableCell) {
        over = over.start == at.start && end == at.end
            ? at
            : FlarkSelection(over.start, end);
        restored = FlarkLiveSnapshot._(document.withSelection(over));
      }
    }
    if (restored is! FlarkLiveSnapshot ||
        restored.selection != over ||
        restored.document.rowAt(over.start).fenced ||
        restored.document.rowAt(over.end).fenced) {
      return false;
    }
    final composed = _snapshot, pending = _pending;
    final refused = _typeComposed(
      restored,
      identical(over, at) ? before.pending : null,
      text,
    );
    if (refused != null) {
      // Composed around other edits, it stays as composed: withdrawn, it
      // would take those edits with it. Composed in one place, it is
      // withdrawn, as typing refuses it.
      if (range == null || !identical(range.$3, now)) return false;
      _snapshot = identical(over, at) ? restored : _restoreSnapshot(before);
      _pending = before.pending;
      _lastRejection = refused;
      return true;
    }
    if (source != composed.source) return true;
    // Typing made the source as composed: the composed snapshot stays, its
    // selection where the platform left it, with typing's intent. Nothing
    // else needs putting back: [_typeComposed] did.
    _snapshot = composed;
    return !identical(_pending, pending);
  }

  /// Types [text] (deletes the selection, when it is empty) from [from]
  /// with [pending], through the typing path: typed, not composed, so its
  /// typed-construct completions apply, and outside history, where the
  /// composition records its own step. Accepted, the typed state stays;
  /// refused, the editor is as it was, and typing's reason is returned.
  FlarkRejection? _typeComposed(
    FlarkLiveSnapshot from,
    PendingStyle? pending,
    String text,
  ) {
    final saved = _editState;
    final rejection = _lastRejection, inert = _inert;
    FlarkRejection? refused;
    var typed = false;
    try {
      _composition = null;
      _snapshot = from;
      _pending = pending;
      _lastRejection = null;
      typed = _withMissingCell(
        text.isEmpty ? const DeleteBackward() : InsertText(text),
      );
      if (!typed) refused = _lastRejection ?? FlarkRejection.unsupportedEdit;
    } finally {
      // Everything goes back but, typed, the text typing made.
      final (snapshot, kept, goal) = (_snapshot, _pending, _goalColumn);
      _editState = saved;
      if (typed) {
        _snapshot = snapshot;
        _pending = kept;
        _goalColumn = goal;
      }
      _lastRejection = rejection;
      _inert = inert;
    }
    return refused;
  }
}
