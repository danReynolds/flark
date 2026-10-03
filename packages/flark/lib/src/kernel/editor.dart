/// The editor facade: the one object a host talks to. It owns the document,
/// applies commands, keeps history, and reports the typing context.
library;

import 'dart:typed_data';

import 'package:characters/characters.dart';
import '../../code.dart';

import '../parse/backend.dart';
import '../parse/render_model.dart';
import '../parse/schema.g.dart';
import 'commands.dart';
import 'document.dart';
import 'history.dart';
import 'notify.dart';
import 'projection.dart';
import 'style_state.dart';

part 'source_mode.dart';
part 'read_document.dart';
part 'admission.dart';
part 'code_editing.dart';
part 'resource_editing.dart';
part 'table_editing.dart';
part 'inline_formatting.dart';

typedef FlarkListener = void Function();

final class FlarkEditor implements FlarkDocumentState {
  FlarkEditor(
    this._backend, {
    String text = '',
    int caret = 0,
    this.codeEditing,
    this.syncLimit = defaultSyncLimit,
    this.liveLimits = const FlarkLiveLimits(),
    this.sourceLimit = 1024 * 1024,
    ProjectionOptions options = const ProjectionOptions(),
    Duration Function()? clock,
  }) : _options = options,
       _clock = clock ?? _stopwatch() {
    checkFlarkLimits(
      syncLimit: syncLimit,
      sourceLimit: sourceLimit,
      liveLimits: liveLimits,
    );
    validateFlarkSource(text);
    if (!_withinLiveByteLimit(text, sourceLimit)) {
      throw ArgumentError('document exceeds writable source limit');
    }
    _snapshot = _buildSnapshot(text, FlarkSelection.collapsed(caret));
  }

  static const int defaultSyncLimit = 16 * 1024;

  static Duration Function() _stopwatch() {
    final watch = Stopwatch()..start();
    return () => watch.elapsed;
  }

  final FlarkParseBackend _backend;

  /// Optional, already initialized snippet service. Its caller owns disposal.
  @override
  final CodeEditingDelegate? codeEditing;
  final ProjectionOptions _options;

  /// Maximum UTF-8 byte length that may be parsed and projected synchronously.
  final int syncLimit;
  final FlarkLiveLimits liveLimits;

  /// Maximum writable UTF-8 source size, including source mode.
  final int sourceLimit;
  int _revision = 0;
  @override
  int get revision => _revision;
  FlarkRejection? _lastRejection;
  FlarkRejection? get lastRejection => _lastRejection;

  /// Set when the command being applied asked for the state the editor
  /// already has. Like a repeated SetStyle, it is a successful no-op: it
  /// publishes nothing, records no history and is not reported as refused.
  bool _inert = false;
  bool _forceSourceMode = false;
  HistoryEntry? _composition;
  bool get composing => _composition != null;
  final History history = History();
  final List<FlarkListener> _listeners = [];
  late FlarkEditorSnapshot _snapshot;
  FlarkEditorSnapshot? _cellOrigin;
  PendingStyle? _pending;
  // Empty code bodies have a collapsed selection, so selection coordinates
  // alone cannot distinguish their first Select All from the second.
  bool _selectedCodeScope = false;

  /// Display column vertical movement aims for, kept across rows.
  int? _goalColumn;

  /// The time history coalescing reads when a command brings none. A
  /// stopwatch started with the editor unless the constructor was given a
  /// clock: a test that types a long run as one undo step passes one, so a
  /// slow machine cannot split the run into several.
  final Duration Function() _clock;
  Duration _now = Duration.zero;

  @override
  FlarkEditorSnapshot get snapshot => _snapshot;
  @override
  FlarkDocument get document => _doc;
  @override
  String get source => _snapshot.source;
  @override
  FlarkSelection get selection => _snapshot.selection;
  @override
  Projection get projection => _doc.projection;
  @override
  bool get sourceMode => _snapshot is FlarkSourceSnapshot;

  /// The live document, or null in source mode.
  FlarkDocument? get _liveDocument => switch (_snapshot) {
    FlarkLiveSnapshot(:final document) => document,
    FlarkSourceSnapshot() => null,
  };

  FlarkDocument get _doc => switch (_snapshot) {
    FlarkLiveSnapshot(:final document) => document,
    FlarkSourceSnapshot() => throw StateError(
      'FlarkEditor has no parsed document in source mode',
    ),
  };

  /// Style bits the next typed character takes: the caret anchor's context,
  /// flipped by any pending formatting command.
  int get typingContext => sourceMode
      ? 0
      : (selection.tableCell == null
                ? _doc.typingContextAt(selection.extent)
                : 0) ^
            (_pending?.styles ?? 0);

  /// Read formatting without parsing or inspecting Markdown delimiters.
  /// Listen to this editor (or a host controller) and read again on changes.
  FlarkStyleState styleState(int style) => _styleState(style);

  void addListener(FlarkListener listener) => _listeners.add(listener);
  void removeListener(FlarkListener listener) => _listeners.remove(listener);

  /// Apply one command. Returns whether anything changed. [at] is the
  /// command's time, used only for history coalescing.
  bool apply(FlarkCommand command, {Duration? at, int? expectedRevision}) {
    _lastRejection = null;
    _inert = false;
    if (expectedRevision != null && expectedRevision != revision) {
      _lastRejection = FlarkRejection.staleRevision;
      return false;
    }
    if (command is SetStyle &&
        !sourceMode &&
        _styleDelimiter(command.style) != null &&
        styleState(command.style).value ==
            (command.enabled ? FlarkStyleValue.on : FlarkStyleValue.off)) {
      return false;
    }
    if (composing && (command is Undo || command is Redo)) commitComposition();
    if (command is! SelectAll) _selectedCodeScope = false;
    _now = at ?? _clock();
    final applied = sourceMode
        ? _applySource(command)
        : _withMissingCell(command);
    if (applied) {
      _notify();
    } else if (!_inert &&
        command is! SetSelection &&
        command is! MoveTableCell &&
        command is! SelectAll &&
        command is! MoveCaret &&
        command is! PlaceCaret) {
      _lastRejection ??= FlarkRejection.unsupportedEdit;
    }
    return applied;
  }

  /// Host actions close composition only if the action is accepted. Snapshot
  /// references are cheap; the small history savepoint is needed only in IME.
  bool applyAfterComposition(FlarkCommand command, {int? expectedRevision}) {
    if (!composing ||
        (expectedRevision != null && expectedRevision != revision)) {
      return apply(command, expectedRevision: expectedRevision);
    }
    final composition = _composition;
    final before = _snapshot, pending = _pending, origin = _cellOrigin;
    final selectedCode = _selectedCodeScope, goal = _goalColumn;
    final savedHistory = history.checkpoint();
    void restore() {
      _composition = composition;
      _snapshot = before;
      _pending = pending;
      _cellOrigin = origin;
      _selectedCodeScope = selectedCode;
      _goalColumn = goal;
      history.restore(savedHistory);
    }

    commitComposition();
    final unpublished = _revision;
    try {
      final accepted = apply(command, expectedRevision: expectedRevision);
      if (!accepted) restore();
      return accepted;
    } catch (_) {
      // Only a command that failed before it was published is withdrawn.
      // Listeners that heard of an edit hold its source, so restoring the
      // snapshot under them would split the document in two.
      if (_revision == unpublished) restore();
      rethrow;
    }
  }

  bool _applyLive(FlarkCommand command) => switch (command) {
    InsertText(:final text) => _insert(text, typing: true),
    Paste(:final text) => _insert(text, typing: false),
    DeleteBackward(:final word) => _delete(backward: true, word: word),
    DeleteForward(:final word) => _delete(backward: false, word: word),
    Newline(:final paragraph) => _newline(paragraph),
    ReplaceRange(:final start, :final end, :final text) => _replace(
      start,
      end,
      text,
    ),
    SetSelection(:final base, :final extent) => _select(
      FlarkSelection(base, extent),
    ),
    SelectAll() => _selectAll(),
    PlaceCaret(:final row, :final offset, :final leadingHalf, :final extend) =>
      _place(row, offset, leadingHalf, extend),
    MoveCaret(:final direction, :final unit, :final extend) => _move(
      direction,
      unit,
      extend,
    ),
    MoveTableCell(:final backward) => _moveTableCell(backward),
    Undo() => _undo(),
    Redo() => _redo(),
    ToggleTask() => _toggleTask(),
    ToggleStyle(:final style) => _setStyle(style, !styleState(style).isOn),
    SetStyle(:final style, :final enabled) => _setStyle(style, enabled),
    SetHeadingLevel(:final level) => _setHeading(level),
    SetCodeLanguage(:final language) => _setCodeLanguage(language),
    SetLink(:final destination, :final text, :final title) => _setResource(
      false,
      destination,
      text,
      title,
    ),
    SetImage(:final destination, :final alt, :final title) => _setResource(
      true,
      destination,
      alt,
      title,
    ),
    RemoveLink() => _removeResource(false),
    RemoveImage() => _removeResource(true),
    Indent() => _shiftBlock(outdent: false),
    Outdent() => _shiftBlock(outdent: true),
  };

  /// Whether the current selection supports creating or updating a resource.
  bool canSetResource({bool image = false}) =>
      !sourceMode && _canSetResource(image);

  /// Exact UTF-16 source splice. Unlike input replacement this never snaps
  /// the supplied range or preserves surrounding Markdown wrappers.
  bool replaceSourceRange(
    int start,
    int end,
    String text, {
    int? expectedRevision,
    bool replaceAll = false,
  }) {
    _lastRejection = null;
    if (expectedRevision != null && expectedRevision != revision) {
      _lastRejection = FlarkRejection.staleRevision;
      return false;
    }
    bool boundary(int offset) =>
        offset >= 0 &&
        offset <= source.length &&
        (offset == 0 ||
            offset == source.length ||
            source.codeUnitAt(offset) < 0xdc00 ||
            source.codeUnitAt(offset) > 0xdfff);
    if (start > end || !boundary(start) || !boundary(end)) {
      _lastRejection = FlarkRejection.invalidSource;
      return false;
    }
    final nextSource = source.replaceRange(start, end, text);
    final delta = text.length - (end - start);
    final old = selection;
    final nextSelection = replaceAll
        ? FlarkSelection.collapsed(nextSource.length)
        : old.isCollapsed && old.extent >= start && old.extent <= end
        ? FlarkSelection.collapsed(start + text.length)
        : old.end <= start
        ? old
        : old.start >= end
        ? FlarkSelection(old.base + delta, old.extent + delta)
        : FlarkSelection.collapsed(start + text.length);
    // Admission happens before composition/history changes. A rejected edit
    // must not commit a preedit or consume an undo step.
    final next = _admitSource(nextSource, nextSelection);
    if (next == null) return false;
    if (nextSource == source &&
        next.selection == selection &&
        _pending == null) {
      return false;
    }
    commitComposition();
    history.recordState(
      source,
      selection,
      pending: _pending,
      typing: false,
      at: _clock(),
    );
    _snapshot = next;
    _pending = null;
    _cellOrigin = null;
    _goalColumn = null;
    _selectedCodeScope = false;
    _notify();
    return true;
  }

  FlarkEditorSnapshot? _admitSource(String text, FlarkSelection selected) {
    try {
      validateFlarkSource(text);
    } on FormatException {
      _lastRejection = FlarkRejection.invalidSource;
      return null;
    }
    if (!_withinLiveByteLimit(text, sourceLimit)) {
      _lastRejection = FlarkRejection.sourceLimit;
      return null;
    }
    try {
      return _buildSnapshot(text, selected, rejectDeviation: !sourceMode);
    } on FlarkParseException catch (error) {
      if (error.code != FlarkParseException.extractionDeviationCode) rethrow;
      _lastRejection = FlarkRejection.extractionDeviation;
      return null;
    }
  }

  /// Establish external content without creating an undo step. Always
  /// publishes a revision, even when resetting identical content.
  bool loadMarkdown(String text, {int? expectedRevision}) {
    _lastRejection = null;
    if (expectedRevision != null && expectedRevision != revision) {
      _lastRejection = FlarkRejection.staleRevision;
      return false;
    }
    final next = _admitSource(text, const FlarkSelection.collapsed(0));
    if (next == null) return false;
    _composition = null;
    _pending = null;
    _cellOrigin = null;
    _goalColumn = null;
    _selectedCodeScope = false;
    history.clear();
    _snapshot = next;
    _notify();
    return true;
  }

  /// Programmatic selection has an explicit scope; keyboard SelectAll keeps
  /// its familiar fence-first, then whole-document progression.
  bool selectAll({bool codeBlock = false, int? expectedRevision}) {
    if (expectedRevision != null && expectedRevision != revision) {
      _lastRejection = FlarkRejection.staleRevision;
      return false;
    }
    if (codeBlock) {
      if (sourceMode || !_doc.caretRow.fenced) {
        _lastRejection = FlarkRejection.unsupportedEdit;
        return false;
      }
      final row = _doc.caretRow;
      return applyAfterComposition(
        SetSelection(
          row.sourceForDisplay(0),
          row.sourceForDisplay(row.text.length),
        ),
      );
    }
    return applyAfterComposition(SetSelection(0, source.length));
  }

