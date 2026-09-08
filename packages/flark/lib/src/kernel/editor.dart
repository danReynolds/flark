/// The editor facade: the one object a host talks to. It owns the document,
/// applies commands, keeps history, and reports the typing context.
library;

import 'package:characters/characters.dart';
import '../../code.dart';

import '../parse/backend.dart';
import '../parse/render_model.dart';
import '../parse/schema.g.dart';
import 'commands.dart';
import 'document.dart';
import 'history.dart';
import 'projection.dart';

part 'source_mode.dart';
part 'admission.dart';
part 'code_editing.dart';
part 'resource_editing.dart';

typedef FlarkListener = void Function();

final class FlarkEditor {
  FlarkEditor(
    this._backend, {
    String text = '',
    int caret = 0,
    this.codeEditing,
    this.syncLimit = defaultSyncLimit,
    this.liveLimits = const FlarkLiveLimits(),
    this.sourceLimit = 1024 * 1024,
    ProjectionOptions options = const ProjectionOptions(),
  }) : _options = options {
    if (syncLimit < 0 ||
        sourceLimit < syncLimit ||
        liveLimits.lines < 1 ||
        liveLimits.lineCodeUnits < 1 ||
        liveLimits.blocks < 1 ||
        liveLimits.runs < 1 ||
        liveLimits.blockCodeUnits < 1 ||
        liveLimits.containerDepth < 0) {
      throw ArgumentError('invalid Flark source or live limits');
    }
    validateFlarkSource(text);
    if (!_withinLiveByteLimit(text, sourceLimit)) {
      throw ArgumentError('document exceeds writable source limit');
    }
    _snapshot = _buildSnapshot(text, FlarkSelection.collapsed(caret));
  }

  static const int defaultSyncLimit = 16 * 1024;

  final FlarkParseBackend _backend;

  /// Optional, already initialized snippet service. Its caller owns disposal.
  final CodeEditingDelegate? codeEditing;
  final ProjectionOptions _options;

  /// Maximum UTF-8 byte length that may be parsed and projected synchronously.
  final int syncLimit;
  final FlarkLiveLimits liveLimits;

  /// Maximum writable UTF-8 source size, including source mode.
  final int sourceLimit;
  int _revision = 0;
  int get revision => _revision;
  FlarkRejection? _lastRejection;
  FlarkRejection? get lastRejection => _lastRejection;
  bool _forceSourceMode = false;
  HistoryEntry? _composition;
  bool get composing => _composition != null;
  final History history = History();
  final List<FlarkListener> _listeners = [];
  late FlarkEditorSnapshot _snapshot;
  PendingStyle? _pending;
  // Empty code bodies have a collapsed selection, so selection coordinates
  // alone cannot distinguish their first Select All from the second.
  bool _selectedCodeScope = false;

  /// Display column vertical movement aims for, kept across rows.
  int? _goalColumn;
  final Stopwatch _clock = Stopwatch()..start();
  Duration _now = Duration.zero;

  FlarkEditorSnapshot get snapshot => _snapshot;
  FlarkDocument get document => _doc;
  String get source => _snapshot.source;
  FlarkSelection get selection => _snapshot.selection;
  Projection get projection => _doc.projection;
  bool get sourceMode => _snapshot is FlarkSourceSnapshot;

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
      : _doc.typingContextAt(selection.extent) ^ (_pending?.styles ?? 0);

  void addListener(FlarkListener listener) => _listeners.add(listener);
  void removeListener(FlarkListener listener) => _listeners.remove(listener);

