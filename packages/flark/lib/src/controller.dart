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

  int sourceOffsetForTextOffset(int offset) {
    final local = offset.clamp(0, text.length);
    if (sourceExact) return sourceUtf16Start + local;
    if (local == 0) return sourceUtf16Start;
    if (local == text.length) return sourceUtf16End;
    return local * 2 <= text.length ? sourceUtf16Start : sourceUtf16End;
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

  int sourceOffsetForTextOffset(int offset) {
    final local = offset.clamp(0, text.length);
    if (runs.isEmpty) return globalUtf16Start + local;
    var consumed = 0;
    for (final run in runs) {
      final runEnd = consumed + run.text.length;
      if (local <= runEnd) {
        return run.sourceOffsetForTextOffset(local - consumed);
      }
      consumed = runEnd;
    }
    return runs.last.sourceUtf16End;
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

final class _EditorSelectionSnapshot {
  const _EditorSelectionSnapshot(this.selection, this.activeOrdinal);

  final TextSelection selection;
  final int? activeOrdinal;
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
  int _pageIndex = 0;
  int _editGeneration = 0;
  int _pendingEdits = 0;
  Object? _lastError;
  bool _closed = false;
  bool _semanticViewportCurrent = false;
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
  FlarkInputResyncReason _validateDeltaBatch(List<TextEditingDelta> deltas) {
    if (deltas.isEmpty) return FlarkInputResyncReason.none;
    if (flarkWindowTextSha256(deltas.first.oldText) != _windowTextSha256) {
      return FlarkInputResyncReason.oldTextMismatch;
    }
    var value = _inputValue;
    var runningHash = _windowTextSha256;
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

  FlarkSurfaceRow surfaceRow(FlarkViewportRow row) {
    final mappedSource = _mapViewportRange(row.sourceUtf16);
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
    final exactRange = semanticRange;
    if (_crossRowSelection &&
        (_selectionIntersects(exactRange) || _activeOrdinal == row.ordinal)) {
      return _exactSelectionSurfaceRow(range: exactRange, ordinal: row.ordinal);
    }
    if (_activeOrdinal == row.ordinal) {
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
    final runs = rowCertified && row.inlineFacts != null
        ? _projectInlineRuns(range, row.inlineFacts!)
        : [_exactSurfaceRun(range)];
    return FlarkSurfaceRow(
      leadingText: leadingText,
      text: runs.map((run) => run.text).join(),
      globalUtf16Start: range.start,
      kind: rowCertified ? row.kind : 0,
      headingLevel: row.headingLevel,
      blockQuoteDepth: rowCertified ? blockQuote?.nestingDepth : null,
      codeBlock: rowCertified ? row.codeBlock : null,
      thematicBreak: rowCertified && row.thematicBreak,
      ordinal: row.ordinal,
      active: false,
      selection: null,
      runs: runs,
    );
  }

  FlarkSurfaceRow neutralSurfaceRow({
    required int globalUtf16Start,
    required String text,
    required int ordinal,
  }) {
    final surfaceOrdinal = -ordinal - 1;
    final range = FlarkSourceRange(
      globalUtf16Start,
      globalUtf16Start + text.length,
    );
    if (_crossRowSelection &&
        (_selectionIntersects(range) || _activeOrdinal == surfaceOrdinal)) {
      return _exactSelectionSurfaceRow(range: range, ordinal: surfaceOrdinal);
    }
    if (_activeOrdinal == surfaceOrdinal &&
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
      };

  void activateRow(FlarkViewportRow row, int globalUtf16Offset) {
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
    );
    unawaited(_installCanonicalSelection(_selectionSnapshot()));
  }

  void activateNeutralLine({
    required String text,
    required int globalUtf16Start,
    required int globalUtf16Offset,
    required int ordinal,
  }) {
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    _abandonOversizedSelection();
    _activateWindow(
      text: text,
      sourceStart: globalUtf16Start,
      caret: globalUtf16Offset,
      ordinal: -ordinal - 1,
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
    if (_historyReplayPending) {
      notifyListeners();
      return;
    }
    final rejection = _validateDeltaBatch(deltas);
    if (rejection != FlarkInputResyncReason.none) {
      _resynchronize(rejection);
      return;
    }
    _platformMutation = true;
    try {
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
        _inputValue = finalValue;
        _trackCompositionWithoutMutation(finalValue.composing);
        _updateGlobalSelection();
        unawaited(_installCanonicalSelection(_selectionSnapshot()));
      } else {
        final before = _inputValue.text;
        final after = finalValue.text;
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
            before.codeUnitAt(oldSuffix - 1) ==
                after.codeUnitAt(newSuffix - 1)) {
          oldSuffix -= 1;
          newSuffix -= 1;
        }
        final accepted = _acceptMutation(
          _TextMutation(prefix, oldSuffix, after.substring(prefix, newSuffix)),
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
    if (_historyReplayPending) {
      notifyListeners();
      return;
    }
    _platformMutation = true;
    try {
      _updateEditingValueFromPlatform(value);
    } finally {
      _platformMutation = false;
    }
  }

  void _updateEditingValueFromPlatform(TextEditingValue value) {
    if (value.text == _inputValue.text) {
      _breakTypingHistoryGroup();
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
    if (_oversizedSelection) {
      replaceSelection('');
      return;
    }
    final selection = _inputValue.selection;
    if (!selection.isCollapsed) {
      replaceSelection('');
      return;
    }
    if (_deleteProjectedListPrefix(selection.extentOffset) ||
        _deleteProjectedBlockQuotePrefix(selection.extentOffset) ||
        _deleteProjectedHeadingPrefix(selection.extentOffset)) {
      return;
    }
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
    if (_oversizedSelection) {
      replaceSelection('');
      return;
    }
    final selection = _inputValue.selection;
    if (!selection.isCollapsed) {
      replaceSelection('');
      return;
    }
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
    final selection = _inputValue.selection;
    if (selection.isCollapsed &&
        (_insertProjectedListNewline(selection.extentOffset) ||
            _insertProjectedBlockQuoteNewline(selection.extentOffset))) {
      return;
    }
    replaceSelection('\n');
  }

  bool _smallEditFits(String source, int start, int end, String replacement) {
    if (start < 0 || end < start || end > source.length) return false;
    final nextInputLength = _replacementLength(source, start, end, replacement);
    if (nextInputLength > _maximumInputCodeUnits) return false;
    final deletedBytes = utf8.encode(source.substring(start, end)).length;
    final replacementBytes = utf8.encode(replacement).length;
    return _smallEditDescriptorBytes + deletedBytes + replacementBytes <=
        _maximumSmallEditBytes;
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
      selection: TextSelection.collapsed(offset: windowCaret),
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
    FlarkViewportRow? boundaryCandidate;
    FlarkViewportRow? precedingCandidate;
    for (final row in _cachedRows) {
      final range = _mappedExactRowRange(row);
      if (range.start <= globalUtf16Offset && globalUtf16Offset < range.end) {
        return row.ordinal;
      }
      if (globalUtf16Offset == range.end) boundaryCandidate ??= row;
      if (range.start <= globalUtf16Offset) precedingCandidate = row;
    }
    if (boundaryCandidate != null) return boundaryCandidate.ordinal;
    if (precedingCandidate != null) return precedingCandidate.ordinal;
    if (_cachedRows.isNotEmpty) return _cachedRows.first.ordinal;
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

  Future<void> _completeQueuedEdit(
    Future<void> operation,
    int generation,
  ) async {
    try {
      await operation;
    } catch (error) {
      _pendingEdits = math.max(0, _pendingEdits - 1);
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
      return;
    }
    _pendingEdits = math.max(0, _pendingEdits - 1);
    notifyListeners();
    if (generation != _editGeneration) return;
    try {
      await _refreshViewport(
        restoreInputWindow: false,
        expectedEditGeneration: generation,
        ensureActiveInputVisible: true,
      );
      if (generation == _editGeneration) _scheduleParsingAfterInput();
    } catch (error) {
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
      await _refreshViewport(restoreInputWindow: false);
      if (!_ensureActiveInputVisible()) _restoreInputWindow();
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
    final installsFreshRows = viewport.rows.isNotEmpty;
    if (installsFreshRows) {
      _cachedRows = viewport.rows;
    }
    _visibleSource = source;
    _certificationRanges = viewport.certificationRanges;
    _certificationRevisionCurrent = viewport.certificationRanges.isNotEmpty;
    if (installsFreshRows) _optimisticViewportEdits.clear();
    _semanticViewportCurrent = viewport.isCertified && installsFreshRows;
    _visibleUtf16Start = viewport.coveredUtf16.start;
    _status = _semanticViewportCurrent
        ? FlarkEditorStatus.ready
        : FlarkEditorStatus.parsing;
    if (restoreInputWindow) {
      _restoreInputWindow();
    } else if (ensureActiveInputVisible) {
      _ensureActiveInputVisible();
    }
    notifyListeners();
  }

  void _restoreInputWindow() {
    if (_crossRowSelection) {
      _restoreSelectionSnapshot(_selectionSnapshot());
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

  bool _deleteProjectedHeadingPrefix(int localCaret) {
    final row = _activeCachedRow();
    if (row == null ||
        row.kind != 12 ||
        row.headingStyle != FlarkHeadingStyle.atx ||
        row.editableUtf16 == null) {
      return false;
    }
    final source = _mapViewportRange(row.sourceUtf16);
    final editable = _mapViewportRange(row.editableUtf16!);
    final globalCaret = _inputGlobalUtf16Start + localCaret;
    if (!_rowSemanticsCurrent(source) ||
        globalCaret != editable.start ||
        source.start >= editable.start) {
      return false;
    }
    final localStart = source.start - _inputGlobalUtf16Start;
    final localEnd = editable.start - _inputGlobalUtf16Start;
    if (localStart < 0 || localEnd > _inputValue.text.length) return false;
    _inputValue = _inputValue.copyWith(
      selection: TextSelection(baseOffset: localStart, extentOffset: localEnd),
      composing: TextRange.empty,
    );
    replaceSelection('');
    return true;
  }

  bool _deleteProjectedListPrefix(int localCaret) {
    final row = _activeCachedRow();
    final listItem = row?.listItem;
    final editableRange = row?.editableUtf16;
    if (row == null ||
        listItem == null ||
        editableRange == null ||
        !listItem.simpleContinuation) {
      return false;
    }
    final source = _mapViewportRange(row.sourceUtf16);
    final prefix = _mapViewportRange(listItem.prefixUtf16);
    final editable = _mapViewportRange(editableRange);
    final globalCaret = _inputGlobalUtf16Start + localCaret;
    if (!_rowSemanticsCurrent(FlarkSourceRange(prefix.start, source.end)) ||
        globalCaret != editable.start ||
        prefix.start >= prefix.end) {
      return false;
    }
    _replaceProjectedPrefix(
      prefix,
      listItem.startsList || prefix.end < source.start ? '' : '\n',
    );
    return true;
  }

  bool _insertProjectedListNewline(int localCaret) {
    final row = _activeCachedRow();
    final listItem = row?.listItem;
    final editableRange = row?.editableUtf16;
    if (row == null ||
        listItem == null ||
        editableRange == null ||
        !listItem.simpleContinuation) {
      return false;
    }
    final source = _mapViewportRange(row.sourceUtf16);
    final prefix = _mapViewportRange(listItem.prefixUtf16);
    final editable = _mapViewportRange(editableRange);
    final globalCaret = _inputGlobalUtf16Start + localCaret;
    if (!_rowSemanticsCurrent(FlarkSourceRange(prefix.start, source.end)) ||
        globalCaret < editable.start ||
        globalCaret > editable.end) {
      return false;
    }
    if (row.kind == 14 || editable.start == editable.end) {
      _replaceProjectedPrefix(prefix, prefix.end < source.start ? '' : '\n');
      return true;
    }
    final continuation =
        '${''.padLeft(listItem.markerOffset)}${listItem.nextMarkerText} '
        '${listItem.taskChecked == null ? '' : '[ ] '}';
    replaceSelection('\n$continuation');
    return true;
  }

  bool _deleteProjectedBlockQuotePrefix(int localCaret) {
    final row = _activeCachedRow();
    final blockQuote = row?.blockQuote;
    final editableRange = row?.editableUtf16;
    if (row == null ||
        blockQuote == null ||
        editableRange == null ||
        !blockQuote.simpleContinuation) {
      return false;
    }
    final source = _mapViewportRange(row.sourceUtf16);
    final prefix = _mapViewportRange(blockQuote.prefixUtf16);
    final editable = _mapViewportRange(editableRange);
    final globalCaret = _inputGlobalUtf16Start + localCaret;
    if (!_rowSemanticsCurrent(FlarkSourceRange(prefix.start, source.end)) ||
        globalCaret != editable.start ||
        prefix.start >= prefix.end) {
      return false;
    }
    _replaceProjectedPrefix(prefix, '');
    return true;
  }

  bool _insertProjectedBlockQuoteNewline(int localCaret) {
    final row = _activeCachedRow();
    final blockQuote = row?.blockQuote;
    final editableRange = row?.editableUtf16;
    if (row == null ||
        blockQuote == null ||
        editableRange == null ||
        !blockQuote.simpleContinuation) {
      return false;
    }
    final source = _mapViewportRange(row.sourceUtf16);
    final prefix = _mapViewportRange(blockQuote.prefixUtf16);
    final editable = _mapViewportRange(editableRange);
    final globalCaret = _inputGlobalUtf16Start + localCaret;
    if (!_rowSemanticsCurrent(FlarkSourceRange(prefix.start, source.end)) ||
        globalCaret < editable.start ||
        globalCaret > editable.end ||
        prefix.start >= prefix.end) {
      return false;
    }
    final exactPrefix = _sliceVisibleUtf16(prefix.start, prefix.end);
    if (exactPrefix.isEmpty) return false;
    replaceSelection('\n$exactPrefix');
    return true;
  }

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
    if (row.listItem != null &&
        editable != null &&
        editable.start < row.sourceUtf16.start) {
      return editable;
    }
    return row.sourceUtf16;
  }

  void _replaceProjectedPrefix(FlarkSourceRange prefix, String replacement) {
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    final exactPrefix = _sliceVisibleUtf16(prefix.start, prefix.end);
    if (!_smallEditFits(exactPrefix, 0, exactPrefix.length, replacement)) {
      return;
    }
    final beforeSelection = _selectionSnapshot();
    final delta = replacement.length - prefix.length;
    if (_inputGlobalUtf16Start >= prefix.end) {
      _inputGlobalUtf16Start += delta;
    }
    _updateGlobalSelection();
    _queueNativeEdit(
      prefix.start,
      prefix.end,
      replacement,
      beforeSelection: beforeSelection,
      afterSelection: _selectionSnapshot(),
      coalesceTyping: false,
      compositionHistoryGroup: null,
    );
    notifyListeners();
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
    if (!_certificationRevisionCurrent) return false;
    return _certificationRanges.any(
      (range) =>
          range.isCertified &&
          range.sourceUtf16.start <= mappedSource.start &&
          mappedSource.end <= range.sourceUtf16.end,
    );
  }
}