  void _notify() {
    _revision++;
    notifyEach(_listeners);
  }

  /// Input methods may publish several preedit values as one logical action.
  /// Cancellation restores source, selection and typing intent without using
  /// or clearing the user's undo/redo stacks.
  void beginComposition() {
    _composition ??= HistoryEntry(
      source,
      selection,
      _pending,
      history.openGroup,
    );
  }

  void commitComposition() {
    final before = _composition;
    if (before == null) return;
    if (source != before.source) {
      history.recordState(
        before.source,
        before.selection,
        pending: before.pending,
        typing: false,
        at: _clock(),
      );
    }
    _composition = null;
    history.breakCoalescing();
  }

  void cancelComposition() {
    final before = _composition;
    if (before == null) return;
    final next = _restoreSnapshot(before);
    _snapshot = next;
    _pending = before.pending;
    _composition = null;
    _notify();
  }

  /// Explicit source editing remains available for unsupported rich edits.
  /// Returning to rendered mode uses the same admission path as opening.
  void setSourceMode(bool enabled) {
    commitComposition();
    _selectedCodeScope = false;
    final previous = _forceSourceMode;
    _forceSourceMode = enabled;
    try {
      final next = _buildSnapshot(source, selection, previous: _liveDocument);
      _snapshot = next;
      _pending = null;
      history.breakCoalescing();
      _notify();
    } catch (_) {
      _forceSourceMode = previous;
      rethrow;
    }
  }

  /// [live] is the byte/line admission a caller already computed for [text].
  /// [previous], the live document being replaced, lends the new projection
  /// the rows the edit did not touch.
  FlarkEditorSnapshot _buildSnapshot(
    String text,
    FlarkSelection selected, {
    bool rejectDeviation = false,
    RenderModel? parsed,
    bool? live,
    FlarkDocument? previous,
  }) {
    return _projectSnapshot(
      _backend,
      text,
      selected,
      _options,
      liveLimits,
      syncLimit,
      forceSourceMode: _forceSourceMode,
      rejectDeviation: rejectDeviation,
      parsed: parsed,
      live: live,
      previous: previous,
    );
  }

  /// Parse, admit and project before publishing source or history.
  bool _commit(
    String newSource,
    FlarkSelection sel, {
    required bool typing,
    bool completeTypedFence = false,
    String? typedUnderline,
    PendingStyle? pending,
    bool Function(FlarkDocument)? accept,
    bool acceptSourceMode = false,
  }) {
    // An edit that leaves the source as it is has nothing to parse or undo:
    // at most it moves the selection, and when it does not even do that it is
    // inert. Recording it would leave an Undo that changes nothing. A missing
    // cell's private preparation is itself the change, so it commits as usual.
    if (newSource == source && _cellOrigin == null) {
      final moved = sourceMode ? _selectSource(sel) : _select(sel);
      _inert = !moved;
      return moved;
    }
    final stats = _SourceStats.of(newSource);
    if (!stats.valid) {
      _lastRejection = FlarkRejection.invalidSource;
      return false;
    }
    if (stats.utf8Bytes > sourceLimit) {
      _lastRejection = FlarkRejection.sourceLimit;
      return false;
    }
    final live = stats.utf8Bytes <= syncLimit && liveLimits._admitsStats(stats);
    late FlarkEditorSnapshot next;
    try {
      RenderModel? parsed;
      if (completeTypedFence && live) {
        parsed = _backend.parse(newSource);
        final completed = _completeTypedFence(newSource, sel.extent, parsed);
        if (completed != null) {
          return _commit(
            completed.source,
            FlarkSelection.collapsed(completed.caret),
            typing: false,
          );
        }
      }
      if (typedUnderline != null && live) {
        parsed = _backend.parse(newSource);
        final separated = _separateTypedUnderline(
          newSource,
          sel.extent,
          typedUnderline,
          parsed,
        );
        if (separated != null) {
          return _commit(
            separated.source,
            FlarkSelection.collapsed(separated.caret),
            pending: pending,
            typing: typing,
          );
        }
      }
      next = _buildSnapshot(
        newSource,
        sel,
        rejectDeviation: !sourceMode,
        parsed: parsed,
        live: live,
        previous: _liveDocument,
      );
    } on FlarkParseException catch (error) {
      if (error.code != FlarkParseException.extractionDeviationCode) rethrow;
      _lastRejection = FlarkRejection.extractionDeviation;
      return false;
    }
    if (accept != null &&
        (next is FlarkLiveSnapshot
            ? !accept(next.document)
            : !acceptSourceMode)) {
      return false;
    }
    if (!composing) {
      history.recordState(
        _cellOrigin?.source ?? source,
        _cellOrigin?.selection ?? selection,
        pending: _pending,
        typing: typing,
        at: _now,
      );
    }
    _snapshot = next;
    _pending = next is FlarkLiveSnapshot ? pending : null;
    _goalColumn = null;
    return true;
  }

  bool _select(FlarkSelection s) {
    final next = _doc.withSelection(s);
    if (next.selection == selection) return false;
    history.breakCoalescing();
    _pending = null;
    _goalColumn = null;
    _snapshot = FlarkLiveSnapshot._(next);
    return true;
  }

  bool _selectAll() {
    final row = _doc.caretRow;
    if (!_selectedCodeScope &&
        !_wholeRange(selection.start, selection.end) &&
        row.fenced &&
        _doc.rowAt(selection.base).index == row.index) {
      _selectedCodeScope = true;
      return _select(
        FlarkSelection(
          row.sourceForDisplay(0),
          row.sourceForDisplay(row.text.length),
        ),
      );
    }
    _selectedCodeScope = false;
    return _select(FlarkSelection(0, source.length));
  }

  // --------------------------------------------------------- source mode

  bool _applySource(FlarkCommand command) => switch (command) {
    InsertText(:final text) => _sourceInsert(text, typing: true),
    Paste(:final text) => _sourceInsert(text, typing: false),
    DeleteBackward(:final word) => _sourceDelete(backward: true, word: word),
    DeleteForward(:final word) => _sourceDelete(backward: false, word: word),
    Newline() => _sourceInsert(_lineBreakAt(selection.start), typing: false),
    ReplaceRange(:final start, :final end, :final text) => _sourceReplace(
      start,
      end,
      text,
    ),
    SetSelection(:final base, :final extent) => _selectSource(
      FlarkSelection(base, extent),
    ),
    SelectAll() => _selectSource(FlarkSelection(0, source.length)),
    MoveCaret(:final direction, :final unit, :final extend) => _moveSource(
      direction,
      unit,
      extend,
    ),
    Undo() => _undo(),
    Redo() => _redo(),
    MoveTableCell() ||
    PlaceCaret() ||
    ToggleTask() ||
    ToggleStyle() ||
    SetStyle() ||
    SetHeadingLevel() ||
    SetCodeLanguage() ||
    SetLink() ||
    SetImage() ||
    RemoveLink() ||
    RemoveImage() ||
    Indent() ||
    Outdent() => false,
  };

  FlarkSourceSnapshot get _sourceSnapshot => _snapshot as FlarkSourceSnapshot;

  bool _sourceInsert(String text, {required bool typing}) {
    if (text.isEmpty) return false;
    final sel = selection;
    return _commit(
      source.replaceRange(sel.start, sel.end, text),
      FlarkSelection.collapsed(sel.start + text.length),
      typing: typing && text != '\n' && text.characters.length == 1,
    );
  }

  bool _sourceReplace(int start, int end, String text) {
    var from = _sourceSnapshot._legalize(start);
    var to = _sourceSnapshot._legalize(end);
    if (to < from) (from, to) = (to, from);
    if (from == to && text.isEmpty) return false;
    return _commit(
      source.replaceRange(from, to, text),
      FlarkSelection.collapsed(from + text.length),
      typing: false,
    );
  }

  bool _sourceDelete({required bool backward, bool word = false}) {
    final sel = selection;
    var from = sel.start;
    var to = sel.end;
    if (sel.isCollapsed) {
      if (backward) {
        from = word
            ? _sourceSnapshot._legalize(_wordStart(source, sel.extent))
            : _sourceSnapshot._previousGraphemeBoundary(sel.extent);
      } else {
        to = word
            ? _sourceSnapshot._legalize(_wordEnd(source, sel.extent))
            : _sourceSnapshot._nextGraphemeBoundary(sel.extent);
      }
    }
    if (from == to) return false;
    return _commit(
      source.replaceRange(from, to, ''),
      FlarkSelection.collapsed(from),
      typing: sel.isCollapsed && !word,
    );
  }

  bool _selectSource(FlarkSelection selection) {
    final next = _sourceSnapshot._withSelection(selection);
    if (next.selection == this.selection) return false;
    history.breakCoalescing();
    _pending = null;
    _goalColumn = null;
    _snapshot = next;
    return true;
  }

  bool _moveSource(MoveDirection direction, MoveUnit unit, bool extend) {
    if (unit == MoveUnit.row) return false;
    final sel = selection;
    final forward = direction == MoveDirection.forward;
    final current = sel.extent;
    int target;
    if (!extend && !sel.isCollapsed && unit == MoveUnit.grapheme) {
      target = forward ? sel.end : sel.start;
    } else {
      target = switch (unit) {
        MoveUnit.grapheme =>
          forward
              ? _sourceSnapshot._nextGraphemeBoundary(current)
              : _sourceSnapshot._previousGraphemeBoundary(current),
        MoveUnit.word => _sourceSnapshot._legalize(
          forward ? _wordEnd(source, current) : _wordStart(source, current),
        ),
        MoveUnit.line =>
          forward ? _sourceLineEnd(current) : _sourceLineStart(current),
        MoveUnit.row => current,
      };
    }
    return _selectSource(
      extend
          ? FlarkSelection(sel.base, target)
          : FlarkSelection.collapsed(target),
    );
  }

  int _sourceLineStart(int offset) =>
      offset <= 0 ? 0 : source.lastIndexOf('\n', offset - 1) + 1;

  int _sourceLineEnd(int offset) {
    final newline = source.indexOf('\n', offset);
    var end = newline < 0 ? source.length : newline;
    if (end > 0 && source.codeUnitAt(end - 1) == 0x0D) end--;
    return end;
  }

  /// The line break Return inserts at [offset]: the one that ends its line,
  /// or on a last line the one before it, so a CRLF document stays CRLF.
  String _lineBreakAt(int offset) {
    var newline = source.indexOf('\n', offset);
    if (newline < 0 && offset > 0) {
      newline = source.lastIndexOf('\n', offset - 1);
    }
    return newline > 0 && source.codeUnitAt(newline - 1) == 0x0D
        ? '\r\n'
        : '\n';
  }

  // ------------------------------------------------------------- inline

  bool _wholeRange(int start, int end) =>
      start == 0 && end == source.length && start != end;

  bool _replaceWhole(String text) =>
      _commit(text, FlarkSelection.collapsed(text.length), typing: false);

  /// A visible selection contained in an inline owner edits its content even
  /// when navigation chose the outer anchor at one of its hidden boundaries.
  /// Leave ranges that include visible text outside the owner untouched.
  ({int start, int end}) _contentRange(int start, int end) {
    if (start == end) return (start: start, end: end);
    for (var changed = true; changed;) {
      changed = false;
      for (final o in [
        ..._doc.ownersAt(start),
        ..._doc.ownersAt(end),
        ..._doc.ownersTouching(start),
        ..._doc.ownersTouching(end),
      ]) {
        if (start < o.start || end > o.end) continue;
        final a = start < o.contentStart ? o.contentStart : start;
        final b = end > o.contentEnd ? o.contentEnd : end;
        if (a < b && (a != start || b != end)) {
          start = a;
          end = b;
          changed = true;
        }
      }
    }
    return (start: start, end: end);
  }

