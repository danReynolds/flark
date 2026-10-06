/// The editor facade: the one object a host talks to. It owns the document,
/// applies commands, keeps history, and reports the typing context.
library;

import 'package:characters/characters.dart';
import '../../code.dart';

import '../parse/backend.dart';
import '../parse/render_model.dart';
import '../parse/schema.g.dart';
import 'calls.dart';
import 'commands.dart';
import 'continuation.dart';
import 'document.dart';
import 'edits.dart';
import 'history.dart';
import 'notify.dart';
import 'projection.dart';
import 'resource.dart';
import 'row_queries.dart';
import 'style_state.dart';

part 'snapshot.dart';
part 'read_document.dart';
part 'admission.dart';
part 'code_editing.dart';
part 'resource_editing.dart';
part 'table_editing.dart';
part 'inline_formatting.dart';
part 'typed_lines.dart';
part 'composition.dart';

typedef FlarkListener = void Function();

final class FlarkEditor implements FlarkDocumentState {
  FlarkEditor(
    this._backend, {
    String text = '',
    int caret = 0,
    this.codeEditing,
    this.syncLimit = defaultSyncLimit,
    this.liveLimits = const FlarkLiveLimits(),
    this.sourceLimit = defaultSourceLimit,
    ProjectionOptions options = const ProjectionOptions(),
    Duration Function()? clock,
  }) : _options = options,
       _clock = clock ?? _stopwatch() {
    checkFlarkLimits(
      syncLimit: syncLimit,
      sourceLimit: sourceLimit,
      liveLimits: liveLimits,
    );
    if (flarkSourceRefusal(text, sourceLimit: sourceLimit)
        case (_, final error)?) {
      throw error;
    }
    _snapshot = _buildSnapshot(text, FlarkSelection.collapsed(caret));
  }

  static const int defaultSyncLimit = 16 * 1024;

  /// The writable source limit an editor has unless it is given another:
  /// 1 MiB of UTF-8.
  static const int defaultSourceLimit = 1024 * 1024;

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

  /// Why the last call refused its edit, or null. After
  /// [applyAfterComposition] whose command applied, it is why the
  /// composition it ended was withdrawn, if it was.
  FlarkRejection? get lastRejection => _lastRejection;

  /// Set when the command being applied asked for the state the editor
  /// already has. Like a repeated SetStyle, it is a successful no-op: it
  /// publishes nothing, records no history and is not reported as refused.
  bool _inert = false;
  bool _forceSourceMode = false;
  HistoryEntry? _composition;
  bool get composing => _composition != null;

  /// Where the open composition's text is: its range of the source, the
  /// source it is a range of, and whether the platform composed apart from
  /// that text (an input method's correction beside its word). See
  /// [_compose].
  (int, int, String, bool)? _composed;

  /// Told of each call to this editor's editing API once it returns or
  /// throws: the call as data, what it returned (null for a call that
  /// returns nothing) and what it threw. A call made inside another (one
  /// a listener makes while the editor notifies) is part of that one. A
  /// `FlarkEditRecorder` from `package:flark/recorder.dart` sets it to record
  /// a session as a replayable test. It serves one observer at a time.
  void Function(FlarkEditorCall call, bool? returned, Object? error)? onCall;
  bool _observing = false;

  /// Runs [body], the work of [call], and tells [onCall] of it.
  T _observe<T>(FlarkEditorCall call, T Function() body) {
    final observer = onCall;
    if (observer == null || _observing) return body();
    _observing = true;
    Object? returned, error;
    try {
      final value = body();
      returned = value;
      return value;
    } catch (thrown) {
      error = thrown;
      rethrow;
    } finally {
      _observing = false;
      observer(call, returned is bool ? returned : null, error);
    }
  }

  /// The projection options this editor renders with.
  ProjectionOptions get options => _options;

  /// Whether source mode was asked for ([setSourceMode]), apart from a
  /// document past the live tier, which shows its source anyway.
  bool get sourceModeForced => _forceSourceMode;

  /// Whether the next typed text takes a pending style: one a formatting
  /// command set at the caret, or one an emptied span left behind.
  bool get hasPendingStyle => _pending != null;

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

  /// The editing state as it is, history as its checkpoint ([_EditState]).
  _EditState get _editState => (
    snapshot: _snapshot,
    pending: _pending,
    goalColumn: _goalColumn,
    selectedCodeScope: _selectedCodeScope,
    composition: _composition,
    composed: _composed,
    cellOrigin: _cellOrigin,
    history: history.checkpoint(),
  );

