import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flark_core/flark_core.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'input_window.dart';

const _maximumVisibleBytes = 16 * 1024;
const _maximumInputCodeUnits = 16 * 1024;
const _maximumPaintCodeUnits = 2 * 1024;
const _maximumSmallEditBytes = 4 * 1024;
const _smallEditDescriptorBytes = 32;
const _viewportRowsPerPage = 32;
const _parseIdleDelay = Duration(milliseconds: 32);
const _maximumSemanticSuccessors = 7;

enum FlarkEditorStatus { opening, parsing, ready, editing, faulted, disposed }

enum FlarkSurfaceInlineStyle { emphasis, strong, code, strikethrough, link }

final class FlarkSurfaceTextRun {
  const FlarkSurfaceTextRun({
    required this.text,
    required this.sourceUtf16Start,
    required this.sourceUtf16End,
    required this.sourceExact,
    required this.styles,
  }) : assert(!sourceExact || sourceUtf16End - sourceUtf16Start == text.length);

  final String text;
  final int sourceUtf16Start;
  final int sourceUtf16End;
  final bool sourceExact;
  final Set<FlarkSurfaceInlineStyle> styles;

  int sourceOffsetForTextOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final local = offset.clamp(0, text.length);
    if (sourceExact) return sourceUtf16Start + local;
    if (local == 0) return sourceUtf16Start;
    if (local == text.length) return sourceUtf16End;
    return affinity == TextAffinity.upstream
        ? sourceUtf16Start
        : sourceUtf16End;
  }

  int textOffsetForSourceOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final local = (offset - sourceUtf16Start).clamp(
      0,
      sourceUtf16End - sourceUtf16Start,
    );
    if (sourceExact) return local;
    if (local == 0) return 0;
    if (local == sourceUtf16End - sourceUtf16Start) return text.length;
    return affinity == TextAffinity.upstream ? 0 : text.length;
  }
}

final class FlarkSurfaceRow {
  const FlarkSurfaceRow({
    required this.leadingText,
    required this.text,
    required this.globalUtf16Start,
    required this.kind,
    required this.headingLevel,
    required this.blockQuoteDepth,
    required this.codeBlock,
    required this.thematicBreak,
    required this.ordinal,
    required this.active,
    required this.selection,
    required this.runs,
  });

  final String leadingText;
  final String text;
  final int globalUtf16Start;
  final int kind;
  final int? headingLevel;
  final int? blockQuoteDepth;
  final FlarkCodeBlockPresentation? codeBlock;
  final bool thematicBreak;
  final int ordinal;
  final bool active;
  final TextSelection? selection;
  final List<FlarkSurfaceTextRun> runs;

  int sourceOffsetForTextOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final local = offset.clamp(0, text.length);
    if (runs.isEmpty) return globalUtf16Start + local;
    var consumed = 0;
    for (var index = 0; index < runs.length; index += 1) {
      final run = runs[index];
      final runEnd = consumed + run.text.length;
      if (local < runEnd) {
        return run.sourceOffsetForTextOffset(
          local - consumed,
          affinity: affinity,
        );
      }
      if (local == runEnd) {
        if (affinity == TextAffinity.downstream && index + 1 < runs.length) {
          return runs[index + 1].sourceUtf16Start;
        }
        return run.sourceUtf16End;
      }
      consumed = runEnd;
    }
    return runs.last.sourceUtf16End;
  }

  int textOffsetForSourceOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    if (runs.isEmpty) {
      return (offset - globalUtf16Start).clamp(0, text.length);
    }
    var consumed = 0;
    for (final run in runs) {
      if (offset < run.sourceUtf16Start) return consumed;
      if (offset <= run.sourceUtf16End) {
        return consumed +
            run.textOffsetForSourceOffset(offset, affinity: affinity);
      }
      consumed += run.text.length;
    }
    return text.length;
  }
}

final class _TextMutation {
  const _TextMutation(this.start, this.end, this.replacement);

  final int start;
  final int end;
  final String replacement;
}

final class _OptimisticViewportEdit {
  const _OptimisticViewportEdit({
    required this.start,
    required this.end,
    required this.replacementLength,
  });

  final int start;
  final int end;
  final int replacementLength;

  int get delta => replacementLength - (end - start);
}

final class _ProjectionContinuitySurface {
  const _ProjectionContinuitySurface({
    required this.receipt,
    required this.presentation,
  });

  final FlarkProjectionContinuityReceipt receipt;
  final FlarkSurfaceRow presentation;
}

final class _EditorSelectionSnapshot {
  const _EditorSelectionSnapshot(this.selection, this.activeOrdinal);

  final TextSelection selection;
  final int? activeOrdinal;
}

sealed class _SemanticInputSuccessor {
  const _SemanticInputSuccessor();
}

final class _ProvisionalInputBatch extends _SemanticInputSuccessor {
  const _ProvisionalInputBatch({
    required this.before,
    required this.after,
    required this.typingInput,
  }) : super();

  final TextEditingValue before;
  final TextEditingValue after;
  final bool typingInput;
}

enum _DeferredInputCommand { deleteBackward, deleteForward, insertNewline }

final class _DeferredInputSuccessor extends _SemanticInputSuccessor {
  const _DeferredInputSuccessor(this.command, {this.replacement}) : super();

  final _DeferredInputCommand? command;
  final String? replacement;
}

final class _PendingSemanticInput {
  _PendingSemanticInput({
    required this.base,
    required this.inputGlobalUtf16Start,
    required this.initialCallbackStartedEpochMicros,
    this.provisionalMutation,
    required TextEditingValue provisionalAfter,
  }) : provisionalTail = provisionalAfter;

  final TextEditingValue base;
  final int inputGlobalUtf16Start;
  final int initialCallbackStartedEpochMicros;
  final _TextMutation? provisionalMutation;
  int initialCallbackMicros = 0;
  TextEditingValue provisionalTail;
  final List<_SemanticInputSuccessor> successors = [];
}

/// Briefly retains the platform-provisional lineage after a semantic receipt
/// wins the race. The text service may emit one or more deltas against its old
/// newline window before it adopts the committed replacement window.
final class _LateSemanticInput {
  _LateSemanticInput({
    required this.provisionalTail,
    required this.reconciliation,
    required this.successorCount,
  });

  TextEditingValue provisionalTail;
  final _InputReconciliationMap reconciliation;
  int successorCount;
}

/// Layer attribution for the most recent platform-observed semantic edit.
/// The profile harness joins this receipt to Flutter's proving frame.
final class FlarkSemanticEditPerformance {
  const FlarkSemanticEditPerformance({
    required this.platformCallbackMicros,
    required this.coreQueueMicros,
    required this.workerRoundTripMicros,
    required this.workerQueueMicros,
    required this.nativeFfiMicros,
    required this.coreAdoptionMicros,
    required this.flutterReceiptAdoptionMicros,
    required this.callbackToReceiptMicros,
  });

  final int platformCallbackMicros;
  final int coreQueueMicros;
  final int workerRoundTripMicros;
  final int workerQueueMicros;
  final int nativeFfiMicros;
  final int coreAdoptionMicros;
  final int flutterReceiptAdoptionMicros;
  final int callbackToReceiptMicros;
}

/// One bounded monotone map between a platform-provisional input window and
/// the Rust-committed window. Offsets inside the differing interiors are
/// intentionally unmappable; callers resynchronize instead of guessing.
final class _InputReconciliationMap {
  const _InputReconciliationMap({
    required this.fromStart,
    required this.fromEnd,
    required this.toStart,
    required this.toEnd,
  });

  final int fromStart;
  final int fromEnd;
  final int toStart;
  final int toEnd;

  static _InputReconciliationMap? forSemanticBarrier({
    required _PendingSemanticInput pending,
    required FlarkCoreEditIntentReceiptV1 receipt,
  }) {
    final provisional = pending.provisionalMutation;
    if (provisional != null) {
      final windowStart = pending.inputGlobalUtf16Start;
      final windowEnd = windowStart + pending.base.text.length;
      if (receipt.baseUtf16End <= windowStart ||
          receipt.baseUtf16Start >= windowEnd) {
        // The semantic source splice sits outside the bounded platform
        // window. Rust may still move that window globally (an empty terminal
        // list exposes a zero-length window after its marker), but its local
        // text remains the base value. Reconcile by reversing only the
        // platform's provisional splice.
        return _InputReconciliationMap(
          fromStart: provisional.start,
          fromEnd: provisional.start + provisional.replacement.length,
          toStart: provisional.start,
          toEnd: provisional.end,
        );
      }
      final committedStart =
          receipt.baseUtf16Start - pending.inputGlobalUtf16Start;
      final committedEnd = receipt.baseUtf16End - pending.inputGlobalUtf16Start;
      if (provisional.start < 0 ||
          provisional.end < provisional.start ||
          provisional.end > pending.base.text.length ||
          committedStart < 0 ||
          committedEnd < committedStart ||
          committedEnd > pending.base.text.length) {
        return null;
      }

      // Both the text service and Rust edit the same base input window, but a
      // structural command need not choose the same splice. For example,
      // Return on an empty list provisionally inserts a newline after `- `,
      // while Rust commits by removing that marker. Reconcile their complete
      // union in base coordinates, leaving unchanged prefixes and suffixes
      // exactly mappable and treating the differing interior as opaque.
      final affectedStart = math.min(provisional.start, committedStart);
      final affectedEnd = math.max(provisional.end, committedEnd);
      final fromStart = _mapBaseBoundaryThroughSplice(
        affectedStart,
        start: provisional.start,
        end: provisional.end,
        replacementLength: provisional.replacement.length,
        downstream: false,
      );
      final fromEnd = _mapBaseBoundaryThroughSplice(
        affectedEnd,
        start: provisional.start,
        end: provisional.end,
        replacementLength: provisional.replacement.length,
        downstream: true,
      );
      final toStart = _mapBaseBoundaryThroughSplice(
        affectedStart,
        start: committedStart,
        end: committedEnd,
        replacementLength: receipt.replacement.length,
        downstream: false,
      );
      final toEnd = _mapBaseBoundaryThroughSplice(
        affectedEnd,
        start: committedStart,
        end: committedEnd,
        replacementLength: receipt.replacement.length,
        downstream: true,
      );
      return _InputReconciliationMap(
        fromStart: fromStart,
        fromEnd: fromEnd,
        toStart: toStart,
        toEnd: toEnd,
      );
    }
    final windowStart = pending.inputGlobalUtf16Start;
    final windowEnd = windowStart + pending.base.text.length;
    if (receipt.baseUtf16End <= windowStart ||
        receipt.baseUtf16Start >= windowEnd) {
      return const _InputReconciliationMap(
        fromStart: 0,
        fromEnd: 0,
        toStart: 0,
        toEnd: 0,
      );
    }
    if (receipt.baseUtf16Start < windowStart ||
        receipt.baseUtf16End > windowEnd) {
      return null;
    }
    final localStart = receipt.baseUtf16Start - windowStart;
    final localEnd = receipt.baseUtf16End - windowStart;
    return _InputReconciliationMap(
      fromStart: localStart,
      fromEnd: localEnd,
      toStart: localStart,
      toEnd: localStart + receipt.replacement.length,
    );
  }

  static int _mapBaseBoundaryThroughSplice(
    int offset, {
    required int start,
    required int end,
    required int replacementLength,
    required bool downstream,
  }) {
    if (offset < start) return offset;
    if (offset > end) return start + replacementLength + offset - end;
    if (start == end && offset == start) {
      return downstream ? start + replacementLength : start;
    }
    if (offset == start) return start;
    if (offset == end) return start + replacementLength;
    throw StateError('union boundary fell inside a source splice');
  }

  int? mapOffset(int offset, {required bool downstream}) {
    if (offset < fromStart) return offset;
    if (offset > fromEnd) return toEnd + offset - fromEnd;
    if (fromStart == fromEnd && offset == fromStart) {
      return downstream ? toEnd : toStart;
    }
    if (offset == fromStart) return toStart;
    if (offset == fromEnd) return toEnd;
    return null;
  }
}

/// UI-isolate state for a bounded viewport and bounded platform input window.
///
/// The complete document remains in [FlarkCoreDocument]'s worker/native actor.
/// This controller retains one bounded viewport page and at most 16 Ki UTF-16
/// code units in the platform text input connection, so a keystroke does not
/// copy a multi-megabyte document on Flutter's UI isolate.
final class FlarkEditorController extends ChangeNotifier {
  FlarkEditorController._(this._document)
    : _session = FlarkCoreEditorSession(_document);

  final FlarkCoreDocument _document;

  /// Canonical selection, grapheme, and undo policy authority. The controller
  /// is an adapter over it and holds no history stacks of its own.
  final FlarkCoreEditorSession _session;

  FlarkEditorStatus _status = FlarkEditorStatus.opening;
  FlarkViewport? _viewport;
  List<FlarkViewportRow> _cachedRows = const [];
  List<FlarkCertificationRange> _certificationRanges = const [];
  String _visibleSource = '';
  int _visibleUtf16Start = 0;
  TextEditingValue _inputValue = const TextEditingValue(
    selection: TextSelection.collapsed(offset: 0),
  );
  int _inputGlobalUtf16Start = 0;
  int? _activeOrdinal;
  int _globalSelectionBase = 0;
  int _globalSelectionExtent = 0;
  Future<void> _editTail = Future<void>.value();
  Future<void>? _parserTask;
  Timer? _parseTimer;
  Future<bool>? _pageTask;
  final List<int> _pageStarts = [0];
  final List<_OptimisticViewportEdit> _optimisticViewportEdits = [];
  _ProjectionContinuitySurface? _projectionContinuity;
  FlarkCoreCommittedPresentationGapV1? _committedParagraphSplit;
  List<FlarkCoreCommittedPresentationSurfaceV1> _committedStructuralSurfaces =
      const [];
  int _pageIndex = 0;
  int _editGeneration = 0;
  int _pendingEdits = 0;
  Object? _lastError;
  bool _closed = false;
  bool _semanticViewportCurrent = false;
  bool _semanticEditV1Active = false;
  bool _certificationRevisionCurrent = false;
  bool _crossRowSelection = false;
  bool _historyReplayPending = false;
  bool _oversizedSelection = false;

  static int _connectionEpochCounter = 0;
  FlarkInputWindowState _windowState = FlarkInputWindowState.detached;
  FlarkInputResyncReason _lastResyncReason = FlarkInputResyncReason.none;
  int _connectionEpoch = 0;
  int _windowEpoch = 0;
  int _resyncCount = 0;
  String _windowTextSha256 = '';
  String? _shadowText;
  int _shadowWindowStart = 0;
  TextSelection? _shadowSelection;
  bool _platformMutation = false;
  _PendingSemanticInput? _pendingSemanticInput;
  _LateSemanticInput? _lateSemanticInput;
  int? _activePlatformCallbackStartedEpochMicros;
  FlarkSemanticEditPerformance? _lastSemanticEditPerformance;
  bool _platformNewlineMutationAwaitingAction = false;
  bool _platformDeleteBackwardMutationAwaitingSelector = false;
  int _semanticSuccessorHighWatermark = 0;
  int _lastSemanticReconciliationMicros = 0;

  FlarkEditorStatus get status => _status;
  FlarkViewport? get viewport => _viewport;
  String get visibleSource => _visibleSource;
  int get visibleUtf16Start => _visibleUtf16Start;
  int get viewportPageIndex => _pageIndex;
  bool get canPageBackward => _semanticViewportCurrent && _pageIndex > 0;
  bool get canPageForward =>
      _semanticViewportCurrent && (_viewport?.continuation ?? 0) != 0;
  bool get semanticsCurrent => _semanticViewportCurrent;
  TextEditingValue get inputValue => _inputValue;
  int get revision => _document.revision;
  int get sourceByteLength => _document.sourceByteLength;
  int get sourceUtf16Length => _document.sourceUtf16Length;
  int get pendingEdits => _pendingEdits;
  Object? get lastError => _lastError;
  int get globalCaretOffset => _globalSelectionExtent;
  String? get selectedText {
    final selection = _inputValue.selection;
    if (!selection.isValid || selection.isCollapsed) return null;
    final start = math.min(selection.baseOffset, selection.extentOffset);
    final end = math.max(selection.baseOffset, selection.extentOffset);
    if (start < 0 || end > _inputValue.text.length) return null;
    return _inputValue.text.substring(start, end);
  }

  bool get canUndo =>
      !_historyReplayPending && (_session.canUndo || _pendingEdits > 0);
  bool get canRedo => !_historyReplayPending && _session.canRedo;

  FlarkInputWindowState get inputWindowState => _windowState;
  FlarkInputResyncReason get lastResyncReason => _lastResyncReason;
  int get connectionEpoch => _connectionEpoch;
  int get windowEpoch => _windowEpoch;
  int get resyncCount => _resyncCount;
  bool get hasOversizedSelection => _oversizedSelection;
  int get canonicalSelectionGeneration => _session.selectionGeneration;
  int get semanticSuccessorHighWatermark => _semanticSuccessorHighWatermark;
  int get lastSemanticReconciliationMicros => _lastSemanticReconciliationMicros;
  FlarkSemanticEditPerformance? get lastSemanticEditPerformance =>
      _lastSemanticEditPerformance;