  bool _insert(String typed, {required bool typing}) {
    if (typed.isEmpty) return false;
    final sel = selection;
    if (_wholeRange(sel.start, sel.end)) return _replaceWhole(typed);
    final range = _contentRange(sel.start, sel.end);
    if (!_supportedRange(range.start, range.end)) return false;
    final row = _doc.rowAt(sel.extent);
    // A pipe typed into a table cell is that cell's text. Escaped, it stays
    // in the cell; raw, it would split the row, push the last cell's text out
    // of the table and leave the caret before the delimiter it made. Paste
    // and source mode keep Markdown's literal meaning.
    final cell = typing && row.kind == RowKind.tableCell && typed.contains('|');
    final text = cell ? typed.replaceAll('|', r'\|') : typed;
    if (row.fenced && row.contentStarts.every((start) => start < 0)) {
      final code = _pasteCode(text);
      if (code != null) return code;
    }
    // Typed text goes to the code delegate first. What it does not propose,
    // and pasted or composed text, is literal code.
    final code =
        (typing && !composing
            ? _delegateCodeEdit(CodeEditingAction.insert, text: text)
            : null) ??
        _pasteCode(
          text,
          typing: typing && text != '\n' && text.characters.length == 1,
        );
    if (code != null) return code;
    if (sel.isCollapsed) {
      final continued = _continueSpan(range.start, text, typing: typing);
      if (continued != null) return continued;
    }
    var inserted = text;
    var caret = range.start + text.length;
    final p = _pending;
    PendingStyle? pending;
    // Empty-owner intent exits on whitespace. A surviving span continues
    // across spaces, keeping its delimiters around non-whitespace content.
    if (p != null && sel.isCollapsed && text.trim().isNotEmpty) {
      var first = 0, last = text.length;
      if (p.styles & (Style.strong | Style.emphasis | Style.strikethrough) !=
          0) {
        while (first < last && _isSpace(text, first)) {
          first++;
        }
        while (last > first && _isSpace(text, last - 1)) {
          last--;
        }
      }
      inserted =
          '${text.substring(0, first)}${p.open}${text.substring(first, last)}${p.close}${text.substring(last)}';
      caret = sel.start + p.open.length + text.length;
      if (last < text.length) {
        caret += p.close.length;
        pending = PendingStyle(
          p.open,
          p.close,
          p.styles,
          continueAcrossSpaces: true,
        );
      }
    } else if (p != null &&
        sel.isCollapsed &&
        p.continueAcrossSpaces &&
        !text.contains('\n') &&
        !text.contains('\r')) {
      pending = p;
    }
    var start = range.start, end = range.end;
    if (!sel.isCollapsed && text.trim().isEmpty) {
      final expanded = _rangeForEmptying(start, end);
      final heading = _emptySetext(
        expanded.start,
        expanded.end,
        inserted,
        typing: typing && typed != '\n' && typed.characters.length == 1,
      );
      if (heading != null) return heading;
      start = expanded.start;
      end = expanded.end;
      caret = start + inserted.length;
    }
    // Text typed into an empty closed heading can go before the caret.
    final (at, gap) = sel.isCollapsed
        ? _sequencePlace(row, start, inserted)
        : (start, '');
    final normalized = _normalizeInlineEdges(
      at,
      at + end - start,
      '$inserted$gap',
      caret + at - start,
      pending: pending,
    );
    // The escape must keep the row's cells: the edited cell shows exactly the
    // typed text, and every other cell is unchanged.
    final cells = cell
        ? (_tableRowCells(projection, row.tableRowBlock)
            ..[row.column] = row.text.replaceRange(
              row.displayForSource(range.start).$1,
              row.displayForSource(range.end).$1,
              typed,
            ))
        : null;
    return _commit(
      normalized.text,
      FlarkSelection.collapsed(normalized.caret),
      pending: normalized.pending,
      typing: typing && typed != '\n' && typed.characters.length == 1,
      accept: cells != null
          ? (next) => _showsTableRow(next, normalized.caret, row.column, cells)
          : gap.isEmpty
          ? null
          : (next) => _keepsStructure(next, [
              (start, end, inserted.length + gap.length),
            ], const {}),
      acceptSourceMode: gap.isNotEmpty,
      completeTypedFence:
          typing &&
          !composing &&
          sel.isCollapsed &&
          inserted == text &&
          (text == '`' || text == '```' || text == '~' || text == '~~~') &&
          row.kind != RowKind.codeBlock,
      typedUnderline:
          typing &&
              !composing &&
              sel.isCollapsed &&
              inserted == text &&
              row.text.isEmpty &&
              row.kind != RowKind.codeBlock &&
              _underlineRun.hasMatch(text)
          ? text
          : null,
    );
  }

  /// Where [text] placed at [at] in [row] goes, and the space to insert after
  /// it, when a heading's closing sequence starts there. An empty heading's
  /// text starts where its sequence does, since the spaces between them
  /// separate the opening marker; run into the text, the sequence would read
  /// as more of it. After two or more spaces the text goes before the last,
  /// as [_emptyHeadingText] places it, which still separates it from the
  /// sequence, so erasing it restores the heading as it was. After one, the
  /// text takes a space of its own, as text in a heading has, unless it ends
  /// in whitespace. Text with a line break gets neither: a space would not
  /// keep the sequence on the heading's line. A placement with the space
  /// commits only when [_keepsStructure] holds, so a sequence it still paints
  /// is refused.
  (int, String) _sequencePlace(ProjectedRow row, int at, String text) {
    final trail = _headingTrail(row);
    if (trail == null ||
        trail.$1 != at ||
        _isSpace(source, at) ||
        text.isEmpty ||
        text.contains('\n') ||
        text.contains('\r')) {
      return (at, '');
    }
    final placed = _emptyHeadingText(at);
    return placed != at || _isSpace(text, text.length - 1)
        ? (placed, '')
        : (at, ' ');
  }

  /// Where an empty heading's text goes when its closing sequence starts at
  /// [at]: before the last of two or more spaces or tabs there, which then
  /// separates the text from the sequence while the others stay hidden before
  /// the text; after a single one, at [at], where the text needs a space of
  /// its own.
  int _emptyHeadingText(int at) =>
      at > 1 && _isSpace(source, at - 1) && _isSpace(source, at - 2)
      ? at - 1
      : at;

  /// A word typed after the spaces that left an emphasis, strong or
  /// strikethrough span continues that span: its closing syntax moves past
  /// the word, giving `**one two**` rather than `**one** **two**`. The parser
  /// must own that syntax as a span ending where the spaces begin, and must
  /// still see one span from the same opener afterwards; otherwise the word
  /// takes its own pair as before. Null when this does not apply.
  bool? _continueSpan(int at, String text, {required bool typing}) {
    final p = _pending;
    if (p == null ||
        !p.continueAcrossSpaces ||
        text.trim().isEmpty ||
        text.contains('\n') ||
        text.contains('\r')) {
      return null;
    }
    var gap = at;
    while (gap > 0 &&
        (source.codeUnitAt(gap - 1) == 0x20 ||
            source.codeUnitAt(gap - 1) == 0x09)) {
      gap--;
    }
    final closeStart = gap - p.close.length;
    if (gap == at ||
        closeStart < 0 ||
        source.substring(closeStart, gap) != p.close) {
      return _continueSpanBefore(at, text, typing: typing);
    }
    final owner = _doc
        .ownersTouching(gap)
        .where(
          (o) =>
              o.end == gap &&
              (o.kind == RunKind.emph ||
                  o.kind == RunKind.strong ||
                  o.kind == RunKind.strike),
        )
        .firstOrNull;
    if (owner == null) return _continueSpanBefore(at, text, typing: typing);
    var first = 0, last = text.length;
    while (first < last && _isSpace(text, first)) {
      first++;
    }
    while (last > first && _isSpace(text, last - 1)) {
      last--;
    }
    final word = '${source.substring(gap, at)}${text.substring(0, last)}';
    final end = closeStart + word.length + p.close.length;
    final trailing = text.substring(last);
    final continued = _commit(
      source.replaceRange(closeStart, at, '$word${p.close}$trailing'),
      FlarkSelection.collapsed(
        trailing.isEmpty ? end - p.close.length : end + trailing.length,
      ),
      // Trailing spaces leave the span again and keep its intent.
      pending: trailing.isEmpty ? null : p,
      typing: typing && text.characters.length == 1,
      accept: (document) => document
          .ownersTouching(owner.start)
          .any((o) => o.start == owner.start && o.end == end),
    );
    return continued ? true : null;
  }

  /// The mirror of [_continueSpan]: a word typed where an emphasis, strong or
  /// strikethrough span's first word was deleted, before the spaces that now
  /// lead it, rejoins that span. Its opening syntax moves before the word,
  /// giving `**new two**` rather than `**new** **two**`. The parser must see
  /// one span of the same kind from the moved opening to the old closing
  /// syntax; otherwise the word takes its own pair. Null when this does not
  /// apply.
  bool? _continueSpanBefore(int at, String text, {required bool typing}) {
    final p = _pending!;
    var gap = at;
    while (gap < source.length &&
        (source.codeUnitAt(gap) == 0x20 || source.codeUnitAt(gap) == 0x09)) {
      gap++;
    }
    if (gap == at || !source.startsWith(p.open, gap)) return null;
    final owner = _doc
        .ownersTouching(gap)
        .where(
          (o) =>
              o.start == gap &&
              (o.kind == RunKind.emph ||
                  o.kind == RunKind.strong ||
                  o.kind == RunKind.strike),
        )
        .firstOrNull;
    if (owner == null) return null;
    var first = 0;
    while (first < text.length && _isSpace(text, first)) {
      first++;
    }
    final start = at + first;
    final continued = _commit(
      source.replaceRange(
        at,
        gap + p.open.length,
        '${text.substring(0, first)}${p.open}${text.substring(first)}'
        '${source.substring(at, gap)}',
      ),
      FlarkSelection.collapsed(start + p.open.length + text.length - first),
      typing: typing && text.characters.length == 1,
      accept: (document) => document
          .ownersTouching(start)
          .any(
            (o) =>
                o.start == start &&
                o.kind == owner.kind &&
                o.end == owner.end + text.length,
          ),
    );
    return continued ? true : null;
  }

  /// Typing `-` or `=` on the empty line under a paragraph makes that line a
  /// setext underline: the paragraph becomes a heading, and the underline is
  /// hidden markup with no caret position, so the next character would land
  /// at the end of the heading's text. A blank line before the typed line
  /// gives it a block of its own, the bare list marker or paragraph the user
  /// is starting (`---` becomes a thematic break). The parser identifies the
  /// heading and confirms the separation; paste, IME preedit and source mode
  /// keep Markdown's literal meaning.
  ({String source, int caret})? _separateTypedUnderline(
    String candidate,
    int caret,
    String typed,
    RenderModel model,
  ) {
    bool underlines(RenderModel model, int line) => model.blocks.any(
      (block) =>
          block.kind == BlockKind.heading &&
          block.lineCount > 1 &&
          block.firstLine + block.lineCount - 1 == line,
    );
    final start = caret - typed.length;
    final line = model.lineOfUtf16(caret);
    if (start < 0 ||
        model.lineOfUtf16(start) != line ||
        !underlines(model, line)) {
      return null;
    }
    // The typed row was empty, so the text before the typed characters is
    // only its container prefix (quote markers, item indentation).
    final lineStart = model.lineStartUtf16(line);
    final newline =
        lineStart >= 2 && candidate.startsWith('\r\n', lineStart - 2)
        ? '\r\n'
        : '\n';
    final blank =
        '${candidate.substring(lineStart, start).trimRight()}$newline';
    final separated = candidate.replaceRange(lineStart, lineStart, blank);
    final moved = caret + blank.length;
    // The blank line must not cost the edit its admission: near a byte,
    // line or shape limit the typed characters are inserted as they are.
    final stats = _SourceStats.of(separated);
    if (!stats.valid ||
        stats.utf8Bytes > sourceLimit ||
        stats.utf8Bytes > syncLimit ||
        !liveLimits._admitsStats(stats)) {
      return null;
    }
    final check = _backend.parse(separated);
    if (!liveLimits._admitsModel(check) ||
        underlines(check, check.lineOfUtf16(moved))) {
      return null;
    }
    return (source: separated, caret: moved);
  }

  /// The parser identifies a newly typed, bare three-character opener. Pair it
  /// before publication, even if a later existing fence would close it. Source
  /// input and paste retain Markdown's ordinary unclosed-fence meaning.
  ({String source, int caret})? _completeTypedFence(
    String candidate,
    int caret,
    RenderModel model,
  ) {
    final line = model.lineOfUtf16(caret);
    for (final block in model.blocks) {
      if (block.kind != BlockKind.codeBlock ||
          block.flags & 1 == 0 ||
          block.attr != 3 ||
          block.firstLine != line ||
          block.startUtf16 + block.attr != caret ||
          model.codeInfoStart(block.index) != model.codeInfoEnd(block.index)) {
        continue;
      }
      // Preserve quote markers and indentation. A parser-owned item marker
      // or footnote label becomes the indentation that continues it, so
      // subsequent lines stay in the same item, including tab-padded and
      // nested items, or footnote.
      final prefix = _continuationPrefix(
        candidate,
        model,
        line,
        block.startUtf16,
        block.index,
      );
      final fence = candidate.substring(block.startUtf16, caret);
      // An empty info range may follow ASCII padding on the opener line.
      final openerEnd = model.codeInfoStart(block.index);
      final newline = candidate.startsWith('\r\n', openerEnd) ? '\r\n' : '\n';
      final inserted =
          '$newline$prefix$newline$prefix$fence$newline$prefix'
          '${openerEnd == candidate.length ? newline : ''}';
      return (
        source: candidate.replaceRange(openerEnd, openerEnd, inserted),
        caret: openerEnd + newline.length + prefix.length,
      );
    }
    return null;
  }

  bool _replace(int start, int end, String text) {
    final range = FlarkSelection(start, end);
    if (_wholeRange(range.start, range.end)) return _replaceWhole(text);
    var s = _doc.legalize(start.clamp(0, source.length));
    var e = _doc.legalize(end.clamp(0, source.length));
    if (e < s) (s, e) = (e, s);
    if (s == e && text.isEmpty) return false;
    final content = _contentRange(s, e);
    s = content.start;
    e = content.end;
    if (!_supportedRange(s, e)) return false;
    if (text.isEmpty) {
      return _deleteContent(s, e, typing: false);
    }
    // A replacement in a fenced body is literal code, as a paste there is.
    final code = _pasteCode(text, from: s, to: e);
    if (code != null) return code;
    if (text.trim().isEmpty) {
      final expanded = _rangeForEmptying(s, e);
      final heading = _emptySetext(
        expanded.start,
        expanded.end,
        text,
        typing: false,
      );
      if (heading != null) return heading;
      s = expanded.start;
      e = expanded.end;
    }
    // A collapsed replacement places text as typing does.
    final (at, gap) = s == e ? _sequencePlace(_doc.rowAt(s), s, text) : (s, '');
    final normalized = _normalizeInlineEdges(
      at,
      at + e - s,
      '$text$gap',
      at + text.length,
    );
    return _commit(
      normalized.text,
      FlarkSelection.collapsed(normalized.caret),
      typing: false,
      pending: normalized.pending,
      acceptSourceMode: true,
      accept: gap.isEmpty
          ? null
          : (next) => _keepsStructure(next, [
              (s, e, text.length + gap.length),
            ], const {}),
    );
  }