  /// Apply one command. Returns whether anything changed. [at] is the
  /// command's time, used only for history coalescing.
  bool apply(FlarkCommand command, {Duration? at, int? expectedRevision}) {
    _lastRejection = null;
    if (expectedRevision != null && expectedRevision != revision) {
      _lastRejection = FlarkRejection.staleRevision;
      return false;
    }
    if (composing && (command is Undo || command is Redo)) commitComposition();
    if (command is! SelectAll) _selectedCodeScope = false;
    _now = at ?? _clock.elapsed;
    final applied = sourceMode
        ? _applySource(command)
        : switch (command) {
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
            PlaceCaret(
              :final row,
              :final offset,
              :final leadingHalf,
              :final extend,
            ) =>
              _place(row, offset, leadingHalf, extend),
            MoveCaret(:final direction, :final unit, :final extend) => _move(
              direction,
              unit,
              extend,
            ),
            Undo() => _undo(),
            Redo() => _redo(),
            ToggleTask() => _toggleTask(),
            ToggleStyle(:final style) => _toggleStyle(style),
            SetHeadingLevel(:final level) => _setHeading(level),
            SetCodeLanguage(:final language) => _setCodeLanguage(language),
            SetLink(:final destination, :final text, :final title) =>
              _setResource(false, destination, text, title),
            SetImage(:final destination, :final alt, :final title) =>
              _setResource(true, destination, alt, title),
            RemoveLink() => _removeResource(false),
            RemoveImage() => _removeResource(true),
            Indent() => _shiftBlock(outdent: false),
            Outdent() => _shiftBlock(outdent: true),
          };
    if (applied) {
      _notify();
    } else if (command is! SetSelection &&
        command is! SelectAll &&
        command is! MoveCaret &&
        command is! PlaceCaret) {
      _lastRejection ??= FlarkRejection.unsupportedEdit;
    }
    return applied;
  }