  FlarkInputWindowShadow get inputWindowShadow => FlarkInputWindowShadow(
    connectionEpoch: _connectionEpoch,
    windowEpoch: _windowEpoch,
    representedRevision: _document.revision,
    globalUtf16Start: _inputGlobalUtf16Start,
    windowUtf16Length: _inputValue.text.length,
    windowTextSha256: _windowTextSha256,
    selectionGeneration: _session.selectionGeneration,
  );

  /// Reconciles the serialized platform shadow on every notification so no
  /// window-rewrite site can bypass the connection/window epoch discipline:
  /// a platform-accepted update advances the window epoch on the active
  /// connection, while any host-originated change to the exposed text, range,
  /// or selection retires the connection and starts a new one.
  @override
  void notifyListeners() {
    _reconcileWindowShadow();
    super.notifyListeners();
  }

  void _reconcileWindowShadow() {
    if (_closed) {
      _windowState = FlarkInputWindowState.closed;
      return;
    }
    if (_status == FlarkEditorStatus.faulted) {
      _windowState = FlarkInputWindowState.faulted;
      return;
    }
    final text = _inputValue.text;
    final selection = _inputValue.selection;
    final textChanged = !identical(text, _shadowText) && text != _shadowText;
    final startChanged = _inputGlobalUtf16Start != _shadowWindowStart;
    final selectionChanged = selection != _shadowSelection;
    if (_connectionEpoch != 0 &&
        !textChanged &&
        !startChanged &&
        !selectionChanged) {
      return;
    }
    if (_platformMutation && _connectionEpoch != 0 && !startChanged) {
      _windowEpoch += 1;
      if (textChanged) _windowTextSha256 = flarkWindowTextSha256(text);
    } else {
      _connectionEpoch = ++_connectionEpochCounter;
      _windowEpoch = 1;
      if (textChanged || _windowTextSha256.isEmpty) {
        _windowTextSha256 = flarkWindowTextSha256(text);
      }
      _windowState = FlarkInputWindowState.synchronized;
    }
    _shadowText = text;
    _shadowWindowStart = _inputGlobalUtf16Start;
    _shadowSelection = selection;
  }

  /// A rejected active-connection callback mutates nothing: the connection
  /// retires with a typed reason and the unchanged authoritative window is
  /// re-exposed on a fresh connection epoch.
  void _resynchronize(FlarkInputResyncReason reason) {
    _lateSemanticInput = null;
    _lastResyncReason = reason;
    _resyncCount += 1;
    _windowState = FlarkInputWindowState.resyncRequired;
    _connectionEpoch = ++_connectionEpochCounter;
    _windowEpoch = 1;
    _windowTextSha256 = flarkWindowTextSha256(_inputValue.text);
    _shadowText = _inputValue.text;
    _shadowWindowStart = _inputGlobalUtf16Start;
    _shadowSelection = _inputValue.selection;
    _windowState = FlarkInputWindowState.synchronized;
    super.notifyListeners();
  }

  /// Validates a complete platform delta batch against the serialized shadow
  /// before anything is applied: the first delta's old-text hash must equal
  /// the shadow's, each later delta's old hash must equal the prior delta's
  /// new hash, every range and selection must stay inside the simulated
  /// window, and a multi-delta batch must fit the whole-batch small-edit
  /// envelope. A bad or over-cap second delta therefore cannot leave the
  /// first applied.
  FlarkInputResyncReason _validateDeltaBatch(
    List<TextEditingDelta> deltas, {
    TextEditingValue? against,
    String? expectedTextSha256,
  }) {
    if (deltas.isEmpty) return FlarkInputResyncReason.none;
    final initial = against ?? _inputValue;
    final expectedHash = expectedTextSha256 ?? _windowTextSha256;
    if (flarkWindowTextSha256(deltas.first.oldText) != expectedHash) {
      return FlarkInputResyncReason.oldTextMismatch;
    }
    var value = initial;
    var runningHash = expectedHash;
    var envelopeBytes = 0;
    var mutatingDeltas = 0;
    for (final delta in deltas) {
      if (flarkWindowTextSha256(delta.oldText) != runningHash) {
        return FlarkInputResyncReason.deltaChainMismatch;
      }
      final mutation = _mutationFor(delta);
      if (mutation != null) {
        if (mutation.start < 0 ||
            mutation.end < mutation.start ||
            mutation.end > value.text.length) {
          return FlarkInputResyncReason.rangeOutOfWindow;
        }
        mutatingDeltas += 1;
        envelopeBytes += _smallEditDescriptorBytes;
        envelopeBytes += utf8
            .encode(value.text.substring(mutation.start, mutation.end))
            .length;
        envelopeBytes += utf8.encode(mutation.replacement).length;
      }
      try {
        value = delta.apply(value);
      } on Object {
        return FlarkInputResyncReason.rangeOutOfWindow;
      }
      final selection = delta.selection;
      if (!selection.isValid ||
          selection.start < 0 ||
          selection.end > value.text.length) {
        return FlarkInputResyncReason.rangeOutOfWindow;
      }
      final composing = delta.composing;
      if (composing != TextRange.empty &&
          (!composing.isValid ||
              composing.start < 0 ||
              composing.end > value.text.length)) {
        return FlarkInputResyncReason.rangeOutOfWindow;
      }
      runningHash = flarkWindowTextSha256(value.text);
    }
    if (mutatingDeltas > 1 && envelopeBytes > _maximumSmallEditBytes) {
      return FlarkInputResyncReason.batchOverEnvelope;
    }
    return FlarkInputResyncReason.none;
  }

  static Future<FlarkEditorController> open(
    String source, {
    required String libraryPath,
    int historyBudgetBytes = 8 * 1024 * 1024,
  }) async {
    final document = await FlarkCoreDocument.open(
      source,
      libraryPath: libraryPath,
      historyBudgetBytes: historyBudgetBytes,
    );
    final controller = FlarkEditorController._(document);
    await controller._refreshViewport(restoreInputWindow: true);
    await controller._session.setSelectionUtf16(
      controller._globalSelectionBase,
      controller._globalSelectionExtent,
      adapterState: controller._selectionSnapshot(),
    );
    return controller;
  }

  Future<void> continueParsing() {
    _parseTimer?.cancel();
    _parseTimer = null;
    if (_closed ||
        (_document.isReady && _semanticViewportCurrent) ||
        _status == FlarkEditorStatus.faulted) {
      return Future<void>.value();
    }
    return _parserTask ??= _finishParsing().whenComplete(
      () => _parserTask = null,
    );
  }

  Future<bool> nextViewportPage() {
    if (_closed || !canPageForward) return Future<bool>.value(false);
    return _pageTask ??= _loadNextViewportPage().whenComplete(
      () => _pageTask = null,
    );
  }

  Future<bool> previousViewportPage() {
    if (_closed || !canPageBackward) return Future<bool>.value(false);
    return _pageTask ??= _loadPreviousViewportPage().whenComplete(
      () => _pageTask = null,
    );
  }

  List<FlarkViewportRow> get rows => _cachedRows;

  FlarkSurfaceRow surfaceRow(
    FlarkViewportRow row, {
    bool includeEditingState = true,
  }) {
    final structurals = _structuralSurfacesFor(row.ordinal);
    if (structurals.isNotEmpty) {
      final structural = structurals.firstWhere(
        (candidate) =>
            candidate.sourceUtf16.start <= _globalSelectionExtent &&
            _globalSelectionExtent <= candidate.sourceUtf16.end,
        orElse: () => structurals.first,
      );
      return _committedStructuralSurfaceRow(
        structural,
        includeEditingState: includeEditingState,
      );
    }
    final mappedSource = surfaceSourceRange(row);
    final listItem = row.listItem;
    final blockQuote = row.blockQuote;
    final presentationPrefix = listItem?.prefixUtf16 ?? blockQuote?.prefixUtf16;
    final mappedPrefix = presentationPrefix == null
        ? null
        : _mapViewportRange(presentationPrefix);
    final semanticRange = mappedPrefix == null
        ? mappedSource
        : FlarkSourceRange(mappedPrefix.start, mappedSource.end);
    final rowCertified = _rowSemanticsCurrent(semanticRange);
    final exactLeadingText = mappedPrefix == null
        ? ''
        : _sliceVisibleUtf16(mappedPrefix.start, mappedPrefix.end);
    final continuity = _projectionContinuity;
    final continuityOwnsRow =
        includeEditingState &&
        continuity != null &&
        semanticRange.start <= _globalSelectionExtent &&
        _globalSelectionExtent <= semanticRange.end;
    final caretOwnsRow =
        includeEditingState &&
        !_crossRowSelection &&
        semanticRange.start <= _globalSelectionExtent &&
        _globalSelectionExtent < semanticRange.end;
    final active =
        includeEditingState &&
        (_activeOrdinal == row.ordinal || continuityOwnsRow || caretOwnsRow);
    final selected =
        includeEditingState &&
        _crossRowSelection &&
        (_selectionIntersects(semanticRange) || active);
    if (continuityOwnsRow) {
      return _activeContinuitySurface(continuity, row.ordinal);
    }
    if (selected && !active && (!rowCertified || row.table != null)) {
      return _exactSelectionSurfaceRow(
        range: semanticRange,
        ordinal: row.ordinal,
      );
    }
    if (active && !rowCertified) {
      final paintInput = _paintInputWindow();
      return FlarkSurfaceRow(
        leadingText: exactLeadingText,
        text: paintInput.text,
        globalUtf16Start: paintInput.globalStart,
        kind: rowCertified ? row.kind : 0,
        headingLevel: row.headingLevel,
        blockQuoteDepth: rowCertified ? blockQuote?.nestingDepth : null,
        codeBlock: rowCertified ? row.codeBlock : null,
        thematicBreak: rowCertified && row.thematicBreak,
        ordinal: row.ordinal,
        active: active,
        selection: includeEditingState ? paintInput.selection : null,
        runs: [
          FlarkSurfaceTextRun(
            text: paintInput.text,
            sourceUtf16Start: paintInput.globalStart,
            sourceUtf16End: paintInput.globalStart + paintInput.text.length,
            sourceExact: true,
            styles: const {},
          ),
        ],
      );
    }
    final baseRange = rowCertified
        ? (row.editableUtf16 ?? row.sourceUtf16)
        : row.sourceUtf16;
    final range = _mapViewportRange(baseRange);
    final leadingText = !rowCertified
        ? exactLeadingText
        : listItem != null
        ? _projectedListPrefix(listItem)
        : blockQuote != null
        ? _projectedBlockQuotePrefix(blockQuote)
        : '';
    final runs = rowCertified && row.projectionSegments != null
        ? row.projectionSegments!
              .map(
                (segment) =>
                    _exactSurfaceRun(_mapViewportRange(segment.sourceUtf16)),
              )
              .toList(growable: false)
        : rowCertified && row.table != null && row.inlineFacts != null
        ? _projectTableRuns(row.table!, row.inlineFacts!)
        : rowCertified && row.inlineFacts != null
        ? _projectInlineRuns(range, row.inlineFacts!)
        : [_exactSurfaceRun(range)];
    final text = runs.map((run) => run.text).join();
    return FlarkSurfaceRow(
      leadingText: leadingText,
      text: text,
      globalUtf16Start: range.start,
      kind: rowCertified ? row.kind : 0,
      headingLevel: row.headingLevel,
      blockQuoteDepth: rowCertified ? blockQuote?.nestingDepth : null,
      codeBlock: rowCertified ? row.codeBlock : null,
      thematicBreak: rowCertified && row.thematicBreak,
      ordinal: row.ordinal,
      active: active,
      selection: active || selected
          ? _projectedSelection(runs, text.length)
          : null,
      runs: runs,
    );
  }

  /// Ordered framework-neutral presentations currently replacing one stale
  /// certified row. Most rows return one surface; a structural edit that
  /// temporarily creates mixed block semantics may return a small set.
  List<FlarkSurfaceRow> surfaceRowsFor(
    FlarkViewportRow row, {
    bool includeEditingState = true,
  }) {
    final structurals = _structuralSurfacesFor(row.ordinal);
    if (structurals.isEmpty) {
      return [surfaceRow(row, includeEditingState: includeEditingState)];
    }
    return List.unmodifiable(
      structurals.map(
        (structural) => _committedStructuralSurfaceRow(
          structural,
          includeEditingState: includeEditingState,
        ),
      ),
    );
  }

  List<FlarkCoreCommittedPresentationSurfaceV1> _structuralSurfacesFor(
    int ordinal,
  ) => _committedStructuralSurfaces
      .where((surface) => surface.rowOrdinal == ordinal)
      .toList(growable: false);

  /// Exact source extent currently owned by one rendered row.
  FlarkSourceRange surfaceSourceRange(FlarkViewportRow row) {
    final structurals = _structuralSurfacesFor(row.ordinal);
    if (structurals.isNotEmpty) {
      return FlarkSourceRange(
        structurals.first.sourceUtf16.start,
        structurals.last.sourceUtf16.end,
      );
    }
    final mapped = _mappedExactRowRange(row);
    final split = _committedParagraphSplit;
    if (split == null ||
        split.rowOrdinal != row.ordinal ||
        split.rowEndUtf16 < mapped.start ||
        split.rowEndUtf16 > mapped.end) {
      return mapped;
    }
    return FlarkSourceRange(mapped.start, split.rowEndUtf16);
  }

  FlarkSurfaceRow neutralSurfaceRow({
    required int globalUtf16Start,
    required String text,
    required int ordinal,
    bool includeEditingState = true,
  }) {
    final surfaceOrdinal = -ordinal - 1;
    final range = FlarkSourceRange(
      globalUtf16Start,
      globalUtf16Start + text.length,
    );
    if (includeEditingState &&
        _crossRowSelection &&
        (_selectionIntersects(range) || _activeOrdinal == surfaceOrdinal)) {
      return _exactSelectionSurfaceRow(range: range, ordinal: surfaceOrdinal);
    }
    if (includeEditingState &&
        _activeOrdinal == surfaceOrdinal &&
        _globalSelectionExtent >= globalUtf16Start &&
        _globalSelectionExtent <= globalUtf16Start + text.length) {
      final paintInput = _paintInputWindow(
        sourceStart: globalUtf16Start,
        sourceEnd: globalUtf16Start + text.length,
      );
      return FlarkSurfaceRow(
        leadingText: '',
        text: paintInput.text,
        globalUtf16Start: paintInput.globalStart,
        kind: 0,
        headingLevel: null,
        blockQuoteDepth: null,
        codeBlock: null,
        thematicBreak: false,
        ordinal: surfaceOrdinal,
        active: true,
        selection: paintInput.selection,
        runs: [
          FlarkSurfaceTextRun(
            text: paintInput.text,
            sourceUtf16Start: paintInput.globalStart,
            sourceUtf16End: paintInput.globalStart + paintInput.text.length,
            sourceExact: true,
            styles: const {},
          ),
        ],
      );
    }
    return FlarkSurfaceRow(
      leadingText: '',
      text: text,
      globalUtf16Start: globalUtf16Start,
      kind: 0,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: surfaceOrdinal,
      active: false,
      selection: null,
      runs: [
        FlarkSurfaceTextRun(
          text: text,
          sourceUtf16Start: globalUtf16Start,
          sourceUtf16End: globalUtf16Start + text.length,
          sourceExact: true,
          styles: const {},
        ),
      ],
    );
  }

  FlarkSurfaceRow _exactSelectionSurfaceRow({
    required FlarkSourceRange range,
    required int ordinal,
  }) {
    final text = _sliceVisibleUtf16(range.start, range.end);
    return FlarkSurfaceRow(
      leadingText: '',
      text: text,
      globalUtf16Start: range.start,
      kind: 0,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: ordinal,
      active: _activeOrdinal == ordinal,
      selection: _selectionForRange(range),
      runs: [
        FlarkSurfaceTextRun(
          text: text,
          sourceUtf16Start: range.start,
          sourceUtf16End: range.end,
          sourceExact: true,
          styles: const {},
        ),
      ],
    );
  }

  TextSelection _projectedSelection(
    List<FlarkSurfaceTextRun> runs,
    int textLength,
  ) {
    int project(int sourceOffset, TextAffinity affinity) {
      if (runs.isEmpty) return 0;
      var consumed = 0;
      for (final run in runs) {
        if (sourceOffset < run.sourceUtf16Start) return consumed;
        if (sourceOffset <= run.sourceUtf16End) {
          return (consumed +
                  run.textOffsetForSourceOffset(
                    sourceOffset,
                    affinity: affinity,
                  ))
              .clamp(0, textLength);
        }
        consumed += run.text.length;
      }
      return textLength;
    }

    final affinity = _inputValue.selection.affinity;
    return TextSelection(
      baseOffset: project(_globalSelectionBase, affinity),
      extentOffset: project(_globalSelectionExtent, affinity),
      affinity: affinity,
      isDirectional: _inputValue.selection.isDirectional,
    );
  }