  bool _delete({required bool backward, bool word = false}) {
    final sel = selection;
    if (_wholeRange(sel.start, sel.end)) return _replaceWhole('');
    if (!sel.isCollapsed) {
      final range = _contentRange(sel.start, sel.end);
      if (!_supportedRange(range.start, range.end)) return false;
      return _deleteContent(range.start, range.end, typing: false);
    }
    final pos = _doc.displayOf(sel.extent);
    final row = projection.rows[pos.row];
    final d = pos.offset;
    if (backward ? d == 0 : d >= row.text.length) {
      return backward ? _joinBackward(row) : _joinForward(row);
    }
    // The rendered grapheme's own source bytes, hidden neighbours excluded.
    final int a, b;
    final atomic = _adjacentAtomicSegment(row, d, forward: !backward);
    if (word) {
      a = row.sourceForDisplay(
        backward ? _wordStart(row.text, d) : d,
        anchor: Anchor.after,
      );
      b = row.sourceForDisplay(
        backward ? d : _wordEnd(row.text, d),
        anchor: Anchor.before,
      );
      if (!_supportedRange(a, b)) return false;
    } else if (atomic != null) {
      a = atomic.sourceStart;
      b = atomic.sourceEnd;
    } else if (backward) {
      final g = row.text.substring(0, d).characters.last.length;
      a = row.sourceForDisplay(d - g, anchor: Anchor.after);
      b = row.sourceForDisplay(d, anchor: Anchor.before);
    } else {
      final g = row.text.substring(d).characters.first.length;
      a = row.sourceForDisplay(d, anchor: Anchor.after);
      b = row.sourceForDisplay(d + g, anchor: Anchor.before);
    }
    if (b <= a) return false;
    // A line ending inside inline code is that code's text, painted as a
    // space, so it deletes like any other character of the span below. Any
    // other line ending joins two lines, an undo step of its own; in a fenced
    // body that join deletes literal code below too.
    final join = atomic?.lineBreak == true && atomic!.run < 0;
    if (join && row.fenced && row.displayForSource(b).$1 != atomic.displayEnd) {
      // A code line's break runs on through the next line's prefix, where a
      // tab can show columns of code after the break. Deleting the break
      // deletes that tab, and the columns it shows with it, which the key did
      // not reach, so this join is refused.
      return false;
    }
    if (join && !row.fenced) {
      var start = a;
      // Editable spaces before a hard break remain visible caret positions,
      // but deleting the break removes its entire parser-authenticated marker.
      for (final r in _doc.model.runsOfBlock(row.block)) {
        if (r.kind == RunKind.hardBreak &&
            r.startUtf16 <= a &&
            r.endUtf16 >= a &&
            r.endUtf16 <= b) {
          start = r.startUtf16;
          break;
        }
      }
      return _joinContent(start, b, _checkedKind(row.kind), {row.index});
    }
    // EP1-DELETE-TO-EMPTY: an owner this deletion empties goes with it, and
    // its delimiters wait for the next ordinary character.
    return _deleteContent(a, b, typing: !word && !join);
  }

  bool _deleteContent(int start, int end, {required bool typing}) {
    // A deletion in a fenced body is literal code, as typing there is.
    final code = _pasteCode('', from: start, to: end, typing: typing);
    if (code != null) return code;
    final expanded = _rangeForEmptying(start, end);
    final heading = _emptySetext(
      expanded.start,
      expanded.end,
      '',
      typing: typing,
      pending: expanded.pending,
    );
    if (heading != null) return heading;
    var caret = expanded.start;
    var pending = expanded.pending;
    final previous = _pending;
    if (previous != null &&
        previous.continueAcrossSpaces &&
        selection.isCollapsed &&
        source.substring(expanded.start, expanded.end).trim().isEmpty) {
      pending = previous;
      // Removing the separator returns to the surviving span. Do not place
      // a new delimiter pair directly beside that span's closing syntax.
      for (final anchor in _doc.anchorsAt(caret)) {
        if (_doc.typingContextAt(anchor) == typingContext) {
          caret = anchor;
          pending = null;
          break;
        }
      }
      // The separator can also be the whitespace that led the span after the
      // caret, once its first word was deleted: then return into that span,
      // beside its opening syntax.
      final removed = expanded.end - expanded.start;
      for (final anchor
          in pending == null ? const <int>[] : _doc.anchorsAt(expanded.end)) {
        if (anchor > expanded.end &&
            _doc.typingContextAt(anchor) == typingContext) {
          caret = anchor - removed;
          pending = null;
          break;
        }
      }
    }
    final normalized = _normalizeInlineEdges(
      expanded.start,
      expanded.end,
      '',
      caret,
      pending: pending,
    );
    return _commit(
      normalized.text,
      FlarkSelection.collapsed(normalized.caret),
      typing: typing,
      pending: normalized.pending,
    );
  }

  /// Both inserting and deleting can expose whitespace at a formatting edge.
  /// Move it outside parser-owned emphasis delimiters before the next parse.
  /// The same transaction publishes the visible caret and continuation intent.
  ({String text, int caret, PendingStyle? pending}) _normalizeInlineEdges(
    int start,
    int end,
    String inserted,
    int caret, {
    PendingStyle? pending,
  }) {
    var text = source.replaceRange(start, end, inserted);
    final removed = end - start - inserted.length;
    final intent = _doc.ownersAt(selection.extent).map((o) => o.run).toSet();
    // Deleting the first/last word can expose whitespace at an emphasis edge.
    // Move it outside the authenticated delimiters so surviving words stay
    // styled. Work inside-out; each reordering preserves source length.
    final shared = _doc
        .ownersAt(start)
        .where(
          (o) =>
              end <= o.contentEnd &&
              (o.kind == RunKind.emph ||
                  o.kind == RunKind.strong ||
                  o.kind == RunKind.strike),
        );
    for (final o in shared.toList().reversed) {
      final cs = o.contentStart, ce = o.contentEnd - removed;
      var first = cs, last = ce;
      while (first < last && _isSpace(text, first)) {
        first++;
      }
      while (last > first && _isSpace(text, last - 1)) {
        last--;
      }
      if (first == last || (first == cs && last == ce)) continue;
      final finish = o.end - removed;
      final open = text.substring(o.start, cs),
          close = text.substring(ce, finish);
      text = text.replaceRange(
        o.start,
        finish,
        '${text.substring(cs, first)}$open${text.substring(first, last)}$close${text.substring(last, ce)}',
      );
      var escaped = false;
      if (first > cs && caret >= cs && caret <= first) {
        final at = _doc.displayOf(o.start);
        final shown = projection.rows[at.row].text;
        if (caret < first &&
            at.offset > 0 &&
            shown.codeUnitAt(at.offset - 1) != 0x0A) {
          // Deleting an owner's first word leaves the caret before the
          // whitespace that now leads it, and that whitespace stays visible
          // before the owner. The caret keeps that visible deletion point and
          // the owner's typing intent, so a replacement word is styled and
          // stays apart from the word after it.
          caret = o.start + (caret - cs);
          escaped = true;
        } else {
          // At a line start the moved whitespace is indentation Markdown does
          // not display, and can be outside the row's legal caret spans; after
          // typed whitespace the caret already follows it. Keep the caret in
          // the surviving owner instead of leaving pending delimiters adjacent
          // to that same owner's opening delimiter.
          caret = first;
        }
      } else if (last < ce && caret >= last && caret <= ce) {
        caret += close.length;
        escaped = true;
      }
      if (escaped && intent.contains(o.run)) {
        pending = PendingStyle(
          '$open${pending?.open ?? ''}',
          '${pending?.close ?? ''}$close',
          (pending?.styles ?? 0) | o.style,
          continueAcrossSpaces: true,
        );
      }
    }
    return (text: text, caret: caret, pending: pending);
  }

  /// Expand a visible range through any inline owner that it empties, so its
  /// hidden delimiters cannot be stranded in the source. A setext heading the
  /// range empties is respelled by [_emptySetext].
  ({int start, int end, PendingStyle? pending}) _rangeForEmptying(
    int start,
    int end,
  ) {
    final atomic = expandFlarkAtomicRange(_doc, start, end);
    var s = atomic.start, e = atomic.end, styles = 0, recreate = true;
    for (var grew = true; grew;) {
      grew = false;
      for (final owner in [
        ..._doc.ownersOfContent(s, e),
        ..._doc.ownersAt(s),
        ..._doc.ownersAt(e),
      ]) {
        if (owner.contentStart < s || owner.contentEnd > e) continue;
        if (owner.start >= s && owner.end <= e) continue;
        s = owner.start < s ? owner.start : s;
        e = owner.end > e ? owner.end : e;
        styles |= owner.style;
        if (owner.kind == RunKind.escape) recreate = false;
        grew = true;
      }
    }
    final intent = _doc
        .ownersAt(selection.extent)
        .where((o) => o.start >= s && o.end <= e)
        .toList();
    final pending = recreate && styles != 0 && intent.isNotEmpty
        ? PendingStyle(
            intent.map((o) => source.substring(o.start, o.contentStart)).join(),
            intent.reversed
                .map((o) => source.substring(o.contentEnd, o.end))
                .join(),
            intent.fold(0, (mask, o) => mask | o.style),
          )
        : null;
    return (start: s, end: e, pending: pending);
  }

  /// Range edits may contain complete owners or stay within their content.
  /// Crossing only one content boundary would strand hidden syntax. Until
  /// cross-block transformations have semantics of their own, reject them too.
  bool _supportedRange(int start, int end) {
    if (start == end) return true;
    if (_doc.displayOf(start).row != _doc.displayOf(end).row) return false;
    for (final o in [..._doc.ownersAt(start), ..._doc.ownersAt(end)]) {
      final within = start >= o.contentStart && end <= o.contentEnd;
      final contains =
          (start <= o.start && end >= o.end) ||
          (start == o.contentStart && end == o.contentEnd);
      if (!within && !contains) return false;
    }
    return true;
  }

  // ---------------------------------------------------------- structure