  void _notify() {
    _revision++;
    for (final listener in List.of(_listeners)) {
      listener();
    }
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
        at: _clock.elapsed,
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
      final next = _buildSnapshot(source, selection);
      _snapshot = next;
      _pending = null;
      history.breakCoalescing();
      _notify();
    } catch (_) {
      _forceSourceMode = previous;
      rethrow;
    }
  }

  FlarkEditorSnapshot _buildSnapshot(
    String text,
    FlarkSelection selected, {
    bool rejectDeviation = false,
    RenderModel? parsed,
  }) {
    if (_forceSourceMode ||
        !_withinLiveByteLimit(text, syncLimit) ||
        !liveLimits._admitsSource(text)) {
      return FlarkSourceSnapshot._(text, selected);
    }
    try {
      final model = parsed ?? _backend.parse(text);
      if (!liveLimits._admitsModel(model)) {
        return FlarkSourceSnapshot._(text, selected);
      }
      return FlarkLiveSnapshot._(
        projectFlarkDocument(text, model, selected, _options),
      );
    } on FlarkParseException catch (error) {
      if (error.code != FlarkParseException.extractionDeviationCode ||
          rejectDeviation) {
        rethrow;
      }
      return FlarkSourceSnapshot._(text, selected);
    }
  }

  /// Parse, admit and project before publishing source or history.
  bool _commit(
    String newSource,
    FlarkSelection sel, {
    required bool typing,
    bool completeTypedFence = false,
    PendingStyle? pending,
    bool Function(FlarkDocument)? accept,
    bool acceptSourceMode = false,
  }) {
    try {
      validateFlarkSource(newSource);
    } on FormatException {
      _lastRejection = FlarkRejection.invalidSource;
      return false;
    }
    if (!_withinLiveByteLimit(newSource, sourceLimit)) {
      _lastRejection = FlarkRejection.sourceLimit;
      return false;
    }
    late FlarkEditorSnapshot next;
    try {
      RenderModel? parsed;
      if (completeTypedFence &&
          _withinLiveByteLimit(newSource, syncLimit) &&
          liveLimits._admitsSource(newSource)) {
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
      next = _buildSnapshot(
        newSource,
        sel,
        rejectDeviation: !sourceMode,
        parsed: parsed,
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
        source,
        selection,
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
    final row = _doc.rowAt(selection.extent);
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
    Newline() => _sourceInsert('\n', typing: false),
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
    PlaceCaret() ||
    ToggleTask() ||
    ToggleStyle() ||
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

  bool _insert(String text, {required bool typing}) {
    if (text.isEmpty) return false;
    final sel = selection;
    if (_wholeRange(sel.start, sel.end)) return _replaceWhole(text);
    final range = _contentRange(sel.start, sel.end);
    if (!_supportedRange(range.start, range.end)) return false;
    final row = _doc.rowAt(sel.extent);
    if (row.fenced && row.contentStarts.every((start) => start < 0)) {
      final code = _pasteCode(text);
      if (code != null) return code;
    }
    if (typing && !composing) {
      final code = _delegateCodeEdit(CodeEditingAction.insert, text: text);
      if (code != null) return code;
    } else if (!typing && !composing) {
      final code = _pasteCode(text);
      if (code != null) return code;
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
      start = expanded.start;
      end = expanded.end;
      caret = start + inserted.length;
    }
    final normalized = _normalizeInlineEdges(
      start,
      end,
      inserted,
      caret,
      pending: pending,
    );
    return _commit(
      normalized.text,
      FlarkSelection.collapsed(normalized.caret),
      pending: normalized.pending,
      typing: typing && text != '\n' && text.characters.length == 1,
      completeTypedFence:
          typing &&
          !composing &&
          sel.isCollapsed &&
          inserted == text &&
          (text == '`' || text == '```' || text == '~' || text == '~~~') &&
          _doc.rowAt(sel.extent).kind != RowKind.codeBlock,
    );
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
          block.attr0 != 3 ||
          block.firstLine != line ||
          block.startUtf16 + block.attr0 != caret ||
          block.attr1 != block.attr2) {
        continue;
      }
      final lineStart = model.lineStartUtf16(line);
      var prefix = candidate.substring(lineStart, block.startUtf16);
      // Preserve quote markers and indentation. Replace a parser-owned item
      // marker with equal-width whitespace so subsequent lines stay in the
      // same item, including tab-padded and nested items.
      var child = block;
      for (var parent = block.parent; parent != noParent;) {
        final ancestor = model.blockAt(parent);
        if (ancestor.kind == BlockKind.item && ancestor.firstLine == line) {
          final from = ancestor.startUtf16 - lineStart;
          final to = child.startUtf16 - lineStart;
          final padding = prefix
              .substring(from, to)
              .split('')
              .map((char) => char == '\t' ? '\t' : ' ')
              .join();
          prefix = prefix.replaceRange(from, to, padding);
        }
        child = ancestor;
        parent = ancestor.parent;
      }
      final fence = candidate.substring(block.startUtf16, caret);
      // An empty info range may follow ASCII padding on the opener line.
      final openerEnd = caret + block.attr1 - block.startByte - block.attr0;
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
    if (text.trim().isEmpty) {
      final expanded = _rangeForEmptying(s, e);
      s = expanded.start;
      e = expanded.end;
    }
    final normalized = _normalizeInlineEdges(s, e, text, s + text.length);
    return _commit(
      normalized.text,
      FlarkSelection.collapsed(normalized.caret),
      typing: false,
      pending: normalized.pending,
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
    if (atomic?.lineBreak == true) {
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
      return _joinContent(start, b);
    }
    // EP1-DELETE-TO-EMPTY: an owner this deletion empties goes with it, and
    // its delimiters wait for the next ordinary character.
    return _deleteContent(a, b, typing: !word);
  }

  bool _deleteContent(int start, int end, {required bool typing}) {
    final expanded = _rangeForEmptying(start, end);
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
        // Leading indentation can be outside the row's legal caret spans.
        // Keep the caret in the surviving owner instead of leaving pending
        // delimiters adjacent to that same owner's opening delimiter.
        caret = first;
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
  /// hidden delimiters cannot be stranded in the source.
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
    if (row.kind == RowKind.heading) return _setHeading(0);
    if (row.fenced && row.text.isEmpty) {
      final block = _doc.model.blockAt(row.block);
      if (block.flags & 2 != 0) {
        return _commit(
          source.replaceRange(block.startUtf16, block.endUtf16, ''),
          FlarkSelection.collapsed(block.startUtf16),
          typing: false,
        );
      }
    }
    final i = (_doc.model.lineOfUtf16(selection.extent) - row.firstLine).clamp(
      0,
      row.contentStarts.length - 1,
    );
    final prefixStart = row.prefixStarts[i],
        contentStart = row.contentStarts[i];
    if (prefixStart >= 0 && prefixStart < contentStart) {
      // A lifted line that would lazily continue the previous paragraph
      // stays a paragraph of its own.
      final prev = row.index > 0 ? projection.rows[row.index - 1] : null;
      final separate =
          row.text.isNotEmpty &&
          prev != null &&
          prev.kind == RowKind.paragraph &&
          prev.firstLine + prev.lineCount == row.firstLine;
      return _commit(
        source.replaceRange(prefixStart, contentStart, separate ? '\n' : ''),
        FlarkSelection.collapsed(separate ? prefixStart + 1 : prefixStart),
        typing: false,
      );
    }
    if (row.index == 0) return false;
    final prev = projection.rows[row.index - 1];
    if (prev.kind == RowKind.tableCell) return false;
    if (prev.firstLine + prev.lineCount > row.firstLine) return false;
    final from = _lastCaretEnd(prev), to = _firstCaretStart(row);
    if (from < 0 || to < 0 || to <= from) return false;
    return _joinContent(from, to);
  }

  /// Delete at a row end: join the next row onto this one.
  bool _joinForward(ProjectedRow row) {
    if (row.kind == RowKind.tableCell) return false;
    if (row.index + 1 >= projection.rows.length) return false;
    final next = projection.rows[row.index + 1];
    if (next.kind == RowKind.tableCell) return false;
    if (next.firstLine < row.firstLine + row.lineCount) return false;
    final from = _lastCaretEnd(row), to = _firstCaretStart(next);
    if (from < 0 || to < 0 || to <= from) return false;
    return _joinContent(from, to);
  }

  /// Joining physical lines also joins matching boundary owners. Leaving
  /// adjacent closing/opening runs (for example **a****b**) exposes markers.
  bool _joinContent(int from, int to) {
    final left = _doc.ownersAt(_doc.anchorsAt(from).first);
    final right = _doc.ownersAt(_doc.anchorsAt(to).last);
    if (left.isNotEmpty && left.length == right.length) {
      var compatible = true;
      for (var i = 0; i < left.length; i++) {
        final a = left[i], b = right[i];
        if (a.end > from ||
            b.start < to ||
            a.kind != b.kind ||
            source.substring(a.start, a.contentStart) !=
                source.substring(b.start, b.contentStart) ||
            source.substring(a.contentEnd, a.end) !=
                source.substring(b.contentEnd, b.end)) {
          compatible = false;
        }
      }
      if (compatible) {
        from = left.last.contentEnd;
        to = right.last.contentStart;
      }
    }
    return _commit(
      source.replaceRange(from, to, ''),
      FlarkSelection.collapsed(from),
      typing: false,
    );
  }

  static int _lastCaretEnd(ProjectedRow row) {
    for (var i = row.contentEnds.length - 1; i >= 0; i--) {
      if (row.contentEnds[i] >= 0) return row.contentEnds[i];
    }
    return -1;
  }

  static int _firstCaretStart(ProjectedRow row) {
    for (final s in row.contentStarts) {
      if (s >= 0) return s;
    }
    return -1;
  }

  bool _newline(bool paragraph) {
    final sel = selection;
    if (_wholeRange(sel.start, sel.end)) {
      return _replaceWhole(paragraph ? '\n\n' : '\n');
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
      return _splitInline(range.start, range.end, paragraph ? '\n\n' : '\n');
    }
    final caret = sel.extent;
    final pos = _doc.displayOf(caret);
    final row = projection.rows[pos.row];
    if (row.kind == RowKind.tableCell) return _returnFromTable(row);
    final line = _doc.model.lineOfUtf16(caret);
    final i = (line - row.firstLine).clamp(0, row.contentStarts.length - 1);
    String text;
    if (row.kind == RowKind.codeBlock) {
      return _codeNewline(row, caret, caret);
    } else if (row.shells.isNotEmpty) {
      final prefixStart = row.prefixStarts[i],
          contentStart = row.contentStarts[i];
      // Return on an empty container line exits the container.
      if (row.text.isEmpty && prefixStart >= 0 && prefixStart < contentStart) {
        final outer = source.substring(
          _doc.model.lineStartUtf16(line),
          prefixStart,
        );
        final replacement = line > 0 ? '\n$outer' : '';
        return _commit(
          source.replaceRange(prefixStart, contentStart, replacement),
          FlarkSelection.collapsed(prefixStart + replacement.length),
          typing: false,
        );
      }
      final inner = row.shells.last;
      text =
          '\n${inner.kind == ShellKind.item ? _nextMarker(inner) : source.substring(_doc.model.lineStartUtf16(line), contentStart)}';
    } else {
      text = paragraph && row.kind == RowKind.paragraph ? '\n\n' : '\n';
    }
    return _splitInline(caret, caret, text);
  }

  bool _returnFromTable(ProjectedRow row) {
    for (var i = row.index + 1; i < projection.rows.length; i++) {
      final next = projection.rows[i];
      if (next.kind != RowKind.tableCell || next.tableBlock != row.tableBlock) {
        break;
      }
      if (next.column == row.column && next.firstLine > row.firstLine) {
        return _select(FlarkSelection.collapsed(next.sourceStart));
      }
    }
    // Keep a blank separator after the table even when a trailing gap already
    // exists. Typing into that gap alone would make it another table row.
    if (row.shells.isNotEmpty) return false;
    final end = _doc.model.block(row.tableBlock, BlockField.endUtf16);
    return _commit(
      source.replaceRange(end, end, '\n\n'),
      FlarkSelection.collapsed(end + 2),
      typing: false,
    );
  }

  /// Split parser-owned spans with the block. Move a terminal break outside
  /// closing syntax; at an interior split close and reopen the nonempty parts.
  /// No empty delimiter pair is ever published as an intermediate document.
  bool _splitInline(int start, int end, String separator) {
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
    final inserted =
        '${left.join()}$leftSpace$separator$rightSpace${right.join()}';
    return _commit(
      source.replaceRange(from, to, inserted),
      FlarkSelection.collapsed(from + inserted.length),
      typing: false,
    );
  }

  /// The marker line for the item after [item]: the same outer prefixes,
  /// the next number for ordered lists, an unchecked box for tasks.
  String _nextMarker(Shell item) {
    final m = _doc.model;
    final itemLine = m.block(item.block, BlockField.firstLine),
        itemStart = m.block(item.block, BlockField.startUtf16);
    // The parser converts column padding (including partially consumed tabs)
    // to an exact source endpoint before the optional task checkbox.
    final markerEnd = m.block(item.block, BlockField.markerEndUtf16);
    final outer = source.substring(m.lineStartUtf16(itemLine), itemStart);
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
    final row = _doc.rowAt(selection.extent);
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
    final list = m.block(item.block, BlockField.parent);
    final first = m.block(item.block, BlockField.firstLine),
        n = m.block(item.block, BlockField.lineCount);
    final column =
        m.block(item.block, BlockField.startUtf16) - m.lineStartUtf16(first);
    int width;
    if (!outdent) {
      var prev = -1;
      for (var b = list + 1; b < item.block; b++) {
        if (m.block(b, BlockField.parent) == list) prev = b;
      }
      if (prev < 0) return false;
      width = m.block(prev, BlockField.attr0);
    } else {
      Shell? parent;
      for (var i = idx - 1; i >= 0; i--) {
        if (shells[i].kind == ShellKind.item) {
          parent = shells[i];
          break;
        }
      }
      if (parent == null) return false;
      width = m.block(parent.block, BlockField.attr0);
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

  bool _setHeading(int level) {
    if (level < 0 || level > 6) return false;
    final row = _doc.rowAt(selection.extent);
    if (row.kind != RowKind.paragraph && row.kind != RowKind.heading) {
      return false;
    }
    if (level > 0 &&
        row.kind == RowKind.heading &&
        row.contentStarts.where((s) => s >= 0).length > 1) {
      return false;
    }
    final m = _doc.model;
    final blockStart = m.block(row.block, BlockField.startUtf16),
        blockEnd = m.block(row.block, BlockField.endUtf16);
    final prefix = level == 0 ? '' : '${'#' * level} ';
    var s = source;
    if (row.kind == RowKind.heading && blockEnd > row.sourceEnd) {
      s = s.replaceRange(row.sourceEnd, blockEnd, '');
    }
    s = s.replaceRange(blockStart, row.sourceStart, prefix);
    final shift = prefix.length - (row.sourceStart - blockStart);
    int move(int o) => o >= row.sourceStart ? o + shift : o;
    return _commit(
      s,
      FlarkSelection(move(selection.base), move(selection.extent)),
      typing: false,
    );
  }

  bool _toggleStyle(int style) {
    final delimiter = switch (style) {
      Style.emphasis => '*',
      Style.strong => '**',
      Style.strikethrough => '~~',
      Style.code => '`',
      _ => null,
    };
    if (delimiter == null) return false;
    final sel = selection;
    if (!sel.isCollapsed) {
      final range = _contentRange(sel.start, sel.end);
      for (final o in _doc.ownersOfContent(range.start, range.end)) {
        if (o.style != style) continue;
        final s = source
            .replaceRange(o.contentEnd, o.end, '')
            .replaceRange(o.start, o.contentStart, '');
        return _commit(
          s,
          FlarkSelection(o.start, o.start + (o.contentEnd - o.contentStart)),
          typing: false,
        );
      }
      final contentStart = range.start + delimiter.length;
      final contentEnd = range.end + delimiter.length;
      return _commit(
        source.replaceRange(
          range.start,
          range.end,
          '$delimiter${source.substring(range.start, range.end)}$delimiter',
        ),
        FlarkSelection(contentStart, contentEnd),
        typing: false,
        accept: (next) => next
            .ownersOfContent(contentStart, contentEnd)
            .any((owner) => owner.style == style),
      );
    }
    final caret = sel.extent;
    // At an edge of an owner, step across its delimiter: out when inside,
    // in when outside. Strictly inside, unwrap it.
    for (final o in _doc.ownersAt(caret)) {
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
    for (final o in _doc.ownersTouching(caret)) {
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
    final pos = _doc.displayOf(cur);
    final row = projection.rows[pos.row];
    final d = pos.offset;
    int target;
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
          final other = _rowAfter(row.index, forward: forward);
          if (other == null) return false;
          final goal = _goalColumn ?? d;
          target = _anchorFor(
            other,
            goal.clamp(0, other.text.length),
            forward: forward,
          );
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

  int _step(
    ProjectedRow row,
    int d, {
    required bool forward,
    required int cur,
  }) {
    if (forward) {
      if (d < row.text.length) {
        final atomic = _adjacentAtomicSegment(row, d, forward: true);
        if (atomic != null) {
          return _anchorFor(row, atomic.displayEnd, forward: true);
        }
        return _anchorFor(
          row,
          d + row.text.substring(d).characters.first.length,
          forward: true,
        );
      }
      final next = _rowAfter(row.index, forward: true);
      return next == null
          ? _doc.anchorsAt(cur).last
          : _anchorFor(next, 0, forward: true);
    }
    if (d > 0) {
      final atomic = _adjacentAtomicSegment(row, d, forward: false);
      if (atomic != null) {
        return _anchorFor(row, atomic.displayStart, forward: false);
      }
      return _anchorFor(
        row,
        d - row.text.substring(0, d).characters.last.length,
        forward: false,
      );
    }
    final prev = _rowAfter(row.index, forward: false);
    return prev == null
        ? _doc.anchorsAt(cur).first
        : _anchorFor(prev, prev.text.length, forward: false);
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
      _buildSnapshot(entry.source, entry.selection);
}