  FlarkSurfaceRow _committedStructuralSurfaceRow(
    FlarkCoreCommittedPresentationSurfaceV1 structural, {
    required bool includeEditingState,
  }) {
    final presentation = structural.presentation;
    final runs = presentation.runs
        .map(_surfaceRunFromCore)
        .toList(growable: false);
    final active =
        includeEditingState &&
        structural.sourceUtf16.start <= _globalSelectionExtent &&
        _globalSelectionExtent <= structural.sourceUtf16.end;
    final selected =
        includeEditingState &&
        _crossRowSelection &&
        _selectionIntersects(structural.sourceUtf16);
    return FlarkSurfaceRow(
      leadingText: presentation.leadingText,
      text: presentation.text,
      globalUtf16Start: presentation.globalUtf16Start,
      kind: presentation.kind,
      headingLevel: presentation.headingLevel,
      blockQuoteDepth: presentation.blockQuoteDepth,
      codeBlock: presentation.codeBlock,
      thematicBreak: presentation.thematicBreak,
      ordinal: structural.rowOrdinal,
      active: active,
      selection: active || selected
          ? _projectedSelection(runs, presentation.text.length)
          : null,
      runs: runs,
    );
  }

  FlarkSurfaceRow _activeContinuitySurface(
    _ProjectionContinuitySurface continuity,
    int ordinal,
  ) {
    final presentation = continuity.presentation;
    return FlarkSurfaceRow(
      leadingText: presentation.leadingText,
      text: presentation.text,
      globalUtf16Start: presentation.globalUtf16Start,
      kind: presentation.kind,
      headingLevel: presentation.headingLevel,
      blockQuoteDepth: presentation.blockQuoteDepth,
      codeBlock: presentation.codeBlock,
      thematicBreak: presentation.thematicBreak,
      ordinal: ordinal,
      active: true,
      selection: _projectedSelection(
        presentation.runs,
        presentation.text.length,
      ),
      runs: presentation.runs,
    );
  }

  FlarkSurfaceTextRun _exactSurfaceRun(FlarkSourceRange range) =>
      FlarkSurfaceTextRun(
        text: _sliceVisibleUtf16(range.start, range.end),
        sourceUtf16Start: range.start,
        sourceUtf16End: range.end,
        sourceExact: true,
        styles: const {},
      );

  List<FlarkSurfaceTextRun> _projectInlineRuns(
    FlarkSourceRange range,
    List<FlarkInlineFact> facts,
  ) {
    if (facts.isEmpty) return [_exactSurfaceRun(range)];
    final mapped = facts
        .map(
          (fact) => (
            kind: fact.kind,
            source: _mapViewportRange(fact.sourceUtf16),
            content: _mapViewportRange(fact.contentUtf16),
            replacement: fact.replacement,
          ),
        )
        .toList(growable: false);
    final boundaries = <int>{range.start, range.end};
    final hidden = <FlarkSourceRange>[];
    for (final fact in mapped) {
      boundaries
        ..add(fact.source.start)
        ..add(fact.content.start)
        ..add(fact.content.end)
        ..add(fact.source.end);
      if (fact.source.start < fact.content.start) {
        hidden.add(FlarkSourceRange(fact.source.start, fact.content.start));
      }
      if (fact.content.end < fact.source.end) {
        hidden.add(FlarkSourceRange(fact.content.end, fact.source.end));
      }
    }
    final ordered = boundaries.toList()..sort();
    final runs = <FlarkSurfaceTextRun>[];
    for (var index = 0; index + 1 < ordered.length; index++) {
      final start = ordered[index];
      final end = ordered[index + 1];
      if (start == end ||
          start < range.start ||
          end > range.end ||
          hidden.any((cut) => start >= cut.start && end <= cut.end)) {
        continue;
      }
      final styles = <FlarkSurfaceInlineStyle>{};
      for (final fact in mapped) {
        if (start >= fact.content.start && end <= fact.content.end) {
          final style = _surfaceStyleFor(fact.kind);
          if (style != null) styles.add(style);
        }
      }
      String? replacement;
      for (final fact in mapped) {
        if (fact.replacement != null &&
            fact.source.start == start &&
            fact.source.end == end) {
          replacement = fact.replacement;
          break;
        }
      }
      final sourceExact = replacement == null;
      final text = replacement ?? _sliceVisibleUtf16(start, end);
      if (runs.isNotEmpty &&
          sourceExact &&
          runs.last.sourceExact &&
          runs.last.sourceUtf16End == start &&
          setEquals(runs.last.styles, styles)) {
        final prior = runs.removeLast();
        runs.add(
          FlarkSurfaceTextRun(
            text: prior.text + text,
            sourceUtf16Start: prior.sourceUtf16Start,
            sourceUtf16End: end,
            sourceExact: true,
            styles: Set.unmodifiable(styles),
          ),
        );
      } else {
        runs.add(
          FlarkSurfaceTextRun(
            text: text,
            sourceUtf16Start: start,
            sourceUtf16End: end,
            sourceExact: sourceExact,
            styles: Set.unmodifiable(styles),
          ),
        );
      }
    }
    return List.unmodifiable(runs);
  }

  List<FlarkSurfaceTextRun> _projectTableRuns(
    FlarkTablePresentation table,
    List<FlarkInlineFact> facts,
  ) {
    final runs = <FlarkSurfaceTextRun>[];
    for (var rowIndex = 0; rowIndex < table.rows.length; rowIndex++) {
      final cells = table.rows[rowIndex];
      for (var column = 0; column < cells.length; column++) {
        final cell = cells[column];
        final content = _mapViewportRange(cell.contentUtf16);
        final cellFacts = facts
            .where(
              (fact) =>
                  fact.sourceUtf16.start >= cell.contentUtf16.start &&
                  fact.sourceUtf16.end <= cell.contentUtf16.end,
            )
            .toList(growable: false);
        runs.addAll(_projectInlineRuns(content, cellFacts));
        final lastColumn = column + 1 == cells.length;
        final lastRow = rowIndex + 1 == table.rows.length;
        if (!lastColumn) {
          final next = _mapViewportRange(cells[column + 1].contentUtf16);
          runs.add(
            FlarkSurfaceTextRun(
              text: ' │ ',
              sourceUtf16Start: content.end,
              sourceUtf16End: next.start,
              sourceExact: false,
              styles: const {},
            ),
          );
        } else if (!lastRow) {
          final next = _mapViewportRange(
            table.rows[rowIndex + 1].first.contentUtf16,
          );
          runs.add(
            FlarkSurfaceTextRun(
              text: '\n',
              sourceUtf16Start: content.end,
              sourceUtf16End: next.start,
              sourceExact: false,
              styles: const {},
            ),
          );
        }
      }
    }
    return List.unmodifiable(runs);
  }

  static FlarkSurfaceInlineStyle? _surfaceStyleFor(FlarkInlineFactKind kind) =>
      switch (kind) {
        FlarkInlineFactKind.emphasis => FlarkSurfaceInlineStyle.emphasis,
        FlarkInlineFactKind.strong => FlarkSurfaceInlineStyle.strong,
        FlarkInlineFactKind.code => FlarkSurfaceInlineStyle.code,
        FlarkInlineFactKind.strikethrough =>
          FlarkSurfaceInlineStyle.strikethrough,
        FlarkInlineFactKind.autolinkUri ||
        FlarkInlineFactKind.autolinkEmail ||
        FlarkInlineFactKind.directLink ||
        FlarkInlineFactKind.referenceLink => FlarkSurfaceInlineStyle.link,
        FlarkInlineFactKind.backslashEscape ||
        FlarkInlineFactKind.hardLineBreak ||
        FlarkInlineFactKind.replacement ||
        FlarkInlineFactKind.directImage ||
        FlarkInlineFactKind.referenceImage => null,
        FlarkInlineFactKind.tableCell => null,
      };

  void activateRow(
    FlarkViewportRow row,
    int globalUtf16Offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    _semanticEditV1Active = _supportsSemanticEditV1(row);
    _committedParagraphSplit = null;
    _projectionContinuity = null;
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    _abandonOversizedSelection();
    final range = _mapViewportRange(_activationRange(row));
    final text = _sliceVisibleUtf16(range.start, range.end);
    _activateWindow(
      text: text,
      sourceStart: range.start,
      caret: globalUtf16Offset,
      ordinal: row.ordinal,
      affinity: affinity,
    );
    unawaited(_installCanonicalSelection(_selectionSnapshot()));
  }