  /// Puts [state] back whole.
  set _editState(_EditState state) {
    _snapshot = state.snapshot;
    _pending = state.pending;
    _goalColumn = state.goalColumn;
    _selectedCodeScope = state.selectedCodeScope;
    _composition = state.composition;
    _composed = state.composed;
    _cellOrigin = state.cellOrigin;
    history.restore(state.history);
  }

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
    final now = at ?? _clock();
    return _observe(
      ApplyCall(command, now),
      () => _apply(command, now, expectedRevision),
    );
  }

  /// The step an editing call starts with: the call's outcome starts unset,
  /// and a call made for a revision the editor has left is refused.
  bool _enter(int? expectedRevision) {
    _lastRejection = null;
    _inert = false;
    if (expectedRevision == null || expectedRevision == revision) return true;
    _lastRejection = FlarkRejection.staleRevision;
    return false;
  }

  bool _apply(FlarkCommand command, Duration at, int? expectedRevision) {
    if (!_enter(expectedRevision)) return false;
    // Undo during a composition commits it, so Undo takes it back. Redo has
    // nothing to redo once it commits: during a composition it does nothing.
    if (composing && command is Redo) {
      _inert = true;
      return false;
    }
    if (composing && command is Undo) _commitComposition();
    if (command is! SelectAll) _selectedCodeScope = false;
    _now = at;
    final applied = sourceMode
        ? _applySource(command)
        : composing && command is InsertText
        ? _compose(selection.start, selection.end, command.text)
        : composing && command is ReplaceRange
        ? _compose(command.start, command.end, command.text)
        : _withMissingCell(command);
    if (applied) {
      _notify();
    } else if (!_inert &&
        command is! SetSelection &&
        command is! MoveTableCell &&
        command is! SelectAll &&
        command is! MoveCaret &&
        command is! PlaceCaret &&
        // Tab where nothing can be indented, or where indenting would change
        // the blocks around (an empty item under a paragraph would underline
        // it), does nothing; source mode is no way to indent it.
        command is! Indent &&
        command is! Outdent) {
      _lastRejection ??= FlarkRejection.unsupportedEdit;
    }
    return applied;
  }

  /// Host actions close composition only if the action is accepted. Snapshot
  /// references are cheap; the small history savepoint is needed only in IME.
  /// [at] is the command's time, used only for history coalescing.
  bool applyAfterComposition(
    FlarkCommand command, {
    Duration? at,
    int? expectedRevision,
  }) {
    final now = at ?? _clock();
    return _observe(
      ApplyCall(command, now, afterComposition: true),
      () =>
          _enter(expectedRevision) &&
          _applyAfterComposition(() => command, now),
    );
  }

  /// [command], made at [at], is built once the composition has ended:
  /// committing its text can respell the source the command's offsets are
  /// in.
  bool _applyAfterComposition(FlarkCommand Function() command, Duration at) {
    if (!composing) return _apply(command(), at, null);
    final whileComposing = _editState;
    // The commit publishes with the command, which may yet be refused.
    _endComposition();
    final withdrawn = _lastRejection, unpublished = _revision;
    try {
      final accepted = _apply(command(), at, null);
      if (!accepted) {
        _editState = whileComposing;
      } else {
        // A command that applied still reports a composition it withdrew.
        _lastRejection ??= withdrawn;
      }
      return accepted;
    } catch (_) {
      // Only a command that failed before it was published is withdrawn.
      // Listeners that heard of an edit hold its source, so restoring the
      // snapshot under them would split the document in two.
      if (_revision == unpublished) _editState = whileComposing;
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
    SetHeadingLevel(:final level) => _setHeading(level) is _Committed,
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

  /// Whether [SetHeadingLevel] would try to change the caret's block at some
  /// level: the checks the command makes before the parser validates its
  /// edit, as [canSetResource] makes a link's. The command sets the caret's
  /// block alone, so a selection's other rows do not count.
  bool canSetHeading() =>
      !sourceMode &&
      Iterable.generate(7, _headingPlan).any(
        (plan) =>
            plan != _HeadingPlan.refused && plan != _HeadingPlan.unchanged,
      );

  /// Exact UTF-16 source splice. Unlike input replacement this never snaps
  /// the supplied range or preserves surrounding Markdown wrappers.
  bool replaceSourceRange(
    int start,
    int end,
    String text, {
    int? expectedRevision,
    bool replaceAll = false,
  }) => _observe(
    ReplaceSourceRangeCall(start, end, text, replaceAll: replaceAll),
    () => _replaceSourceRange(
      start,
      end,
      text,
      expectedRevision: expectedRevision,
      replaceAll: replaceAll,
    ),
  );

  bool _replaceSourceRange(
    int start,
    int end,
    String text, {
    int? expectedRevision,
    bool replaceAll = false,
  }) {
    if (!_enter(expectedRevision)) return false;
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
    // As with any edit, a splice that leaves the source as it is has nothing
    // to parse or undo: at most it moves the selection, and when it does not
    // even do that it changes nothing, a pending style included.
    if (nextSource == source) {
      final moved = sourceMode
          ? _selectSource(nextSelection)
          : _select(nextSelection);
      if (moved) _notify();
      return moved;
    }
    // Admission happens before composition/history changes. A rejected edit
    // must not commit a preedit or consume an undo step.
    final next = _admitSource(nextSource, nextSelection);
    if (next == null) return false;
    // The splice's offsets are in the composed text as it stands.
    _endComposition(retype: false);
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

  /// Whether [text] keeps the source contract (no bare CR, well-formed
  /// UTF-16) within the writable limit, and if so whether it is rendered
  /// live. Null, with [_lastRejection] set, when it is refused. One pass
  /// over the text decides all of it, for every edit.
  bool? _admits(String text) {
    final stats = _SourceStats.of(text);
    if (!stats.valid) {
      _lastRejection = FlarkRejection.invalidSource;
      return null;
    }
    if (stats.utf8Bytes > sourceLimit) {
      _lastRejection = FlarkRejection.sourceLimit;
      return null;
    }
    return liveLimits._admitsLive(stats, syncLimit);
  }

  FlarkEditorSnapshot? _admitSource(String text, FlarkSelection selected) {
    final live = _admits(text);
    if (live == null) return null;
    try {
      return _buildSnapshot(
        text,
        selected,
        rejectDeviation: !sourceMode,
        live: live,
      );
    } on FlarkParseException catch (error) {
      if (error.code != FlarkParseException.extractionDeviationCode) rethrow;
      _lastRejection = FlarkRejection.extractionDeviation;
      return null;
    }
  }

  /// Establish external content without creating an undo step. Always
  /// publishes a revision, even when resetting identical content.
  bool loadMarkdown(String text, {int? expectedRevision}) => _observe(
    LoadMarkdownCall(text),
    () => _loadMarkdown(text, expectedRevision: expectedRevision),
  );

  bool _loadMarkdown(String text, {int? expectedRevision}) {
    if (!_enter(expectedRevision)) return false;
    final next = _admitSource(text, const FlarkSelection.collapsed(0));
    if (next == null) return false;
    _composition = null;
    _composed = null;
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
  bool selectAll({bool codeBlock = false, int? expectedRevision}) => _observe(
    SelectAllCall(codeBlock: codeBlock),
    () => _enter(expectedRevision) && _selectAllIn(codeBlock: codeBlock),
  );

  bool _selectAllIn({required bool codeBlock}) {
    if (codeBlock) {
      if (sourceMode || !_doc.caretRow.fenced) {
        _lastRejection = FlarkRejection.unsupportedEdit;
        return false;
      }
      return _applyAfterComposition(() {
        final row = _doc.caretRow;
        return SetSelection(
          row.sourceForDisplay(0),
          row.sourceForDisplay(row.text.length),
        );
      }, _clock());
    }
    return _applyAfterComposition(
      () => SetSelection(0, source.length),
      _clock(),
    );
  }

  void _notify() {
    _revision++;
    notifyEach(_listeners);
  }

  /// Input methods may publish several preedit values as one logical action.
  /// While it composes, its text goes into the source as it is ([_compose]);
  /// committing makes of it what typing would, as one undo step.
  /// Cancellation restores source, selection and typing intent without using
  /// or clearing the user's undo/redo stacks.
  void beginComposition() {
    _enter(null);
    // Composition calls that change nothing (a host that commits or begins
    // on every focus change) are no calls worth observing.
    if (composing) return;
    _observe(const CompositionCall(CompositionStep.begin), _beginComposition);
  }

  void _beginComposition() {
    if (_composition != null) return;
    _composition = HistoryEntry(source, selection, _pending, history.openGroup);
    _composed = null;
  }

  /// Ends the composition with its text typed. Typing can refuse what was
  /// composed, which withdraws it as [cancelComposition] does:
  /// [lastRejection] then says why.
  void commitComposition() {
    _enter(null);
    if (!composing) return;
    _observe(const CompositionCall(CompositionStep.commit), _commitComposition);
  }

  void _commitComposition() {
    if (_endComposition()) _notify();
  }

  /// Ends the composition, one undo step from the state it began in, with
  /// what typing makes of its text unless [retype] is false. Publishes
  /// nothing; true when that changed the snapshot or the typing intent.
  bool _endComposition({bool retype = true}) {
    final before = _composition;
    if (before == null) return false;
    final retyped = retype && _retypeComposition(before);
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
    _composed = null;
    history.breakCoalescing();
    return retyped;
  }

  void cancelComposition() {
    _enter(null);
    if (!composing) return;
    _observe(const CompositionCall(CompositionStep.cancel), _cancelComposition);
  }

  void _cancelComposition() {
    final before = _composition;
    if (before == null) return;
    final next = _restoreSnapshot(before);
    _snapshot = next;
    _pending = before.pending;
    _composition = null;
    _composed = null;
    _notify();
  }

  /// Explicit source editing remains available for unsupported rich edits.
  /// Returning to rendered mode uses the same admission path as opening.
  void setSourceMode(bool enabled) => _observe(SetSourceModeCall(enabled), () {
    _enter(null);
    _setSourceMode(enabled);
  });

  void _setSourceMode(bool enabled) {
    _commitComposition();
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

  /// Parse, admit and project before publishing source or history: whether
  /// [_attempt] committed.
  bool _commit(
    String newSource,
    FlarkSelection sel, {
    required bool coalesce,
    bool completeTypedFence = false,
    PendingStyle? pending,
    bool Function(FlarkDocument)? accept,
    bool Function(FlarkDocument next, int at, int length)? acceptCompleted,
    bool acceptSourceMode = false,
  }) =>
      _attempt(
            newSource,
            sel,
            coalesce: coalesce,
            completeTypedFence: completeTypedFence,
            pending: pending,
            accept: accept,
            acceptCompleted: acceptCompleted,
            acceptSourceMode: acceptSourceMode,
          )
          is _Committed;

  /// Parse, admit and project [newSource] with [sel], and when [accept]
  /// holds for the result, publish it with its history step. [coalesce]
  /// lets history join the commit to the typing before it, as it joins one
  /// keystroke ([_keystroke]). Past the live tier no parse checks [accept],
  /// and the source commits only with [acceptSourceMode].
  /// [acceptCompleted] checks the source that completes a typed fence in
  /// [newSource] by inserting `length` characters at `at` of it; without it
  /// the completion commits unchecked. A refusal also sets [_lastRejection],
  /// the call's outcome.
  _Outcome _attempt(
    String newSource,
    FlarkSelection sel, {
    required bool coalesce,
    bool completeTypedFence = false,
    PendingStyle? pending,
    bool Function(FlarkDocument)? accept,
    bool Function(FlarkDocument next, int at, int length)? acceptCompleted,
    bool acceptSourceMode = false,
  }) {
    // An edit that leaves the source as it is has nothing to parse or undo:
    // at most it moves the selection, and when it does not even do that it is
    // inert. Recording it would leave an Undo that changes nothing. A missing
    // cell's private preparation is itself the change, so it commits as usual.
    if (newSource == source && _cellOrigin == null) {
      // Its check still holds, of the document as it is with the selection
      // moved, or the edit is not kept: unchanged is not done.
      final read = accept == null || sourceMode
          ? null
          : _doc.withSelection(sel);
      if (accept != null &&
          (read == null ? !acceptSourceMode : !accept(read))) {
        return _NotKept(passedOver: sourceMode, read: read);
      }
      final moved = sourceMode ? _selectSource(sel) : _select(sel);
      _inert = !moved;
      return moved ? const _Committed() : const _Unchanged();
    }
    final live = _admits(newSource);
    if (live == null) return _Refused(_lastRejection!);
    late FlarkEditorSnapshot next;
    try {
      RenderModel? parsed;
      if (completeTypedFence && live) {
        parsed = _backend.parse(newSource);
        final completed = _completeTypedFence(newSource, sel.extent, parsed);
        if (completed != null) {
          final length = completed.source.length - newSource.length;
          return _attempt(
            completed.source,
            FlarkSelection.collapsed(completed.caret),
            coalesce: false,
            acceptSourceMode: acceptSourceMode,
            accept: acceptCompleted == null
                ? null
                : (next) => acceptCompleted(next, completed.at, length),
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
      return const _Refused(FlarkRejection.extractionDeviation);
    }
    if (accept != null) {
      if (next is FlarkLiveSnapshot) {
        if (!accept(next.document)) return _NotKept(read: next.document);
      } else if (!acceptSourceMode) {
        return const _NotKept(passedOver: true);
      }
    }
    if (!composing) {
      history.recordState(
        _cellOrigin?.source ?? source,
        _cellOrigin?.selection ?? selection,
        pending: _pending,
        typing: coalesce,
        at: _now,
      );
    }
    _snapshot = next;
    _pending = next is FlarkLiveSnapshot ? pending : null;
    _goalColumn = null;
    return const _Committed();
  }

  /// Commits the first of [spellings], in their order, that the parser reads
  /// as [keep] says it must, and says what became of the edit. [keep] gets
  /// the spelling and the edits that made the document it reads: the
  /// spelling's own, or with the typed fence it completed
  /// ([completeTypedFence]) put in them.
  ///
  /// Only the edit as asked can cost the edit its admission: when the parser
  /// refuses it, the edit is refused, and a respelling it refuses is passed
  /// over. A spelling past the live tier, where no parse checks it, commits
  /// unchecked only when it is the edit as asked
  /// (EP1-RESULT-PRESENTATION-001); a respelling there is passed over too.
  /// The typed-line [tier] instead commits the first spelling past the tier
  /// when none qualifies (see [_Tier.firstPast]), or before it [atLimit],
  /// when a respelling was passed over at a limit.
  _Outcome _commitSpellings(
    Iterable<Spelling> spellings,
    bool Function(FlarkDocument next, Spelling spelling, Edits edits) keep, {
    required bool coalesce,
    _Tier tier = _Tier.asAsked,
    bool completeTypedFence = false,
    Spelling? atLimit,
  }) {
    // A refusal the search passes over is no outcome of the call.
    final before = _lastRejection;
    Spelling? past;
    var passedOver = false;
    for (final spelling in spellings) {
      final edits = spelling.edits;
      final outcome = _attempt(
        edits.apply(source),
        spelling.selection,
        coalesce: coalesce,
        pending: spelling.pending,
        completeTypedFence: completeTypedFence,
        acceptSourceMode: tier == _Tier.asAsked && spelling.asAsked,
        accept: (next) => keep(next, spelling, edits),
        acceptCompleted: (next, at, length) => keep(
          next,
          spelling,
          edits.withInsertion(at, next.source.substring(at, at + length)),
        ),
      );
      switch (outcome) {
        case _NotKept(passedOver: true):
          past ??= spelling;
          if (!spelling.asAsked) passedOver = true;
        case _NotKept():
          break;
        case _Refused() when !spelling.asAsked:
          passedOver = true;
          _lastRejection = before;
        case _Committed() || _Unchanged() || _Refused():
          return outcome;
      }
    }
    if (tier == _Tier.firstPast) {
      if (atLimit != null && passedOver) {
        return _attempt(
          atLimit.edits.apply(source),
          atLimit.selection,
          coalesce: coalesce,
          pending: atLimit.pending,
        );
      }
      if (past != null) {
        return _attempt(
          past.edits.apply(source),
          past.selection,
          coalesce: coalesce,
          pending: past.pending,
          completeTypedFence: completeTypedFence,
          acceptSourceMode: true,
          accept: (_) => false,
          acceptCompleted: (_, _, _) => false,
        );
      }
    }
    return _NotKept(passedOver: passedOver);
  }

  /// Whether [text], typed, is one keystroke, which history joins to the
  /// typing before it within the coalescing window: a single grapheme other
  /// than a line break. A typed line break, a paste or text typed in one
  /// go is an undo step of its own.
  static bool _keystroke(String text) =>
      text != '\n' && text.characters.length == 1;

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
      coalesce: typing && _keystroke(text),
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
      coalesce: false,
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
      coalesce: sel.isCollapsed && !word,
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
      _commit(text, FlarkSelection.collapsed(text.length), coalesce: false);

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
    // An autolink hides its delimiters too, and its text is its address: a
    // range over part of the address edits the address, one over all of it
    // replaces the link, delimiters and all.
    if (_doc.autolinkAround(start, end) case final link?) {
      final a = start < link.contentStart ? link.contentStart : start;
      final b = end > link.contentEnd ? link.contentEnd : end;
      if (a == link.contentStart && b == link.contentEnd) {
        return (start: link.start, end: link.end);
      }
      if (a < b) return (start: a, end: b);
    }
    return (start: start, end: end);
  }

  /// A word that shows all of an inline owner's text and text beside it,
  /// where the word's edge falls inside the owner's hidden delimiters (the
  /// word `bold ` deleted back from after `**bold** `), covers the owner:
  /// its delimiters go with its text, as deleting all of an owner's text
  /// takes them (EP1-DELETE-TO-EMPTY), so none is stranded. A word of
  /// exactly an owner's text stays its content. An explicit selection keeps
  /// its own range (see [_supportedRange]).
  ({int start, int end}) _coverOwners(({int start, int end}) range) {
    var (:start, :end) = range;
    if (start == end) return range;
    for (var grew = true; grew;) {
      grew = false;
      // An autolink's delimiters are hidden as an owner's are.
      for (final o in [
        ..._doc.ownersAt(start),
        ..._doc.ownersAt(end),
        ?_doc.autolinkAround(start, start),
        ?_doc.autolinkAround(end, end),
      ]) {
        final covers = start <= o.contentStart && end >= o.contentEnd;
        final content = start == o.contentStart && end == o.contentEnd;
        final whole = start <= o.start && end >= o.end;
        if (covers && !content && !whole) {
          if (o.start < start) start = o.start;
          if (o.end > end) end = o.end;
          grew = true;
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
    // of the table and leave the caret before the delimiter it made. A typed
    // backslash must show as typed too: against the cell's delimiter it
    // would escape it (GFM reads any backslash before a pipe as escaping it,
    // a doubled one too), so there it is refused. Paste and source mode keep
    // Markdown's literal meaning, and a delimiter row shown as its source is
    // no cell.
    if (typing &&
        row.kind == RowKind.tableCell &&
        !row.delimiterSource &&
        (typed.contains('|') || typed.contains(r'\'))) {
      return _insertText(
        row,
        range,
        typed,
        typed.replaceAll('|', r'\|'),
        cell: true,
        typing: true,
      );
    }
    return _insertText(row, range, typed, typed, cell: false, typing: typing);
  }

  /// Puts [text] in over [range] of [row], for [_insert] and [_replace]:
  /// [typed] as the cell escapes it when [cell]; without a pending style's
  /// delimiters around it unless [styled].
  bool _insertText(
    ProjectedRow row,
    ({int start, int end}) range,
    String typed,
    String text, {
    required bool cell,
    required bool typing,
    bool styled = true,
  }) {
    final collapsed = range.start == range.end;
    if (row.bodyless) {
      final code = _pasteCode(text, from: range.start, to: range.end);
      if (code != null) return code;
    }
    // Typed text goes to the code delegate first. What it does not propose,
    // and pasted text, is literal code. Composed text reaches this path only
    // through [_typeComposed], which sets the composition aside; in code it
    // stays literal (see [_compose]).
    final code =
        (typing
            ? _delegateCodeEdit(CodeEditingAction.insert, text: text)
            : null) ??
        _pasteCode(
          text,
          from: range.start,
          to: range.end,
          typing: typing && _keystroke(text),
        );
    if (code != null) return code;
    // A task item's checkbox ends at the space after it: text typed against
    // it would run into the checkbox and show its source. When the item's
    // text is on the next line, that space is the row's first, and the text
    // goes after it.
    if (collapsed &&
        row.shells.isNotEmpty &&
        row.shells.last.kind == ShellKind.item &&
        row.shells.last.checkboxEnd == range.start &&
        range.start < source.length &&
        (source.codeUnitAt(range.start) == 0x20 ||
            source.codeUnitAt(range.start) == 0x09)) {
      range = (start: range.start + 1, end: range.start + 1);
    }
    if (collapsed &&
        styled &&
        _continueSpan(range.start, text, typing: typing) is _Committed) {
      return true;
    }
    var inserted = text;
    var caret = range.start + text.length;
    final p = styled ? _pending : null;
    PendingStyle? pending;
    // Where a pending style's opening delimiter goes in [inserted], and how
    // much text it wraps; -1 without one.
    var wrapAt = -1, wrapped = 0;
    // Empty-owner intent exits on whitespace. A surviving span continues
    // across spaces, keeping its delimiters around non-whitespace content.
    if (p != null && collapsed && text.trim().isNotEmpty) {
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
      if (!text.contains('\n') && !text.contains('\r')) {
        (wrapAt, wrapped) = (first, last - first);
      }
      caret = range.start + p.open.length + text.length;
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
        collapsed &&
        p.continueAcrossSpaces &&
        !text.contains('\n') &&
        !text.contains('\r')) {
      pending = p;
    }
    var start = range.start, end = range.end;
    if (!collapsed && text.trim().isEmpty) {
      final expanded = _rangeForEmptying(start, end);
      final heading = _emptySetext(
        expanded.start,
        expanded.end,
        inserted,
        typing: typing && _keystroke(typed),
      );
      if (heading != null) return heading;
      start = expanded.start;
      end = expanded.end;
      caret = start + inserted.length;
    }
    // Text typed into an empty closed heading can go before the caret.
    final (at, gap) = collapsed
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
    // typed text, and every other cell is unchanged. Other text keeps them
    // too, the edited cell showing whatever Markdown reads in it.
    final cells = cell
        ? (_tableRowCells(projection, row)
            ..[row.column] = row.text.replaceRange(
              row.displayForSource(range.start).$1,
              row.displayForSource(range.end).$1,
              typed,
            ))
        : null;
    final kept = cell ? null : _cellsKept(row, text);
    final one = typing && _keystroke(typed);
    final fence =
        typing &&
        collapsed &&
        inserted == text &&
        (text == '`' || text == '```' || text == '~' || text == '~~~') &&
        row.kind != RowKind.codeBlock;
    final underline =
        typing &&
            collapsed &&
            inserted == text &&
            row.text.isEmpty &&
            row.kind != RowKind.codeBlock &&
            _underlineRun.hasMatch(text)
        ? text
        : null;
    // A pending style's delimiters must pair around the text: after a
    // backslash, inside an autolink or beside another delimiter run they
    // would be painted, or pair with that run and show or hide its
    // characters. The line typed on must show what it showed with the text
    // in it, whitespace at its edges aside (Markdown's to show or strip);
    // otherwise the text goes in without the style.
    final shown = wrapAt < 0
        ? null
        : () {
            final d = row.displayForSource(range.start).$1;
            final (start, end) = row.displayLineAt(d);
            return row.text
                .substring(start, end)
                .replaceRange(d - start, d - start, text)
                .trim();
          }();
    bool wraps(FlarkDocument next, int typedAt) {
      if (wrapAt < 0) return true;
      if (!_TypedLines._wrapShows(next, typedAt + wrapAt, p!, wrapped)) {
        return false;
      }
      final at = typedAt + wrapAt + p.open.length;
      final typedRow = next.rowAt(at);
      final (start, end) = typedRow.displayLineAt(
        typedRow.displayForSource(at).$1,
      );
      return typedRow.text.substring(start, end).trim() == shown;
    }

    var outcome = gap.isEmpty && cells == null
        ? _typeOnLine(
            row,
            at,
            inserted,
            normalized,
            typed,
            typing: one,
            removed: end - start,
            fence: fence,
            underline: underline,
            wraps: wrapAt < 0 ? null : wraps,
          )
        : null;
    if (outcome == null) {
      // Text completing block markup can hide the caret's own line, which
      // sends the caret to another: a table's delimiter row, or a fence's
      // opening line. Text the parser reads as kept but for the line it
      // hides tries the respellings that keep the line (see [_unhideLine]),
      // and failing them goes in as typed.
      bool accept(FlarkDocument next) =>
          (cells == null ||
              _showsTableRow(next, normalized.caret, row.column, cells)) &&
          (kept == null ||
              _showsTableRow(
                next,
                normalized.caret,
                row.column,
                kept,
                edited: true,
              )) &&
          (gap.isEmpty ||
              _keepsStructure(
                next,
                Edits([(start, end, '$inserted$gap')]),
                const {},
              )) &&
          wraps(next, at);
      bool keepsLine(FlarkDocument next) =>
          !one ||
          row.kind == RowKind.tableCell ||
          next.model.lineOfUtf16(next.selection.extent) ==
              next.model.lineOfUtf16(normalized.caret);

      outcome = _attempt(
        normalized.text,
        FlarkSelection.collapsed(normalized.caret),
        pending: normalized.pending,
        coalesce: one,
        accept: (next) => accept(next) && keepsLine(next),
        acceptSourceMode: cells == null,
        completeTypedFence: fence,
      );
      final read = outcome is _NotKept ? outcome.read : null;
      if (read != null && !keepsLine(read) && accept(read)) {
        outcome = _unhideLine(row, at, end - start, normalized, typed);
        if (outcome is _NotKept || outcome is _Unchanged) {
          outcome = _attempt(
            normalized.text,
            FlarkSelection.collapsed(normalized.caret),
            pending: normalized.pending,
            coalesce: one,
            acceptSourceMode: true,
            accept: accept,
          );
        }
      }
    }
    // Text the pending style's delimiters cannot wrap goes in without them,
    // unless it was refused. History keeps the intent it was typed with.
    if (wrapAt >= 0 && (outcome is _NotKept || outcome is _Unchanged)) {
      return _insertText(
        row,
        range,
        typed,
        text,
        cell: cell,
        typing: typing,
        styled: false,
      );
    }
    return outcome is _Committed;
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
  /// takes its own pair as before. The continuation respells the word typed
  /// with its own pair, so one the parser refuses is passed over too. Null
  /// when this does not apply.
  _Outcome? _continueSpan(int at, String text, {required bool typing}) {
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
    return _commitSpellings(
      [
        Spelling(
          Edits([(closeStart, at, '$word${p.close}$trailing')]),
          FlarkSelection.collapsed(
            trailing.isEmpty ? end - p.close.length : end + trailing.length,
          ),
          // Trailing spaces leave the span again and keep its intent.
          pending: trailing.isEmpty ? null : p,
        ),
      ],
      (document, _, _) => document
          .ownersTouching(owner.start)
          .any((o) => o.start == owner.start && o.end == end),
      coalesce: typing && _keystroke(text),
    );
  }

  /// The mirror of [_continueSpan]: a word typed where an emphasis, strong or
  /// strikethrough span's first word was deleted, before the spaces that now
  /// lead it, rejoins that span. Its opening syntax moves before the word,
  /// giving `**new two**` rather than `**new** **two**`. The parser must see
  /// one span of the same kind from the moved opening to the old closing
  /// syntax; otherwise the word takes its own pair. Null when this does not
  /// apply.
  _Outcome? _continueSpanBefore(int at, String text, {required bool typing}) {
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
    final moved =
        '${text.substring(0, first)}${p.open}${text.substring(first)}'
        '${source.substring(at, gap)}';
    return _commitSpellings(
      [
        Spelling(
          Edits([(at, gap + p.open.length, moved)]),
          FlarkSelection.collapsed(start + p.open.length + text.length - first),
        ),
      ],
      (document, _, _) => document
          .ownersTouching(start)
          .any(
            (o) =>
                o.start == start &&
                o.kind == owner.kind &&
                o.end == owner.end + text.length,
          ),
      coalesce: typing && _keystroke(text),
    );
  }

  /// The parser identifies a newly typed, bare three-character opener, its
  /// last marker typed anywhere in the run. Pair it before publication, even
  /// if a later existing fence would close it. Source input and paste retain
  /// Markdown's ordinary unclosed-fence meaning. [at] is where the body and
  /// closing fence go into [candidate].
  ({String source, int caret, int at})? _completeTypedFence(
    String candidate,
    int caret,
    RenderModel model,
  ) {
    final line = model.lineOfUtf16(caret);
    for (final block in model.blocks) {
      if (block.kind != BlockKind.codeBlock ||
          block.flags & BlockFlag.fenced == 0 ||
          block.attr != 3 ||
          block.firstLine != line ||
          caret <= block.startUtf16 ||
          caret > block.startUtf16 + block.attr ||
          model.codeInfoStart(block.index) != model.codeInfoEnd(block.index)) {
        continue;
      }
      // Preserve quote markers and indentation. A parser-owned item marker
      // or footnote label becomes the indentation that continues it, so
      // subsequent lines stay in the same item, including tab-padded and
      // nested items, or footnote.
      final prefix = continuationPrefix(
        candidate,
        model,
        line,
        block.startUtf16,
        block.index,
      );
      final fence = candidate.substring(
        block.startUtf16,
        block.startUtf16 + block.attr,
      );
      // An empty info range may follow ASCII padding on the opener line.
      final openerEnd = model.codeInfoStart(block.index);
      final newline = candidate.startsWith('\r\n', openerEnd) ? '\r\n' : '\n';
      final inserted =
          '$newline$prefix$newline$prefix$fence$newline$prefix'
          '${openerEnd == candidate.length ? newline : ''}';
      return (
        source: candidate.replaceRange(openerEnd, openerEnd, inserted),
        caret: openerEnd + newline.length + prefix.length,
        at: openerEnd,
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
      final row = _doc.rowAt(s);
      return row.delimiterSource
          ? _cutDelimiterRow(row, s, e, typing: false)
          : _deleteContent(s, e, typing: false);
    }
    // A replacement goes in as a paste of its text over its range does,
    // without a pending style, so the routes of one insertion agree:
    // literal code in a fenced body, text in an empty owner, text after a
    // lone task checkbox's space.
    return _insertText(
      _doc.rowAt(s),
      (start: s, end: e),
      text,
      text,
      cell: false,
      typing: false,
      styled: false,
    );
  }

  /// The cells of [row]'s table row, which one line of [text] put in that
  /// cell must keep, or null outside a table's header or body: a pipe or
  /// whitespace that would split the row, end it or end the table is
  /// refused, as table restructuring uses source mode.
  List<String>? _cellsKept(ProjectedRow row, String text) =>
      row.kind == RowKind.tableCell &&
          !row.delimiterSource &&
          !text.contains('\n') &&
          !text.contains('\r')
      ? _tableRowCells(projection, row)
      : null;

  bool _delete({required bool backward, bool word = false}) {
    final sel = selection;
    if (_wholeRange(sel.start, sel.end)) return _replaceWhole('');
    final pos = _doc.displayOf(sel.extent);
    final row = projection.rows[pos.row];
    final d = pos.offset;
    if (sel.isCollapsed && _atDocumentEdge(row, d, backward: backward)) {
      _inert = true;
      return false;
    }
    final delimiter = _deleteInDelimiterRow(backward: backward, word: word);
    if (delimiter != null) return delimiter;
    if (!sel.isCollapsed) {
      final range = _contentRange(sel.start, sel.end);
      if (!_supportedRange(range.start, range.end)) return false;
      return _deleteContent(range.start, range.end, typing: false);
    }
    if (backward ? d == 0 : d >= row.text.length) {
      return backward ? _joinBackward(row) : _joinForward(row);
    }
    // The rendered grapheme's own source bytes, hidden neighbours excluded.
    final int a, b;
    final atomic = _adjacentAtomicSegment(row, d, forward: !backward);
    if (word) {
      final covered = _coverOwners((
        start: row.sourceForDisplay(
          backward ? _wordStart(row.text, d) : d,
          anchor: Anchor.after,
        ),
        end: row.sourceForDisplay(
          backward ? d : _wordEnd(row.text, d),
          anchor: Anchor.before,
        ),
      ));
      a = covered.start;
      b = covered.end;
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
    if (join &&
        !word &&
        row.fenced &&
        row.displayForSource(b).$1 != atomic.displayEnd) {
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
      for (final r in _doc.hardBreaksOf(row)) {
        if (r.startUtf16 <= a && r.endUtf16 >= a && r.endUtf16 <= b) {
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

  /// Whether a collapsed deletion at display offset [d] of [row] has nothing
  /// to delete, at the start of the first row with nothing to lift (no
  /// heading marker, rule, empty fence, container or indentation) or at the
  /// end of the last: hidden markup beside the caret (a closing `**`, a
  /// fence, a cell's pipe) is no grapheme of its own. It does nothing, and is
  /// no refused edit a host would report.
  bool _atDocumentEdge(ProjectedRow row, int d, {required bool backward}) {
    if (!backward) {
      return d >= row.text.length && row.index == projection.rows.length - 1;
    }
    if (d != 0 || row.index != 0 || row.shells.isNotEmpty) return false;
    final line = _doc.model.lineOfUtf16(selection.extent);
    final i = (line - row.firstLine).clamp(0, row.contentStarts.length - 1);
    // A byte order mark starting the document is no prefix to delete.
    return (row.prefixStarts[i] == row.contentStarts[i] ||
            row.kind == RowKind.blank &&
                row.contentStarts[i] ==
                    lineStartPastMark(source, _doc.model, line)) &&
        row.kind != RowKind.heading &&
        row.kind != RowKind.thematicBreak &&
        !_isBareHeading(row) &&
        !(row.fenced && row.text.isEmpty);
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
    return _deleteKept(expanded.start, expanded.end, normalized, typing);
  }

  /// A deletion inside a row changes that row only: other rows keep their
  /// kinds and containers, the row its kind, a table row its cells, and an
  /// emptied line shows none of its prefix. Otherwise it respells what it
  /// emptied, checked by the parser: spaces after text deleted at a line's
  /// start go too, keeping an item's content column; an emptied line goes
  /// with its line break, or takes the blank lines after it (an item's first
  /// line) or a blank line before it (an item under a paragraph's line); an
  /// emptied cell of a row without its outer pipe keeps one. With none, a
  /// deletion that empties its line or a cell and changes another row, its
  /// table row or what its line shows refuses; another goes ahead as Markdown
  /// reads it (literal HTML and definitions too), so erasing never sticks.
  bool _deleteKept(
    int start,
    int end,
    ({String text, int caret, PendingStyle? pending}) plain,
    bool typing,
  ) {
    final row = _doc.rowAt(start), m = _doc.model, rows = projection.rows;
    final top = row.nearestLineIndexOf(m, start),
        bottom = row.nearestLineIndexOf(m, end);
    final at = lineStartPastMark(source, m, row.firstLine + top);
    final cs = row.contentStarts[top], ce = row.contentEnds[bottom];
    final a = row.displayForSource(start).$1, b = row.displayForSource(end).$1;
    final ls = row.displayForSource(cs).$1, le = row.displayForSource(ce).$1;
    final emptied =
        cs >= 0 && _blanks(row.text, ls, a) && _blanks(row.text, b, le);
    final below = row.contentStarts.elementAtOrNull(bottom + 1) ?? -1;
    final cells = row.kind == RowKind.tableCell
        ? _tableRowCells(projection, row)
        : null;
    // A table row is checked whole, its cells' text by [_showsTableRow].
    final cell0 = row.index - (cells == null ? 0 : row.column);
    final edited = {for (var c = 0; c < (cells?.length ?? 1); c++) cell0 + c};
    // Whether [next], which [edits] made, keeps every other row, and its own
    // rows as the deletion must.
    (bool, bool) check(FlarkDocument next, Edits edits) {
      int map(int offset) => edits.forward(offset, caret: true);
      final caret = next.selection.extent;
      final shown = cells == null
          ? !emptied || !_paints(next.rowAt(caret), map(at), map(cs))
          : _showsTableRow(next, caret, row.column, cells, edited: true);
      // The row's remaining text, before the deletion, after it or on the
      // next line, keeps its containers, and its kind unless it moved up
      // onto the emptied line: left below it, the line after a setext
      // heading's emptied first line could read as indented code.
      final onward = a == 0 && emptied && below >= 0,
          left = a > 0
              ? map(row.sourceStart)
              : emptied
              ? (below < 0 ? -1 : map(below))
              : caret,
          up = onward && left == map(start);
      // A table row shows none of its other source either: a backslash the
      // deletion leaves before the cell's delimiter would escape it, and an
      // emptied first cell of a row without its leading pipe would make that
      // pipe lead the row, so a cell the table drops would show. Nor do the
      // lines left below an emptied first line, as a heading's underline
      // would.
      return (
        shown &&
            _keepsStructure(
              next,
              edits,
              edited,
              movesText: cells != null || onward && !up,
              shown: cells == null ? null : (row.sourceStart, row.sourceEnd),
              shells: true,
            ),
        (!emptied || next.rowAt(caret).sameContainerKinds(row)) &&
            (left < 0 ||
                next.rowAt(left).sameContainerKinds(row) &&
                    (up || next.rowAt(left).kind == row.kind)),
      );
    }

    var spaced = end, k = row.index + 1;
    while (spaced < ce && _isSpace(source, spaced)) {
      spaced++;
    }
    while (k < rows.length && rows[k].kind == RowKind.blank) {
      k++;
    }
    int startOf(ProjectedRow row) =>
        lineStartPastMark(source, m, row.firstLine);
    final gap = emptied && k > row.index + 1 && k < rows.length
        ? (startOf(rows[row.index + 1]), startOf(rows[k]), '')
        : null;
    final item = row.shells
        .where((s) => s.kind == ShellKind.item && m.blockStart(s.block) >= at)
        .lastOrNull;
    final prefix = source
        .substring(
          at,
          item == null ? (cs < at ? at : cs) : m.blockStart(item.block),
        )
        .trimRight();
    final apart = (at, at, '$prefix${_lineBreakAt(at)}'),
        cut = (start, end, '');
    final above = top > 0 ? row.contentEnds[top - 1] : -1;
    // The deletion as asked, read as the removal of [start]..[end]: the
    // whitespace it moves out of a span's delimiters keeps the length.
    final asked = Edits([(start, end, '')]),
        // The normalization moves whitespace only within the row and the
        // range it deletes, so the comparison need not go past them.
        made = Edits.between(
          source,
          plain.text,
          from: start < row.sourceStart ? start : row.sourceStart,
          to: end > row.sourceEnd ? end : row.sourceEnd,
        );
    final deletion = Spelling(
      made,
      FlarkSelection.collapsed(plain.caret),
      pending: plain.pending,
      asAsked: true,
    );
    // Failing every spelling, the deletion as asked goes ahead as Markdown
    // reads it, unless it empties its line or a cell and changes another
    // row: literal HTML and definitions go ahead even then.
    final literal = row.kind == RowKind.htmlBlock || row.block < 0;
    final readAsIs = Spelling(
      made,
      FlarkSelection.collapsed(plain.caret),
      pending: plain.pending,
      asAsked: true,
    );
    return _commitSpellings(
          [
            deletion,
            for (final edits in [
              if (cells != null) ...[
                if (emptied) [(start, end, '|')],
                if (emptied) [(start, end, '||')],
              ] else ...[
                if (spaced > end && !emptied && ls == a) [(start, spaced, '')],
                if (gap != null) [cut, gap],
                if (emptied && below >= 0) [(start, below, '')],
                if (emptied && below < 0 && above >= 0) [(above, end, '')],
                if (row.firstLine + top > 0 && cs >= 0) [apart, cut],
                if (gap != null && row.firstLine + top > 0) [apart, cut, gap],
              ],
            ])
              Spelling.carrying(
                Edits(edits),
                FlarkSelection.collapsed(start),
                pending: plain.pending,
              ),
            readAsIs,
          ],
          (next, spelling, edits) {
            if (identical(spelling, readAsIs)) {
              final (kept, _) = check(next, asked);
              return kept || !(emptied || cells != null) || literal;
            }
            final (kept, own) = check(
              next,
              identical(spelling, deletion) ? asked : edits,
            );
            return kept && own;
          },
          coalesce: typing,
        )
        is _Committed;
  }

  /// Whether [row] paints source between [start] and [end].
  static bool _paints(ProjectedRow row, int start, int end) =>
      start < end &&
      row.segments.any(
        (s) => !s.lineBreak && s.sourceStart < end && s.sourceEnd > start,
      );

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
        // Past a line break the opening syntax goes after the next line's
        // container prefix, which would otherwise follow it as text.
        if (text.codeUnitAt(first++) != 0x0A) continue;
        final row = _doc.rowAt(o.start);
        final l = row.lineIndexOf(_doc.model, first + removed);
        final resume = l >= 0 && l < row.lineCount
            ? row.contentStarts[l] - removed
            : -1;
        if (resume > first && resume <= last) first = resume;
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
        final ownerRow = projection.rows[at.row];
        if (caret < first && ownerRow.displayLineAt(at.offset).$1 < at.offset) {
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
  /// the previous row. Only a table row's cells share a line, and neither
  /// Backspace nor Delete joins from a cell, so the rows a join meets lie on
  /// lines of their own.
  bool _joinBackward(ProjectedRow row) {
    if (row.kind == RowKind.tableCell) return false;
    if (row.kind == RowKind.heading || _isBareHeading(row)) {
      final lifted = _setHeading(0);
      if (lifted is _Committed) return true;
      // An empty heading whose marker cannot go without changing the block
      // before it (in a list, the emptied item's `- ` would underline the
      // paragraph above) goes with its line instead, as Delete at the end
      // of that paragraph takes it.
      if (row.text.isNotEmpty || row.index == 0 || lifted is _Refused) {
        return false;
      }
      final prev = projection.rows[row.index - 1];
      return _joinRows(prev, row);
    }
    if (row.fenced && row.text.isEmpty) {
      final block = _doc.model.blockAt(row.block);
      // Only a closed fence's block range is the whole construct. An unclosed
      // one ends at its opening line, so deleting that range leaves any
      // closing delimiter behind as a new unclosed block.
      if (block.flags & BlockFlag.closed != 0) {
        final start = block.startUtf16, end = block.endUtf16;
        return _commit(
          source.replaceRange(start, end, ''),
          FlarkSelection.collapsed(start),
          coalesce: false,
          acceptSourceMode: true,
          // The blocks around it must stay as they were. A fence at a column
          // outside a list or quote ends that container, and once it is gone
          // what follows can run on into the container instead.
          accept: (next) => _keepsStructure(next, Edits([(start, end, '')]), {
            row.index,
          }, shells: true),
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
          _removeLineAbove(projection.rows[row.index - 1], row) is _Committed;
    }
    final i = row.nearestLineIndexOf(_doc.model, selection.extent);
    final line = row.firstLine + i;
    final prefixStart = row.prefixStarts[i],
        contentStart = row.contentStarts[i];
    if (prefixStart >= 0 && prefixStart < contentStart) {
      // A lifted line that would lazily continue the previous paragraph
      // stays a paragraph of its own (see [_leavingPrefix]).
      final prev = row.index > 0 ? projection.rows[row.index - 1] : null;
      final replacement = _leavingPrefix(
        row,
        line,
        prefixStart,
        apart:
            row.text.isNotEmpty &&
            prev != null &&
            prev.kind == RowKind.paragraph &&
            prev.firstLine + prev.lineCount == row.firstLine,
      );
      final caret = prefixStart + replacement.length;
      final lifted = _commitApart(
        row,
        Edits([(prefixStart, contentStart, replacement)]),
        FlarkSelection.collapsed(caret),
        prefixStart,
        // The lifted line keeps its kind: lifting `> ` from `> foo` above
        // `---` would make it a setext heading that swallows the rule. Only
        // indented code becomes the paragraph its text reads as, in no new
        // container (`>a` would open a quote), and a line that displays
        // nothing has no kind to keep.
        (next) {
          final now = next.rowAt(caret);
          return row.text.isEmpty ||
              now.kind == row.kind ||
              row.kind == RowKind.codeBlock &&
                  now.kind == RowKind.paragraph &&
                  now.shells.length <= row.shells.length;
        },
      );
      if (lifted is _Committed) return true;
      if (lifted is _Refused) return false;
      // An empty line whose prefix cannot go alone (without its `>`, the
      // line between two items of a quoted list would end the quote) goes
      // whole, as an empty line without a prefix joins the row before it.
      if (row.kind != RowKind.blank || row.index == 0) return false;
      final above = projection.rows[row.index - 1];
      return _joinRows(above, row);
    }
    if (row.index == 0) {
      // Nothing precedes this row to join onto. A thematic break is the one
      // row that displays nothing yet owns a range the crate guarantees is
      // exactly its line's content, so it can go on its own; anything else
      // would be deleting through a range comrak does not pin down.
      if (row.kind != RowKind.thematicBreak) return false;
      // The blocks after the rule stay in containers of the same kinds, and
      // keep their kinds.
      final start = row.sourceStart, end = row.sourceEnd;
      final removed = _attempt(
        source.replaceRange(start, end, ''),
        FlarkSelection.collapsed(start),
        coalesce: false,
        acceptSourceMode: true,
        accept: (next) => _keepsStructure(
          next,
          Edits([(start, end, '')]),
          {row.index},
          movesText: false,
          shells: true,
        ),
      );
      if (removed is _Committed) return true;
      // A blank line belongs to whichever container its neighbors give it,
      // so the first row that is not one is the block that counts.
      final following = projection.rows
          .skip(1)
          .where((r) => r.kind != RowKind.blank)
          .firstOrNull;
      // A rule that was all of a list item leaves that item empty, and an
      // empty item's content starts one column past its marker rather than
      // where the rule did, so a block too shallow for the rule's item can
      // move into it. The rule's whole line goes instead, marker and all, as
      // Delete on the rule takes it.
      return following?.index == 1 &&
          removed is! _Refused &&
          _removeLineAbove(row, following!) is _Committed;
    }
    final prev = projection.rows[row.index - 1];
    return _joinRows(prev, row);
  }

  /// What replaces the innermost container prefix of [row]'s [line], from
  /// [prefixStart] to where the line's content starts, when the line leaves
  /// that container: nothing, or, to keep the line apart from the one before
  /// it ([apart]), a line break and the line's outer prefix, which keep it in
  /// the containers around. A container that opens on the line has its
  /// marker in that prefix, where it would open another (`- >` would give
  /// two items): such a line cannot be lazy and stays as it is.
  String _leavingPrefix(
    ProjectedRow row,
    int line,
    int prefixStart, {
    required bool apart,
  }) {
    final m = _doc.model;
    final opens = row.shells.any(
      (shell) =>
          shell.kind != ShellKind.list &&
          m.blockFirstLine(shell.block) == line &&
          m.blockStart(shell.block) < prefixStart,
    );
    if (!apart || opens) return '';
    final outer = source.substring(m.lineStartUtf16(line), prefixStart);
    return '${_lineBreakAt(prefixStart)}$outer';
  }

  /// Delete at a row end: join the next row onto this one. [_delete] answers
  /// the end of the last row as the document's edge ([_atDocumentEdge]), so
  /// a next row follows.
  bool _joinForward(ProjectedRow row) {
    if (row.kind == RowKind.tableCell) return false;
    final next = projection.rows[row.index + 1];
    // As in _joinBackward, no join crosses the opening fence of code.
    if (next.fenced && next.text.isNotEmpty) {
      return _removeLineAbove(row, next) is _Committed;
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
      // An empty fence after it goes whole instead of joining its closer,
      // and the caret stays at the end of the code, not past its closer.
      if (right.fenced) {
        final from = _lastCaretEnd(left), code = _lastContentEnd(left);
        final end = projection.lineContentEnd(
          right.firstLine + right.lineCount - 1,
        );
        return from >= 0 &&
            end > from &&
            _joinContent(from, end, null, rows, at: code, movesText: false);
      }
    }
    // A row that displays nothing gives way to the one joined onto it.
    bool empty(ProjectedRow row) =>
        row.kind == RowKind.blank || row.kind == RowKind.thematicBreak;
    // A table row keeps its cells, so only an empty line or a rule, which
    // goes whole, joins one: anything else joined onto its line would be
    // read as a cell, or as one the table drops from view.
    if (left.kind == RowKind.tableCell && !empty(right) ||
        right.kind == RowKind.tableCell && !empty(left)) {
      return false;
    }
    // An empty line or a rule before a row with text goes whole, so the row
    // keeps its own prefix and markup: joining onto the line would delete a
    // heading's `#` or a quote's `>` instead of the gap, and text joined
    // after a rule would paint its markup. Backspace after a rule, or Delete
    // on it, removes the rule's line, and the next row takes its place in
    // its own containers. A row that displays nothing joins as usual, which
    // removes that row.
    if (empty(left) && (right.text.isNotEmpty || right.fenced)) {
      final removed = _removeLineAbove(left, right);
      if (removed is _Committed) return true;
      // A rule that opens an item takes the item's marker with its line, so
      // the next row joins the rule's line instead, staying in that item.
      return left.kind == RowKind.thematicBreak &&
          removed is! _Refused &&
          _joinContent(left.sourceStart, to, _checkedKind(right.kind), {
            left.index,
          });
    }
    final from = _lastCaretEnd(left);
    if (from < 0 || to <= from) return false;
    if (right.text.isNotEmpty &&
        (_headingTrail(left) != null || _headingTrail(right) != null)) {
      return _joinHeadingText(left, right, from, to);
    }
    // Joining the line break before a fence that displays nothing turns its
    // delimiter line into text: the way to delete one that has no body.
    final bodyless = right.bodyless;
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
      // strand Backspace at the end of the document. It does bring the row
      // after it up against [left], where [_joinContent] keeps it in its own
      // containers: without the gap, a paragraph after a quote or list item
      // would read on lazily inside it.
      movesText: !empty(right) || right.fenced,
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
  /// is refused when [row] changes kind or moves out of its containers, or
  /// when [_keepsStructure] fails. Code that stays code in containers of the
  /// same kinds keeps its text: its own lines are untouched, and only the
  /// line opening an item could change their columns, which takes the code
  /// out of that item.
  _Outcome _removeLineAbove(ProjectedRow above, ProjectedRow row) {
    if ((above.kind != RowKind.blank && above.kind != RowKind.thematicBreak) ||
        above.firstLine + above.lineCount != row.firstLine) {
      return const _NotKept();
    }
    final m = _doc.model;
    final start = lineStartPastMark(source, m, above.firstLine);
    final end = m.lineStartUtf16(row.firstLine);
    final caret = _firstCaretStart(row) - (end - start);
    return _attempt(
      source.replaceRange(start, end, ''),
      FlarkSelection.collapsed(caret),
      coalesce: false,
      acceptSourceMode: true,
      accept: (next) {
        final now = next.rowAt(caret);
        if (now.kind != row.kind || !now.sameContainerKinds(row)) {
          return false;
        }
        // Without the gap, [row]'s text can run on from the paragraph above
        // and pair delimiters with it, painting markup that closed a span.
        return _keepsStructure(
          next,
          Edits([(start, end, '')]),
          {above.index, row.index},
          movesText: row.text.isNotEmpty,
          shells: true,
        );
      },
    );
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
  /// heading's text ([_emptiedSetext]) instead respells it as an empty ATX
  /// heading of its level in the same containers, as a level change does,
  /// followed by [text]. The caret follows [text], where typing goes on in
  /// the heading unless [text] breaks the line. Null when the edit is not in
  /// a setext heading or leaves some of its text; false when the parser would
  /// move or change another block.
  bool? _emptySetext(
    int start,
    int end,
    String text, {
    required bool typing,
    PendingStyle? pending,
  }) {
    // Only Markdown's own whitespace leaves the heading empty: another space
    // (U+00A0, U+3000) is text the heading keeps.
    if (text.codeUnits.any(
      (u) => u != 0x20 && u != 0x09 && u != 0x0A && u != 0x0D,
    )) {
      return null;
    }
    final row = _doc.rowAt(start), atx = _emptiedSetext(row, start, end);
    if (atx == null) return null;
    final replaced = '${atx.marker}$text';
    final origin = atx.start + atx.marker.length;
    return _commit(
      source.replaceRange(atx.start, atx.end, replaced),
      FlarkSelection.collapsed(origin + text.length),
      coalesce: typing,
      pending: pending,
      acceptSourceMode: true,
      accept: (next) {
        final now = next.rowAt(origin);
        // The heading keeps its first line's prefix, and with it the
        // containers that line is in.
        return now.kind == RowKind.heading &&
            now.headingLevel == row.headingLevel &&
            now.text.isEmpty &&
            _keepsStructure(
              next,
              Edits([(atx.start, atx.end, replaced)]),
              {row.index},
              movesText: false,
              shells: true,
            );
      },
    );
  }

  /// The empty ATX heading that setext heading [row] becomes when an edit of
  /// [start]..[end] leaves none of its text: its level's marker in place of
  /// the row from where its text starts through its underline. Null when
  /// [row] is no setext heading or the edit leaves some of its text. What is
  /// left is read in the source, not in what is shown: only the spaces or
  /// tabs Markdown strips count as none, so an image without alt text, which
  /// shows nothing, keeps the heading. An ATX closing sequence shares the
  /// text's line and may close an empty heading; only an underline sits on a
  /// line of its own.
  ({int start, int end, String marker})? _emptiedSetext(
    ProjectedRow row,
    int start,
    int end,
  ) {
    final trail = _headingTrail(row), first = _firstCaretStart(row);
    final m = _doc.model;
    if (trail == null ||
        first > start ||
        end > trail.$1 ||
        m.lineOfUtf16(trail.$1) == m.lineOfUtf16(trail.$2) ||
        !_blanks(source, first, start) ||
        !_blanks(source, end, trail.$1)) {
      return null;
    }
    return (start: first, end: trail.$2, marker: '${'#' * row.headingLevel} ');
  }

  /// Whether [text] holds only Markdown's spaces and tabs, which show
  /// nothing at a line's edges, from [from] to [to].
  static bool _blanks(String text, int from, int to) {
    for (var i = from; i < to; i++) {
      if (!_isBlank(text, i)) return false;
    }
    return true;
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
    // The text moves ahead of [left]'s markup.
    final edits = Edits([(a, b, ''), (textEnd, lineEnd, '$gap$markup')]);
    // Sorted splices count one side of a move as new text, which the check
    // passes over. Read with the markup in place and the text new instead,
    // the markup maps back to where it was, and is checked like the text:
    // hidden source the join must not paint.
    final inPlace = kept == null
        ? edits
        : Edits([
            (a, kept.$1, '${source.substring(b, textEnd)}$gap'),
            (kept.$2, lineEnd, ''),
          ]);
    int old(int offset) {
      final text = edits.back(offset);
      return text >= 0 ? text : inPlace.back(offset);
    }

    return _commit(
      edits.apply(source),
      FlarkSelection.collapsed(a),
      coalesce: false,
      acceptSourceMode: true,
      accept: (next) =>
          next.rowAt(a).kind == left.kind &&
          _keepsStructure(
            next,
            edits,
            {left.index, right.index},
            movesText: false,
            shells: true,
          ) &&
          !_revealsHiddenText(next, old),
    );
  }

  /// Delete [from]..[to], the line break and prefixes a join of [rows]
  /// removes, with the caret at [at] when that precedes the join and at
  /// [from] otherwise. The result is kept only when the caret's row is still
  /// of [kind] (when given) and [_keepsStructure] holds, every other row in
  /// containers of the same kinds, however far from the join.
  bool _joinContent(
    int from,
    int to,
    RowKind? kind,
    Set<int> rows, {
    int? at,
    bool movesText = true,
    (int, int)? shown,
  }) {
    (from, to) = _joinedOwners(from, to);
    final caret = at != null && at >= 0 && at < from ? at : from;
    return _commit(
      source.replaceRange(from, to, ''),
      FlarkSelection.collapsed(caret),
      coalesce: false,
      acceptSourceMode: true,
      accept: (next) =>
          (kind == null || next.rowAt(caret).kind == kind) &&
          _keepsStructure(
            next,
            Edits([(from, to, '')]),
            rows,
            movesText: movesText,
            shown: shown,
            shells: true,
          ),
    );
  }

  /// Whether [next], a join or lift of [rows] made by [edits] of the current
  /// source, keeps what the user sees elsewhere. A reparse can otherwise
  /// turn it into a structural change. Every other row keeps its kind:
  /// removing the blank line between `a` and `---` makes `a` a setext
  /// heading that swallows the rule, and lifting an item's marker can turn
  /// the item's later blocks into indented code. When the edit [movesText]
  /// onto another line, nothing the projection hides is painted either: text
  /// after a closing fence or sequence paints it, and so do the brackets of
  /// references to a definition the edit merged into a paragraph. [shown] is
  /// hidden source the edit may paint. With [shells], every other row also
  /// stays in containers of the same kinds.
  bool _keepsStructure(
    FlarkDocument next,
    Edits edits,
    Set<int> rows, {
    bool movesText = true,
    (int, int)? shown,
    bool shells = false,
  }) {
    // Rows inside a container whose marker the edit removed, from its line
    // (a lift) or before text the line keeps (an item joined up), leave it;
    // others can be carried from a join, even past an empty row it removed.
    bool lifted(ProjectedRow row) => row.shells.any((shell) {
      final at = _doc.model.blockStart(shell.block);
      final line = _doc.model.lineOfUtf16(at);
      return shell.kind != ShellKind.list &&
          edits.list.any(
            (e) =>
                e.$1 <= at &&
                at < e.$2 &&
                (e.$1 >= _doc.model.lineStartUtf16(line) ||
                    e.$2 < projection.lineContentEnd(line)),
          );
    });

    // Both projections list rows in source order, so one walk pairs each row
    // with the row that now holds its start. A start past a row's end lies in
    // the markup before the next row, which then holds it.
    final now = next.projection.rows;
    var j = 0;
    for (final row in projection.rows) {
      if (row.kind == RowKind.blank || rows.contains(row.index)) continue;
      final at = edits.forward(row.sourceStart);
      if (at < 0) continue;
      while (j + 1 < now.length && now[j + 1].sourceStart <= at) {
        j++;
      }
      final holder = at > now[j].sourceEnd && j + 1 < now.length
          ? now[j + 1]
          : now[j];
      if (holder.kind != row.kind ||
          shells && !holder.sameContainerKinds(row)) {
        if (holder.kind != row.kind || !lifted(row)) return false;
      }
    }
    return !movesText || !_revealsHiddenText(next, edits.back, shown: shown);
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
  /// that the current projection hides, or with [whitespace] any character.
  /// [old] maps an offset in [next]'s source to the current one; [shown] is
  /// hidden source allowed to appear.
  bool _revealsHiddenText(
    FlarkDocument next,
    int Function(int) old, {
    (int, int)? shown,
    bool whitespace = false,
  }) {
    final rows = projection.rows;
    // Whether the current projection paints [from] through [to]. Rows come in
    // source order and do not overlap, so the last row with text that starts
    // at or before [from] is the one whose segments can paint it.
    bool painted(int from, int to) {
      if (shown != null && from >= shown.$1 && to < shown.$2) return true;
      var low = 0, high = rows.length - 1;
      while (low <= high) {
        final mid = (low + high) >> 1;
        if (rows[mid].sourceStart <= from) {
          low = mid + 1;
        } else {
          high = mid - 1;
        }
      }
      for (var r = high; r >= 0; r--) {
        final row = rows[r];
        if (row.sourceEnd <= row.sourceStart) continue;
        // Segments are in source order: from the first that ends past
        // [from], follow those that paint on from where the last ended.
        final segments = row.segments;
        var lo = 0, hi = segments.length;
        while (lo < hi) {
          final mid = (lo + hi) >> 1;
          if (segments[mid].sourceEnd <= from) {
            lo = mid + 1;
          } else {
            hi = mid;
          }
        }
        var at = from;
        for (var k = lo; k < segments.length && at <= to; k++) {
          final s = segments[k];
          if (s.sourceStart > at) break;
          if (!s.lineBreak && at < s.sourceEnd) at = s.sourceEnd;
        }
        return at > to;
      }
      return false;
    }

    for (final row in next.projection.rows) {
      // A row the projection carried over from this one paints what it
      // painted, moved with it, where [old] maps the row's ends.
      final was = next.projection.reusedFrom(row, projection);
      if (was != null &&
          row.text.isNotEmpty &&
          row.sourceEnd > row.sourceStart &&
          old(row.sourceStart) == was.sourceStart &&
          old(row.sourceEnd - 1) == was.sourceEnd - 1) {
        continue;
      }
      for (final segment in row.segments) {
        final a = segment.sourceStart, b = segment.sourceEnd;
        if (segment.lineBreak || a >= b) continue;
        final first = old(a), last = old(b - 1);
        if (first >= 0 && last - first == b - 1 - a && painted(first, last)) {
          continue;
        }
        for (var o = a; o < b; o++) {
          if (!whitespace && _isSpace(next.source, o)) continue;
          final at = old(o);
          if (at >= 0 && at < source.length && !painted(at, at)) return true;
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
    final (:start, :end) = _contentRange(sel.start, sel.end);
    final row = sel.isCollapsed
        ? projection.rows[_doc.caretPosition.row]
        : _doc.rowAt(start);
    if (row.kind == RowKind.tableCell) {
      return sel.isCollapsed && _returnFromTable(row);
    }
    if (!_supportedRange(start, end)) return false;
    if (row.kind == RowKind.codeBlock) {
      if (sel.isCollapsed && !paragraph) {
        final exited = _exitCodeOnBlankLine(row);
        if (exited != null) return exited;
      }
      return _codeNewline(row, start, end);
    }
    final line = _doc.model.lineOfUtf16(start);
    final i = (line - row.firstLine).clamp(0, row.contentStarts.length - 1);
    final prefixStart = row.prefixStarts[i],
        contentStart = row.contentStarts[i];
    // Return on an empty container line exits the container, unless the
    // blocks after it would change containers (`   b` after an empty `>`
    // under `1. a` would move into the item). An empty heading is not that
    // line: its own markup is part of the prefix, so exiting would delete the
    // heading with the container marker.
    if (sel.isCollapsed &&
        row.shells.isNotEmpty &&
        row.text.isEmpty &&
        row.kind != RowKind.heading &&
        prefixStart >= 0 &&
        prefixStart < contentStart) {
      // A line after another goes after an empty one in the outer
      // containers, which keeps text typed on it from reading on lazily.
      final replacement = _leavingPrefix(
        row,
        line,
        prefixStart,
        apart: line > 0,
      );
      final caret = prefixStart + replacement.length;
      final left = _attempt(
        source.replaceRange(prefixStart, contentStart, replacement),
        FlarkSelection.collapsed(caret),
        coalesce: false,
        acceptSourceMode: true,
        // The line leaves containers and enters none: an empty item's own
        // nested items, left without it, would take in the line after it.
        accept: (next) =>
            next.rowAt(caret).withinContainerKindsOf(row) &&
            _keepsStructure(
              next,
              Edits([(prefixStart, contentStart, replacement)]),
              {row.index},
              movesText: false,
              shells: true,
            ),
      );
      // Leaving would move the blocks after the line, as an empty item's
      // nested items would: Return does nothing, with no refusal to report.
      if (left is _NotKept) _inert = true;
      return left is _Committed;
    }
    // The new line continues the containers, a selection's as a caret at its
    // start would: an item as the next item, a quote or footnote with
    // [_rowPrefix].
    final inner = row.shells.isEmpty ? null : row.shells.last;
    final continued = inner == null
        ? paragraph && row.kind == RowKind.paragraph
              ? nl
              : ''
        : inner.kind == ShellKind.item
        ? _nextMarker(inner)
        : _rowPrefix(row, line);
    final separator = '$nl$continued';
    final split = _splitRow(row, start, end, separator);
    if (split is _Committed) return true;
    if (split is _Refused || start != end) return false;
    // A caret beside hidden syntax shows the same place from the anchors
    // around it. Where a break at its own anchor would split a construct
    // that holds no split (the end of an autolink's text, before its hidden
    // `>`), the break goes beside the construct instead: a respelling,
    // passed over when the parser refuses it.
    final before = _lastRejection;
    for (final other in _doc.anchorsAt(start)) {
      if (other == start) continue;
      if (_splitRow(row, other, other, separator) is _Committed) return true;
      _lastRejection = before;
    }
    return false;
  }

  /// The prefix that continues [row]'s containers after [line]: the line's
  /// prefix, never the markers of the items and definitions that open on it,
  /// which would open new ones, nor a heading's own marker, which belongs to
  /// the heading. A lazy line has none, so the row's first line gives it,
  /// or, when that line reads on lazily too after the link reference
  /// definitions its paragraph's block opens with, the block's first line,
  /// as for text typed on a lazy line.
  String _rowPrefix(ProjectedRow row, int line) {
    final m = _doc.model;
    var i = (line - row.firstLine).clamp(0, row.contentStarts.length - 1);
    if (row.prefixStarts[i] == row.contentStarts[i]) i = 0;
    if (row.block >= 0 &&
        row.prefixStarts[i] == row.contentStarts[i] &&
        m.blockFirstLine(row.block) < row.firstLine) {
      return _continuing(row.block);
    }
    final contentStart = row.contentStarts[i], at = row.firstLine + i;
    final block = row.block >= 0 ? row.block : row.shells.last.block;
    final end =
        row.block >= 0 &&
            m.blockFirstLine(block) == at &&
            m.blockStart(block) < contentStart
        ? m.blockStart(block)
        : contentStart;
    return continuationPrefix(source, m, at, end, block);
  }

  /// Return from [start] to [end] in [row]. A heading's underline or closing
  /// sequence stays with the part of the heading before the split, where it
  /// still ends the heading, instead of underlining the new line or being
  /// painted on it. A split that starts on an earlier line of a multi-line
  /// setext heading, or at the start of its last line, keeps the text after
  /// it above the underline, so the plain split serves.
  _Outcome _splitRow(ProjectedRow row, int start, int end, String separator) {
    final trail = _headingTrail(row);
    if (trail == null) return _splitInline(row, start, end, separator);
    final m = _doc.model;
    final line = m.lineOfUtf16(trail.$1);
    if (m.lineOfUtf16(end) != line) {
      return _splitInline(row, start, end, separator);
    }
    final first = row.lineIndexOf(m, start);
    // Text on the split's first line before it, and text after it. Display
    // offsets count what is shown, so hidden delimiters are neither.
    final before =
        row.displayForSource(start).$1 >
        row.displayForSource(row.contentStarts[first]).$1;
    final after = row.displayForSource(end).$1 < row.text.length;
    if (m.lineOfUtf16(trail.$2) == line ||
        before && first == line - row.firstLine) {
      return _splitInline(row, start, end, separator, keep: trail);
    }
    if (after) return _splitInline(row, start, end, separator);
    // The split takes the last line's text to its end. Text before it on an
    // earlier line keeps the underline after it. From the start of a later
    // line, the lines before it remain and the underline would have to move
    // up to them, which a split cannot do faithfully, so the edit is refused.
    if (before) return _splitInline(row, start, end, separator, keep: trail);
    if (first != 0) return const _NotKept();
    // From the heading's start, the split can leave the heading empty, read
    // as deleting the text it takes reads it ([_emptiedSetext]): it becomes
    // the empty ATX heading that deletion leaves, before the line break.
    final taken = _rangeForEmptying(start, end);
    final atx = _emptiedSetext(row, taken.start, taken.end);
    if (atx != null) {
      return _commitReturn(row, start, end, [
        (atx.start, atx.end, '${atx.marker}$separator'),
      ], separator);
    }
    // Text that shows nothing (an image without alt text) stays the
    // heading's, before the line break with its underline, or after it.
    return _blanks(source, _firstCaretStart(row), taken.start)
        ? _splitInline(row, start, end, separator)
        : _splitInline(row, start, end, separator, keep: trail);
  }

  bool _returnFromTable(ProjectedRow row) {
    // A delimiter row shown as its source is none of the table's rows:
    // Return ends its line, wherever on it the caret is (a break inside it
    // would split the row and dissolve the table), and the next line takes
    // the first body row. Every other row keeps its containers.
    if (row.delimiterSource) {
      final m = _doc.model, line = m.lineOfUtf16(row.sourceStart);
      final end = projection.lineContentEnd(line);
      // The new line repeats the delimiter row's container prefix.
      final nl =
          '${_lineBreakAt(end)}'
          '${source.substring(lineStartPastMark(source, m, line), row.sourceStart)}';
      return selection.isCollapsed &&
          _commit(
            source.replaceRange(end, end, nl),
            FlarkSelection.collapsed(end + nl.length),
            coalesce: false,
            acceptSourceMode: true,
            accept: (next) => _keepsStructure(next, Edits([(end, end, nl)]), {
              row.index,
            }, shells: true),
          );
    }
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
      coalesce: false,
    );
  }

  /// Split parser-owned spans with the block. Move a terminal break outside
  /// closing syntax; at an interior split close and reopen the nonempty parts.
  /// No empty delimiter pair is ever published as an intermediate document,
  /// and an empty owner, which holds no split, stays whole before the break.
  /// [keep] is heading markup after the split that stays with the first part.
  _Outcome _splitInline(
    ProjectedRow row,
    int start,
    int end,
    String separator, {
    (int, int)? keep,
  }) {
    var from = start, to = end;
    for (final o in _doc.ownersAt(start)) {
      if (start == end && o.contentStart == o.contentEnd && o.end > to) {
        from = to = o.end;
      }
    }
    final shared = _doc
        .ownersAt(from)
        .where((o) => to <= o.contentEnd)
        .toList();
    final left = <String>[], right = <String>[];
    var leftSpace = '', rightSpace = '';
    final m = _doc.model;
    String? lazyPrefix;
    // The line break before line [i] of the row and its container prefix,
    // from [a] to [b]. A lazy line has no prefix of its own: it gets the
    // row's, as [_commitReturn] gives one the split empties ([emptied]) or
    // that keeps all of its text below the new line.
    String gap(int a, int b, int i, {required bool emptied}) {
      if (row.shells.isEmpty || row.prefixStarts[i] != row.contentStarts[i]) {
        return source.substring(a, b);
      }
      final prefix = lazyPrefix ??= _rowPrefix(row, row.firstLine);
      if (emptied && prefix.trim().isEmpty) return source.substring(a, b);
      return '${source.substring(a, m.lineStartUtf16(row.firstLine + i))}'
          '$prefix';
    }

    // Whether the whitespace after the split ran on past a line break: the
    // text after it then keeps its own line, below the new one.
    var below = false;
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
      // Keep those bytes adjacent to the break, outside the split owners. At
      // a line's edge the whitespace runs on through the line break and the
      // next line's container prefix, never into the prefix alone, so the
      // delimiters stay on their text's lines.
      if (shared.isNotEmpty && shared.last.kind != RunKind.code) {
        while (from > shared.first.contentStart) {
          final i = row.nearestLineIndexOf(m, from);
          if (from > row.contentStarts[i] && _isBlank(source, from - 1)) {
            leftSpace = source.substring(from - 1, from) + leftSpace;
            from--;
          } else if (from == row.contentStarts[i] &&
              i > 0 &&
              row.contentEnds[i - 1] >= 0) {
            final before = row.contentEnds[i - 1];
            leftSpace = gap(before, from, i, emptied: true) + leftSpace;
            from = before;
          } else {
            break;
          }
          grew = true;
        }
        while (to < shared.first.contentEnd) {
          final i = row.nearestLineIndexOf(m, to);
          if (to < row.contentEnds[i] && _isBlank(source, to)) {
            rightSpace += source.substring(to, to + 1);
            to++;
          } else if (to == row.contentEnds[i] &&
              i + 1 < row.lineCount &&
              row.contentStarts[i + 1] >= 0) {
            final after = row.contentStarts[i + 1];
            rightSpace += gap(to, after, i + 1, emptied: false);
            to = after;
            below = true;
          } else {
            break;
          }
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
    final opened = right.join(), tail = '$separator$rightSpace$opened';
    // The caret goes where the text after the split now starts, or, when
    // that text kept its line, on the new line before it.
    final moved = below ? rightSpace.length + opened.length : 0;
    if (keep == null || keep.$1 < to) {
      final text = '$head$tail';
      return _commitReturn(
        row,
        start,
        end,
        [(from, to, text)],
        separator,
        caret: text.length - moved,
      );
    }
    final (markupStart, markupEnd) = keep;
    final text = '$head${source.substring(markupStart, markupEnd)}$tail';
    return _commitReturn(
      row,
      start,
      end,
      [(from, to, text), (markupStart, markupEnd, '')],
      separator,
      caret: text.length - moved,
    );
  }

  /// Commit Return's [split] of [row] at [start]..[end], sorted edits with
  /// the caret [caret] units into the first one's text (by default after
  /// it), when the parser reads what Return promises: the new line in
  /// containers of the kinds of the line it split, other rows of the same
  /// kinds in the same kinds of containers, nothing hidden painted, and the
  /// text shown with only the line break, whitespace beside it aside. Lazy
  /// lines, which have no prefix of their own, take [_rowPrefix] where the
  /// split would leave them outside the row's containers: the split line
  /// when nothing is left on it, the lines after it when nothing moves. When
  /// the parser reads the plain split as other Markdown, these are tried in
  /// turn, else Return refuses: escaping the first ASCII punctuation of the
  /// first word moved (`> b`, `1. b`, `=`) or of the last word left (`a\`,
  /// `# a #`, `a*b*`); dropping the backslash of a hard break the split
  /// follows; a blank line after the row, as a lift keeps the next block
  /// apart (a heading's text moved into a paragraph above indented code);
  /// for an item whose later blocks follow blank lines, the next item's
  /// marker after them, as an empty item would end at a blank line. Only the
  /// plain split may leave the live tier.
  _Outcome _commitReturn(
    ProjectedRow row,
    int start,
    int end,
    List<(int, int, String)> split,
    String separator, {
    int? caret,
  }) {
    final m = _doc.model, at = split.first, rows = projection.rows;
    final offset = caret ?? at.$3.length;
    final first = row.lineIndexOf(m, start), last = row.lineIndexOf(m, end);
    int shown(int offset) => row.displayForSource(offset).$1;
    final prefix = row.shells.isEmpty ? '' : _rowPrefix(row, row.firstLine);
    final lazy = [
      for (var i = first < 1 ? 1 : first; i < row.lineCount; i++)
        if (row.contentStarts[i] >= 0 &&
            row.prefixStarts[i] == row.contentStarts[i] &&
            (i == first
                ? shown(start) == shown(row.contentStarts[i]) &&
                      prefix.trim().isNotEmpty
                : i > last && shown(end) == shown(row.contentEnds[last])))
          (m.lineStartUtf16(row.firstLine + i), row.contentStarts[i], prefix),
    ];
    // The edits splice the source in order, a lazy line's prefix before a
    // split at the same offset. One whose line break the split takes (with
    // the line's prefix, which the split gives the row's, see
    // [_splitInline]) is no edit of its own: spliced as well, it would
    // overlap the split and cut the source backwards.
    bool inSplit((int, int, String) e) =>
        split.any((s) => e.$1 > s.$1 && e.$1 <= s.$2);
    final ordered = [
      ...lazy.where((e) => e.$1 <= at.$1),
      ...split,
      ...lazy.where((e) => e.$1 > at.$1 && !inSplit(e)),
    ];
    final indexed = [for (final (i, e) in ordered.indexed) (i, e)]
      ..sort((x, y) {
        final byStart = x.$2.$1.compareTo(y.$2.$1);
        return byStart != 0 ? byStart : x.$1.compareTo(y.$1);
      });
    final edits = [for (final (_, e) in indexed) e];
    final breaks = '\n' * '\n'.allMatches(separator).length;
    final a = shown(start), b = shown(end);
    // Whether [next] shows the current rows' text with [row]'s from [a] to
    // [b] made [breaks], and with [apart] a line break after the row,
    // whitespace beside line breaks aside. Rows that show what they showed,
    // from either end up to the edit, are compared as they are (a reused
    // row's text is the same string), so only the rest is joined.
    bool showsBreak(FlarkDocument next, {required bool apart}) {
      final now = next.projection.rows;
      var p = 0, s = 0, off = 0;
      while (p < row.index && p < now.length && rows[p].text == now[p].text) {
        p++;
      }
      while (s < rows.length - row.index - 1 &&
          s < now.length - p - 1 &&
          rows[rows.length - 1 - s].text == now[now.length - 1 - s].text) {
        s++;
      }
      for (var i = p; i < row.index; i++) {
        off += rows[i].text.length + 1;
      }
      String join(List<ProjectedRow> list) =>
          [for (var i = p; i < list.length - s; i++) list[i].text].join('\n');
      var expected = join(rows).replaceRange(off + a, off + b, breaks);
      if (apart) {
        final e = off + row.text.length + breaks.length - (b - a);
        expected = expected.replaceRange(e, e, '\n');
      }
      final lead = p > 0 ? '\n' : '', trail = s > 0 ? '\n' : '';
      return '$lead$expected$trail'.replaceAll(_breakSpace, '\n') ==
          '$lead${join(now)}$trail'.replaceAll(_breakSpace, '\n');
    }

    // [list] with the caret [offset] units into [caretEdit]'s text, or null
    // when an alternative's escape falls in what the split moves (`#` of a
    // heading's closing sequence): that spelling is passed over. The plain
    // split's own edits never overlap.
    Spelling? spellingOf(
      List<Edit> list,
      Edit caretEdit,
      int offset, {
      bool asAsked = false,
    }) {
      if (!Edits.isDisjoint(list)) {
        assert(!asAsked, 'Return spliced overlapping edits $list');
        return null;
      }
      var shift = 0, caret = 0;
      for (final edit in list) {
        if (edit == caretEdit) caret = edit.$1 + shift + offset;
        shift += edit.$3.length - (edit.$2 - edit.$1);
      }
      return Spelling(
        Edits(list),
        FlarkSelection.collapsed(caret),
        asAsked: asAsked,
      );
    }

    final plain = spellingOf(edits, at, offset, asAsked: true);
    final lineStart = row.contentStarts[first];
    final moved = _firstEscapable.matchAsPrefix(source, at.$2);
    final kept = _lastEscapable.firstMatch(
      source.substring(lineStart, at.$1 < lineStart ? lineStart : at.$1),
    );
    final rowLast = row.firstLine + row.lineCount - 1;
    final rowEnd = projection.lineContentEnd(rowLast);
    final blank = (
      rowEnd,
      rowEnd,
      '${_lineBreakAt(rowEnd)}'
          '${prefix.isEmpty ? '' : _rowPrefix(row, rowLast).trimRight()}',
    );
    (int, int, String)? unbreak;
    for (final r in _doc.hardBreaksOf(row)) {
      if (r.endUtf16 == at.$1 && r.contentStartUtf16 == at.$1) {
        final s = r.startUtf16;
        unbreak = (s, projection.lineContentEnd(m.lineOfUtf16(s)), '');
      }
    }
    final spellings = [?plain];
    // The blank line after the row, which [showsBreak] expects.
    Spelling? apart;
    for (final edit in [
      if (moved != null &&
          moved.end <= projection.lineContentEnd(m.lineOfUtf16(end)))
        (moved.end - 1, moved.end - 1, r'\'),
      if (kept != null) (lineStart + kept.start, lineStart + kept.start, r'\'),
      ?unbreak,
      if (rowLast + 1 < m.lineCount) blank,
    ]) {
      // The edit among the others, after those that start where it does.
      final i = edits.lastIndexWhere((e) => e.$1 <= edit.$1) + 1;
      final respelled = spellingOf(
        [...edits.take(i), edit, ...edits.skip(i)],
        at,
        offset,
      );
      if (respelled == null) continue;
      if (edit == blank) apart = respelled;
      spellings.add(respelled);
    }
    final item = row.shells.isEmpty ? null : row.shells.last;
    final following = rows
        .skip(row.index + 1)
        .where((r) => r.kind != RowKind.blank)
        .firstOrNull;
    if (item?.kind == ShellKind.item &&
        following != null &&
        following.index != row.index + 1 &&
        following.shells.any((s) => s.block == item!.block) &&
        shown(end) >= row.text.length &&
        (start == end || split.length == 1 && at.$3.endsWith(separator))) {
      final lineAt = m.lineStartUtf16(following.firstLine);
      final marker = separator.substring(separator.indexOf('\n') + 1);
      final edit = (lineAt, lineAt, '$marker${_lineBreakAt(lineAt)}');
      final continued = spellingOf(
        [
          if (start != end)
            (at.$1, at.$2, at.$3.substring(0, at.$3.length - separator.length)),
          edit,
        ],
        edit,
        marker.length,
      );
      if (continued != null) spellings.add(continued);
    }
    // Failing faithful spellings, the caret can go before the delimiters the
    // split reopens, to the nearest place before its own where the line
    // starts. The plain split's check finds that place in the document it
    // reads, where it fails for its caret alone; the split is then tried
    // once more with its caret there.
    int? lineStartAt;
    Iterable<Spelling> candidates() sync* {
      yield* spellings;
      if (lineStartAt case final o?) {
        if (spellingOf(edits, at, o, asAsked: true) case final s?) yield s;
      }
    }

    bool keeps(FlarkDocument next, Spelling spelling, Edits edits) {
      // Whether [caret] is in containers of the kinds of the line split. A
      // footnote's continuation line holds only its indentation, and is in
      // no footnote yet while nothing follows it there.
      bool contained(int caret) {
        final now = next.rowAt(caret);
        return now.sameContainerKinds(row) ||
            now.kind == RowKind.blank &&
                row.shells.lastOrNull?.kind == ShellKind.footnoteDefinition &&
                now.shells.length == row.shells.length - 1 &&
                now.withinContainerKindsOf(row);
      }

      // Whether [offset] shows where a line starts.
      bool startsLine(int offset) {
        final shownAt = next.displayOf(offset);
        return shownAt.offset == 0 ||
            next.projection.rows[shownAt.row].text.codeUnitAt(
                  shownAt.offset - 1,
                ) ==
                0x0A;
      }

      final caret = spelling.selection.extent;
      // A line ended by Return can read as a link reference definition,
      // which shows its source.
      final defined =
          row.kind != RowKind.definition &&
          next.rowAt(start).kind == RowKind.definition;
      if (!contained(caret) ||
          !_keepsStructure(
            next,
            edits,
            {row.index},
            shown: defined ? (row.sourceStart, start) : null,
            shells: true,
          )) {
        return false;
      }
      if (defined) return true;
      // The caret starts the line Return made: a delimiter the split
      // reopens can pair with literal ones after it otherwise and leave
      // them before the caret (`*foo**bar*` split before the `**`).
      if (!startsLine(caret)) {
        if (identical(spelling, plain)) {
          final floor = at.$3.lastIndexOf('\n') + 1;
          for (var o = offset - 1; o >= floor; o--) {
            if (startsLine(caret - (offset - o))) {
              lineStartAt = o;
              break;
            }
          }
        }
        return false;
      }
      return showsBreak(next, apart: identical(spelling, apart));
    }

    return _commitSpellings(candidates(), keeps, coalesce: false);
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
    final outer = continuationPrefix(
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
  /// sibling's content offset; Outdent lifts it to its parent's column,
  /// however deep it was nested (four spaces, a tab), onto a line of its own
  /// if it opens on the parent's (`- - a` gives `-` over `- a`). All its
  /// lines shift at its column. The parser must show every row as it was,
  /// the item's a list deeper or shallower, or the shift refuses.
  bool _shiftItem({required bool outdent}) {
    final row = _doc.caretRow, shells = row.shells, m = _doc.model;
    final idx = shells.lastIndexWhere((s) => s.kind == ShellKind.item);
    if (idx < 0 ||
        outdent && (idx < 2 || shells[idx - 2].kind != ShellKind.item)) {
      return false;
    }
    final item = shells[idx].block, list = m.blockParent(item);
    var other = outdent ? shells[idx - 2].block : -1;
    for (var b = list + 1; !outdent && b < item; b++) {
      if (m.blockParent(b) == list) other = b;
    }
    if (other < 0) return false;
    int columnOf(int block) => _columns(
      source,
      lineStartPastMark(source, m, m.blockFirstLine(block)),
      m.blockStart(block),
    );
    final first = m.blockFirstLine(item), start = m.blockStart(item);
    final column = columnOf(item);
    var delta = outdent ? columnOf(other) - column : m.blockAttr(other);
    var from = first;
    final edits = <(int, int, String)>[];
    if (outdent && m.blockFirstLine(other) == first) {
      // The parent keeps its marker, emptied; the item's new line continues
      // the containers around the parent, as Return's continuation does.
      var end = m.itemMarkerEnd(other);
      for (; _isSpace(source, end - 1); end--) {}
      final prefix = _continuing(other);
      edits.add((end, start, '${_lineBreakAt(end)}$prefix'));
      delta = _columns(prefix, 0, prefix.length) - column;
      from++;
    }
    for (var l = from; l < first + m.blockLineCount(item); l++) {
      final edit = _shiftLine(l, column, delta);
      if (edit != null) edits.add(edit);
    }
    // An ordered item nested under its sibling starts a new list at 1.
    final digits = RegExp('[0-9]*').matchAsPrefix(source, start)!.end;
    if (shells[idx].ordered && !outdent) edits.add((start, digits, '1'));
    final cut = outdent ? idx - 2 : idx;
    List<ShellKind> moved(ProjectedRow r) {
      final kinds = r.containerKinds;
      final mine = r.shells.length > idx && r.shells[idx].block == item;
      if (mine) {
        kinds.replaceRange(cut, idx, [
          if (!outdent) ...[ShellKind.item, ShellKind.list],
        ]);
      }
      return kinds;
    }

    edits.sort((a, b) => a.$1 == b.$1 ? a.$2 - b.$2 : a.$1 - b.$1);
    return edits.isNotEmpty &&
        _commitSpellings(
              [Spelling.carrying(Edits(edits), selection, asAsked: true)],
              (next, _, edits) =>
                  _showsRows(next, edits, shells: moved) &&
                  next.caretRow.hasContainerKinds(moved(row)),
              coalesce: false,
            )
            is _Committed;
  }

  /// The visual column after [text] from [from] to [to], tabs counted to
  /// the next multiple of four, as Markdown counts them.
  static int _columns(String text, int from, int to, [int column = 0]) {
    for (var i = from; i < to; i++) {
      column = text.codeUnitAt(i) == 0x09 ? (column ~/ 4 + 1) * 4 : column + 1;
    }
    return column;
  }

  /// The prefix a new line takes to continue the containers of [block].
  String _continuing(int block) {
    final m = _doc.model, line = m.blockFirstLine(block);
    return continuationPrefix(source, m, line, m.blockStart(block), block);
  }

  /// The edit moving what [line] holds after visual [column] by [delta]:
  /// spaces inserted, or up to -[delta] columns of whitespace before it
  /// removed, that whitespace respelled as the spaces it shows. Null for a
  /// line of whitespace, or a lazy one whose text starts before [column].
  (int, int, String)? _shiftLine(int line, int column, int delta) {
    final start = lineStartPastMark(source, _doc.model, line);
    final end = projection.lineContentEnd(line);
    var at = start, col = 0, a = start, b = start;
    while (at < end && col < column) {
      col = _columns(source, at, ++at, col);
    }
    // Whitespace that is a row's content (code's own indentation) stays.
    final spans = projection.lineSpans(line);
    final content = spans.isEmpty ? end : spans[0].$1;
    for (a = at; a > start && _isSpace(source, a - 1); a--) {}
    for (b = at; b < content && _isSpace(source, b); b++) {}
    if (col < column || b == end || content < at) return null;
    final left = _columns(source, start, a);
    final width = _columns(source, a, b, left) - left;
    final shifted =
        width + (delta > 0 ? delta : -(column - left).clamp(0, -delta));
    return shifted == width ? null : (a, b, ' ' * shifted);
  }

  /// Whether [next], which [edits] made, shows each row of the current
  /// projection that shows anything as it was (but those [shells] gives none
  /// for): the row where its start goes, as a caret would, has its kind,
  /// level and text, in containers of the kinds [shells] gives (by default
  /// the row's own), and nothing else shows but [added] rows.
  bool _showsRows(
    FlarkDocument next,
    Edits edits, {
    int added = 0,
    List<ShellKind>? Function(ProjectedRow)? shells,
  }) {
    final now = next.projection.rows;
    var j = 0, count = added;
    for (final row in projection.rows) {
      final kinds = shells == null ? row.containerKinds : shells(row);
      if (row.kind == RowKind.blank || kinds == null) continue;
      count++;
      final at = edits.forward(row.sourceStart, caret: true);
      for (; j + 1 < now.length && now[j + 1].sourceStart <= at; j++) {}
      final r = now[j];
      if ((r.sourceStart, r.kind, r.text, r.headingLevel) !=
              (at, row.kind, row.text, row.headingLevel) ||
          !r.hasContainerKinds(kinds)) {
        return false;
      }
    }
    return now.where((r) => r.kind != RowKind.blank).length == count;
  }

  /// Apply sorted, non-overlapping edits; returns the new source and a map
  /// from old offsets to new ones, as a caret goes ([Edits.forward]).
  (String, int Function(int)) _edited(List<(int, int, String)> edits) {
    final made = Edits(edits);
    return (made.apply(source), (o) => made.forward(o, caret: true));
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
        coalesce: false,
      );
    }
    // No task item holds the caret: there is no checkbox to toggle, which
    // is nothing to do rather than an edit Markdown cannot make.
    _inert = true;
    return false;
  }

  /// A bare `#` is projected as authoring text, but it is still the parser's
  /// heading: a level command has to replace that marker, not prepend to it.
  bool _isBareHeading(ProjectedRow row) =>
      projection.isBarePrefix(row) &&
      _doc.model.blockKind(row.block) == BlockKind.heading;

  /// What [SetHeadingLevel] makes of [level] at the caret before the parser
  /// has a say, which [canSetHeading] reports.
  _HeadingPlan _headingPlan(int level) {
    if (level < 0 || level > 6) return _HeadingPlan.refused;
    final row = _doc.caretRow;
    if (row.kind == RowKind.blank && selection.isCollapsed) {
      // An empty line is no heading already: clearing its level has
      // nothing to do, as on a paragraph.
      return level == 0 ? _HeadingPlan.unchanged : _HeadingPlan.emptyLine;
    }
    if (row.kind != RowKind.paragraph && row.kind != RowKind.heading) {
      return _HeadingPlan.refused;
    }
    // A heading already at [level] needs nothing, however it is spelled:
    // rewriting a setext or closed heading as plain ATX would respell source
    // the user wrote and record an undo step that changes nothing shown. A
    // paragraph has level 0 already, unless it is a bare `#`, which the
    // parser reads as a heading.
    if (row.kind == RowKind.heading
        ? row.headingLevel == level
        : level == 0 && !_isBareHeading(row)) {
      return _HeadingPlan.unchanged;
    }
    final starts = row.contentStarts.where((s) => s >= 0);
    if (level == 0 || starts.length < 2) return _HeadingPlan.block;
    // A heading is one line. Of a paragraph of several, the first is headed,
    // and the caret must be on it; a heading of several takes no new level.
    if (row.kind == RowKind.heading || _isBareHeading(row)) {
      return _HeadingPlan.refused;
    }
    final m = _doc.model;
    final (_, firstEnd) = row.displayLineAt(0);
    return firstEnd < row.text.length &&
            m.lineOfUtf16(selection.extent) == m.lineOfUtf16(starts.first)
        ? _HeadingPlan.firstLine
        : _HeadingPlan.refused;
  }

  _Outcome _setHeading(int level) {
    final row = _doc.caretRow;
    switch (_headingPlan(level)) {
      case _HeadingPlan.refused:
        return const _NotKept();
      case _HeadingPlan.unchanged:
        _inert = true;
        return const _Unchanged();
      case _HeadingPlan.emptyLine:
        return _emptyLineHeading(row, level);
      case _HeadingPlan.firstLine:
        return _headFirstLine(row, level);
      case _HeadingPlan.block:
        break;
    }
    final m = _doc.model;
    // A paragraph whose block opens with link reference definitions starts
    // its row on its own first line, and its heading there: from the block's
    // start the edit would take the definitions with it.
    final blockStart = m.blockFirstLine(row.block) == row.firstLine
            ? m.blockStart(row.block)
            : row.contentStarts[0],
        blockEnd = m.blockEnd(row.block);
    final prefix = level == 0 ? '' : '${'#' * level} ';
    // The bare marker is the row's own text, so it is what the prefix replaces.
    final contentStart = _isBareHeading(row) ? row.sourceEnd : row.sourceStart;
    final shift = prefix.length - (contentStart - blockStart);
    int move(int o) => o >= contentStart ? o + shift : o;
    return _commitApart(
      row,
      Edits([
        (blockStart, contentStart, prefix),
        if (row.kind == RowKind.heading && blockEnd > row.sourceEnd)
          (row.sourceEnd, blockEnd, ''),
      ]),
      FlarkSelection(move(selection.base), move(selection.extent)),
      blockStart,
      // A level makes the row a heading, and clearing one a paragraph, or an
      // empty line where the heading had no text, in the same containers:
      // `# >` made a paragraph is a quote, `# ---` a rule, and `# x` under a
      // table the table's next row.
      (next) {
        final now = next.rowAt(move(selection.extent));
        return (level > 0
                ? now.kind == RowKind.heading
                : now.kind == RowKind.paragraph ||
                      now.kind == RowKind.blank &&
                          (row.text.isEmpty || _isBareHeading(row))) &&
            now.sameContainerKinds(row);
      },
    );
  }

  /// A level set on an empty line makes an empty heading in the line's
  /// containers, the typing intent kept. Its marker follows the line's
  /// prefix (spaced from a list marker), else that prefix without trailing
  /// whitespace, else the prefix that continues the containers (an item runs
  /// on over unindented empty lines), else goes on a line after this one
  /// (HTML runs to an empty line); the parser must show every row as it was.
  _Outcome _emptyLineHeading(ProjectedRow row, int level) {
    final m = _doc.model, start = lineStartPastMark(source, m, row.firstLine);
    final end = row.sourceEnd, text = source.substring(start, end);
    String marked(String p) =>
        '$p${p.isEmpty || _isSpace(p, p.length - 1) ? '' : ' '}${'#' * level} ';
    final child = row.shells.isEmpty ? m.blockCount : row.shells.last.block + 1;
    final lines = {
      marked(text),
      marked(text.trimRight()),
      if (child < m.blockCount && m.blockParent(child) == child - 1)
        marked(_continuing(child)),
      '$text${_lineBreakAt(end)}${marked(text.trimRight())}',
    };
    return _commitSpellings(
      [
        for (final (i, line) in lines.indexed)
          Spelling.carrying(
            Edits([(start, end, line)]),
            selection,
            pending: _pending,
            asAsked: i == 0,
          ),
      ],
      (next, _, edits) {
        final now = next.rowAt(edits.forward(end, caret: true));
        return (now.kind, now.headingLevel, now.text) ==
                (RowKind.heading, level, '') &&
            now.sameContainersAs(row, next.model, _doc.model) &&
            _showsRows(next, edits, added: 1);
      },
      coalesce: false,
    );
  }

  /// A heading is one line: a level set on a paragraph of several heads the
  /// first, where [_headingPlan] has found the caret, and the rest stays a
  /// paragraph in the same containers, its first line respelled with the
  /// containers' prefix if lazy or indented as code. Parts showing other
  /// text (a span) refuse.
  _Outcome _headFirstLine(ProjectedRow row, int level) {
    final m = _doc.model;
    // Where the first line's text ends.
    final (_, split) = row.displayLineAt(0);
    final starts = row.contentStarts.where((s) => s >= 0).toList();
    final at = starts[0];
    final marker = (at, at, '${'#' * level} '), prefix = _continuing(row.block);
    final lazy = lineStartPastMark(source, m, m.lineOfUtf16(starts[1]));
    var next = starts[1];
    for (; _isSpace(source, next); next++) {}
    return _commitSpellings(
      [
        Spelling.carrying(Edits([marker]), selection, asAsked: true),
        if (source.substring(lazy, next) != prefix)
          Spelling.carrying(Edits([marker, (lazy, next, prefix)]), selection),
      ],
      (doc, _, edits) {
        final now = doc.rowAt(edits.forward(selection.extent, caret: true));
        final rest = doc.projection.rows.elementAtOrNull(now.index + 1) ?? now;
        final head = row.text.substring(0, split);
        final tail = row.text.substring(split + 1).trimLeft();
        List<ShellKind>? others(ProjectedRow r) =>
            r == row ? null : r.containerKinds;
        return (now.kind, now.headingLevel) == (RowKind.heading, level) &&
            now.sameContainersAs(row, doc.model, _doc.model) &&
            now.text.trimRight() == head.trimRight() &&
            (rest.kind, rest.text.trimLeft()) == (RowKind.paragraph, tail) &&
            rest.sameContainersAs(row, doc.model, _doc.model) &&
            _showsRows(doc, edits, added: 2, shells: others);
      },
      coalesce: false,
    );
  }

  /// Commit [edits] to the current source in [row] (a lifted container
  /// marker, or a heading's markup), with [selected] after them, when
  /// [accept] holds and [_keepsStructure] does: rows elsewhere keep their
  /// kinds and no hidden markup is painted. The block after the row can
  /// instead join it lazily: `1. a` lifted above `2. b` would read as the
  /// paragraph `a 2. b`, and `### a` turned into a paragraph above indented
  /// code would absorb the code. A blank line after the row keeps that block
  /// apart when the parser agrees; it carries the row's container prefix up
  /// to [prefixEnd], without the markers of containers that open on that
  /// line. Otherwise the edit is refused.
  _Outcome _commitApart(
    ProjectedRow row,
    Edits edits,
    FlarkSelection selected,
    int prefixEnd,
    bool Function(FlarkDocument) accept,
  ) {
    final m = _doc.model;
    final last = row.firstLine + row.lineCount - 1;
    final rowEnd = projection.lineContentEnd(last);
    final spellings = [Spelling(edits, selected, asAsked: true)];
    if (row.text.isNotEmpty &&
        row.block >= 0 &&
        rowEnd >= edits.list.last.$2 &&
        last + 1 < m.lineCount) {
      final outer = continuationPrefix(
        source,
        m,
        m.lineOfUtf16(prefixEnd),
        prefixEnd,
        row.block,
      );
      final blank = '${_lineBreakAt(rowEnd)}${outer.trimRight()}';
      // The blank line goes where the edit as asked leaves the row's end,
      // and a selection past it moves on with the text.
      final at = edits.forward(rowEnd);
      final after = Edits([(at, at, blank)]);
      spellings.add(
        Spelling(
          Edits([...edits.list, (rowEnd, rowEnd, blank)]),
          FlarkSelection(
            after.forward(selected.base),
            after.forward(selected.extent),
          ),
        ),
      );
    }
    return _commitSpellings(
      spellings,
      (next, _, edits) =>
          accept(next) &&
          _keepsStructure(
            next,
            edits,
            {row.index},
            movesText: row.text.isNotEmpty,
            shells: true,
          ),
      coalesce: false,
    );
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

  /// A space or tab: whitespace within a line.
  static bool _isBlank(String text, int i) =>
      text.codeUnitAt(i) == 0x20 || text.codeUnitAt(i) == 0x09;

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
    final FlarkSelection placed;
    if (projection.isMissingCell(row.index) &&
        (!extend || selection.base == row.sourceStart)) {
      placed = FlarkSelection.collapsed(row.sourceStart, tableCell: row.index);
    } else {
      final target = _doc.pointerAnchorAt(
        row.index,
        offset,
        leadingHalf: leadingHalf,
      );
      placed = extend
          ? FlarkSelection(selection.base, target)
          : FlarkSelection.collapsed(target);
    }
    if (_select(placed)) return true;
    // A press takes the context of where it lands, as in common editors,
    // even where the caret already was: a pending style does not outlast
    // it, and typing after it is an undo step of its own.
    history.breakCoalescing();
    _goalColumn = null;
    if (_pending == null) return false;
    _pending = null;
    return true;
  }

  // ----------------------------------------------------------- history

  bool _undo() {
    final currentSource = source;
    final currentSelection = selection;
    final currentPending = _pending;
    final target = history.undoTarget;
    // With nothing to undo the command does nothing, and is no refused edit.
    if (target == null) {
      _inert = true;
      return false;
    }
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
    if (target == null) {
      _inert = true;
      return false;
    }
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

/// The editor's editing state, which one capture sets aside and one restore
/// puts back whole: around a host's command that may yet be refused after
/// the composition it ends, a composition's dry run of typing, and an
/// unwritten table cell's private preparation (audit C7). The call's own
/// outcome (its refusal, an inert no-op) is no part of it.
typedef _EditState = ({
  FlarkEditorSnapshot snapshot,
  PendingStyle? pending,
  int? goalColumn,
  bool selectedCodeScope,
  HistoryEntry? composition,
  (int, int, String, bool)? composed,
  FlarkEditorSnapshot? cellOrigin,
  HistoryCheckpoint history,
});

/// What became of an edit ([FlarkEditor._attempt],
/// [FlarkEditor._commitSpellings]), so that no caller reads it back from the
/// editor's state.
sealed class _Outcome {
  const _Outcome();
}

/// The edit was committed, or, changing no source, moved the selection.
final class _Committed extends _Outcome {
  const _Committed();
}

/// The edit asked for what the editor already has: a successful no-op that
/// publishes nothing.
final class _Unchanged extends _Outcome {
  const _Unchanged();
}

/// No spelling read as it must, and the parser refused none but respellings,
/// which are passed over. [passedOver]: a spelling went unchecked past the
/// live tier, or, from the search, a respelling past the live tier or a
/// refused one. [read]: the document a check found wanting, from an attempt
/// that checked one.
final class _NotKept extends _Outcome {
  const _NotKept({this.passedOver = false, this.read});

  final bool passedOver;
  final FlarkDocument? read;
}

/// The parser refused the edit: its source was not valid, past the
/// writable limit, or read with an extraction deviation.
final class _Refused extends _Outcome {
  const _Refused(this.reason);

  final FlarkRejection reason;
}

/// Which spelling may leave the live tier, where no parse checks it.
enum _Tier {
  /// The edit as asked, unchecked; respellings past the tier are passed over
  /// (EP1-RESULT-PRESENTATION-001).
  asAsked,

  /// None while the search runs. Text put where it changes its line's block
  /// structure may have no spelling as asked: when none qualifies, the first
  /// spelling past the tier enters source mode, as ordinary text does.
  firstPast,
}

/// What [FlarkEditor._setHeading] makes of a level at the caret before the
/// parser has a say ([FlarkEditor._headingPlan]).
enum _HeadingPlan {
  /// The row takes no such level: it is no paragraph, heading or empty line,
  /// it is a heading of several lines, or the caret is past the first line
  /// of a paragraph of several.
  refused,

  /// The row has the level already.
  unchanged,

  /// An empty line becomes an empty heading ([FlarkEditor._emptyLineHeading]).
  emptyLine,

  /// A paragraph of several lines has its first line headed
  /// ([FlarkEditor._headFirstLine]).
  firstLine,

  /// The row's block takes the level, or loses it, whole.
  block,
}

/// Characters that can underline a setext heading.
final _underlineRun = RegExp(r'^(?:-+|=+)$');

/// Spaces and tabs beside a line break, which Markdown shows or strips.
final _breakSpace = RegExp(r'[ \t]*\n[ \t]*');

/// The first ASCII punctuation, which a backslash escapes, of the first word
/// at a position, and the last of the last word before an end.
final _firstEscapable = RegExp(r'[ \t]*[^\s!-/:-@\[-`{-~]*[!-/:-@\[-`{-~]');
final _lastEscapable = RegExp(r'[!-/:-@\[-`{-~][^\s!-/:-@\[-`{-~]*[ \t]*$');