  /// Backspace at a row start: lift the line's innermost prefix, else join
  /// the previous row.
  bool _joinBackward(ProjectedRow row) {
    if (row.kind == RowKind.tableCell) return false;
    if (row.kind == RowKind.heading || _isBareHeading(row)) {
      if (_setHeading(0)) return true;
      // An empty heading whose marker cannot go without changing the block
      // before it (in a list, the emptied item's `- ` would underline the
      // paragraph above) goes with its line instead, as Delete at the end
      // of that paragraph takes it.
      if (row.text.isNotEmpty || row.index == 0 || _lastRejection != null) {
        return false;
      }
      final prev = projection.rows[row.index - 1];
      return prev.kind != RowKind.tableCell &&
          prev.firstLine + prev.lineCount <= row.firstLine &&
          _joinRows(prev, row);
    }
    if (row.fenced && row.text.isEmpty) {
      final block = _doc.model.blockAt(row.block);
      // Only a closed fence's block range is the whole construct. An unclosed
      // one ends at its opening line, so deleting that range leaves any
      // closing delimiter behind as a new unclosed block.
      if (block.flags & 2 != 0) {
        final start = block.startUtf16, end = block.endUtf16;
        // The first row after the block that shows anything: a blank line
        // belongs to whichever container its neighbors give it.
        final following = projection.rows
            .skip(row.index + 1)
            .where((r) => r.kind != RowKind.blank)
            .firstOrNull;
        final after = following == null ? -1 : _firstCaretStart(following);
        return _commit(
          source.replaceRange(start, end, ''),
          FlarkSelection.collapsed(start),
          typing: false,
          acceptSourceMode: true,
          // The blocks around it must stay as they were. A fence at a column
          // outside a list or quote ends that container, and once it is gone
          // what follows can run on into the container instead.
          accept: (next) =>
              (after < start ||
                  _keepsShells(next, following!, after - (end - start))) &&
              _keepsStructure(next, [(start, end, 0)], {row.index}),
        );
      }
    }
    // Code follows its opening fence, and each code line carries its
    // container prefix. Lifting that prefix or joining across the opener
    // would turn the code into prose and leave the closer opening a new
    // block, so the join refuses, like other commands that cross a code
    // block's boundary. Only an empty line or a rule directly above can go.
    if (row.fenced && row.text.isNotEmpty) {
      return row.index > 0 &&
          _removeLineAbove(projection.rows[row.index - 1], row);
    }
    final i = (_doc.model.lineOfUtf16(selection.extent) - row.firstLine).clamp(
      0,
      row.contentStarts.length - 1,
    );
    final line = row.firstLine + i;
    final prefixStart = row.prefixStarts[i],
        contentStart = row.contentStarts[i];
    if (prefixStart >= 0 && prefixStart < contentStart) {
      // A lifted line that would lazily continue the previous paragraph
      // stays a paragraph of its own. A line break and the line's outer
      // container prefix go before it, so it stays in the quote or item it
      // was in. A line that opens an outer container cannot be lazy.
      final prev = row.index > 0 ? projection.rows[row.index - 1] : null;
      final m = _doc.model;
      final separate =
          row.text.isNotEmpty &&
          prev != null &&
          prev.kind == RowKind.paragraph &&
          prev.firstLine + prev.lineCount == row.firstLine &&
          !row.shells.any(
            (shell) =>
                shell.kind != ShellKind.list &&
                m.blockFirstLine(shell.block) == line &&
                m.blockStart(shell.block) < prefixStart,
          );
      final outer = source.substring(m.lineStartUtf16(line), prefixStart);
      final replacement = separate ? '${_lineBreakAt(prefixStart)}$outer' : '';
      final caret = prefixStart + replacement.length;
      return _commitApart(
        row,
        source.replaceRange(prefixStart, contentStart, replacement),
        [(prefixStart, contentStart, replacement.length)],
        FlarkSelection.collapsed(caret),
        prefixStart,
        // The lifted line keeps its kind: lifting `> ` from `> foo` above
        // `---` would make it a setext heading that swallows the rule. Only
        // indented code becomes the paragraph its text reads as, and a line
        // that displays nothing has no kind to keep.
        (next) {
          final kind = next.rowAt(caret).kind;
          return row.text.isEmpty ||
              kind == row.kind ||
              row.kind == RowKind.codeBlock && kind == RowKind.paragraph;
        },
      );
    }
    if (row.index == 0) {
      // Nothing precedes this row to join onto. A thematic break is the one
      // row that displays nothing yet owns a range the crate guarantees is
      // exactly its line's content, so it can go on its own; anything else
      // would be deleting through a range comrak does not pin down.
      if (row.kind != RowKind.thematicBreak) return false;
      // The block after the rule stays in containers of the same kinds, and
      // other rows keep their kinds. A blank line belongs to whichever
      // container its neighbors give it, so the first row that is not one
      // is the block that counts.
      final following = projection.rows
          .skip(1)
          .where((r) => r.kind != RowKind.blank)
          .firstOrNull;
      final after = following == null ? -1 : _firstCaretStart(following);
      final start = row.sourceStart, end = row.sourceEnd;
      if (_commit(
        source.replaceRange(start, end, ''),
        FlarkSelection.collapsed(start),
        typing: false,
        acceptSourceMode: true,
        accept: (next) =>
            (after < 0 ||
                _keepsShells(next, following!, after - (end - start))) &&
            _keepsStructure(
              next,
              [(start, end, 0)],
              {row.index},
              movesText: false,
            ),
      )) {
        return true;
      }
      // A rule that was all of a list item leaves that item empty, and an
      // empty item's content starts one column past its marker rather than
      // where the rule did, so a block too shallow for the rule's item can
      // move into it. The rule's whole line goes instead, marker and all, as
      // Delete on the rule takes it.
      return following?.index == 1 &&
          _lastRejection == null &&
          _removeLineAbove(row, following!);
    }
    final prev = projection.rows[row.index - 1];
    if (prev.kind == RowKind.tableCell) return false;
    if (prev.firstLine + prev.lineCount > row.firstLine) return false;
    return _joinRows(prev, row);
  }

  /// Delete at a row end: join the next row onto this one.
  bool _joinForward(ProjectedRow row) {
    if (row.kind == RowKind.tableCell) return false;
    if (row.index + 1 >= projection.rows.length) return false;
    final next = projection.rows[row.index + 1];
    if (next.kind == RowKind.tableCell) return false;
    if (next.firstLine < row.firstLine + row.lineCount) return false;
    // As in _joinBackward, no join crosses the opening fence of code.
    if (next.fenced && next.text.isNotEmpty) {
      return _removeLineAbove(row, next);
    }
    return _joinRows(row, next);
  }

  /// Join the start of [right] onto the end of [left], the row before it.
  bool _joinRows(ProjectedRow left, ProjectedRow right) {
    final to = _firstCaretStart(right);
    if (to < 0) return false;
    final rows = {left.index, right.index};
    if (left.fenced) {
      // A fence's delimiter line never takes text: on a closer the text stops
      // it closing, so everything after it becomes code, and on a fence that
      // displays nothing it becomes an info string the editor never shows.
      if (right.text.isNotEmpty) return false;
      // An empty fence after it goes whole instead of joining its closer.
      if (right.fenced) {
        final from = _lastCaretEnd(left);
        final end = projection.lineContentEnd(
          right.firstLine + right.lineCount - 1,
        );
        return from >= 0 &&
            end > from &&
            _joinContent(from, end, null, rows, movesText: false);
      }
    }
    // A row that displays nothing gives way to the one joined onto it.
    bool empty(ProjectedRow row) =>
        row.kind == RowKind.blank || row.kind == RowKind.thematicBreak;
    // An empty line or a rule before a row with text goes whole, so the row
    // keeps its own prefix and markup: joining onto the line would delete a
    // heading's `#` or a quote's `>` instead of the gap, and text joined
    // after a rule would paint its markup. Backspace after a rule, or Delete
    // on it, removes the rule's line, and the next row takes its place in
    // its own containers. A row that displays nothing joins as usual, which
    // removes that row.
    if (empty(left) && (right.text.isNotEmpty || right.fenced)) {
      if (_removeLineAbove(left, right)) return true;
      // A rule that opens an item takes the item's marker with its line, so
      // the next row joins the rule's line instead, staying in that item.
      return left.kind == RowKind.thematicBreak &&
          _lastRejection == null &&
          _joinContent(
            left.sourceStart,
            to,
            _checkedKind(right.kind),
            rows,
            following: right,
          );
    }
    final from = _lastCaretEnd(left);
    if (from < 0 || to <= from) return false;
    if (right.text.isNotEmpty &&
        (_headingTrail(left) != null || _headingTrail(right) != null)) {
      return _joinHeadingText(left, right, from, to);
    }
    // Joining the line break before a fence that displays nothing turns its
    // delimiter line into text: the way to delete one that has no body.
    final bodyless = right.fenced && right.contentStarts.every((s) => s < 0);
    final keepsLeft = !empty(left) || empty(right);
    return _joinContent(
      from,
      to,
      _checkedKind(keepsLeft ? left.kind : right.kind),
      rows,
      // [from] ends [left]'s last line, which can be hidden markup such as a
      // setext underline or a closing fence, where the caret's row would be
      // the one after it. The caret stays at the end of [left]'s text, and
      // the kind [left] keeps is read there.
      at: keepsLeft ? _lastContentEnd(left) : null,
      // Removing a row that displays nothing moves no text, so what the
      // parser makes of the lines left is the document asked for: an empty
      // last item stops being one without its line break. Refusing would
      // strand Backspace at the end of the document.
      movesText: !empty(right) || right.fenced,
      // It does bring the row after it up against [left], and that row must
      // stay in its own containers: without the gap, a paragraph after a
      // quote or list item would read on lazily inside it, the result
      // [_removeLineAbove] refuses when Backspace starts at that paragraph.
      following:
          right.text.isEmpty &&
              !right.fenced &&
              right.index + 1 < projection.rows.length
          ? projection.rows[right.index + 1]
          : null,
      shown: bodyless
          ? (right.sourceStart, projection.lineContentEnd(right.firstLine))
          : null,
    );
  }

  /// The row kind a join must keep, when it can change: a paragraph that
  /// gains a setext underline becomes a heading, and a heading that loses one
  /// a paragraph. Code, rules and blank rows keep their markup lines, and a
  /// trailing blank line may change hands without changing anything shown.
  static RowKind? _checkedKind(RowKind kind) =>
      kind == RowKind.paragraph || kind == RowKind.heading ? kind : null;

  /// Remove [above], an empty line or a rule directly over [row], as a
  /// whole line, so [row] keeps its own container prefix and markup. Code is
  /// the one row no join may reach: its opening fence would go. The result
  /// is refused when [row] changes kind or moves out of its containers, when
  /// code changes, or when [_keepsStructure] fails.
  bool _removeLineAbove(ProjectedRow above, ProjectedRow row) {
    if ((above.kind != RowKind.blank && above.kind != RowKind.thematicBreak) ||
        above.firstLine + above.lineCount != row.firstLine) {
      return false;
    }
    final m = _doc.model;
    final start = _lineStart(source, m, above.firstLine);
    final end = m.lineStartUtf16(row.firstLine);
    final caret = _firstCaretStart(row) - (end - start);
    return _commit(
      source.replaceRange(start, end, ''),
      FlarkSelection.collapsed(caret),
      typing: false,
      acceptSourceMode: true,
      accept: (next) {
        final now = next.rowAt(caret);
        if (now.kind != row.kind ||
            row.fenced && (!now.fenced || now.text != row.text) ||
            !_keepsShells(next, row, caret)) {
          return false;
        }
        // Without the gap, [row]'s text can run on from the paragraph above
        // and pair delimiters with it, painting markup that closed a span.
        return _keepsStructure(
          next,
          [(start, end, 0)],
          {above.index, row.index},
          movesText: row.text.isNotEmpty,
        );
      },
    );
  }

  /// Whether [row] of the current projection, now at [offset] of [next],
  /// sits in containers of the same kinds: a join that removes an empty line
  /// must not move the next block into a quote or list item, or out of one.
  static bool _keepsShells(FlarkDocument next, ProjectedRow row, int offset) {
    final now = next.rowAt(offset).shells;
    if (now.length != row.shells.length) return false;
    for (var i = 0; i < now.length; i++) {
      if (now[i].kind != row.shells[i].kind) return false;
    }
    return true;
  }

  /// The markup a heading keeps after its content on its last line: a setext
  /// underline with the line break before it, or an ATX closing sequence.
  /// Null when the heading's last line ends with its content.
  (int, int)? _headingTrail(ProjectedRow row) {
    if (row.kind != RowKind.heading) return null;
    final end = _lastContentEnd(row);
    final lineEnd = projection.lineContentEnd(
      row.firstLine + row.lineCount - 1,
    );
    return end >= 0 && lineEnd > end ? (end, lineEnd) : null;
  }

  /// A setext heading cannot be empty: its underline would be painted (`===`)
  /// or read as a rule (`---`), and taken with the text it could leave a list
  /// item empty, whose content starts one column past its marker, moving the
  /// blocks after it. An edit of [start]..[end] that leaves none of the
  /// heading's text instead respells it as an empty ATX heading of its level
  /// in the same containers, as a level change does, followed by [text]. The
  /// caret follows [text], where typing goes on in the heading unless [text]
  /// breaks the line. Null when the edit is not in a setext heading or leaves
  /// some of its text; false when the parser would move or change another
  /// block.
  bool? _emptySetext(
    int start,
    int end,
    String text, {
    required bool typing,
    PendingStyle? pending,
  }) {
    final row = _doc.rowAt(start), trail = _headingTrail(row);
    final first = _firstCaretStart(row), m = _doc.model;
    // The text left is read in the source, not in what is shown: only the
    // spaces or tabs Markdown strips count as none, so an empty-alt image,
    // which shows nothing, keeps the heading. An ATX closing sequence shares
    // the text's line and may close an empty heading; only an underline sits
    // on a line of its own.
    if (trail == null ||
        first > start ||
        end > trail.$1 ||
        m.lineOfUtf16(trail.$1) == m.lineOfUtf16(trail.$2) ||
        !'${source.substring(first, start)}${source.substring(end, trail.$1)}'
            .codeUnits
            .every((unit) => unit == 0x20 || unit == 0x09)) {
      return null;
    }
    final marker = '${'#' * row.headingLevel} ', replaced = '$marker$text';
    final origin = first + marker.length;
    // The first row after the heading that shows anything: a blank line
    // belongs to whichever container its neighbors give it.
    final following = projection.rows
        .skip(row.index + 1)
        .where((r) => r.kind != RowKind.blank)
        .firstOrNull;
    final after = following == null
        ? -1
        : _firstCaretStart(following) + replaced.length - (trail.$2 - first);
    return _commit(
      source.replaceRange(first, trail.$2, replaced),
      FlarkSelection.collapsed(origin + text.length),
      typing: typing,
      pending: pending,
      acceptSourceMode: true,
      accept: (next) {
        final now = next.rowAt(origin);
        return now.kind == RowKind.heading &&
            now.headingLevel == row.headingLevel &&
            now.text.isEmpty &&
            _keepsShells(next, row, origin) &&
            (following == null || _keepsShells(next, following, after)) &&
            _keepsStructure(
              next,
              [(first, trail.$2, replaced.length)],
              {row.index},
              movesText: false,
            );
      },
    );
  }