  void activateNeutralLine({
    required String text,
    required int globalUtf16Start,
    required int globalUtf16Offset,
    required int ordinal,
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    _semanticEditV1Active = false;
    _projectionContinuity = null;
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    _abandonOversizedSelection();
    _activateWindow(
      text: text,
      sourceStart: globalUtf16Start,
      caret: globalUtf16Offset,
      ordinal: -ordinal - 1,
      affinity: affinity,
    );
    unawaited(_installCanonicalSelection(_selectionSnapshot()));
  }

  void extendSelectionTo(int globalUtf16Offset, {int? activeOrdinal}) {
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    final local = globalUtf16Offset - _inputGlobalUtf16Start;
    final remainsInActiveWindow =
        !_crossRowSelection &&
        local >= 0 &&
        local <= _inputValue.text.length &&
        (activeOrdinal == null || activeOrdinal == _activeOrdinal);
    if (remainsInActiveWindow) {
      _inputValue = _inputValue.copyWith(
        selection: TextSelection(
          baseOffset: _inputValue.selection.baseOffset,
          extentOffset: local,
          affinity: _inputValue.selection.affinity,
          isDirectional: _inputValue.selection.isDirectional,
        ),
        composing: TextRange.empty,
      );
      _globalSelectionExtent = globalUtf16Offset;
      unawaited(_installCanonicalSelection(_selectionSnapshot()));
      notifyListeners();
      return;
    }

    final visibleEnd = _visibleUtf16Start + _visibleSource.length;
    final start = math.min(_globalSelectionBase, globalUtf16Offset);
    final end = math.max(_globalSelectionBase, globalUtf16Offset);
    if (end - start > _maximumInputCodeUnits ||
        start < _visibleUtf16Start ||
        end > visibleEnd) {
      final exactBase = _globalSelectionBase;
      _globalSelectionExtent = globalUtf16Offset;
      _activeOrdinal = activeOrdinal ?? _surfaceOrdinalAt(globalUtf16Offset);
      _crossRowSelection = true;
      _oversizedSelection = true;
      _restoreCollapsedInputWindow(
        globalUtf16Offset,
        preferredOrdinal: _activeOrdinal,
      );
      _globalSelectionBase = exactBase;
      _globalSelectionExtent = globalUtf16Offset;
      _crossRowSelection = true;
      _oversizedSelection = true;
      notifyListeners();
      unawaited(selectOversizedRangeUtf16(exactBase, globalUtf16Offset));
      return;
    }
    final selection = TextSelection(
      baseOffset: _globalSelectionBase - start,
      extentOffset: globalUtf16Offset - start,
      affinity: _inputValue.selection.affinity,
      isDirectional: true,
    );
    _inputGlobalUtf16Start = start;
    _inputValue = TextEditingValue(
      text: _sliceVisibleUtf16(start, end),
      selection: selection,
    );
    _activeOrdinal = activeOrdinal ?? _surfaceOrdinalAt(globalUtf16Offset);
    _crossRowSelection = !selection.isCollapsed;
    _globalSelectionExtent = globalUtf16Offset;
    unawaited(_installCanonicalSelection(_selectionSnapshot()));
    notifyListeners();
  }

  void applyDeltas(List<TextEditingDelta> deltas) {
    final started = DateTime.now().microsecondsSinceEpoch;
    _activePlatformCallbackStartedEpochMicros = started;
    try {
      _applyDeltas(deltas);
    } finally {
      final elapsed = DateTime.now().microsecondsSinceEpoch - started;
      final pending = _pendingSemanticInput;
      if (pending?.initialCallbackStartedEpochMicros == started) {
        pending!.initialCallbackMicros = math.max(0, elapsed);
      }
      _activePlatformCallbackStartedEpochMicros = null;
    }
  }

  void _applyDeltas(List<TextEditingDelta> deltas) {
    if (_historyReplayPending) {
      notifyListeners();
      return;
    }
    if (_captureSemanticSuccessors(deltas)) return;
    if (_captureLateSemanticSuccessors(deltas)) return;
    final rejection = _validateDeltaBatch(deltas);
    if (rejection != FlarkInputResyncReason.none) {
      _resynchronize(rejection);
      return;
    }
    _platformMutation = true;
    try {
      if (_isPlatformNewlineMutation(deltas) &&
          _queuePlatformSemanticNewline(deltas)) {
        return;
      }
      if (_isPlatformDeleteBackwardMutation(deltas) &&
          _queuePlatformSemanticDeleteBackward(deltas)) {
        return;
      }
      if (_isPlatformNewlineMutation(deltas)) {
        insertNewline();
        return;
      }
      var finalValue = _inputValue;
      var mutatingDeltas = 0;
      var typingInput = true;
      for (final delta in deltas) {
        finalValue = delta.apply(finalValue);
        if (_mutationFor(delta) != null) {
          mutatingDeltas += 1;
          typingInput = typingInput && delta is TextEditingDeltaInsertion;
        }
      }
      if (mutatingDeltas == 0) {
        _breakTypingHistoryGroup();
        finalValue = _normalizeProjectedSelection(finalValue);
        _inputValue = finalValue;
        _trackCompositionWithoutMutation(finalValue.composing);
        _updateGlobalSelection();
        unawaited(_installCanonicalSelection(_selectionSnapshot()));
      } else {
        final before = _inputValue.text;
        final after = finalValue.text;
        final mutation = _differenceMutation(before, after);
        if (mutation == null) {
          _breakTypingHistoryGroup();
          _inputValue = finalValue;
          _trackCompositionWithoutMutation(finalValue.composing);
          _updateGlobalSelection();
          unawaited(_installCanonicalSelection(_selectionSnapshot()));
          notifyListeners();
          return;
        }
        final accepted = _acceptMutation(
          mutation,
          selection: finalValue.selection,
          composing: finalValue.composing,
          typingInput: typingInput,
          fullValue: finalValue.text.length <= _maximumInputCodeUnits
              ? finalValue
              : null,
        );
        if (!accepted) {
          _resynchronize(FlarkInputResyncReason.rangeOutOfWindow);
          return;
        }
      }
      notifyListeners();
    } finally {
      _platformMutation = false;
    }
  }

  void updateEditingValue(TextEditingValue value) {
    final started = DateTime.now().microsecondsSinceEpoch;
    _activePlatformCallbackStartedEpochMicros = started;
    try {
      _updateEditingValue(value);
    } finally {
      final elapsed = DateTime.now().microsecondsSinceEpoch - started;
      final pending = _pendingSemanticInput;
      if (pending?.initialCallbackStartedEpochMicros == started) {
        pending!.initialCallbackMicros = math.max(0, elapsed);
      }
      _activePlatformCallbackStartedEpochMicros = null;
    }
  }

  void _updateEditingValue(TextEditingValue value) {
    if (_historyReplayPending) {
      notifyListeners();
      return;
    }
    if (_captureSemanticSuccessorValue(value)) return;
    if (value.text == _inputValue.text) _lateSemanticInput = null;
    _platformMutation = true;
    try {
      if (_isPlatformNewlineValue(value) &&
          _queuePlatformSemanticNewlineValue(value)) {
        return;
      }
      if (_isPlatformDeleteBackwardValue(value) &&
          _queuePlatformSemanticDeleteBackwardValue(value)) {
        return;
      }
      _updateEditingValueFromPlatform(value);
    } finally {
      _platformMutation = false;
    }
  }

  void _updateEditingValueFromPlatform(TextEditingValue value) {
    if (value.text == _inputValue.text) {
      _breakTypingHistoryGroup();
      value = _normalizeProjectedSelection(value);
      _inputValue = value;
      _trackCompositionWithoutMutation(value.composing);
      _updateGlobalSelection();
      unawaited(_installCanonicalSelection(_selectionSnapshot()));
      notifyListeners();
      return;
    }
    final before = _inputValue.text;
    final after = value.text;
    var prefix = 0;
    while (prefix < before.length &&
        prefix < after.length &&
        before.codeUnitAt(prefix) == after.codeUnitAt(prefix)) {
      prefix += 1;
    }
    var oldSuffix = before.length;
    var newSuffix = after.length;
    while (oldSuffix > prefix &&
        newSuffix > prefix &&
        before.codeUnitAt(oldSuffix - 1) == after.codeUnitAt(newSuffix - 1)) {
      oldSuffix -= 1;
      newSuffix -= 1;
    }
    final replacement = after.substring(prefix, newSuffix);
    _acceptMutation(
      _TextMutation(prefix, oldSuffix, replacement),
      selection: value.selection,
      composing: value.composing,
      fullValue: value.text.length <= _maximumInputCodeUnits ? value : null,
    );
    notifyListeners();
  }

  void replaceSelection(String replacement) {
    if (_deferSemanticSuccessor(replacement: replacement)) return;
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    if (_oversizedSelection) {
      _pendingEdits += 1;
      _status = FlarkEditorStatus.editing;
      notifyListeners();
      unawaited(
        _replaceOversizedSelection(replacement)
            .catchError((Object error, StackTrace stackTrace) {
              _lastError = error;
              _status = FlarkEditorStatus.faulted;
            })
            .whenComplete(() {
              _pendingEdits = math.max(0, _pendingEdits - 1);
              notifyListeners();
            }),
      );
      return;
    }
    final selection = _inputValue.selection;
    final start = math.min(selection.baseOffset, selection.extentOffset);
    final end = math.max(selection.baseOffset, selection.extentOffset);
    final caret = start + replacement.length;
    _acceptMutation(
      _TextMutation(start, end, replacement),
      selection: TextSelection.collapsed(offset: caret),
      composing: TextRange.empty,
    );
    notifyListeners();
  }

  /// Installs a canonical anchored selection larger than the bounded input
  /// window can represent. The platform sees only a collapsed active-extent
  /// surrogate; typing, paste, or deletion against it replaces the complete
  /// exact global selection atomically through the anchor-resolved range.
  Future<int> selectOversizedRangeUtf16(int base, int extent) async {
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    final length = sourceUtf16Length;
    final clampedBase = base.clamp(0, length);
    final clampedExtent = extent.clamp(0, length);
    final exactSelection = _EditorSelectionSnapshot(
      TextSelection(baseOffset: clampedBase, extentOffset: clampedExtent),
      _activeOrdinal,
    );
    final generation = await _installCanonicalSelection(exactSelection);
    _oversizedSelection = true;
    _crossRowSelection = clampedBase != clampedExtent;
    await _restoreHistorySelection(
      _EditorSelectionSnapshot(
        TextSelection.collapsed(offset: clampedExtent),
        null,
      ),
    );
    _globalSelectionBase = clampedBase;
    _globalSelectionExtent = clampedExtent;
    _crossRowSelection = clampedBase != clampedExtent;
    _oversizedSelection = true;
    notifyListeners();
    return generation;
  }

  /// Reads the complete authoritative Markdown source after every edit
  /// already accepted by this controller has settled in the Core session.
  Future<String> readSource() => _editTail.then((_) => _document.readSource());

  /// A user activation abandons the platform surrogate. The immediately
  /// queued ordinary selection replaces the canonical anchors in order.
  void _abandonOversizedSelection() {
    if (!_oversizedSelection) return;
    _oversizedSelection = false;
  }

  /// Resolves the exact core-owned selection after every queued edit or host
  /// selection replacement ahead of it has completed.
  Future<FlarkCoreSelectionSnapshot?> resolveCanonicalSelection() =>
      _editTail.then((_) => _session.resolveSelection());

  /// Reads the complete selected source even when the platform input window
  /// carries only an active-extent surrogate.
  Future<String?> readSelectedText() async {
    if (!_crossRowSelection && !_oversizedSelection) return selectedText;
    final selection = await resolveCanonicalSelection();
    if (selection == null || selection.isCollapsed) return null;
    final start = math.min(selection.base, selection.extent);
    final end = math.max(selection.base, selection.extent);
    return _document.readSourceUtf16Range(start, end);
  }

  Future<void> _replaceOversizedSelection(String replacement) async {
    final resolved = await resolveCanonicalSelection();
    _oversizedSelection = false;
    if (resolved == null) return;
    final start = math.min(resolved.base, resolved.extent);
    final end = math.max(resolved.base, resolved.extent);
    final caret = start + replacement.length;
    final beforeSelection = _EditorSelectionSnapshot(
      TextSelection(baseOffset: resolved.base, extentOffset: resolved.extent),
      null,
    );
    final afterSelection = _EditorSelectionSnapshot(
      TextSelection.collapsed(offset: caret),
      null,
    );
    _globalSelectionBase = caret;
    _globalSelectionExtent = caret;
    _queueNativeEdit(
      start,
      end,
      replacement,
      beforeSelection: beforeSelection,
      afterSelection: afterSelection,
      coalesceTyping: false,
      compositionHistoryGroup: null,
    );
    // The surrogate window rarely contains the post-replace caret, so the
    // synchronous recenter cannot reach it; restore through the async path
    // that can fetch a fresh bounded window once the edit is admitted.
    _editTail = _editTail
        .then((_) async {
          await _restoreHistorySelection(afterSelection);
          notifyListeners();
        })
        .catchError((Object _, StackTrace _) {});
    notifyListeners();
  }

  void deleteBackward() {
    if (_deferSemanticSuccessor(
      command: _DeferredInputCommand.deleteBackward,
    )) {
      return;
    }
    if (_oversizedSelection) {
      replaceSelection('');
      return;
    }
    final selection = _inputValue.selection;
    if (!selection.isCollapsed) {
      replaceSelection('');
      return;
    }
    if (_queueSemanticDeleteBackward(selection.extentOffset)) {
      return;
    }
    if (_deleteProjectedVisible(backward: true)) return;
    if (selection.extentOffset == 0) return;
    final end = selection.extentOffset;
    final cluster = FlarkCoreGraphemePolicy.previousClusterRange(
      _inputValue.text,
      end,
    );
    if (cluster == null) return;
    _inputValue = _inputValue.copyWith(
      selection: TextSelection(baseOffset: cluster.$1, extentOffset: end),
    );
    replaceSelection('');
  }

  void deleteForward() {
    if (_deferSemanticSuccessor(command: _DeferredInputCommand.deleteForward)) {
      return;
    }
    if (_oversizedSelection) {
      replaceSelection('');
      return;
    }
    final selection = _inputValue.selection;
    if (!selection.isCollapsed) {
      replaceSelection('');
      return;
    }
    if (_deleteProjectedVisible(backward: false)) return;
    final start = selection.extentOffset;
    if (start == _inputValue.text.length) return;
    final cluster = FlarkCoreGraphemePolicy.nextClusterRange(
      _inputValue.text,
      start,
    );
    if (cluster == null) return;
    _inputValue = _inputValue.copyWith(
      selection: TextSelection(baseOffset: start, extentOffset: cluster.$2),
    );
    replaceSelection('');
  }

  void insertNewline() {
    if (_deferSemanticSuccessor(command: _DeferredInputCommand.insertNewline)) {
      return;
    }
    final selection = _inputValue.selection;
    if (selection.isCollapsed &&
        _queueSemanticParagraphBreak(selection.extentOffset)) {
      return;
    }
    replaceSelection('\n');
  }

  bool _isPlatformNewlineMutation(List<TextEditingDelta> deltas) {
    if (deltas.length != 1 ||
        _inputValue.composing != TextRange.empty ||
        deltas.single.composing != TextRange.empty) {
      return false;
    }
    final mutation = _mutationFor(deltas.single);
    if (mutation == null || mutation.replacement != '\n') return false;
    final selection = _inputValue.selection;
    if (!selection.isValid) return false;
    return mutation.start ==
            math.min(selection.baseOffset, selection.extentOffset) &&
        mutation.end == math.max(selection.baseOffset, selection.extentOffset);
  }

  bool _queuePlatformSemanticNewline(List<TextEditingDelta> deltas) {
    _lateSemanticInput = null;
    final provisionalMutation = _mutationFor(deltas.single)!;
    final provisionalAfter = deltas.single.apply(_inputValue);
    _pendingSemanticInput = _PendingSemanticInput(
      base: _inputValue,
      inputGlobalUtf16Start: _inputGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          _activePlatformCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      provisionalMutation: provisionalMutation,
      provisionalAfter: provisionalAfter,
    );
    _platformNewlineMutationAwaitingAction = true;
    final queued = _queueSemanticParagraphBreak(
      _inputValue.selection.extentOffset,
    );
    if (!queued) {
      _pendingSemanticInput = null;
      _platformNewlineMutationAwaitingAction = false;
    }
    return queued;
  }

  bool _isPlatformNewlineValue(TextEditingValue value) {
    if (_inputValue.composing != TextRange.empty ||
        value.composing != TextRange.empty) {
      return false;
    }
    final selection = _inputValue.selection;
    if (!selection.isValid) return false;
    final start = math.min(selection.baseOffset, selection.extentOffset);
    final end = math.max(selection.baseOffset, selection.extentOffset);
    return _inputValue.text.replaceRange(start, end, '\n') == value.text;
  }

  bool _queuePlatformSemanticNewlineValue(TextEditingValue value) {
    _lateSemanticInput = null;
    final selection = _inputValue.selection;
    final provisionalMutation = _TextMutation(
      math.min(selection.baseOffset, selection.extentOffset),
      math.max(selection.baseOffset, selection.extentOffset),
      '\n',
    );
    _pendingSemanticInput = _PendingSemanticInput(
      base: _inputValue,
      inputGlobalUtf16Start: _inputGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          _activePlatformCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      provisionalMutation: provisionalMutation,
      provisionalAfter: value,
    );
    _platformNewlineMutationAwaitingAction = true;
    final queued = _queueSemanticParagraphBreak(
      _inputValue.selection.extentOffset,
    );
    if (!queued) {
      _pendingSemanticInput = null;
      _platformNewlineMutationAwaitingAction = false;
    }
    return queued;
  }

  bool _isPlatformDeleteBackwardMutation(List<TextEditingDelta> deltas) {
    if (deltas.length != 1 ||
        _inputValue.composing != TextRange.empty ||
        deltas.single.composing != TextRange.empty ||
        !_inputValue.selection.isCollapsed) {
      return false;
    }
    final mutation = _mutationFor(deltas.single);
    final caret = _inputValue.selection.extentOffset;
    return mutation != null &&
        mutation.replacement.isEmpty &&
        mutation.start < mutation.end &&
        mutation.end == caret;
  }

  bool _queuePlatformSemanticDeleteBackward(List<TextEditingDelta> deltas) {
    return _queueObservedPlatformSemanticDeleteBackward(
      provisionalMutation: _mutationFor(deltas.single)!,
      provisionalAfter: deltas.single.apply(_inputValue),
    );
  }

  bool _isPlatformDeleteBackwardValue(TextEditingValue value) {
    if (_inputValue.composing != TextRange.empty ||
        value.composing != TextRange.empty ||
        !_inputValue.selection.isCollapsed) {
      return false;
    }
    final mutation = _differenceMutation(_inputValue.text, value.text);
    final caret = _inputValue.selection.extentOffset;
    return mutation != null &&
        mutation.replacement.isEmpty &&
        mutation.start < mutation.end &&
        mutation.end == caret;
  }

  bool _queuePlatformSemanticDeleteBackwardValue(TextEditingValue value) {
    return _queueObservedPlatformSemanticDeleteBackward(
      provisionalMutation: _differenceMutation(_inputValue.text, value.text)!,
      provisionalAfter: value,
    );
  }

  bool _queueObservedPlatformSemanticDeleteBackward({
    required _TextMutation provisionalMutation,
    required TextEditingValue provisionalAfter,
  }) {
    _lateSemanticInput = null;
    _pendingSemanticInput = _PendingSemanticInput(
      base: _inputValue,
      inputGlobalUtf16Start: _inputGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          _activePlatformCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      provisionalMutation: provisionalMutation,
      provisionalAfter: provisionalAfter,
    );
    _platformDeleteBackwardMutationAwaitingSelector = true;
    final queued = _queueSemanticDeleteBackward(
      _inputValue.selection.extentOffset,
    );
    if (!queued) {
      _pendingSemanticInput = null;
      _platformDeleteBackwardMutationAwaitingSelector = false;
    }
    return queued;
  }

  /// Adopts a platform action only when no preceding text observation already
  /// carried the newline. macOS deliberately emits both for one Return.
  void observePlatformNewlineAction() {
    if (_platformNewlineMutationAwaitingAction) {
      _platformNewlineMutationAwaitingAction = false;
      return;
    }
    insertNewline();
  }

  /// Adopts a selector only when no preceding text observation already
  /// carried the same Backspace. Desktop embedders may emit both; mobile
  /// generally supplies only the deletion delta or full value.
  void observePlatformDeleteBackwardAction() {
    if (_platformDeleteBackwardMutationAwaitingSelector &&
        (_pendingSemanticInput != null || _lateSemanticInput != null)) {
      _platformDeleteBackwardMutationAwaitingSelector = false;
      return;
    }
    _platformDeleteBackwardMutationAwaitingSelector = false;
    deleteBackward();
  }

  bool _captureSemanticSuccessors(List<TextEditingDelta> deltas) {
    final pending = _pendingSemanticInput;
    if (pending == null) return false;
    if (!_reserveSemanticSuccessor(pending)) return true;
    if (pending.successors.isNotEmpty &&
        pending.successors.last is _DeferredInputSuccessor) {
      _pendingSemanticInput = null;
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    final before = pending.provisionalTail;
    final rejection = _validateDeltaBatch(
      deltas,
      against: before,
      expectedTextSha256: flarkWindowTextSha256(before.text),
    );
    if (rejection != FlarkInputResyncReason.none) {
      _pendingSemanticInput = null;
      _resynchronize(rejection);
      return true;
    }
    var after = before;
    var typingInput = true;
    for (final delta in deltas) {
      after = delta.apply(after);
      if (_mutationFor(delta) != null) {
        typingInput = typingInput && delta is TextEditingDeltaInsertion;
      }
    }
    pending.successors.add(
      _ProvisionalInputBatch(
        before: before,
        after: after,
        typingInput: typingInput,
      ),
    );
    pending.provisionalTail = after;
    _recordSemanticSuccessorHighWatermark(pending);
    return true;
  }

  bool _captureLateSemanticSuccessors(List<TextEditingDelta> deltas) {
    final late = _lateSemanticInput;
    if (late == null) return false;
    final before = late.provisionalTail;
    final rejection = _validateDeltaBatch(
      deltas,
      against: before,
      expectedTextSha256: flarkWindowTextSha256(before.text),
    );
    if (rejection != FlarkInputResyncReason.none) {
      // The platform has adopted the committed window. Let the ordinary lane
      // validate this callback against that current window.
      _lateSemanticInput = null;
      return false;
    }
    if (late.successorCount >= _maximumSemanticSuccessors) {
      _lateSemanticInput = null;
      _resynchronize(FlarkInputResyncReason.successorQueueOverflow);
      return true;
    }
    var after = before;
    var typingInput = true;
    for (final delta in deltas) {
      after = delta.apply(after);
      if (_mutationFor(delta) != null) {
        typingInput = typingInput && delta is TextEditingDeltaInsertion;
      }
    }
    final holder = _PendingSemanticInput(
      base: before,
      inputGlobalUtf16Start: _inputGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          _activePlatformCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      provisionalAfter: before,
    );
    holder.successors.add(
      _ProvisionalInputBatch(
        before: before,
        after: after,
        typingInput: typingInput,
      ),
    );
    late.provisionalTail = after;
    late.successorCount += 1;
    _platformMutation = true;
    try {
      _promoteSemanticSuccessorsWithMap(holder, late.reconciliation);
      notifyListeners();
    } finally {
      _platformMutation = false;
    }
    return true;
  }

  bool _captureSemanticSuccessorValue(TextEditingValue value) {
    final pending = _pendingSemanticInput;
    if (pending == null) return false;
    if (!_reserveSemanticSuccessor(pending)) return true;
    if (!_validObservedValue(value) ||
        (pending.successors.isNotEmpty &&
            pending.successors.last is _DeferredInputSuccessor)) {
      _pendingSemanticInput = null;
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    final before = pending.provisionalTail;
    final mutation = _differenceMutation(before.text, value.text);
    pending.successors.add(
      _ProvisionalInputBatch(
        before: before,
        after: value,
        typingInput:
            mutation != null &&
            mutation.start == mutation.end &&
            mutation.replacement.isNotEmpty,
      ),
    );
    pending.provisionalTail = value;
    _recordSemanticSuccessorHighWatermark(pending);
    return true;
  }

  bool _deferSemanticSuccessor({
    _DeferredInputCommand? command,
    String? replacement,
  }) {
    final pending = _pendingSemanticInput;
    if (pending == null) return false;
    if (!_reserveSemanticSuccessor(pending)) return true;
    if (pending.successors.isNotEmpty &&
        pending.successors.last is _DeferredInputSuccessor) {
      _pendingSemanticInput = null;
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    pending.successors.add(
      _DeferredInputSuccessor(command, replacement: replacement),
    );
    _recordSemanticSuccessorHighWatermark(pending);
    return true;
  }

  bool _reserveSemanticSuccessor(_PendingSemanticInput pending) {
    if (pending.successors.length < _maximumSemanticSuccessors) return true;
    _pendingSemanticInput = null;
    _resynchronize(FlarkInputResyncReason.successorQueueOverflow);
    return false;
  }

  void _recordSemanticSuccessorHighWatermark(_PendingSemanticInput pending) {
    _semanticSuccessorHighWatermark = math.max(
      _semanticSuccessorHighWatermark,
      pending.successors.length,
    );
  }

  bool _validObservedValue(TextEditingValue value) {
    if (value.text.length > _maximumInputCodeUnits ||
        !value.selection.isValid ||
        value.selection.start < 0 ||
        value.selection.end > value.text.length) {
      return false;
    }
    final composing = value.composing;
    return composing == TextRange.empty ||
        (composing.isValid &&
            composing.start >= 0 &&
            composing.end <= value.text.length);
  }

  TextEditingValue _normalizeProjectedSelection(TextEditingValue value) {
    if (!value.selection.isValid || !value.composing.isCollapsed) return value;
    final row = _activeCachedRow();
    if (row == null) return value;
    final presentation = surfaceRow(row, includeEditingState: false);
    if (!_surfaceHasProjection(presentation, row)) return value;

    int normalize(int localOffset) {
      final global = _inputGlobalUtf16Start + localOffset;
      final display = presentation.textOffsetForSourceOffset(
        global,
        affinity: value.selection.affinity,
      );
      final normalizedGlobal = presentation.sourceOffsetForTextOffset(
        display,
        affinity: value.selection.affinity,
      );
      return (normalizedGlobal - _inputGlobalUtf16Start).clamp(
        0,
        value.text.length,
      );
    }

    final selection = TextSelection(
      baseOffset: normalize(value.selection.baseOffset),
      extentOffset: normalize(value.selection.extentOffset),
      affinity: value.selection.affinity,
      isDirectional: value.selection.isDirectional,
    );
    return selection == value.selection
        ? value
        : value.copyWith(selection: selection);
  }

  bool _deleteProjectedVisible({required bool backward}) {
    final row = _activeCachedRow();
    if (row == null) return false;
    final presentation = surfaceRow(row);
    if (!presentation.active || !_surfaceHasProjection(presentation, row)) {
      return false;
    }
    final selection = _inputValue.selection;
    if (!selection.isCollapsed) return false;
    final globalCaret = _inputGlobalUtf16Start + selection.extentOffset;
    final displayCaret = presentation.textOffsetForSourceOffset(
      globalCaret,
      affinity: selection.affinity,
    );
    final cluster = backward
        ? FlarkCoreGraphemePolicy.previousClusterRange(
            presentation.text,
            displayCaret,
          )
        : FlarkCoreGraphemePolicy.nextClusterRange(
            presentation.text,
            displayCaret,
          );
    // The visual edge can still have hidden source on the other side. Treat
    // that edge as a legal stop instead of deleting an invisible delimiter.
    if (cluster == null) return true;
    final sourceStart = presentation.sourceOffsetForTextOffset(
      cluster.$1,
      affinity: TextAffinity.downstream,
    );
    final sourceEnd = presentation.sourceOffsetForTextOffset(
      cluster.$2,
      affinity: TextAffinity.upstream,
    );
    // A visual neighbor separated from the caret by hidden source is not a
    // legal one-code-unit deletion. In particular, Backspace at the start of
    // a later projected quote line must not delete its hidden `> ` prefix one
    // code unit at a time or remove the visible newline while leaving that
    // prefix behind.
    if ((backward && sourceEnd != globalCaret) ||
        (!backward && sourceStart != globalCaret)) {
      return true;
    }
    if (sourceStart >= sourceEnd) return true;
    final localStart = sourceStart - _inputGlobalUtf16Start;
    final localEnd = sourceEnd - _inputGlobalUtf16Start;
    if (localStart < 0 || localEnd > _inputValue.text.length) return false;
    _inputValue = _inputValue.copyWith(
      selection: TextSelection(baseOffset: localStart, extentOffset: localEnd),
      composing: TextRange.empty,
    );
    replaceSelection('');
    return true;
  }

  bool _mutationTouchesOnlyHiddenProjection(_TextMutation mutation) {
    if (mutation.start == mutation.end) return false;
    final row = _activeCachedRow();
    if (row == null) return false;
    final presentation = surfaceRow(row, includeEditingState: false);
    if (!_surfaceHasProjection(presentation, row)) return false;
    final sourceStart = _inputGlobalUtf16Start + mutation.start;
    final sourceEnd = _inputGlobalUtf16Start + mutation.end;
    final displayStart = presentation.textOffsetForSourceOffset(
      sourceStart,
      affinity: TextAffinity.downstream,
    );
    final displayEnd = presentation.textOffsetForSourceOffset(
      sourceEnd,
      affinity: TextAffinity.upstream,
    );
    return displayStart == displayEnd;
  }

  bool _surfaceHasProjection(
    FlarkSurfaceRow presentation,
    FlarkViewportRow row,
  ) {
    if (presentation.runs.isEmpty) return false;
    final activation = _mapViewportRange(_activationRange(row));
    if (!_rowSemanticsCurrent(activation)) return false;
    var sourceCursor = activation.start;
    for (final run in presentation.runs) {
      if (!run.sourceExact || run.sourceUtf16Start != sourceCursor) return true;
      sourceCursor = run.sourceUtf16End;
    }
    return sourceCursor != activation.end;
  }

  int _replacementLength(
    String source,
    int start,
    int end,
    String replacement,
  ) => source.length - (end - start) + replacement.length;

  bool _acceptMutation(
    _TextMutation mutation, {
    required TextSelection selection,
    required TextRange composing,
    TextEditingValue? fullValue,
    bool typingInput = false,
  }) {
    final source = _inputValue.text;
    if (mutation.start < 0 ||
        mutation.end < mutation.start ||
        mutation.end > source.length) {
      return false;
    }
    if (_mutationTouchesOnlyHiddenProjection(mutation)) return false;
    final nextLength = _replacementLength(
      source,
      mutation.start,
      mutation.end,
      mutation.replacement,
    );
    if (!selection.isValid ||
        selection.baseOffset > nextLength ||
        selection.extentOffset > nextLength) {
      return false;
    }
    final inputStart = _inputGlobalUtf16Start;
    final beforeSelection = _selectionSnapshot();
    final wasCrossRowSelection = _crossRowSelection;
    final globalStart = inputStart + mutation.start;
    final globalEnd = inputStart + mutation.end;
    final preferredOrdinal = _surfaceOrdinalAt(globalStart);
    final compositionHistoryGroup = _compositionGroupForMutation(composing);

    if (nextLength <= _maximumInputCodeUnits) {
      _inputValue =
          fullValue ??
          TextEditingValue(
            text: source.replaceRange(
              mutation.start,
              mutation.end,
              mutation.replacement,
            ),
            selection: selection,
            composing: composing,
          );
    } else {
      final window = _boundedReplacementWindow(
        source,
        mutation.start,
        mutation.end,
        mutation.replacement,
        selection.extentOffset,
      );
      final windowEnd = window.start + window.text.length;
      final localBase = (selection.baseOffset - window.start).clamp(
        0,
        window.text.length,
      );
      final localExtent = (selection.extentOffset - window.start).clamp(
        0,
        window.text.length,
      );
      final localComposing =
          composing.isValid &&
              composing.start >= window.start &&
              composing.end <= windowEnd
          ? TextRange(
              start: composing.start - window.start,
              end: composing.end - window.start,
            )
          : TextRange.empty;
      _inputGlobalUtf16Start = inputStart + window.start;
      _inputValue = TextEditingValue(
        text: window.text,
        selection: TextSelection(
          baseOffset: localBase,
          extentOffset: localExtent,
          affinity: selection.affinity,
          isDirectional: selection.isDirectional,
        ),
        composing: localComposing,
      );
    }
    _globalSelectionBase = inputStart + selection.baseOffset;
    _globalSelectionExtent = inputStart + selection.extentOffset;
    _activeOrdinal = preferredOrdinal;
    _crossRowSelection = false;
    final afterSelection = _selectionSnapshot();
    final coalesceTyping =
        typingInput && compositionHistoryGroup == null && !composing.isValid;
    if (!coalesceTyping) _breakTypingHistoryGroup();
    _queueNativeEdit(
      globalStart,
      globalEnd,
      mutation.replacement,
      beforeSelection: beforeSelection,
      afterSelection: afterSelection,
      coalesceTyping: coalesceTyping,
      compositionHistoryGroup: compositionHistoryGroup,
      recenterAfterOptimisticEdit: wasCrossRowSelection,
    );
    return true;
  }

  FlarkCoreSelectionSnapshot _coreSnapshot(_EditorSelectionSnapshot snapshot) =>
      FlarkCoreSelectionSnapshot(
        base: snapshot.selection.baseOffset,
        extent: snapshot.selection.extentOffset,
        affinity: snapshot.selection.affinity == TextAffinity.upstream
            ? FlarkCoreAffinity.upstream
            : FlarkCoreAffinity.downstream,
        adapterState: snapshot,
      );

  Future<int> _installCanonicalSelection(_EditorSelectionSnapshot snapshot) {
    final core = _coreSnapshot(snapshot);
    final operation = _editTail.then(
      (_) => _session.setSelectionUtf16(
        core.base,
        core.extent,
        affinity: core.affinity,
        adapterState: snapshot,
      ),
    );
    _editTail = operation
        .then<void>((_) {})
        .catchError((Object _, StackTrace _) {});
    unawaited(
      operation
          .then((_) {
            if (!_closed) notifyListeners();
          })
          .catchError((Object error, StackTrace stackTrace) {
            _lastError = error;
            _status = FlarkEditorStatus.faulted;
            notifyListeners();
          }),
    );
    return operation;
  }

  _EditorSelectionSnapshot _adapterSnapshot(
    FlarkCoreSelectionSnapshot snapshot,
  ) => switch (snapshot.adapterState) {
    final _EditorSelectionSnapshot adapter => adapter,
    _ => _EditorSelectionSnapshot(
      TextSelection(baseOffset: snapshot.base, extentOffset: snapshot.extent),
      null,
    ),
  };

  void _breakTypingHistoryGroup() => _session.breakTypingGroup();

  int? _compositionGroupForMutation(TextRange composing) =>
      _session.compositionGroupForMutation(composingActive: composing.isValid);

  void _trackCompositionWithoutMutation(TextRange composing) {
    final compositionEnded = _session.trackCompositionWithoutMutation(
      composingActive: composing.isValid,
    );
    if (compositionEnded) _scheduleParsingAfterInput();
  }

  void _endCompositionHistoryGroup() => _session.endCompositionGroup();

  ({int start, String text}) _boundedReplacementWindow(
    String source,
    int start,
    int end,
    String replacement,
    int focus,
  ) {
    final nextLength = _replacementLength(source, start, end, replacement);
    final windowLength = math.min(nextLength, _maximumInputCodeUnits);
    final windowStart = (focus - windowLength ~/ 2).clamp(
      0,
      nextLength - windowLength,
    );
    final windowEnd = windowStart + windowLength;
    final replacementEnd = start + replacement.length;
    final output = StringBuffer();

    void appendIntersection(
      String segment,
      int segmentStart,
      int sourceStart,
      int sourceEnd,
    ) {
      final segmentEnd = segmentStart + sourceEnd - sourceStart;
      final overlapStart = math.max(windowStart, segmentStart);
      final overlapEnd = math.min(windowEnd, segmentEnd);
      if (overlapStart >= overlapEnd) return;
      output.write(
        segment.substring(
          sourceStart + overlapStart - segmentStart,
          sourceStart + overlapEnd - segmentStart,
        ),
      );
    }

    appendIntersection(source, 0, 0, start);
    appendIntersection(replacement, start, 0, replacement.length);
    appendIntersection(source, replacementEnd, end, source.length);
    return (start: windowStart, text: output.toString());
  }

  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    _parseTimer?.cancel();
    _parseTimer = null;
    await _editTail;
    await _session.dispose();
    await _document.dispose();
    _status = FlarkEditorStatus.disposed;
  }

  Future<bool> _loadNextViewportPage() async {
    final current = _viewport;
    if (current == null || current.continuation == 0) return false;
    try {
      final next = await _document.queryViewportNext(
        current,
        maxRows: _viewportRowsPerPage,
      );
      final source = await _readViewportSource(next);
      final nextIndex = _pageIndex + 1;
      if (_pageStarts.length > nextIndex) {
        _pageStarts
          ..removeRange(nextIndex, _pageStarts.length)
          ..add(next.coveredBytes.start);
      } else {
        _pageStarts.add(next.coveredBytes.start);
      }
      _pageIndex = nextIndex;
      _installViewport(next, source, restoreInputWindow: false);
      return true;
    } catch (error) {
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
      return false;
    }
  }

  Future<bool> _loadPreviousViewportPage() async {
    final current = _viewport;
    final previousIndex = _pageIndex - 1;
    if (current == null || previousIndex < 0) return false;
    try {
      await _document.releaseViewportContinuation(current);
      final pageStart = _pageStarts[previousIndex];
      final previous = await _document.queryViewport(
        startByte: pageStart,
        endByte: math.min(sourceByteLength, pageStart + _maximumVisibleBytes),
        maxRows: _viewportRowsPerPage,
      );
      final source = await _readViewportSource(previous);
      _pageIndex = previousIndex;
      _installViewport(previous, source, restoreInputWindow: false);
      return true;
    } catch (error) {
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
      return false;
    }
  }

  void _activateWindow({
    required String text,
    required int sourceStart,
    required int caret,
    required int ordinal,
    required TextAffinity affinity,
  }) {
    final localCaret = (caret - sourceStart).clamp(0, text.length);
    var windowStart = 0;
    var windowEnd = text.length;
    if (text.length > _maximumInputCodeUnits) {
      windowStart = (localCaret - _maximumInputCodeUnits ~/ 2).clamp(
        0,
        text.length - _maximumInputCodeUnits,
      );
      windowEnd = windowStart + _maximumInputCodeUnits;
    }
    final window = text.substring(windowStart, windowEnd);
    final windowCaret = localCaret - windowStart;
    _inputGlobalUtf16Start = sourceStart + windowStart;
    _inputValue = TextEditingValue(
      text: window,
      selection: TextSelection.collapsed(
        offset: windowCaret,
        affinity: affinity,
      ),
    );
    _activeOrdinal = ordinal;
    _crossRowSelection = false;
    _globalSelectionBase = _inputGlobalUtf16Start + windowCaret;
    _globalSelectionExtent = _globalSelectionBase;
    notifyListeners();
  }

  void _updateGlobalSelection() {
    _globalSelectionBase =
        _inputGlobalUtf16Start + _inputValue.selection.baseOffset;
    _globalSelectionExtent =
        _inputGlobalUtf16Start + _inputValue.selection.extentOffset;
  }

  _EditorSelectionSnapshot _selectionSnapshot() => _EditorSelectionSnapshot(
    TextSelection(
      baseOffset: _globalSelectionBase,
      extentOffset: _globalSelectionExtent,
      affinity: _inputValue.selection.affinity,
      isDirectional: _inputValue.selection.isDirectional,
    ),
    _activeOrdinal,
  );

  bool _selectionIntersects(FlarkSourceRange range) {
    final start = math.min(_globalSelectionBase, _globalSelectionExtent);
    final end = math.max(_globalSelectionBase, _globalSelectionExtent);
    if (start == end) return range.start <= start && start <= range.end;
    return start < range.end && range.start < end;
  }

  TextSelection _selectionForRange(FlarkSourceRange range) => TextSelection(
    baseOffset: (_globalSelectionBase - range.start).clamp(0, range.length),
    extentOffset: (_globalSelectionExtent - range.start).clamp(0, range.length),
    affinity: _inputValue.selection.affinity,
    isDirectional: _inputValue.selection.isDirectional,
  );

  FlarkSourceRange _mappedExactRowRange(FlarkViewportRow row) {
    final source = _mapViewportRange(row.sourceUtf16);
    final prefix = row.listItem?.prefixUtf16 ?? row.blockQuote?.prefixUtf16;
    if (prefix == null) return source;
    final mappedPrefix = _mapViewportRange(prefix);
    return FlarkSourceRange(mappedPrefix.start, source.end);
  }

  int? _surfaceOrdinalAt(int globalUtf16Offset) {
    for (final row in _cachedRows) {
      final range = surfaceSourceRange(row);
      if (range.start <= globalUtf16Offset && globalUtf16Offset < range.end) {
        return row.ordinal;
      }
    }
    if (_visibleSource.isEmpty) return -1;
    final local = (globalUtf16Offset - _visibleUtf16Start).clamp(
      0,
      _visibleSource.length,
    );
    var line = 0;
    for (var index = 0; index < local; index++) {
      if (_visibleSource.codeUnitAt(index) == 0x0a) line += 1;
    }
    return -line - 1;
  }

  bool _ensureActiveInputVisible() {
    final caret = _globalSelectionExtent;
    final visibleEnd = _visibleUtf16Start + _visibleSource.length;
    if (_visibleUtf16Start <= caret && caret <= visibleEnd) return false;
    final inputEnd = _inputGlobalUtf16Start + _inputValue.text.length;
    if (caret < _inputGlobalUtf16Start || caret > inputEnd) return false;
    _cachedRows = const [];
    _certificationRanges = const [];
    _certificationRevisionCurrent = false;
    _semanticViewportCurrent = false;
    _visibleSource = _inputValue.text;
    _visibleUtf16Start = _inputGlobalUtf16Start;
    _optimisticViewportEdits.clear();
    _status = _document.isReady
        ? FlarkEditorStatus.ready
        : FlarkEditorStatus.parsing;
    return true;
  }

  Future<void> _restoreHistorySelection(
    _EditorSelectionSnapshot snapshot,
  ) async {
    final selection = snapshot.selection;
    final selectionStart = math
        .min(selection.baseOffset, selection.extentOffset)
        .clamp(0, sourceUtf16Length)
        .toInt();
    final selectionEnd = math
        .max(selection.baseOffset, selection.extentOffset)
        .clamp(selectionStart, sourceUtf16Length)
        .toInt();
    final visibleEnd = _visibleUtf16Start + _visibleSource.length;
    if (selectionStart < _visibleUtf16Start || selectionEnd > visibleEnd) {
      final selectionLength = selectionEnd - selectionStart;
      var windowStart = (selection.extentOffset - _maximumInputCodeUnits ~/ 2)
          .clamp(0, math.max(0, sourceUtf16Length - _maximumInputCodeUnits))
          .toInt();
      if (selectionLength <= _maximumInputCodeUnits) {
        windowStart = math.min(windowStart, selectionStart);
        windowStart = math.max(
          windowStart,
          selectionEnd - _maximumInputCodeUnits,
        );
      }
      final windowEnd = math.min(
        sourceUtf16Length,
        windowStart + _maximumInputCodeUnits,
      );
      _visibleSource = await _document.readSourceUtf16Range(
        windowStart,
        windowEnd,
      );
      _visibleUtf16Start = windowStart;
      _cachedRows = const [];
      _certificationRanges = const [];
      _certificationRevisionCurrent = false;
      _semanticViewportCurrent = false;
      _optimisticViewportEdits.clear();
      _status = _document.isReady
          ? FlarkEditorStatus.ready
          : FlarkEditorStatus.parsing;
    }
    _restoreSelectionSnapshot(snapshot);
  }

  void _restoreSelectionSnapshot(_EditorSelectionSnapshot snapshot) {
    final selection = snapshot.selection;
    final start = math.min(selection.baseOffset, selection.extentOffset);
    final end = math.max(selection.baseOffset, selection.extentOffset);
    final visibleEnd = _visibleUtf16Start + _visibleSource.length;
    if (!selection.isCollapsed &&
        start >= _visibleUtf16Start &&
        end <= visibleEnd &&
        end - start <= _maximumInputCodeUnits) {
      FlarkViewportRow? containingRow;
      for (final row in _cachedRows) {
        final range = _mapViewportRange(_activationRange(row));
        if (range.start <= start && end <= range.end) {
          containingRow = row;
          break;
        }
      }
      if (containingRow != null) {
        final range = _mapViewportRange(_activationRange(containingRow));
        _inputGlobalUtf16Start = range.start;
        _inputValue = TextEditingValue(
          text: _sliceVisibleUtf16(range.start, range.end),
          selection: TextSelection(
            baseOffset: selection.baseOffset - range.start,
            extentOffset: selection.extentOffset - range.start,
            affinity: selection.affinity,
            isDirectional: selection.isDirectional,
          ),
        );
        _globalSelectionBase = selection.baseOffset;
        _globalSelectionExtent = selection.extentOffset;
        _activeOrdinal = containingRow.ordinal;
        _crossRowSelection = false;
        return;
      }
      _inputGlobalUtf16Start = start;
      _inputValue = TextEditingValue(
        text: _sliceVisibleUtf16(start, end),
        selection: TextSelection(
          baseOffset: selection.baseOffset - start,
          extentOffset: selection.extentOffset - start,
          affinity: selection.affinity,
          isDirectional: selection.isDirectional,
        ),
      );
      _globalSelectionBase = selection.baseOffset;
      _globalSelectionExtent = selection.extentOffset;
      _activeOrdinal =
          snapshot.activeOrdinal ?? _surfaceOrdinalAt(selection.extentOffset);
      _crossRowSelection = true;
      return;
    }
    final caret = selection.extentOffset
        .clamp(0, math.max(sourceUtf16Length, visibleEnd))
        .toInt();
    _globalSelectionBase = caret;
    _globalSelectionExtent = caret;
    _restoreCollapsedInputWindow(
      caret,
      preferredOrdinal: snapshot.activeOrdinal,
    );
  }

  void _restoreCollapsedInputWindow(int caret, {int? preferredOrdinal}) {
    FlarkViewportRow? row;
    if (preferredOrdinal != null) {
      for (final candidate in _cachedRows) {
        if (candidate.ordinal == preferredOrdinal) {
          row = candidate;
          break;
        }
      }
    }
    final ordinalAtCaret = _surfaceOrdinalAt(caret);
    if (row == null && ordinalAtCaret != null) {
      for (final candidate in _cachedRows) {
        if (candidate.ordinal == ordinalAtCaret) {
          row = candidate;
          break;
        }
      }
    }
    if (row != null) {
      final range = _mapViewportRange(_activationRange(row));
      final visibleEnd = _visibleUtf16Start + _visibleSource.length;
      if (range.start >= _visibleUtf16Start && range.end <= visibleEnd) {
        _activateWindowWithoutNotification(
          text: _sliceVisibleUtf16(range.start, range.end),
          sourceStart: range.start,
          caret: caret,
          ordinal: row.ordinal,
        );
        return;
      }
    }

    final localCaret = (caret - _visibleUtf16Start).clamp(
      0,
      _visibleSource.length,
    );
    final lineStart = localCaret == 0
        ? 0
        : _visibleSource.lastIndexOf('\n', localCaret - 1) + 1;
    final newline = _visibleSource.indexOf('\n', localCaret);
    final lineEnd = newline == -1 ? _visibleSource.length : newline + 1;
    _activateWindowWithoutNotification(
      text: _visibleSource.substring(lineStart, lineEnd),
      sourceStart: _visibleUtf16Start + lineStart,
      caret: caret,
      ordinal: ordinalAtCaret ?? -1,
    );
  }

  _TextMutation? _mutationFor(TextEditingDelta delta) {
    return switch (delta) {
      TextEditingDeltaInsertion insertion => _TextMutation(
        insertion.insertionOffset,
        insertion.insertionOffset,
        insertion.textInserted,
      ),
      TextEditingDeltaDeletion deletion => _TextMutation(
        deletion.deletedRange.start,
        deletion.deletedRange.end,
        '',
      ),
      TextEditingDeltaReplacement replacement => _TextMutation(
        replacement.replacedRange.start,
        replacement.replacedRange.end,
        replacement.replacementText,
      ),
      TextEditingDeltaNonTextUpdate() => null,
      _ => null,
    };
  }

  void _queueNativeEdit(
    int start,
    int end,
    String replacement, {
    required _EditorSelectionSnapshot beforeSelection,
    required _EditorSelectionSnapshot afterSelection,
    required bool coalesceTyping,
    required int? compositionHistoryGroup,
    bool recenterAfterOptimisticEdit = false,
  }) {
    final split = _committedParagraphSplit;
    if (split != null &&
        (start < split.rowEndUtf16 || end > _committedGapEnd(split))) {
      _committedParagraphSplit = null;
    }
    final structurals = _committedStructuralSurfaces;
    if (structurals.isNotEmpty) {
      final mapped = mapCommittedPresentationSurfacesThroughLiteralSpliceV1(
        surfaces: structurals,
        startUtf16: start,
        endUtf16: end,
        replacement: replacement,
      );
      if (mapped != null) {
        _committedStructuralSurfaces = mapped;
        _projectionContinuity = null;
      } else {
        _prepareProjectionContinuity(start, end, replacement);
        _committedStructuralSurfaces = const [];
      }
    } else {
      _prepareProjectionContinuity(start, end, replacement);
    }
    _applyOptimisticViewportEdit(start, end, replacement);
    if (recenterAfterOptimisticEdit) {
      _restoreSelectionSnapshot(afterSelection);
    }
    _parseTimer?.cancel();
    _parseTimer = null;
    final generation = ++_editGeneration;
    _pendingEdits += 1;
    _status = FlarkEditorStatus.editing;
    final operation = _editTail.then((_) async {
      await _session.applyEditUtf16(
        start,
        end,
        replacement,
        beforeSelection: _coreSnapshot(beforeSelection),
        afterSelection: _coreSnapshot(afterSelection),
        coalesceTyping: coalesceTyping,
        compositionGroup: compositionHistoryGroup,
      );
    });
    _editTail = operation.catchError((Object _, StackTrace _) {});
    unawaited(_completeQueuedEdit(operation, generation));
  }

  bool _queueSemanticParagraphBreak(int localCaret) {
    if (!_inputValue.selection.isCollapsed) return false;
    final row = _activeCachedRow();
    final editableRange = row?.editableUtf16;
    final rowEligible = row != null && _supportsSemanticParagraphBreakV1(row);
    if (!rowEligible && !_semanticEditV1Active) return false;
    if (rowEligible) {
      _semanticEditV1Active = true;
      final editable = _mapViewportRange(editableRange!);
      final globalCaret = _inputGlobalUtf16Start + localCaret;
      if (globalCaret < editable.start || globalCaret > editable.end) {
        // A retained certified row can border the exact neutral island
        // created by the last semantic receipt. While recertification is
        // pending, the lane remains authoritative and Rust reclassifies the
        // current source at the anchor; the stale row range is not a gate.
        if (_semanticViewportCurrent) return false;
      }
    }
    _queueSemanticEdit(FlarkCoreEditIntentV1.insertParagraphBreak);
    return true;
  }

  bool _queueSemanticDeleteBackward(int localCaret) {
    if (!_inputValue.selection.isCollapsed) return false;
    final row = _activeCachedRow();
    final editableRange = row?.editableUtf16;
    final projectedBlockQuote = row != null && _isProjectedBlockQuote(row);
    final rowEligible =
        row != null &&
        (_supportsSemanticDeleteBackwardV1(row) || projectedBlockQuote);
    if (!rowEligible && (!_semanticEditV1Active || localCaret != 0)) {
      return false;
    }
    if (rowEligible) {
      _semanticEditV1Active = true;
      final editable = _mapViewportRange(editableRange!);
      final globalCaret = _inputGlobalUtf16Start + localCaret;
      final atProjectedSegmentStart =
          projectedBlockQuote &&
          row.projectionSegments!.any(
            (segment) =>
                _mapViewportRange(segment.sourceUtf16).start == globalCaret,
          );
      if (!atProjectedSegmentStart &&
          globalCaret != editable.start &&
          (_semanticViewportCurrent || localCaret != 0)) {
        return false;
      }
    }
    _queueSemanticEdit(FlarkCoreEditIntentV1.deleteBackward);
    return true;
  }

  bool _supportsSemanticEditV1(FlarkViewportRow row) {
    return _supportsSemanticParagraphBreakV1(row) ||
        _supportsSemanticDeleteBackwardV1(row);
  }

  bool _supportsSemanticParagraphBreakV1(FlarkViewportRow row) {
    return _supportsSemanticDeleteBackwardV1(row) ||
        _isProjectedBlockQuote(row);
  }

  bool _supportsSemanticDeleteBackwardV1(FlarkViewportRow row) {
    if (row.editableUtf16 == null) return false;
    final listItem = row.listItem;
    final simpleList = listItem != null && listItem.simpleContinuation;
    final simpleBlockQuote = row.blockQuote?.simpleContinuation ?? false;
    final atxHeading =
        row.kind == 12 && row.headingStyle == FlarkHeadingStyle.atx;
    final plainParagraph =
        row.kind == 5 &&
        listItem == null &&
        row.blockQuote == null &&
        row.headingLevel == null &&
        row.codeBlock == null &&
        !row.thematicBreak &&
        row.table == null;
    return plainParagraph || simpleList || simpleBlockQuote || atxHeading;
  }

  bool _isProjectedBlockQuote(FlarkViewportRow row) =>
      row.blockQuote?.nestingDepth == 1 &&
      row.editCapability == FlarkViewportRowEditCapability.projectedReserved &&
      row.projectionSegments != null;

  void _queueSemanticEdit(FlarkCoreEditIntentV1 intent) {
    _ensureSemanticInputBarrier();
    _projectionContinuity = null;
    _breakTypingHistoryGroup();
    _parseTimer?.cancel();
    _parseTimer = null;
    final generation = ++_editGeneration;
    _pendingEdits += 1;
    _status = FlarkEditorStatus.editing;
    final operation = _editTail.then(
      (_) => _session.applyEditIntentV1(
        intent,
        compositionActive: _session.compositionActive,
      ),
    );
    final completion = _completeSemanticEdit(operation, generation);
    _editTail = completion.catchError((Object _, StackTrace _) {});
    unawaited(completion);
    notifyListeners();
  }

  void _ensureSemanticInputBarrier() {
    if (_pendingSemanticInput != null) return;
    _lateSemanticInput = null;
    _pendingSemanticInput = _PendingSemanticInput(
      base: _inputValue,
      inputGlobalUtf16Start: _inputGlobalUtf16Start,
      initialCallbackStartedEpochMicros:
          _activePlatformCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      provisionalAfter: _inputValue,
    );
  }

  Future<void> _completeSemanticEdit(
    Future<FlarkCoreEditIntentReceiptV1> operation,
    int generation,
  ) async {
    try {
      final receipt = await operation;
      final observedInput = _pendingSemanticInput;
      final adoptionWatch = Stopwatch()..start();
      if (receipt.hasCommit) {
        _adoptSemanticReceipt(receipt);
        _promoteSemanticSuccessors(receipt);
      } else {
        _semanticEditV1Active = false;
        final pending = _pendingSemanticInput;
        if (pending?.provisionalMutation != null) {
          _pendingSemanticInput = null;
          _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
        } else if (pending != null) {
          _promoteUncommittedSemanticSuccessors();
        }
      }
      adoptionWatch.stop();
      if (observedInput != null && observedInput.provisionalMutation != null) {
        final telemetry = receipt.telemetry;
        _lastSemanticEditPerformance = FlarkSemanticEditPerformance(
          platformCallbackMicros: observedInput.initialCallbackMicros,
          coreQueueMicros: telemetry.coreQueueMicros,
          workerRoundTripMicros: telemetry.workerRoundTripMicros,
          workerQueueMicros: telemetry.workerQueueMicros,
          nativeFfiMicros: telemetry.nativeFfiMicros,
          coreAdoptionMicros: telemetry.coreAdoptionMicros,
          flutterReceiptAdoptionMicros: adoptionWatch.elapsedMicroseconds,
          callbackToReceiptMicros: math.max(
            0,
            DateTime.now().microsecondsSinceEpoch -
                observedInput.initialCallbackStartedEpochMicros,
          ),
        );
      }
      if (generation != _editGeneration || !receipt.hasCommit) {
        _status = _semanticViewportCurrent
            ? FlarkEditorStatus.ready
            : FlarkEditorStatus.parsing;
        _pendingEdits = math.max(0, _pendingEdits - 1);
        notifyListeners();
        return;
      }
      await _refreshViewport(
        restoreInputWindow: false,
        expectedEditGeneration: generation,
        ensureActiveInputVisible: true,
      );
      if (generation == _editGeneration) _scheduleParsingAfterInput();
      _pendingEdits = math.max(0, _pendingEdits - 1);
      notifyListeners();
    } catch (error) {
      _projectionContinuity = null;
      _pendingSemanticInput = null;
      _pendingEdits = math.max(0, _pendingEdits - 1);
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
    }
  }

  void _adoptSemanticReceipt(FlarkCoreEditIntentReceiptV1 receipt) {
    final transition = _prepareCommittedPresentationTransition(receipt);
    if (transition?.clearPriorGap ?? false) {
      _committedParagraphSplit = null;
    }
    _applyOptimisticViewportEdit(
      receipt.baseUtf16Start,
      receipt.baseUtf16End,
      receipt.replacement,
    );
    _committedStructuralSurfaces = transition?.surfaces ?? const [];
    final removedRowOrdinals = _committedStructuralSurfaces
        .map((surface) => surface.removedRowOrdinal)
        .whereType<int>()
        .toSet();
    if (removedRowOrdinals.isNotEmpty) {
      _cachedRows = List.unmodifiable(
        _cachedRows.where((row) => !removedRowOrdinals.contains(row.ordinal)),
      );
    }
    _committedParagraphSplit = transition?.gap ?? _committedParagraphSplit;
    final caret = receipt.resultSelectionUtf16;
    _globalSelectionBase = caret;
    _globalSelectionExtent = caret;
    _crossRowSelection = false;
    _oversizedSelection = false;
    _activeOrdinal = _surfaceOrdinalAt(caret);
    if (!_installCommittedSemanticInputWindow(receipt, caret)) {
      _restoreCollapsedInputWindow(caret, preferredOrdinal: _activeOrdinal);
    }
  }

  FlarkCoreCommittedPresentationTransitionV1?
  _prepareCommittedPresentationTransition(
    FlarkCoreEditIntentReceiptV1 receipt,
  ) {
    final activeOrdinal = _activeOrdinal;
    final activeIndex = activeOrdinal == null
        ? -1
        : _cachedRows.indexWhere((row) => row.ordinal == activeOrdinal);

    FlarkCorePresentationRow? coreRowAt(int index) {
      if (index < 0 || index >= _cachedRows.length) return null;
      final row = _cachedRows[index];
      return _corePresentationRow(
        surfaceRow(row, includeEditingState: false),
        surfaceSourceRange(row),
      );
    }

    return resolveCommittedPresentationTransitionV1(
      receipt: receipt,
      priorActiveOrdinal: activeOrdinal,
      activeRow: coreRowAt(activeIndex),
      precedingRow: coreRowAt(activeIndex - 1),
      priorGapPending: _committedParagraphSplit != null,
    );
  }

  FlarkCorePresentationRow _corePresentationRow(
    FlarkSurfaceRow row,
    FlarkSourceRange sourceUtf16,
  ) => FlarkCorePresentationRow(
    sourceUtf16: sourceUtf16,
    leadingText: row.leadingText,
    text: row.text,
    globalUtf16Start: row.globalUtf16Start,
    kind: row.kind,
    headingLevel: row.headingLevel,
    blockQuoteDepth: row.blockQuoteDepth,
    codeBlock: row.codeBlock,
    thematicBreak: row.thematicBreak,
    ordinal: row.ordinal,
    runs: List.unmodifiable(
      row.runs.map(
        (run) => FlarkCorePresentationRun(
          text: run.text,
          sourceUtf16Start: run.sourceUtf16Start,
          sourceUtf16End: run.sourceUtf16End,
          sourceExact: run.sourceExact,
          styles: Set.unmodifiable(run.styles.map(_coreStyleFromSurface)),
        ),
      ),
    ),
  );

  FlarkSurfaceTextRun _surfaceRunFromCore(FlarkCorePresentationRun run) =>
      FlarkSurfaceTextRun(
        text: run.text,
        sourceUtf16Start: run.sourceUtf16Start,
        sourceUtf16End: run.sourceUtf16End,
        sourceExact: run.sourceExact,
        styles: Set.unmodifiable(run.styles.map(_surfaceStyleFromCore)),
      );

  static FlarkCorePresentationInlineStyle _coreStyleFromSurface(
    FlarkSurfaceInlineStyle style,
  ) => FlarkCorePresentationInlineStyle.values[style.index];

  static FlarkSurfaceInlineStyle _surfaceStyleFromCore(
    FlarkCorePresentationInlineStyle style,
  ) => FlarkSurfaceInlineStyle.values[style.index];

  bool _installCommittedSemanticInputWindow(
    FlarkCoreEditIntentReceiptV1 receipt,
    int caret,
  ) {
    final pending = _pendingSemanticInput;
    if (pending == null) return false;
    final windowStart = pending.inputGlobalUtf16Start;
    final windowEnd = windowStart + pending.base.text.length;
    final delta =
        receipt.replacement.length -
        (receipt.baseUtf16End - receipt.baseUtf16Start);
    if (receipt.baseUtf16End <= windowStart ||
        receipt.baseUtf16Start >= windowEnd) {
      final resultWindowStart = receipt.baseUtf16End <= windowStart
          ? windowStart + delta
          : windowStart;
      final localCaret = caret - resultWindowStart;
      if (localCaret < 0 || localCaret > pending.base.text.length) {
        return false;
      }
      _inputGlobalUtf16Start = resultWindowStart;
      _inputValue = pending.base.copyWith(
        selection: TextSelection.collapsed(offset: localCaret),
        composing: TextRange.empty,
      );
      return true;
    }
    final localStart = receipt.baseUtf16Start - pending.inputGlobalUtf16Start;
    final localEnd = receipt.baseUtf16End - pending.inputGlobalUtf16Start;
    if (localStart < 0 ||
        localEnd < localStart ||
        localEnd > pending.base.text.length) {
      return false;
    }
    final text = pending.base.text.replaceRange(
      localStart,
      localEnd,
      receipt.replacement,
    );
    final localCaret = caret - pending.inputGlobalUtf16Start;
    if (text.length > _maximumInputCodeUnits ||
        localCaret < 0 ||
        localCaret > text.length) {
      return false;
    }
    _inputGlobalUtf16Start = pending.inputGlobalUtf16Start;
    _inputValue = TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(offset: localCaret),
    );
    return true;
  }

  void _promoteSemanticSuccessors(FlarkCoreEditIntentReceiptV1 receipt) {
    final stopwatch = Stopwatch()..start();
    final pending = _pendingSemanticInput;
    _pendingSemanticInput = null;
    try {
      if (pending == null) return;
      final reconciliation = _InputReconciliationMap.forSemanticBarrier(
        pending: pending,
        receipt: receipt,
      );
      if (reconciliation == null) {
        _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
        return;
      }
      final resyncCount = _resyncCount;
      _promoteSemanticSuccessorsWithMap(pending, reconciliation);
      if (pending.provisionalMutation != null && _resyncCount == resyncCount) {
        _lateSemanticInput = _LateSemanticInput(
          provisionalTail: pending.provisionalTail,
          reconciliation: reconciliation,
          successorCount: pending.successors.length,
        );
      }
    } finally {
      stopwatch.stop();
      _lastSemanticReconciliationMicros = stopwatch.elapsedMicroseconds;
    }
  }

  void _promoteUncommittedSemanticSuccessors() {
    final stopwatch = Stopwatch()..start();
    final pending = _pendingSemanticInput;
    _pendingSemanticInput = null;
    _lateSemanticInput = null;
    try {
      if (pending == null) return;
      _promoteSemanticSuccessorsWithMap(
        pending,
        const _InputReconciliationMap(
          fromStart: 0,
          fromEnd: 0,
          toStart: 0,
          toEnd: 0,
        ),
      );
    } finally {
      stopwatch.stop();
      _lastSemanticReconciliationMicros = stopwatch.elapsedMicroseconds;
    }
  }

  void _promoteSemanticSuccessorsWithMap(
    _PendingSemanticInput pending,
    _InputReconciliationMap reconciliation,
  ) {
    for (final successor in pending.successors) {
      if (successor case _DeferredInputSuccessor(
        command: final command,
        replacement: final replacement,
      )) {
        if (replacement != null) {
          replaceSelection(replacement);
        } else {
          switch (command!) {
            case _DeferredInputCommand.deleteBackward:
              _queueSemanticEdit(FlarkCoreEditIntentV1.deleteBackward);
              break;
            case _DeferredInputCommand.deleteForward:
              deleteForward();
              break;
            case _DeferredInputCommand.insertNewline:
              _queueSemanticEdit(FlarkCoreEditIntentV1.insertParagraphBreak);
              break;
          }
        }
        continue;
      }
      final batch = successor as _ProvisionalInputBatch;
      final selection = _mapProvisionalSelection(
        reconciliation,
        batch.after.selection,
      );
      final composing = _mapProvisionalRange(
        reconciliation,
        batch.after.composing,
      );
      if (selection == null || composing == null) {
        _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
        return;
      }
      final mutation = _differenceMutation(batch.before.text, batch.after.text);
      if (mutation == null) {
        _breakTypingHistoryGroup();
        _inputValue = _inputValue.copyWith(
          selection: selection,
          composing: composing,
        );
        _trackCompositionWithoutMutation(composing);
        _updateGlobalSelection();
        unawaited(_installCanonicalSelection(_selectionSnapshot()));
        continue;
      }
      final mappedStart = reconciliation.mapOffset(
        mutation.start,
        downstream: true,
      );
      final mappedEnd = reconciliation.mapOffset(
        mutation.end,
        downstream: true,
      );
      if (mappedStart == null ||
          mappedEnd == null ||
          !_acceptMutation(
            _TextMutation(mappedStart, mappedEnd, mutation.replacement),
            selection: selection,
            composing: composing,
            typingInput: batch.typingInput,
          )) {
        _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
        return;
      }
    }
  }

  TextSelection? _mapProvisionalSelection(
    _InputReconciliationMap map,
    TextSelection selection,
  ) {
    final downstream = selection.affinity == TextAffinity.downstream;
    final base = map.mapOffset(selection.baseOffset, downstream: downstream);
    final extent = map.mapOffset(
      selection.extentOffset,
      downstream: downstream,
    );
    if (base == null || extent == null) return null;
    return TextSelection(
      baseOffset: base,
      extentOffset: extent,
      affinity: selection.affinity,
      isDirectional: selection.isDirectional,
    );
  }

  TextRange? _mapProvisionalRange(
    _InputReconciliationMap map,
    TextRange range,
  ) {
    if (range == TextRange.empty) return TextRange.empty;
    final start = map.mapOffset(range.start, downstream: true);
    final end = map.mapOffset(range.end, downstream: true);
    if (start == null || end == null) return null;
    return TextRange(start: start, end: end);
  }

  _TextMutation? _differenceMutation(String before, String after) {
    if (before == after) return null;
    var prefix = 0;
    while (prefix < before.length &&
        prefix < after.length &&
        before.codeUnitAt(prefix) == after.codeUnitAt(prefix)) {
      prefix += 1;
    }
    var oldSuffix = before.length;
    var newSuffix = after.length;
    while (oldSuffix > prefix &&
        newSuffix > prefix &&
        before.codeUnitAt(oldSuffix - 1) == after.codeUnitAt(newSuffix - 1)) {
      oldSuffix -= 1;
      newSuffix -= 1;
    }
    return _TextMutation(prefix, oldSuffix, after.substring(prefix, newSuffix));
  }

  int _committedGapEnd(FlarkCoreCommittedPresentationGapV1 split) {
    var end = _visibleUtf16Start + _visibleSource.length;
    for (final row in _cachedRows) {
      if (row.ordinal == split.rowOrdinal) continue;
      final start = surfaceSourceRange(row).start;
      if (start >= split.rowEndUtf16) end = math.min(end, start);
    }
    return end;
  }

  void _prepareProjectionContinuity(int start, int end, String replacement) {
    final current = _projectionContinuity;
    if (current != null) {
      final receipt = current.receipt.continueWith(
        startUtf16: start,
        endUtf16: end,
        replacement: replacement,
      );
      final presentation = receipt == null
          ? null
          : _spliceContinuityPresentation(
              current.presentation,
              receipt.authorizedContentUtf16,
              start,
              end,
              replacement,
            );
      _projectionContinuity = receipt == null || presentation == null
          ? null
          : _ProjectionContinuitySurface(
              receipt: receipt,
              presentation: presentation,
            );
      return;
    }
    // A committed structural surface is already mapped to the current
    // revision by its authoritative receipt. It may therefore seed the next
    // conservative literal continuation even though the older viewport rows
    // still have an optimistic splice in front of them.
    if (_optimisticViewportEdits.isNotEmpty &&
        _committedStructuralSurfaces.isEmpty) {
      return;
    }
    final row = _activeCachedRow();
    final facts =
        row?.inlineFacts ??
        (row?.projectionSegments != null ? const <FlarkInlineFact>[] : null);
    if (row == null || facts == null) {
      return;
    }
    final activation = _mapViewportRange(_activationRange(row));
    if (!_rowSemanticsCurrent(activation)) {
      return;
    }
    final editable = _mapViewportRange(
      row.editableUtf16 ?? _activationRange(row),
    );
    final inlineReceipt = facts.isEmpty
        ? null
        : authorizeInlineProjectionContinuity(
            revision: revision,
            facts: facts,
            startUtf16: start,
            endUtf16: end,
            replacement: replacement,
            table: row.table,
          );
    final tableReceipt = inlineReceipt != null || row.table == null
        ? null
        : authorizeTableCellProjectionContinuity(
            revision: revision,
            table: row.table!,
            tableUtf16: activation,
            tableText: _sliceVisibleUtf16(activation.start, activation.end),
            inlineFacts: facts,
            startUtf16: start,
            endUtf16: end,
            replacement: replacement,
          );
    final receipt =
        inlineReceipt ??
        tableReceipt ??
        (row.table == null
            ? authorizeRowProjectionContinuity(
                revision: revision,
                policy: row.continuityPolicy,
                editableUtf16: editable,
                editableText: _sliceVisibleUtf16(editable.start, editable.end),
                inlineFacts: facts,
                startUtf16: start,
                endUtf16: end,
                replacement: replacement,
              )
            : null);
    if (receipt == null) {
      return;
    }
    final base = surfaceRow(row, includeEditingState: false);
    final presentation = _spliceContinuityPresentation(
      base,
      receipt.authorizedContentUtf16,
      start,
      end,
      replacement,
    );
    if (presentation == null) {
      return;
    }
    _projectionContinuity = _ProjectionContinuitySurface(
      receipt: receipt,
      presentation: presentation,
    );
  }

  FlarkSurfaceRow? _spliceContinuityPresentation(
    FlarkSurfaceRow presentation,
    FlarkSourceRange authorizedContent,
    int start,
    int end,
    String replacement,
  ) {
    final delta = replacement.length - (end - start);
    final baseAuthorizedContent = FlarkSourceRange(
      authorizedContent.start,
      authorizedContent.end - delta,
    );
    var target = -1;
    for (var index = 0; index < presentation.runs.length; index += 1) {
      final run = presentation.runs[index];
      final insertionInside =
          start == end &&
          start >= run.sourceUtf16Start &&
          start <= run.sourceUtf16End;
      final replacementInside =
          start < end &&
          start >= run.sourceUtf16Start &&
          end <= run.sourceUtf16End;
      final runInsideAuthority =
          baseAuthorizedContent.start <= run.sourceUtf16Start &&
          run.sourceUtf16End <= baseAuthorizedContent.end;
      if (run.sourceExact &&
          runInsideAuthority &&
          (insertionInside || replacementInside)) {
        target = index;
        break;
      }
    }
    if (target < 0) return null;

    final runs = <FlarkSurfaceTextRun>[];
    for (var index = 0; index < presentation.runs.length; index += 1) {
      final run = presentation.runs[index];
      if (index < target) {
        runs.add(run);
        continue;
      }
      if (index == target) {
        final localStart = start - run.sourceUtf16Start;
        final localEnd = end - run.sourceUtf16Start;
        runs.add(
          FlarkSurfaceTextRun(
            text: run.text.replaceRange(localStart, localEnd, replacement),
            sourceUtf16Start: run.sourceUtf16Start,
            sourceUtf16End: run.sourceUtf16End + delta,
            sourceExact: true,
            styles: run.styles,
          ),
        );
        continue;
      }
      runs.add(
        FlarkSurfaceTextRun(
          text: run.text,
          sourceUtf16Start: run.sourceUtf16Start + delta,
          sourceUtf16End: run.sourceUtf16End + delta,
          sourceExact: run.sourceExact,
          styles: run.styles,
        ),
      );
    }
    final projected = List<FlarkSurfaceTextRun>.unmodifiable(runs);
    return FlarkSurfaceRow(
      leadingText: presentation.leadingText,
      text: projected.map((run) => run.text).join(),
      globalUtf16Start: presentation.globalUtf16Start,
      kind: presentation.kind,
      headingLevel: presentation.headingLevel,
      blockQuoteDepth: presentation.blockQuoteDepth,
      codeBlock: presentation.codeBlock,
      thematicBreak: presentation.thematicBreak,
      ordinal: presentation.ordinal,
      active: false,
      selection: null,
      runs: projected,
    );
  }

  Future<void> _completeQueuedEdit(
    Future<void> operation,
    int generation,
  ) async {
    try {
      await operation;
      if (generation == _editGeneration) {
        await _refreshViewport(
          restoreInputWindow: false,
          expectedEditGeneration: generation,
          ensureActiveInputVisible: true,
        );
        if (generation == _editGeneration) _scheduleParsingAfterInput();
      }
      _pendingEdits = math.max(0, _pendingEdits - 1);
      notifyListeners();
    } catch (error) {
      _projectionContinuity = null;
      _pendingEdits = math.max(0, _pendingEdits - 1);
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
    }
  }

  Future<bool> undo() => _queueHistoryReplay(undoDirection: true);

  Future<bool> redo() => _queueHistoryReplay(undoDirection: false);

  Future<bool> _queueHistoryReplay({required bool undoDirection}) {
    if (_closed ||
        _status == FlarkEditorStatus.faulted ||
        _historyReplayPending ||
        (!undoDirection && !_session.canRedo) ||
        (undoDirection && !_session.canUndo && _pendingEdits == 0)) {
      return Future<bool>.value(false);
    }
    _historyReplayPending = true;
    _projectionContinuity = null;
    _committedParagraphSplit = null;
    _committedStructuralSurfaces = const [];
    _semanticEditV1Active = false;
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    _parseTimer?.cancel();
    _parseTimer = null;
    final generation = ++_editGeneration;
    _pendingEdits += 1;
    _status = FlarkEditorStatus.editing;
    notifyListeners();

    final operation = _editTail.then((_) async {
      final outcome = undoDirection
          ? await _session.undo()
          : await _session.redo();
      if (outcome == null) return false;
      final restore = _adapterSnapshot(outcome.restoreSelection);
      _optimisticViewportEdits.clear();
      // History replay is one authoritative visual transaction. Do not
      // publish a pending exact-source viewport between the native replay and
      // its parser-certified result; retain the prior frame while bounded
      // parsing catches up, then adopt source, projection, and selection
      // together.
      while (!_document.isReady && !_closed) {
        await _document.pump(workUnits: 512);
      }
      await _refreshViewport(
        restoreInputWindow: false,
        expectedEditGeneration: generation,
      );
      await _restoreHistorySelection(restore);
      if (outcome is FlarkCoreHistoryDropped) return false;
      _scheduleParsingAfterInput();
      notifyListeners();
      return true;
    });
    _editTail = operation
        .then<void>((_) {})
        .catchError((Object _, StackTrace _) {});
    unawaited(
      operation
          .then((didReplay) {
            _pendingEdits = math.max(0, _pendingEdits - 1);
            _historyReplayPending = false;
            if (!didReplay) {
              _status = _semanticViewportCurrent
                  ? FlarkEditorStatus.ready
                  : FlarkEditorStatus.parsing;
            }
            notifyListeners();
          })
          .catchError((Object error, StackTrace stackTrace) {
            _pendingEdits = math.max(0, _pendingEdits - 1);
            _historyReplayPending = false;
            _lastError = error;
            _status = FlarkEditorStatus.faulted;
            notifyListeners();
          }),
    );
    return operation;
  }

  void _scheduleParsingAfterInput() {
    if (_closed ||
        _session.compositionActive ||
        _status == FlarkEditorStatus.faulted) {
      return;
    }
    _parseTimer?.cancel();
    _parseTimer = Timer(_parseIdleDelay, () {
      _parseTimer = null;
      unawaited(continueParsing());
    });
  }

  Future<void> _finishParsing() async {
    try {
      _status = FlarkEditorStatus.parsing;
      notifyListeners();
      while (!_document.isReady && !_closed) {
        await _document.pump(workUnits: 512);
      }
      if (_closed || _session.compositionActive) return;
      await _refreshViewport(
        restoreInputWindow: true,
        ensureActiveInputVisible: true,
      );
    } catch (error) {
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
    }
  }

  Future<void> _refreshViewport({
    required bool restoreInputWindow,
    int? expectedEditGeneration,
    bool ensureActiveInputVisible = false,
  }) async {
    final previous = _viewport;
    if (previous != null && previous.continuation != 0) {
      await _document.releaseViewportContinuation(previous);
    }
    // The visible cache is bounded by bytes as well as rows: one giant
    // physical line would otherwise make a 32-row page exceed the 16 KiB
    // window. Until giant-line fragmentation lands, the page request itself
    // enforces the byte bound in every parse state.
    final requestedEnd = math.min(sourceByteLength, _maximumVisibleBytes);
    final viewport = await _document.queryViewport(
      startByte: 0,
      endByte: requestedEnd,
      maxRows: _viewportRowsPerPage,
    );
    if (expectedEditGeneration != null &&
        expectedEditGeneration != _editGeneration) {
      await _document.releaseViewportContinuation(viewport);
      return;
    }
    final source = await _readViewportSource(viewport);
    if (expectedEditGeneration != null &&
        expectedEditGeneration != _editGeneration) {
      await _document.releaseViewportContinuation(viewport);
      return;
    }
    _pageStarts
      ..clear()
      ..add(0);
    _pageIndex = 0;
    _installViewport(
      viewport,
      source,
      restoreInputWindow: restoreInputWindow,
      ensureActiveInputVisible: ensureActiveInputVisible,
    );
  }

  Future<String> _readViewportSource(FlarkViewport viewport) async {
    return viewport.neutralSource ??
        await _document.readSourceRange(
          viewport.coveredBytes.start,
          viewport.coveredBytes.end,
        );
  }

  // Installation is synchronous so page index, rows, visible source, and
  // certification can never be observed in a torn half-installed state.
  void _installViewport(
    FlarkViewport viewport,
    String source, {
    required bool restoreInputWindow,
    bool ensureActiveInputVisible = false,
  }) {
    _viewport = viewport;
    final hasCommittedSurface =
        _projectionContinuity != null ||
        _committedParagraphSplit != null ||
        _committedStructuralSurfaces.isNotEmpty;
    final retainsExistingSurface =
        !viewport.isCertified && hasCommittedSurface && _cachedRows.isNotEmpty;
    final installsFreshRows =
        viewport.rows.isNotEmpty && !retainsExistingSurface;
    if (installsFreshRows) {
      _cachedRows = viewport.rows;
    } else if (viewport.isCertified) {
      _cachedRows = const [];
    }
    // Rows and their source cache are one publication. A pending viewport can
    // legitimately contain no semantic rows; pairing that new source window
    // with retained rows creates a torn surface and can eject the active row.
    // Optimistic edits have already updated the retained source cache, so keep
    // both halves until a fresh row publication arrives.
    if (!retainsExistingSurface) {
      _visibleSource = source;
      _visibleUtf16Start = viewport.coveredUtf16.start;
    }
    _certificationRanges = viewport.certificationRanges;
    _certificationRevisionCurrent = viewport.certificationRanges.isNotEmpty;
    if (installsFreshRows) _optimisticViewportEdits.clear();
    _semanticViewportCurrent = viewport.isCertified && installsFreshRows;
    if (_viewportSupersedesProjectionContinuity(viewport)) {
      _projectionContinuity = null;
    }
    _status = _semanticViewportCurrent
        ? FlarkEditorStatus.ready
        : FlarkEditorStatus.parsing;
    if (restoreInputWindow) {
      if (!ensureActiveInputVisible || !_ensureActiveInputVisible()) {
        _restoreInputWindow();
      }
    } else if (ensureActiveInputVisible) {
      if (!_ensureActiveInputVisible()) {
        _activeOrdinal = _surfaceOrdinalAt(_globalSelectionExtent);
      }
    }
    if (installsFreshRows &&
        !_cachedRows.any((row) => row.ordinal == _activeOrdinal)) {
      // Row ordinals belong to one viewport publication. A later parsing
      // installment can replace them after the edit refresh has already
      // restored the input window, so resolve the active row from the
      // canonical caret whenever the retained ordinal is no longer present.
      _activeOrdinal = _surfaceOrdinalAt(_globalSelectionExtent);
    }
    final mayRestoreInputWindow =
        restoreInputWindow || ensureActiveInputVisible;
    if (installsFreshRows &&
        mayRestoreInputWindow &&
        (_activeOrdinal ?? 0) < 0) {
      _restoreNeutralInputWindow(_globalSelectionExtent);
    }
    if (_semanticViewportCurrent && (_activeOrdinal ?? -1) >= 0) {
      _committedParagraphSplit = null;
      _committedStructuralSurfaces = const [];
    }
    notifyListeners();
  }

  bool _viewportSupersedesProjectionContinuity(FlarkViewport viewport) {
    final continuity = _projectionContinuity;
    if (continuity == null || !viewport.isCertified || viewport.rows.isEmpty) {
      return false;
    }
    final receipt = continuity.receipt;
    if (viewport.revision < receipt.resultRevision) return false;
    final authorized = receipt.authorizedContentUtf16;
    return viewport.coveredUtf16.start <= authorized.start &&
        authorized.end <= viewport.coveredUtf16.end;
  }

  void _restoreInputWindow() {
    if (_crossRowSelection) {
      _restoreSelectionSnapshot(_selectionSnapshot());
      return;
    }
    if ((_activeOrdinal ?? 0) < 0) {
      _restoreNeutralInputWindow(_globalSelectionExtent);
      return;
    }
    if (_cachedRows.isNotEmpty) {
      final caret = _globalSelectionExtent.clamp(0, sourceUtf16Length);
      final row = _cachedRows.cast<FlarkViewportRow?>().firstWhere((candidate) {
        final range = _activationRange(candidate!);
        return range.start <= caret && caret <= range.end;
      }, orElse: () => _cachedRows.first)!;
      final activationRange = _activationRange(row);
      final text = _sliceVisibleUtf16(
        activationRange.start,
        activationRange.end,
      );
      _activateWindowWithoutNotification(
        text: text,
        sourceStart: activationRange.start,
        caret: caret,
        ordinal: row.ordinal,
      );
      return;
    }
    final newline = _visibleSource.indexOf('\n');
    final end = newline == -1 ? _visibleSource.length : newline + 1;
    final visibleEnd = _visibleUtf16Start + end;
    _activateWindowWithoutNotification(
      text: _visibleSource.substring(0, end),
      sourceStart: _visibleUtf16Start,
      caret: _globalSelectionExtent.clamp(_visibleUtf16Start, visibleEnd),
      ordinal: -1,
    );
  }

  void _restoreNeutralInputWindow(int caret) {
    if (_visibleSource.isEmpty) {
      _activateWindowWithoutNotification(
        text: '',
        sourceStart: _visibleUtf16Start,
        caret: _visibleUtf16Start,
        ordinal: -1,
      );
      return;
    }
    final localCaret = (caret - _visibleUtf16Start).clamp(
      0,
      _visibleSource.length,
    );
    final lineStart = localCaret == 0
        ? 0
        : _visibleSource.lastIndexOf('\n', localCaret - 1) + 1;
    final newline = _visibleSource.indexOf('\n', localCaret);
    final lineEnd = newline == -1 ? _visibleSource.length : newline + 1;
    var lineOrdinal = 0;
    for (var index = 0; index < lineStart; index += 1) {
      if (_visibleSource.codeUnitAt(index) == 0x0a) lineOrdinal += 1;
    }
    _activateWindowWithoutNotification(
      text: _visibleSource.substring(lineStart, lineEnd),
      sourceStart: _visibleUtf16Start + lineStart,
      caret: caret,
      ordinal: -lineOrdinal - 1,
    );
  }

  void _activateWindowWithoutNotification({
    required String text,
    required int sourceStart,
    required int caret,
    required int ordinal,
  }) {
    final localCaret = (caret - sourceStart).clamp(0, text.length);
    final windowStart = text.length <= _maximumInputCodeUnits
        ? 0
        : (localCaret - _maximumInputCodeUnits ~/ 2).clamp(
            0,
            text.length - _maximumInputCodeUnits,
          );
    final windowEnd = math.min(
      text.length,
      windowStart + _maximumInputCodeUnits,
    );
    _inputGlobalUtf16Start = sourceStart + windowStart;
    _inputValue = TextEditingValue(
      text: text.substring(windowStart, windowEnd),
      selection: TextSelection.collapsed(offset: localCaret - windowStart),
    );
    _activeOrdinal = ordinal;
    _crossRowSelection = false;
    _updateGlobalSelection();
  }

  String _sliceVisibleUtf16(int globalStart, int globalEnd) {
    final start = (globalStart - _visibleUtf16Start).clamp(
      0,
      _visibleSource.length,
    );
    final end = (globalEnd - _visibleUtf16Start).clamp(
      start,
      _visibleSource.length,
    );
    return _visibleSource.substring(start, end);
  }

  void _applyOptimisticViewportEdit(
    int globalStart,
    int globalEnd,
    String replacement,
  ) {
    _semanticViewportCurrent = false;
    _certificationRevisionCurrent = false;
    _certificationRanges = const [];
    final localStart = globalStart - _visibleUtf16Start;
    final localEnd = globalEnd - _visibleUtf16Start;
    if (localStart < 0 ||
        localEnd < localStart ||
        localEnd > _visibleSource.length) {
      _viewport = null;
      _cachedRows = const [];
      _visibleSource = _inputValue.text;
      _visibleUtf16Start = _inputGlobalUtf16Start;
      _activeOrdinal = _surfaceOrdinalAt(_globalSelectionExtent);
      _optimisticViewportEdits.clear();
      return;
    }
    final nextLength = _replacementLength(
      _visibleSource,
      localStart,
      localEnd,
      replacement,
    );
    if (nextLength > _maximumInputCodeUnits) {
      final window = _boundedReplacementWindow(
        _visibleSource,
        localStart,
        localEnd,
        replacement,
        _globalSelectionExtent - _visibleUtf16Start,
      );
      _visibleSource = window.text;
      _visibleUtf16Start += window.start;
      _viewport = null;
      _cachedRows = const [];
      _pageStarts
        ..clear()
        ..add(0);
      _pageIndex = 0;
      _activeOrdinal = _surfaceOrdinalAt(_globalSelectionExtent);
      _optimisticViewportEdits.clear();
      return;
    }
    _visibleSource = _visibleSource.replaceRange(
      localStart,
      localEnd,
      replacement,
    );
    _optimisticViewportEdits.add(
      _OptimisticViewportEdit(
        start: globalStart,
        end: globalEnd,
        replacementLength: replacement.length,
      ),
    );
  }

  FlarkSourceRange _mapViewportRange(FlarkSourceRange base) {
    var start = base.start;
    var end = base.end;
    for (final edit in _optimisticViewportEdits) {
      if (end <= edit.start) continue;
      if (start >= edit.end) {
        start += edit.delta;
        end += edit.delta;
        continue;
      }
      start = math.min(start, edit.start);
      end = math.max(edit.start + edit.replacementLength, end + edit.delta);
    }
    return FlarkSourceRange(start, end);
  }

  ({String text, int globalStart, TextSelection selection}) _paintInputWindow({
    int? sourceStart,
    int? sourceEnd,
  }) {
    final value = _inputValue;
    final allowedStart = sourceStart == null
        ? 0
        : (sourceStart - _inputGlobalUtf16Start).clamp(0, value.text.length);
    final allowedEnd = sourceEnd == null
        ? value.text.length
        : (sourceEnd - _inputGlobalUtf16Start).clamp(
            allowedStart,
            value.text.length,
          );
    final allowedLength = allowedEnd - allowedStart;
    if (allowedLength <= _maximumPaintCodeUnits) {
      final text = value.text.substring(allowedStart, allowedEnd);
      return (
        text: text,
        globalStart: _inputGlobalUtf16Start + allowedStart,
        selection: TextSelection(
          baseOffset: (value.selection.baseOffset - allowedStart).clamp(
            0,
            text.length,
          ),
          extentOffset: (value.selection.extentOffset - allowedStart).clamp(
            0,
            text.length,
          ),
          affinity: value.selection.affinity,
          isDirectional: value.selection.isDirectional,
        ),
      );
    }

    final selectionStart = math.min(
      value.selection.baseOffset,
      value.selection.extentOffset,
    );
    final selectionEnd = math.max(
      value.selection.baseOffset,
      value.selection.extentOffset,
    );
    final focus = value.selection.extentOffset.clamp(allowedStart, allowedEnd);
    var start = (focus - _maximumPaintCodeUnits ~/ 2).clamp(
      allowedStart,
      allowedEnd - _maximumPaintCodeUnits,
    );
    if (selectionStart >= allowedStart &&
        selectionEnd <= allowedEnd &&
        selectionEnd - selectionStart <= _maximumPaintCodeUnits) {
      start = math.min(start, selectionStart);
      start = math.max(
        allowedStart,
        math.max(start, selectionEnd - _maximumPaintCodeUnits),
      );
    }
    var end = start + _maximumPaintCodeUnits;
    if (start < value.text.length &&
        _isLowSurrogate(value.text.codeUnitAt(start))) {
      start += 1;
    }
    if (end < value.text.length &&
        _isLowSurrogate(value.text.codeUnitAt(end))) {
      end -= 1;
    }
    final text = value.text.substring(start, end);
    return (
      text: text,
      globalStart: _inputGlobalUtf16Start + start,
      selection: TextSelection(
        baseOffset: (value.selection.baseOffset - start).clamp(0, text.length),
        extentOffset: (value.selection.extentOffset - start).clamp(
          0,
          text.length,
        ),
        affinity: value.selection.affinity,
        isDirectional: value.selection.isDirectional,
      ),
    );
  }

  bool _isLowSurrogate(int codeUnit) =>
      codeUnit >= 0xdc00 && codeUnit <= 0xdfff;

  FlarkViewportRow? _activeCachedRow() {
    final activeOrdinal = _activeOrdinal;
    if (activeOrdinal == null) return null;
    for (final candidate in _cachedRows) {
      if (candidate.ordinal == activeOrdinal) return candidate;
    }
    return null;
  }

  FlarkSourceRange _activationRange(FlarkViewportRow row) {
    final editable = row.editableUtf16;
    if (editable != null && editable.start < row.sourceUtf16.start) {
      return editable;
    }
    return row.sourceUtf16;
  }

  String _projectedListPrefix(FlarkListItemPresentation item) {
    final nestingIndent = math.max(0, item.nestingDepth - 1) * 2;
    final marker = switch (item.taskChecked) {
      null => item.markerText,
      false => '☐',
      true => '☑',
    };
    return '${''.padLeft(nestingIndent + item.markerOffset)}$marker ';
  }

  String _projectedBlockQuotePrefix(FlarkBlockQuotePresentation quote) =>
      List<String>.filled(quote.nestingDepth, '│ ').join();

  bool _rowSemanticsCurrent(FlarkSourceRange mappedSource) {
    if (_semanticViewportCurrent) return true;
    // The committed splice proves that prior rows remain unchanged while the
    // newly introduced gap is painted from exact source as a neutral island.
    if (_committedParagraphSplit != null) return true;
    // A transaction-bound continuity receipt proves that the conservative
    // source edit cannot alter presentation outside its authorized active
    // content. Keep unchanged cached rows semantic as well; demoting the whole
    // viewport for one safe keystroke produces a visible page-wide flash.
    if (_projectionContinuity != null) return true;
    if (_committedStructuralSurfaces.isNotEmpty) return true;
    if (!_certificationRevisionCurrent) return false;
    return _certificationRanges.any(
      (range) =>
          range.isCertified &&
          range.sourceUtf16.start <= mappedSource.start &&
          mappedSource.end <= range.sourceUtf16.end,
    );
  }
}