  /// Join [right]'s first line onto [left] when a heading's markup trails
  /// either. The markup after [left]'s content moves after the joined text,
  /// where it still ends the heading, instead of being painted between the
  /// two; an ATX closing sequence after [right]'s text goes with the rest of
  /// [right]'s markup. [from] is the end of [left]'s last line and [to] where
  /// [right]'s text starts.
  bool _joinHeadingText(
    ProjectedRow left,
    ProjectedRow right,
    int from,
    int to,
  ) {
    var kept = _headingTrail(left);
    // An empty heading's text starts where its closing sequence does: the
    // spaces between them separate the opening marker. Text joined there
    // goes where typed text would: before the last of several spaces, which
    // the sequence keeps, or after a single one with a space of its own
    // before the sequence, as text in a heading has. Run into the text, the
    // sequence would read as more of it.
    if (kept != null && !_isSpace(source, kept.$1)) {
      kept = (_emptyHeadingText(kept.$1), kept.$2);
    }
    final dropped = right.lineCount == 1 ? _headingTrail(right) : null;
    final rightEnd = projection.lineContentEnd(_doc.model.lineOfUtf16(to));
    final textEnd = dropped?.$1 ?? rightEnd, lineEnd = dropped?.$2 ?? rightEnd;
    if (rightEnd < to || textEnd < to) return false;
    final (a, b) = _joinedOwners(kept?.$1 ?? from, to);
    if (b > textEnd) return false;
    final markup = kept == null ? '' : source.substring(kept.$1, kept.$2);
    final gap = markup.isEmpty || _isSpace(markup, 0) ? '' : ' ';
    final edits = [(a, b, 0), (textEnd, lineEnd, gap.length + markup.length)];
    // The text moves ahead of [left]'s markup, which sorted edits can only
    // count as new text, and the check passes over new text. Mapped back to
    // where it was, the markup is checked like the text: hidden source the
    // join must not paint.
    int old(int offset) {
      final o = offset - a, text = textEnd - b;
      if (o < 0) return offset;
      if (o < text) return b + o;
      final m = o - text - gap.length;
      if (m < 0) return -1;
      return m < markup.length ? kept!.$1 + m : lineEnd + m - markup.length;
    }

    return _commit(
      '${source.substring(0, a)}${source.substring(b, textEnd)}$gap$markup'
      '${source.substring(lineEnd)}',
      FlarkSelection.collapsed(a),
      typing: false,
      acceptSourceMode: true,
      accept: (next) =>
          next.rowAt(a).kind == left.kind &&
          _keepsStructure(next, edits, {
            left.index,
            right.index,
          }, movesText: false) &&
          !_revealsHiddenText(next, old),
    );
  }

  /// Delete [from]..[to], the line break and prefixes a join of [rows]
  /// removes, with the caret at [at] when that precedes the join and at
  /// [from] otherwise. The result is kept only when the caret's row is still
  /// of [kind] (when given), when [following], a row after the join, stays
  /// in containers of the same kinds, and when [_keepsStructure] holds.
  bool _joinContent(
    int from,
    int to,
    RowKind? kind,
    Set<int> rows, {
    int? at,
    bool movesText = true,
    ProjectedRow? following,
    (int, int)? shown,
  }) {
    (from, to) = _joinedOwners(from, to);
    final caret = at != null && at >= 0 && at < from ? at : from;
    final after = following == null ? -1 : _firstCaretStart(following);
    return _commit(
      source.replaceRange(from, to, ''),
      FlarkSelection.collapsed(caret),
      typing: false,
      acceptSourceMode: true,
      accept: (next) =>
          (kind == null || next.rowAt(caret).kind == kind) &&
          (after < 0 || _keepsShells(next, following!, after - (to - from))) &&
          _keepsStructure(
            next,
            [(from, to, 0)],
            rows,
            movesText: movesText,
            shown: shown,
          ),
    );
  }

  /// Whether [next], a join or lift of [rows] made by [edits] (sorted
  /// replacements of current source: start, end, and the length of what
  /// replaced it), keeps what the user sees elsewhere. A reparse can
  /// otherwise turn it into a structural change. Every other row keeps its
  /// kind: removing the blank line between `a` and `---` makes `a` a setext
  /// heading that swallows the rule, and lifting an item's marker can turn
  /// the item's later blocks into indented code. When the edit [movesText]
  /// onto another line, nothing the projection hides is painted either: text
  /// after a closing fence or sequence paints it, and so do the brackets of
  /// references to a definition the edit merged into a paragraph. [shown] is
  /// hidden source the edit may paint.
  bool _keepsStructure(
    FlarkDocument next,
    List<(int, int, int)> edits,
    Set<int> rows, {
    bool movesText = true,
    (int, int)? shown,
  }) {
    // A current offset in [next]'s source, or -1 inside a replacement.
    int forward(int offset) {
      var shift = 0;
      for (final (start, end, length) in edits) {
        if (offset <= start) break;
        if (offset < end) return -1;
        shift += length - (end - start);
      }
      return offset + shift;
    }

    // An offset of [next]'s source in the current one, or -1 in new text.
    int back(int offset) {
      var shift = 0;
      for (final (start, end, length) in edits) {
        if (offset < start + shift) break;
        if (offset < start + shift + length) return -1;
        shift += length - (end - start);
      }
      return offset - shift;
    }

    // Both projections list rows in source order, so one walk pairs each row
    // with the row that now holds its start. A start past a row's end lies in
    // the markup before the next row, which then holds it.
    final now = next.projection.rows;
    var j = 0;
    for (final row in projection.rows) {
      if (row.kind == RowKind.blank || rows.contains(row.index)) continue;
      final at = forward(row.sourceStart);
      if (at < 0) continue;
      while (j + 1 < now.length && now[j + 1].sourceStart <= at) {
        j++;
      }
      final holder = at > now[j].sourceEnd && j + 1 < now.length
          ? now[j + 1]
          : now[j];
      if (holder.kind != row.kind) return false;
    }
    return !movesText || !_revealsHiddenText(next, back, shown: shown);
  }

  /// Joining physical lines also joins matching boundary owners. Leaving
  /// adjacent closing/opening runs (for example **a****b**) exposes markers.
  (int, int) _joinedOwners(int from, int to) {
    final left = _doc.ownersAt(_doc.anchorsAt(from).first);
    final right = _doc.ownersAt(_doc.anchorsAt(to).last);
    if (left.isEmpty || left.length != right.length) return (from, to);
    for (var i = 0; i < left.length; i++) {
      final a = left[i], b = right[i];
      if (a.end > from ||
          b.start < to ||
          a.kind != b.kind ||
          source.substring(a.start, a.contentStart) !=
              source.substring(b.start, b.contentStart) ||
          source.substring(a.contentEnd, a.end) !=
              source.substring(b.contentEnd, b.end)) {
        return (from, to);
      }
    }
    return (left.last.contentEnd, right.last.contentStart);
  }

  /// Whether [next] paints a non-whitespace character of the current source
  /// that the current projection hides. [old] maps an offset in [next]'s
  /// source to the current one; [shown] is hidden source allowed to appear.
  bool _revealsHiddenText(
    FlarkDocument next,
    int Function(int) old, {
    (int, int)? shown,
  }) {
    // Painted source as prefix counts, so a segment that only moved is
    // cleared in constant time and only text near the edit is read.
    final depth = Int32List(source.length + 1);
    void paint(int start, int end) {
      depth[start]++;
      depth[end]--;
    }

    for (final row in projection.rows) {
      for (final segment in row.segments) {
        if (!segment.lineBreak) paint(segment.sourceStart, segment.sourceEnd);
      }
    }
    if (shown != null) paint(shown.$1, shown.$2);
    final painted = Int32List(source.length + 1);
    for (var i = 0, open = 0; i < source.length; i++) {
      open += depth[i];
      painted[i + 1] = painted[i] + (open > 0 ? 1 : 0);
    }
    for (final row in next.projection.rows) {
      for (final segment in row.segments) {
        final a = segment.sourceStart, b = segment.sourceEnd;
        if (segment.lineBreak || a >= b) continue;
        final first = old(a), last = old(b - 1);
        if (first >= 0 &&
            last - first == b - 1 - a &&
            painted[last + 1] - painted[first] == b - a) {
          continue;
        }
        for (var o = a; o < b; o++) {
          if (_isSpace(next.source, o)) continue;
          final at = old(o);
          if (at >= 0 && at < source.length && painted[at + 1] == painted[at]) {
            return true;
          }
        }
      }
    }
    return false;
  }

  /// Where a join from the following row lands: the end of this row's last
  /// physical line, not of its last caret span. A closing fence and a setext
  /// underline hold no caret, and joining from the content before them would
  /// swallow the markup between — one Backspace erasing a whole `===` line, or
  /// splicing the next row's text into a fence's info string, where it stops
  /// being displayed.
  int _lastCaretEnd(ProjectedRow row) {
    final last = row.firstLine + row.lineCount - 1;
    if (last >= 0 && last < _doc.model.lineCount) {
      final end = projection.lineContentEnd(last);
      if (end >= 0) return end;
    }
    return _lastContentEnd(row);
  }

  /// The end of [row]'s last caret span, or -1 when it has none.
  static int _lastContentEnd(ProjectedRow row) {
    for (var i = row.contentEnds.length - 1; i >= 0; i--) {
      if (row.contentEnds[i] >= 0) return row.contentEnds[i];
    }
    return -1;
  }

  /// Where a join onto the previous row starts. A fence that displays nothing
  /// has no content record, but its markup begins at its own source start, so
  /// a join takes only the line break before it — never the fence itself, as
  /// reaching for its source end would.
  static int _firstCaretStart(ProjectedRow row) {
    for (final s in row.contentStarts) {
      if (s >= 0) return s;
    }
    return row.fenced ? row.sourceStart : -1;
  }

  bool _newline(bool paragraph) {
    final sel = selection;
    final nl = _lineBreakAt(sel.start);
    if (_wholeRange(sel.start, sel.end)) {
      return _replaceWhole(paragraph ? '$nl$nl' : nl);
    }
    if (!sel.isCollapsed) {
      final range = _contentRange(sel.start, sel.end);
      if (!_supportedRange(range.start, range.end) ||
          _doc.rowAt(range.start).kind == RowKind.tableCell) {
        return false;
      }
      final row = _doc.rowAt(range.start);
      if (row.kind == RowKind.codeBlock) {
        return _codeNewline(row, range.start, range.end);
      }
      return _splitRow(row, range.start, range.end, paragraph ? '$nl$nl' : nl);
    }
    final caret = sel.extent;
    final pos = _doc.caretPosition;
    final row = projection.rows[pos.row];
    if (row.kind == RowKind.tableCell) return _returnFromTable(row);
    final line = _doc.model.lineOfUtf16(caret);
    final i = (line - row.firstLine).clamp(0, row.contentStarts.length - 1);
    String text;
    if (row.kind == RowKind.codeBlock) {
      if (!paragraph) {
        final exited = _exitCodeOnBlankLine(row);
        if (exited != null) return exited;
      }
      return _codeNewline(row, caret, caret);
    } else if (row.shells.isNotEmpty) {
      final prefixStart = row.prefixStarts[i],
          contentStart = row.contentStarts[i];
      // Return on an empty container line exits the container. An empty
      // heading is not that line: its own markup is part of the prefix, so
      // exiting would delete the heading with the container marker.
      if (row.text.isEmpty &&
          row.kind != RowKind.heading &&
          prefixStart >= 0 &&
          prefixStart < contentStart) {
        final outer = source.substring(
          _doc.model.lineStartUtf16(line),
          prefixStart,
        );
        final replacement = line > 0 ? '$nl$outer' : '';
        return _commit(
          source.replaceRange(prefixStart, contentStart, replacement),
          FlarkSelection.collapsed(prefixStart + replacement.length),
          typing: false,
        );
      }
      final inner = row.shells.last;
      if (inner.kind == ShellKind.item) {
        text = '$nl${_nextMarker(inner)}';
      } else {
        // The new line continues the quote or footnote, so it takes the
        // line's container prefix: never the markers of the items and
        // definitions that open on this line, which would open new ones, nor
        // a heading's own marker, which belongs to the heading.
        final m = _doc.model, block = row.block >= 0 ? row.block : inner.block;
        final end =
            row.block >= 0 &&
                m.blockFirstLine(block) == line &&
                m.blockStart(block) < contentStart
            ? m.blockStart(block)
            : contentStart;
        text = '$nl${_continuationPrefix(source, m, line, end, block)}';
      }
    } else {
      text = paragraph && row.kind == RowKind.paragraph ? '$nl$nl' : nl;
    }
    return _splitRow(row, caret, caret, text);
  }

  /// Return from [start] to [end] in [row]. A heading's underline or closing
  /// sequence stays with the part of the heading before the split, where it
  /// still ends the heading, instead of underlining the new line or being
  /// painted on it. A split that starts on an earlier line of a multi-line
  /// setext heading, or at the start of its last line, keeps the text after
  /// it above the underline, so the plain split serves.
  bool _splitRow(ProjectedRow row, int start, int end, String separator) {
    final trail = _headingTrail(row);
    if (trail == null) return _splitInline(start, end, separator);
    final m = _doc.model;
    final line = m.lineOfUtf16(trail.$1);
    if (m.lineOfUtf16(end) != line) return _splitInline(start, end, separator);
    final first = m.lineOfUtf16(start) - row.firstLine;
    // Text on the split's first line before it, and text after it. Display
    // offsets count what is shown, so hidden delimiters are neither.
    final before =
        row.displayForSource(start).$1 >
        row.displayForSource(row.contentStarts[first]).$1;
    final after = row.displayForSource(end).$1 < row.text.length;
    if (m.lineOfUtf16(trail.$2) == line ||
        before && first == line - row.firstLine) {
      return _splitInline(start, end, separator, keep: trail);
    }
    if (after) return _splitInline(start, end, separator);
    // The split takes the last line's text to its end. Text before it on an
    // earlier line keeps the underline after it. With no text before, from
    // the heading's start, the split leaves the heading empty, so it becomes
    // the empty heading deleting its text leaves, before the line break.
    // From the start of a later line, the lines before it remain and the
    // underline would have to move up to them, which a split cannot do
    // faithfully, so the edit is refused.
    if (before) return _splitInline(start, end, separator, keep: trail);
    return first == 0 &&
        (_emptySetext(
              row.contentStarts[0],
              trail.$1,
              separator,
              typing: false,
            ) ??
            false);
  }

  bool _returnFromTable(ProjectedRow row) {
    for (var i = row.index + 1; i < projection.rows.length; i++) {
      final next = projection.rows[i];
      if (next.kind != RowKind.tableCell || next.tableBlock != row.tableBlock) {
        break;
      }
      if (next.column == row.column && next.firstLine > row.firstLine) {
        return _place(next.index, 0, true, false);
      }
    }
    // Keep a blank separator after the table even when a trailing gap already
    // exists. Typing into that gap alone would make it another table row.
    if (row.shells.isNotEmpty) return false;
    final end = _doc.model.blockEnd(row.tableBlock);
    final nl = _lineBreakAt(end);
    return _commit(
      source.replaceRange(end, end, '$nl$nl'),
      FlarkSelection.collapsed(end + 2 * nl.length),
      typing: false,
    );
  }

  /// Split parser-owned spans with the block. Move a terminal break outside
  /// closing syntax; at an interior split close and reopen the nonempty parts.
  /// No empty delimiter pair is ever published as an intermediate document.
  /// [keep] is heading markup after the split that stays with the first part.
  bool _splitInline(int start, int end, String separator, {(int, int)? keep}) {
    var from = start, to = end;
    final shared = _doc
        .ownersAt(start)
        .where((o) => end <= o.contentEnd)
        .toList();
    final left = <String>[], right = <String>[];
    var leftSpace = '', rightSpace = '';
    for (var grew = true; grew;) {
      grew = false;
      for (final o in shared.reversed) {
        if (from == o.contentStart && o.start < from) {
          from = o.start;
          grew = true;
        }
        if (to == o.contentEnd && o.end > to) {
          to = o.end;
          grew = true;
        }
      }
      // Emphasis delimiters cannot close after or open before whitespace.
      // Keep those bytes adjacent to the break, outside the split owners.
      if (shared.isNotEmpty && shared.last.kind != RunKind.code) {
        while (from > shared.first.contentStart && _isSpace(source, from - 1)) {
          leftSpace = source.substring(from - 1, from) + leftSpace;
          from--;
          grew = true;
        }
        while (to < shared.first.contentEnd && _isSpace(source, to)) {
          rightSpace += source.substring(to, to + 1);
          to++;
          grew = true;
        }
      }
    }
    for (final o in shared) {
      if (from > o.contentStart) {
        left.insert(0, source.substring(o.contentEnd, o.end));
      }
      if (to < o.contentEnd) {
        right.add(source.substring(o.start, o.contentStart));
      }
    }
    final head = '${left.join()}$leftSpace';
    final tail = '$separator$rightSpace${right.join()}';
    if (keep == null || keep.$1 < to) {
      return _commit(
        source.replaceRange(from, to, '$head$tail'),
        FlarkSelection.collapsed(from + head.length + tail.length),
        typing: false,
      );
    }
    final (markupStart, markupEnd) = keep;
    final markup = source.substring(markupStart, markupEnd);
    return _commit(
      '${source.substring(0, from)}$head$markup$tail'
      '${source.substring(to, markupStart)}${source.substring(markupEnd)}',
      FlarkSelection.collapsed(
        from + head.length + markup.length + tail.length,
      ),
      typing: false,
      acceptSourceMode: true,
      accept: (next) => next.rowAt(from).kind == RowKind.heading,
    );
  }

  /// The marker line for the item after [item]: the same outer prefixes,
  /// the next number for ordered lists, an unchecked box for tasks.
  String _nextMarker(Shell item) {
    final m = _doc.model;
    final itemLine = m.blockFirstLine(item.block),
        itemStart = m.blockStart(item.block);
    // The parser converts column padding (including partially consumed tabs)
    // to an exact source endpoint before the optional task checkbox.
    final markerEnd = m.itemMarkerEnd(item.block);
    final outer = _continuationPrefix(
      source,
      m,
      itemLine,
      itemStart,
      item.block,
    );
    var marker = source.substring(itemStart, markerEnd);
    if (item.ordered) {
      final delimiter = marker.trimRight();
      marker =
          '${item.start + item.itemIndex + 1}${delimiter.substring(delimiter.length - 1)} ';
    } else if (marker.trimRight() == marker) {
      marker = '$marker ';
    }
    if (item.task) marker = '$marker[ ] ';
    return outer + marker;
  }

  /// Indent nests the caret's item under its previous sibling by that
  /// sibling's content offset; Outdent removes the parent's. Every line of
  /// the item shifts at the item's own column.
  bool _shiftItem({required bool outdent}) {
    final row = _doc.caretRow;
    final shells = row.shells;
    var idx = -1;
    for (var i = shells.length - 1; i >= 0; i--) {
      if (shells[i].kind == ShellKind.item) {
        idx = i;
        break;
      }
    }
    if (idx < 0) return false;
    final item = shells[idx];
    final m = _doc.model;
    final list = m.blockParent(item.block);
    final first = m.blockFirstLine(item.block),
        n = m.blockLineCount(item.block);
    final column = m.blockStart(item.block) - m.lineStartUtf16(first);
    int width;
    if (!outdent) {
      var prev = -1;
      for (var b = list + 1; b < item.block; b++) {
        if (m.blockParent(b) == list) prev = b;
      }
      if (prev < 0) return false;
      width = m.blockAttr(prev);
    } else {
      Shell? parent;
      for (var i = idx - 1; i >= 0; i--) {
        if (shells[i].kind == ShellKind.item) {
          parent = shells[i];
          break;
        }
      }
      if (parent == null) return false;
      width = m.blockAttr(parent.block);
    }
    final edits = <(int, int, String)>[];
    for (var l = first; l < first + n; l++) {
      final ls = m.lineStartUtf16(l), at = ls + column;
      var le = l + 1 < m.lineCount ? m.lineStartUtf16(l + 1) : source.length;
      while (le > ls && source.codeUnitAt(le - 1) == 0x0A) {
        le--;
      }
      if (at > le) continue;
      if (!outdent) {
        edits.add((at, at, ' ' * width));
        // An ordered item nested under its sibling starts a new list at 1.
        if (l == first && item.ordered) {
          var k = 0;
          while (at + k < le &&
              source.codeUnitAt(at + k) >= 0x30 &&
              source.codeUnitAt(at + k) <= 0x39) {
            k++;
          }
          if (k > 0) edits.add((at, at + k, '1'));
        }
        continue;
      }
      var k = 0;
      while (k < width &&
          at - k > ls &&
          source.codeUnitAt(at - k - 1) == 0x20) {
        k++;
      }
      if (k > 0) edits.add((at - k, at, ''));
    }
    if (edits.isEmpty) return false;
    final (s, map) = _edited(edits);
    return _commit(
      s,
      FlarkSelection(map(selection.base), map(selection.extent)),
      typing: false,
    );
  }

  /// Apply sorted, non-overlapping edits; returns the new source and a map
  /// from old offsets to new ones.
  (String, int Function(int)) _edited(List<(int, int, String)> edits) {
    final out = StringBuffer();
    var last = 0;
    for (final (a, b, text) in edits) {
      out.write(source.substring(last, a));
      out.write(text);
      last = b;
    }
    out.write(source.substring(last));
    int map(int o) {
      var d = 0;
      for (final (a, b, text) in edits) {
        if (o < a) break;
        if (o < b) return a + d;
        d += text.length - (b - a);
      }
      return o + d;
    }

    return (out.toString(), map);
  }

  bool _toggleTask() {
    for (final sh in _doc.rowAt(selection.extent).shells.reversed) {
      if (sh.kind != ShellKind.item || !sh.task) continue;
      return _commit(
        source.replaceRange(
          sh.checkboxStart + 1,
          sh.checkboxEnd - 1,
          sh.checked ? ' ' : 'x',
        ),
        selection,
        typing: false,
      );
    }
    return false;
  }

  /// A bare `#` is projected as authoring text, but it is still the parser's
  /// heading: a level command has to replace that marker, not prepend to it.
  bool _isBareHeading(ProjectedRow row) =>
      row.kind == RowKind.paragraph &&
      row.block >= 0 &&
      _doc.model.blockKind(row.block) == BlockKind.heading;

  bool _setHeading(int level) {
    if (level < 0 || level > 6) return false;
    final row = _doc.caretRow;
    if (row.kind == RowKind.blank && selection.isCollapsed) {
      return level > 0 && _insert('${'#' * level} ', typing: false);
    }
    if (row.kind != RowKind.paragraph && row.kind != RowKind.heading) {
      return false;
    }
    // A heading already at [level] needs nothing, however it is spelled:
    // rewriting a setext or closed heading as plain ATX would respell source
    // the user wrote and record an undo step that changes nothing shown.
    if (row.kind == RowKind.heading && row.headingLevel == level) {
      _inert = true;
      return false;
    }
    final heading = row.kind == RowKind.heading || _isBareHeading(row);
    if (level > 0 &&
        heading &&
        row.contentStarts.where((s) => s >= 0).length > 1) {
      return false;
    }
    final m = _doc.model;
    final blockStart = m.blockStart(row.block),
        blockEnd = m.blockEnd(row.block);
    final prefix = level == 0 ? '' : '${'#' * level} ';
    var s = source;
    if (row.kind == RowKind.heading && blockEnd > row.sourceEnd) {
      s = s.replaceRange(row.sourceEnd, blockEnd, '');
    }
    // The bare marker is the row's own text, so it is what the prefix replaces.
    final contentStart = _isBareHeading(row) ? row.sourceEnd : row.sourceStart;
    s = s.replaceRange(blockStart, contentStart, prefix);
    final shift = prefix.length - (contentStart - blockStart);
    int move(int o) => o >= contentStart ? o + shift : o;
    return _commitApart(
      row,
      s,
      [
        (blockStart, contentStart, prefix.length),
        if (row.kind == RowKind.heading && blockEnd > row.sourceEnd)
          (row.sourceEnd, blockEnd, 0),
      ],
      FlarkSelection(move(selection.base), move(selection.extent)),
      blockStart,
      (next) =>
          (next.rowAt(move(selection.extent)).kind == RowKind.heading) ==
          (level > 0),
    );
  }

  /// Commit [candidate], which made [edits] to the current source in [row]
  /// (a lifted container marker, or a heading's markup), when [accept]
  /// holds and [_keepsStructure] does: rows elsewhere keep their kinds and
  /// no hidden markup is painted. The block after the row can instead join
  /// it lazily: `1. a` lifted above `2. b` would read as the paragraph
  /// `a 2. b`, and `### a` turned into a paragraph above indented code would
  /// absorb the code. A blank line after the row keeps that block apart when
  /// the parser agrees; it carries the row's container prefix up to
  /// [prefixEnd], without the markers of containers that open on that line.
  /// Otherwise the edit is refused.
  bool _commitApart(
    ProjectedRow row,
    String candidate,
    List<(int, int, int)> edits,
    FlarkSelection selected,
    int prefixEnd,
    bool Function(FlarkDocument) accept,
  ) {
    bool keeps(FlarkDocument next, List<(int, int, int)> edits) =>
        accept(next) &&
        _keepsStructure(next, edits, {
          row.index,
        }, movesText: row.text.isNotEmpty);
    if (_commit(
      candidate,
      selected,
      typing: false,
      acceptSourceMode: true,
      accept: (next) => keeps(next, edits),
    )) {
      return true;
    }
    if (_inert || _lastRejection != null || row.text.isEmpty || row.block < 0) {
      return false;
    }
    final m = _doc.model;
    final last = row.firstLine + row.lineCount - 1;
    final rowEnd = projection.lineContentEnd(last);
    if (rowEnd < edits.last.$2 || last + 1 >= m.lineCount) return false;
    var at = rowEnd;
    for (final (start, end, length) in edits) {
      at += length - (end - start);
    }
    final outer = _continuationPrefix(
      source,
      m,
      m.lineOfUtf16(prefixEnd),
      prefixEnd,
      row.block,
    );
    final blank = '${_lineBreakAt(rowEnd)}${outer.trimRight()}';
    int after(int offset) => offset > at ? offset + blank.length : offset;
    return _commit(
      candidate.replaceRange(at, at, blank),
      FlarkSelection(after(selected.base), after(selected.extent)),
      typing: false,
      acceptSourceMode: true,
      accept: (next) => keeps(next, [...edits, (rowEnd, rowEnd, blank.length)]),
    );
  }

  bool _toggleCaretStyle(int style) {
    final sel = selection;
    final caret = sel.extent;
    // At an edge of an owner, step across its delimiter: out when inside,
    // in when outside. Strictly inside, unwrap it.
    for (final o in sel.tableCell == null ? _doc.ownersAt(caret) : <Owner>[]) {
      if (o.style != style) continue;
      if (caret == o.contentEnd) {
        return _select(FlarkSelection.collapsed(o.end));
      }
      if (caret == o.contentStart) {
        return _select(FlarkSelection.collapsed(o.start));
      }
      final s = source
          .replaceRange(o.contentEnd, o.end, '')
          .replaceRange(o.start, o.contentStart, '');
      return _commit(
        s,
        FlarkSelection.collapsed(caret - (o.contentStart - o.start)),
        typing: false,
      );
    }
    for (final o
        in sel.tableCell == null ? _doc.ownersTouching(caret) : <Owner>[]) {
      if (o.style != style) continue;
      return _select(
        FlarkSelection.collapsed(
          caret == o.end ? o.contentEnd : o.contentStart,
        ),
      );
    }
    final mask = (_pending?.styles ?? 0) ^ style;
    // Recombine the supported formatting intents before materializing them.
    // Link/image destinations are separate pending closures, not style bits we
    // can reconstruct without their original owner records.
    if (mask &
            ~(Style.strong |
                Style.emphasis |
                Style.strikethrough |
                Style.code) !=
        0) {
      return false;
    }
    final delimiters = <String>[
      if (mask & Style.strong != 0) '**',
      if (mask & Style.emphasis != 0) '*',
      if (mask & Style.strikethrough != 0) '~~',
      if (mask & Style.code != 0) '`',
    ];
    _pending = mask == 0
        ? null
        : PendingStyle(delimiters.join(), delimiters.reversed.join(), mask);
    history.breakCoalescing();
    _goalColumn = null;
    return true;
  }

  // ------------------------------------------------------------- caret

  /// The anchor for display offset [d] of [row] when arriving from the
  /// direction given: the caret keeps the context it came from, so it
  /// changes only by crossing a glyph of another style.
  int _anchorFor(ProjectedRow row, int d, {required bool forward}) {
    final anchors = _doc.anchorsAt(
      row.sourceForDisplay(d, anchor: forward ? Anchor.before : Anchor.after),
    );
    return forward ? anchors.first : anchors.last;
  }

  ProjectedRow? _rowAfter(int index, {required bool forward}) {
    final i = forward ? index + 1 : index - 1;
    return i < 0 || i >= projection.rows.length ? null : projection.rows[i];
  }

  bool _move(MoveDirection direction, MoveUnit unit, bool extend) {
    final sel = selection;
    final forward = direction == MoveDirection.forward;
    final cur = sel.extent;
    final pos = _doc.caretPosition;
    final row = projection.rows[pos.row];
    final d = pos.offset;
    int target;
    if (sel.tableCell != null) {
      if (unit == MoveUnit.line) return false;
      final other = _rowAfter(row.index, forward: forward);
      return other != null &&
          _place(other.index, forward ? 0 : other.text.length, forward, extend);
    }
    if (sel.isCollapsed &&
        unit != MoveUnit.line &&
        (forward ? d == row.text.length : d == 0)) {
      final other = _rowAfter(row.index, forward: forward);
      if (other != null && projection.isMissingCell(other.index)) {
        return _place(other.index, 0, forward, extend);
      }
    }
    if (!extend && !sel.isCollapsed && unit == MoveUnit.grapheme) {
      target = forward ? sel.end : sel.start;
    } else {
      switch (unit) {
        case MoveUnit.grapheme:
          target = _step(row, d, forward: forward, cur: cur);
        case MoveUnit.word:
          final t = forward ? _wordEnd(row.text, d) : _wordStart(row.text, d);
          target = t == d
              ? _step(row, d, forward: forward, cur: cur)
              : _anchorFor(row, t, forward: forward);
        case MoveUnit.line:
          // Row edges take the outermost anchor, so the caret leaves a span there.
          final anchors = _doc.anchorsAt(
            row.sourceForDisplay(
              forward ? row.text.length : 0,
              anchor: forward ? Anchor.before : Anchor.after,
            ),
          );
          target = forward ? anchors.last : anchors.first;
        case MoveUnit.row:
          final goal = _goalColumn ?? d;
          // A row nothing displays — an empty table cell the source never
          // wrote — anchors back into its neighbour. Vertical movement must
          // leave the caret's own row, so keep looking past those.
          var other = _rowAfter(row.index, forward: forward);
          int? landing;
          while (other != null) {
            final candidate = _anchorFor(
              other,
              goal.clamp(0, other.text.length),
              forward: forward,
            );
            if (_doc.displayOf(candidate).row != row.index) {
              landing = candidate;
              break;
            }
            other = _rowAfter(other.index, forward: forward);
          }
          if (landing == null) return false;
          target = landing;
          final moved = _select(
            extend
                ? FlarkSelection(sel.base, target)
                : FlarkSelection.collapsed(target),
          );
          _goalColumn = goal;
          return moved;
      }
    }
    return _select(
      extend
          ? FlarkSelection(sel.base, target)
          : FlarkSelection.collapsed(target),
    );
  }

  /// One grapheme step, continued until the caret actually moves. Virtual
  /// leading spaces and a table cell a row never wrote both anchor back to the
  /// offset the caret already holds; reporting that as the target stalls the
  /// caret at the edge of a code block or inside a table forever.
  int _step(
    ProjectedRow row,
    int d, {
    required bool forward,
    required int cur,
  }) {
    var current = row, offset = d;
    // Every branch either advances `offset` strictly within the row or steps
    // `current` one row toward an end, so this terminates; the counter bounds
    // it anyway, because a stall here is a hung keystroke.
    var guard = projection.rows.length + row.text.length + 2;
    while (guard-- > 0) {
      if (forward && offset < current.text.length) {
        final atomic = _adjacentAtomicSegment(current, offset, forward: true);
        final next =
            atomic?.displayEnd ??
            offset + current.text.substring(offset).characters.first.length;
        final target = _anchorFor(current, next, forward: true);
        if (target != cur) return target;
        offset = next;
        continue;
      }
      if (!forward && offset > 0) {
        final atomic = _adjacentAtomicSegment(current, offset, forward: false);
        final next =
            atomic?.displayStart ??
            offset - current.text.substring(0, offset).characters.last.length;
        final target = _anchorFor(current, next, forward: false);
        if (target != cur) return target;
        offset = next;
        continue;
      }
      final other = _rowAfter(current.index, forward: forward);
      if (other == null) {
        return forward ? _doc.anchorsAt(cur).last : _doc.anchorsAt(cur).first;
      }
      offset = forward ? 0 : other.text.length;
      final target = _anchorFor(other, offset, forward: forward);
      if (target != cur) return target;
      current = other;
      guard += other.text.length;
    }
    return forward ? _doc.anchorsAt(cur).last : _doc.anchorsAt(cur).first;
  }

  static Segment? _adjacentAtomicSegment(
    ProjectedRow row,
    int displayOffset, {
    required bool forward,
  }) {
    for (final segment in row.segments) {
      if (segment.exact ||
          segment.sourceEnd <= segment.sourceStart ||
          segment.displayEnd <= segment.displayStart) {
        continue;
      }
      if (forward
          ? segment.displayStart == displayOffset
          : segment.displayEnd == displayOffset) {
        return segment;
      }
    }
    return null;
  }

  static bool _isSpace(String text, int i) =>
      text.codeUnitAt(i) == 0x20 ||
      text.codeUnitAt(i) == 0x0D ||
      text.codeUnitAt(i) == 0x0A ||
      text.codeUnitAt(i) == 0x09;

  static int _wordEnd(String text, int d) {
    var i = d;
    while (i < text.length && _isSpace(text, i)) {
      i++;
    }
    while (i < text.length && !_isSpace(text, i)) {
      i++;
    }
    return i;
  }

  static int _wordStart(String text, int d) {
    var i = d;
    while (i > 0 && _isSpace(text, i - 1)) {
      i--;
    }
    while (i > 0 && !_isSpace(text, i - 1)) {
      i--;
    }
    return i;
  }

  bool _place(int rowIndex, int offset, bool leadingHalf, bool extend) {
    if (projection.rows.isEmpty) return false;
    final row = projection.rows[rowIndex.clamp(0, projection.rows.length - 1)];
    if (projection.isMissingCell(row.index) &&
        (!extend || selection.base == row.sourceStart)) {
      return _select(
        FlarkSelection.collapsed(row.sourceStart, tableCell: row.index),
      );
    }
    final target = _doc.pointerAnchorAt(
      row.index,
      offset,
      leadingHalf: leadingHalf,
    );
    return _select(
      extend
          ? FlarkSelection(selection.base, target)
          : FlarkSelection.collapsed(target),
    );
  }

  // ----------------------------------------------------------- history

  bool _undo() {
    final currentSource = source;
    final currentSelection = selection;
    final currentPending = _pending;
    final target = history.undoTarget;
    if (target == null) return false;
    final next = _restoreSnapshot(target);
    final e = history.undoState(
      currentSource,
      currentSelection,
      currentPending,
    )!;
    assert(identical(e, target));
    _snapshot = next;
    _pending = _snapshot is FlarkLiveSnapshot ? e.pending : null;
    _goalColumn = null;
    return true;
  }

  bool _redo() {
    final currentSource = source;
    final currentSelection = selection;
    final currentPending = _pending;
    final target = history.redoTarget;
    if (target == null) return false;
    final next = _restoreSnapshot(target);
    final e = history.redoState(
      currentSource,
      currentSelection,
      currentPending,
    )!;
    assert(identical(e, target));
    _snapshot = next;
    _pending = _snapshot is FlarkLiveSnapshot ? e.pending : null;
    _goalColumn = null;
    return true;
  }

  FlarkEditorSnapshot _restoreSnapshot(HistoryEntry entry) =>
      _buildSnapshot(entry.source, entry.selection, previous: _liveDocument);
}

/// Characters that can underline a setext heading.
final _underlineRun = RegExp(r'^(?:-+|=+)$');

/// Any character of an item marker but a tab, which keeps its own width.
final _notTab = RegExp(r'[^\t]');

/// Where [line] of [text] starts, past the byte order mark that may lead the
/// first line. comrak skips the mark, so it belongs to the document rather
/// than to that line: removing the line must keep it, and a prefix copied
/// from the line must not carry it into the middle of the document, where it
/// is text that would stop the copied markers from being read.
int _lineStart(String text, RenderModel model, int line) {
  final start = model.lineStartUtf16(line);
  return start == 0 && text.isNotEmpty && text.codeUnitAt(0) == 0xFEFF
      ? 1
      : start;
}

/// comrak's footnote continuation indent: a line continues a footnote
/// definition when it is indented at least four columns past the containers
/// around the definition (`parse_footnote_definition_block_prefix`). The
/// render model gives continuation lines this indent as their prefix range,
/// but a label's line has no such range to copy.
const _footnoteIndent = '    ';

/// The container prefix that continues [line] of [text] up to [end], for a
/// line inside [block] (whose own markers stay out of it): the line's prefix
/// with the markers of the items and footnote definitions that open on [line]
/// turned into the indentation that continues them. A copied marker would
/// open another item or definition, where a continuation line belongs to the
/// open ones. An item continues at its marker's width, tabs kept; a footnote
/// definition at [_footnoteIndent]. Every range comes from the parser's
/// blocks.
String _continuationPrefix(
  String text,
  RenderModel model,
  int line,
  int end,
  int block,
) {
  final lineStart = _lineStart(text, model, line);
  var prefix = text.substring(lineStart, end);
  var child = block;
  for (var parent = model.blockParent(block); parent != noParent;) {
    final kind = model.blockKind(parent);
    if ((kind == BlockKind.item || kind == BlockKind.footnoteDefinition) &&
        model.blockFirstLine(parent) == line) {
      var from = model.blockStart(parent) - lineStart;
      final to = model.blockStart(child) - lineStart;
      if (from >= 0 && from < to && to <= prefix.length) {
        final marker = prefix.substring(from, to);
        // The indent counts from the end of the containers' prefix, not from
        // the label, so the label's own indentation goes with it; kept, it
        // would indent what follows past the definition's content, turning
        // an item's next marker into an underline or adding spaces to code.
        // That end is known for a definition at the document level, where it
        // is the line's start. An item opening on the line pads up to the
        // label; the model records no end for a quote's prefix or an earlier
        // item's indentation, so a label indented inside those keeps it.
        if (kind == BlockKind.footnoteDefinition &&
            model.blockKind(model.blockParent(parent)) == BlockKind.document) {
          while (from > 0 &&
              (prefix.codeUnitAt(from - 1) == 0x20 ||
                  prefix.codeUnitAt(from - 1) == 0x09)) {
            from--;
          }
        }
        prefix = prefix.replaceRange(
          from,
          to,
          kind == BlockKind.item
              ? marker.replaceAll(_notTab, ' ')
              : _footnoteIndent,
        );
      }
    }
    child = parent;
    parent = model.blockParent(parent);
  }
  return prefix;
}
