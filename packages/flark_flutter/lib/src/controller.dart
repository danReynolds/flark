import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'editor_performance.dart';
import 'editor_input_state.dart';
import 'editor_transactions.dart';
import 'input_transaction_state.dart';
import 'input_window.dart';
import 'input_reconciliation.dart';
import 'platform_input_bridge.dart';
import 'text_adaptation.dart';

export 'editor_performance.dart'
    show
        FlarkSemanticEditPerformance,
        FlarkSourceEditPerformance,
        FlarkSourceEditPerformanceKind;

const _maximumVisibleBytes = 16 * 1024;
const _maximumInputCodeUnits = 16 * 1024;
const _viewportRowsPerPage = 32;
const _maximumActiveViewportPageHops =
    (_maximumInputCodeUnits + _viewportRowsPerPage - 1) ~/
        _viewportRowsPerPage +
    1;
const _parseIdleDelay = Duration(milliseconds: 32);
const _maximumSemanticSuccessors = 7;
// The bounded head window every parse-pending consumer polls during a
// streamed open: the same 4 KiB certification-probe discipline the core
// layer applies inside its own opening viewport query.
const _openingHeadProbeBytes = 4 * 1024;

/// Carries admission acknowledgements that arrive before the controller they
/// belong to exists, then forwards them for the rest of the load.
///
/// **Streamed admission is epoch-neutral for the input window.** An admitted
/// append can only add bytes after the admitted frontier: the engine proves
/// no byte inside the previously admitted prefix changes, and the edit
/// revision does not advance. The input window always lies inside certified
/// text, which is inside that unchanged prefix, so an append cannot
/// invalidate the window text, its hash identity, or the selection it
/// carries. Admission therefore never advances the window or connection
/// epoch and never forces a resync — it only grows the document's length
/// mirrors, which the notification below surfaces for length-derived UI
/// (scroll extent estimates, progress) without touching input authority.
///
/// A literal edit during load is the separate case: it advances the edit
/// revision and flows through the ordinary revision-resync machinery
/// unchanged, exactly as an edit against a fully loaded document does.
final class _AdmissionHost {
  FlarkEditorController? controller;
  bool _pendingAdmission = false;

  void onAdmission() {
    final controller = this.controller;
    if (controller == null) {
      _pendingAdmission = true;
      return;
    }
    if (controller._closed) return;
    controller.notifyListeners();
  }

  void attach(FlarkEditorController controller) {
    this.controller = controller;
    if (!_pendingAdmission || controller._closed) return;
    _pendingAdmission = false;
    controller.notifyListeners();
  }
}

/// UI-isolate state for a bounded viewport and bounded platform input window.
///
/// The complete document remains in [FlarkCoreDocument]'s worker/native actor.
/// This controller retains one bounded viewport page and at most 16 Ki UTF-16
/// code units in the platform text input connection, so a keystroke does not
/// copy a multi-megabyte document on Flutter's UI isolate.
final class FlarkEditorController extends ChangeNotifier
    implements ValueListenable<FlarkEditorSnapshot> {
  FlarkEditorController._(this._document)
    : _session = FlarkCoreEditorSession(_document) {
    _viewportPager = FlarkEditorViewportPager(
      source: _document,
      coordinator: _coordinator,
      maximumVisibleBytes: _maximumVisibleBytes,
      rowsPerPage: _viewportRowsPerPage,
      maximumCaretPageHops: _maximumActiveViewportPageHops,
    );
  }

  final FlarkCoreDocument _document;

  /// Canonical selection, grapheme, and undo policy authority. The controller
  /// is an adapter over it and holds no history stacks of its own.
  final FlarkCoreEditorSession _session;
  final FlarkEditorCoordinator _coordinator = FlarkEditorCoordinator();
  final ObserverList<VoidCallback> _inputStateListeners =
      ObserverList<VoidCallback>();

  final FlarkEditorViewportState _viewportState = FlarkEditorViewportState();
  late final FlarkEditorViewportPager _viewportPager;
  final FlarkEditorInputState _inputState = FlarkEditorInputState();
  Timer? _parseTimer;
  FlarkPendingPresentationSnapshot get _pendingPresentation =>
      _coordinator.pendingPresentation;

  void _clearPendingTaskChecks() {
    _coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.taskChecks,
    });
  }

  void _setPendingTaskCheck(int rowOrdinal, bool checked) {
    _coordinator.setPendingTaskCheck(rowOrdinal, checked);
  }

  final FlarkPlatformInputBridge _platformInput = FlarkPlatformInputBridge();
  final FlarkInputTransactionState _inputTransactions =
      FlarkInputTransactionState();

  String? _debugLastSemanticReceiptDescription;
  final FlarkEditorPerformanceLog _performance = FlarkEditorPerformanceLog();
  final Completer<void> _firstCertifiedPublication = Completer<void>();
  int? _firstCertifiedPublicationEpochMicros;
  // The revision whose certified head the streamed-open loop has published,
  // and the waiter woken by the next publication. A streamed open's parse
  // task deliberately runs for the whole load, so presentation barriers key
  // on these instead of on the task completing.
  Completer<void>? _openingPublication;

  FlarkEditorStatus get _status => _coordinator.status;
  set _status(FlarkEditorStatus value) => _coordinator.transitionStatus(value);
  Object? get _lastError => _coordinator.lastError;
  set _lastError(Object? value) => _coordinator.setLastError(value);
  bool get _closed => _coordinator.closed;
  int get _editGeneration => _coordinator.editGeneration;
  int get _interactionGeneration => _coordinator.interactionGeneration;
  int get _publishedSourceGeneration => _coordinator.publishedSourceGeneration;
  int get _publishedDocumentRevision => _coordinator.publishedDocumentRevision;
  bool get _publicationCertificationBarrierActive =>
      _coordinator.publicationCertificationBarrierActive;
  FlarkEditorSnapshot? _snapshot;

  int get _openingPublishedRevision => _coordinator.openingPublishedRevision;

  FlarkEditorStatus get status => _status;

  /// The quiescent (non-editing, non-faulted) status for the document's
  /// current phase. While a streamed open is still admitting source the
  /// editor is live but neither parsing-toward-ready nor ready, so every
  /// quiescent transition reports [FlarkEditorStatus.streaming]; otherwise
  /// [current] picks the familiar ready/parsing split.
  FlarkEditorStatus _idleStatus({required bool current}) {
    if (_document.isOpening) return FlarkEditorStatus.streaming;
    return current ? FlarkEditorStatus.ready : FlarkEditorStatus.parsing;
  }

  FlarkViewport? get viewport => _viewportState.viewport;
  String get visibleSource => _viewportState.visibleSource;
  int get visibleUtf16Start => _viewportState.visibleUtf16Start;
  int get viewportPageIndex => _viewportPager.pageIndex;
  bool get canPageBackward =>
      _viewportState.semanticCurrent && _viewportPager.canPageBackward;
  bool get canPageForward => _viewportPager.canPageForward(
    semanticsCurrent: _viewportState.semanticCurrent,
    viewport: _viewportState.viewport,
  );

  bool get semanticsCurrent => _viewportState.semanticCurrent;
  TextEditingValue get inputValue => _inputState.value;
  int get revision => _document.revision;

  /// Monotonic generation of the source currently exposed to the renderer.
  ///
  /// [_editGeneration] advances when a command is admitted so stale async
  /// work can be rejected. A semantic command does not expose new source
  /// until its native receipt arrives, so paint evidence must not use that
  /// internal generation and label the retained pre-command source as new.
  int get sourceGeneration => _publishedSourceGeneration;
  int get interactionGeneration => _interactionGeneration;
  int get sourceByteLength => _document.sourceByteLength;
  int get sourceUtf16Length => _document.sourceUtf16Length;
  int get pendingEdits => _coordinator.pendingEdits;

  /// Test-only visibility into whether an optimistic parser proof is still
  /// driving the active presentation.
  @visibleForTesting
  bool get debugProjectionContinuityActive =>
      _pendingPresentation.dependency != null;

  @visibleForTesting
  int get debugStructuralSurfaceCount =>
      _pendingPresentation.structuralSurfaces.length;

  @visibleForTesting
  bool get debugStructuralSurfaceContinuityActive => _pendingPresentation
      .structuralSurfaces
      .any((state) => state.continuity != null);

  @visibleForTesting
  bool get debugCaretBoundaryActive =>
      _pendingPresentation.caretBoundary != null;

  @visibleForTesting
  bool get debugPublicationCertificationBarrierActive =>
      _publicationCertificationBarrierActive;

  /// Whether Tab currently belongs to a table whose certified cell ranges are
  /// being transformed by an optimistic projection edit. The retained visual
  /// shell is not structural-navigation authority, so callers must consume
  /// Tab without navigating until a fresh table publication arrives.
  bool get pendingTableNavigationLocked {
    final continuity = _pendingPresentation.dependency;
    if (continuity == null) return false;
    for (final row in _viewportState.rows) {
      if (row.ordinal == continuity.rowOrdinal) return row.table != null;
    }
    return false;
  }

  /// Test-only visibility into the ordinary 32 ms parse debounce. Edit-cell
  /// islands deliberately bypass this timer so their exact island is replaced
  /// by fresh parser authority as soon as the native edit commits.
  @visibleForTesting
  bool get debugDelayedParseScheduled => _parseTimer?.isActive ?? false;

  @visibleForTesting
  int? get debugActiveOrdinal => _inputState.activeOrdinal;

  @visibleForTesting
  bool get debugSemanticEditV1Active => _inputState.semanticEditActive;

  @visibleForTesting
  bool get debugPendingSemanticInputActive =>
      _inputTransactions.pendingSemantic != null;

  @visibleForTesting
  bool get debugLateSemanticInputActive =>
      _inputTransactions.lateSemantic != null;

  bool get _certificationDeferredInputActive =>
      _inputTransactions.pendingSemantic?.certificationPromotion != null;

  Completer<void>? get _certificationDeferredInputPromotion =>
      _inputTransactions.pendingSemantic?.certificationPromotion;

  @visibleForTesting
  String? get debugLastSemanticReceiptDescription =>
      _debugLastSemanticReceiptDescription;

  Object? get lastError => _lastError;
  int get globalSelectionBase => _inputState.selectionBaseUtf16;
  int get globalSelectionExtent => _inputState.selectionExtentUtf16;
  int get globalCaretOffset => _inputState.selectionExtentUtf16;

  /// The last immutable bounded state sealed at this controller's outward
  /// notification boundary. The lazy path exists only for initial attachment;
  /// every subsequent notification replaces this object before listeners run.
  FlarkEditorSnapshot get snapshot => _snapshot ??= _captureEditorSnapshot();

  @override
  FlarkEditorSnapshot get value => snapshot;
  String? get selectedText {
    final selection = _inputState.value.selection;
    if (!selection.isValid || selection.isCollapsed) return null;
    final start = math.min(selection.baseOffset, selection.extentOffset);
    final end = math.max(selection.baseOffset, selection.extentOffset);
    if (start < 0 || end > _inputState.value.text.length) return null;
    return _inputState.value.text.substring(start, end);
  }

  bool get canUndo =>
      !_coordinator.historyReplayPending &&
      (_session.canUndo || _coordinator.pendingEdits > 0);
  bool get canRedo => !_coordinator.historyReplayPending && _session.canRedo;

  FlarkInputWindowState get inputWindowState => _platformInput.state;
  FlarkInputResyncReason get lastResyncReason =>
      _platformInput.lastResyncReason;
  int get connectionEpoch => _platformInput.connectionEpoch;
  int get windowEpoch => _platformInput.windowEpoch;
  int get resyncCount => _platformInput.resyncCount;
  bool get hasOversizedSelection => _inputState.oversizedSelection;
  int get canonicalSelectionGeneration => _session.selectionGeneration;
  int get semanticSuccessorHighWatermark =>
      _inputTransactions.successorHighWatermark;
  int get lastSemanticReconciliationMicros =>
      _inputTransactions.lastReconciliationMicros;
  FlarkSemanticEditPerformance? get lastSemanticEditPerformance =>
      _performance.lastSemantic;
  List<FlarkSemanticEditPerformance> get semanticEditPerformanceReceipts =>
      _performance.semantic;
  List<FlarkSourceEditPerformance> get sourceEditPerformanceReceipts =>
      _performance.source;

  /// Completes when this controller first publishes a viewport that carries
  /// parser-certified semantic rows — for a streamed open, the moment the
  /// certified head becomes paintable and editable while admission
  /// continues. Never completes if the controller closes or faults first.
  /// This is a receipt hook: the frame-profile harness joins it to the next
  /// engine frame to measure open-call to first-certified-painted-frame.
  Future<void> get firstCertifiedPublication =>
      _firstCertifiedPublication.future;

  /// Epoch microseconds of [firstCertifiedPublication], or null while no
  /// certified viewport has been published.
  int? get firstCertifiedPublicationEpochMicros =>
      _firstCertifiedPublicationEpochMicros;

  @visibleForTesting
  List<FlarkCertificationRange> get debugCertificationRanges =>
      List.unmodifiable(_viewportState.certificationRanges);

  FlarkInputWindowShadow get inputWindowShadow => _platformInput.snapshot(
    representedRevision: _document.revision,
    selectionGeneration: _session.selectionGeneration,
    fallbackValue: _inputState.value,
  );

  /// Reconciles the serialized platform shadow on every notification so no
  /// window-rewrite site can bypass the connection/window epoch discipline:
  /// a platform-accepted update advances the window epoch on the active
  /// connection, while any host-originated change to the exposed text, range,
  /// or selection retires the connection and starts a new one.
  @override
  void notifyListeners() {
    // A source transaction without parser-owned result presentation is kept
    // atomic at the controller's sole publication boundary. This also blocks
    // unrelated async selection acknowledgements from leaking the optimistic
    // source while certification is in flight. Faults still publish so a
    // failed barrier cannot hide terminal state from the host.
    if (_publicationCertificationBarrierActive &&
        _status != FlarkEditorStatus.faulted) {
      // A platform callback has already installed its provisional value before
      // entering the client. Once that mutation is accepted, keep the input
      // shadow in lockstep even when presentation certification suppresses the
      // outward notification. Otherwise the next same-burst delta is rejected
      // against the pre-edit hash and a valid typing sequence loses liveness.
      // Internal async work must not advance the shadow while its result is
      // still unpublished, hence the platform-mutation guard.
      if (_inputTransactions.platformMutationActive) _reconcileWindowShadow();
      return;
    }
    _reconcileSettledSemanticLane();
    _reconcileWindowShadow();
    _publishSnapshot();
  }

  /// Registers a text-service-only observer. Unlike the ordinary controller
  /// notification, this channel may advance while a certified visual frame is
  /// deliberately retained behind a parser handoff.
  void addInputStateListener(VoidCallback listener) =>
      _inputStateListeners.add(listener);

  void removeInputStateListener(VoidCallback listener) =>
      _inputStateListeners.remove(listener);

  void _publishCommandInputState() {
    if (!_publicationCertificationBarrierActive ||
        _status == FlarkEditorStatus.faulted) {
      notifyListeners();
      return;
    }
    _reconcileWindowShadow();
    for (final listener in List<VoidCallback>.of(_inputStateListeners)) {
      listener();
    }
  }

  void _reconcileSettledSemanticLane() {
    // The lane bit is transitional authority, not durable user intent. Once
    // parser-certified rows own the caret and no semantic mutation remains,
    // derive capability from that canonical row instead of callback order.
    // Neutral carets re-enter the lane from geometry on the next structural
    // command.
    if (_coordinator.pendingEdits != 0 ||
        _inputTransactions.pendingSemantic != null ||
        !_viewportState.semanticCurrent ||
        _session.compositionActive) {
      return;
    }
    final activeRow = _activeCachedRow();
    _inputState.setSemanticEditActive(
      activeRow?.semanticCapabilities.supportsSemanticEdit ?? false,
    );
  }

  void _reconcileWindowShadow() {
    _installWindowShadow(
      text: _inputState.value.text,
      globalStart: _inputState.globalUtf16Start,
      selection: _inputState.value.selection,
      platformOriginated: _inputTransactions.platformMutationActive,
    );
  }

  /// Records the exact value already installed by the platform. This is an
  /// input-authority transition, not a visual publication: the controller may
  /// normalize its own caret or wait for parser-certified geometry while the
  /// text service continues issuing callbacks from this provisional value.
  void _acceptPlatformWindowShadow(
    TextEditingValue value, {
    required int globalStart,
  }) {
    _installWindowShadow(
      text: value.text,
      globalStart: globalStart,
      selection: value.selection,
      platformOriginated: true,
    );
  }

  bool get _platformShadowMatchesCurrentInput => _platformInput.matches(
    text: _inputState.value.text,
    globalStart: _inputState.globalUtf16Start,
    selection: _inputState.value.selection,
  );

  void _installWindowShadow({
    required String text,
    required int globalStart,
    required TextSelection selection,
    required bool platformOriginated,
  }) {
    _platformInput.install(
      text: text,
      globalStart: globalStart,
      selection: selection,
      platformOriginated: platformOriginated,
      closed: _closed,
      faulted: _status == FlarkEditorStatus.faulted,
    );
  }

  /// A rejected active-connection callback mutates nothing: the connection
  /// retires with a typed reason and the unchanged authoritative window is
  /// re-exposed on a fresh connection epoch.
  void _resynchronize(FlarkInputResyncReason reason) {
    _inputTransactions.lateSemantic = null;
    if (_certificationDeferredInputActive) {
      _inputTransactions.discardPendingSemantic();
      _cancelCertificationDeferredInput();
    }
    var compositionEnded = false;
    if (_session.compositionActive) {
      _endCompositionHistoryGroup();
      _inputState.replaceValue(
        _inputState.value.copyWith(composing: TextRange.empty),
      );
      compositionEnded = true;
    }
    _platformInput.resynchronize(
      reason: reason,
      authoritativeValue: _inputState.value,
      globalStart: _inputState.globalUtf16Start,
    );
    if (compositionEnded) _scheduleParsingAfterInput();
    _publishSnapshot();
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
  }) => _platformInput.validateDeltaBatch(
    deltas,
    against: against,
    expectedTextSha256: expectedTextSha256,
    fallbackValue: _inputState.value,
  );

  static Future<FlarkEditorController> open(
    String source, {
    String? libraryPath,
    int historyBudgetBytes = 8 * 1024 * 1024,
  }) async {
    final document = await FlarkCoreDocument.open(
      source,
      libraryPath: libraryPath,
      historyBudgetBytes: historyBudgetBytes,
    );
    return _openDocument(document);
  }

  /// Opens a document from a raw UTF-8 byte stream through the streamed
  /// admission path (RFC 029 A3) without ever holding the complete source on
  /// the Dart side.
  ///
  /// The returned controller is live before [chunks] ends: it publishes the
  /// first parser-certified head viewport as soon as [continueParsing] (or
  /// the editor widget, which calls it) drives certification there, reports
  /// [FlarkEditorStatus.streaming] until the stream seals, and accepts
  /// literal edits against the admitted prefix throughout. Regions beyond
  /// certification present as pending exact source under the ordinary
  /// live-projection contract. [expectedBytes] declares a known stream
  /// length for the runtime to enforce; null declares an unknown-length
  /// stream that only the close of [chunks] ends.
  ///
  /// Requires a native library built with the `opening-session` cargo
  /// feature; other builds reject the streamed open here with the same
  /// typed [FlarkCoreNativeException] surface every failed open uses
  /// (probe availability first with [streamedOpenSupported]).
  static Future<FlarkEditorController> openUtf8Stream(
    Stream<Uint8List> chunks, {
    int? expectedBytes,
    String? libraryPath,
    int historyBudgetBytes = 8 * 1024 * 1024,
  }) async {
    // The hook fires on the owner isolate after each admission
    // acknowledgement, which happens before this future completes for the
    // first chunks; the holder lets those early acknowledgements find the
    // controller once it exists instead of forcing admission to wait.
    final host = _AdmissionHost();
    final document = await FlarkCoreDocument.openUtf8Stream(
      chunks,
      expectedBytes: expectedBytes,
      libraryPath: libraryPath,
      historyBudgetBytes: historyBudgetBytes,
      onOpeningProgress: host.onAdmission,
    );
    final controller = await _openDocument(document);
    host.attach(controller);
    return controller;
  }

  /// Opens [source] through the streamed admission path of [openUtf8Stream]:
  /// the string is encoded chunk-by-chunk at Unicode scalar boundaries, so
  /// no second complete UTF-8 copy of the document is allocated and the
  /// certified head becomes editable while the tail is still being
  /// admitted. [open] remains the ordinary buffered path.
  static Future<FlarkEditorController> openStreaming(
    String source, {
    String? libraryPath,
    int historyBudgetBytes = 8 * 1024 * 1024,
  }) async {
    final document = await FlarkCoreDocument.openStreaming(
      source,
      libraryPath: libraryPath,
      historyBudgetBytes: historyBudgetBytes,
    );
    return _openDocument(document);
  }

  /// Probes whether the loaded native library carries the streamed-open
  /// entry points (`opening-session` cargo feature builds), so applications
  /// can gate streamed-open affordances up front instead of surfacing a
  /// rejected open. The probe opens and immediately disposes one streamed
  /// session; the capability answer is decided at open, before any source
  /// is admitted.
  static Future<bool> streamedOpenSupported({String? libraryPath}) async {
    // An already-complete stream keeps the probe bounded: a library with the
    // entry points opens and seals an empty load immediately, and one
    // without rejects at the creation transaction. A stream that stayed open
    // would instead leave an unsupported library waiting for bytes it will
    // never be asked for.
    try {
      final document = await FlarkCoreDocument.openUtf8Stream(
        const Stream<Uint8List>.empty(),
        libraryPath: libraryPath,
      );
      await document.dispose();
      return true;
    } on FlarkCoreNativeException {
      return false;
    }
  }

  static Future<FlarkEditorController> _openDocument(
    FlarkCoreDocument document,
  ) async {
    final controller = FlarkEditorController._(document);
    await controller._refreshViewport(restoreInputWindow: true);
    await controller._session.setSelectionUtf16(
      controller._inputState.selectionBaseUtf16,
      controller._inputState.selectionExtentUtf16,
      adapterState: controller._selectionSnapshot(),
    );
    return controller;
  }

  Future<void> continueParsing() {
    _parseTimer?.cancel();
    _parseTimer = null;
    if (_closed ||
        (_document.isReady && _viewportState.semanticCurrent) ||
        _status == FlarkEditorStatus.faulted) {
      return Future<void>.value();
    }
    return _coordinator.runParser(_finishParsing);
  }

  Future<bool> nextViewportPage() {
    if (_closed || !canPageForward) return Future<bool>.value(false);
    return _coordinator.runPage(_loadNextViewportPage);
  }

  Future<bool> previousViewportPage() {
    if (_closed || !canPageBackward) return Future<bool>.value(false);
    return _coordinator.runPage(_loadPreviousViewportPage);
  }

  List<FlarkViewportRow> get rows => _viewportState.rows;

  /// The sole outward publication function. Every listener observes the exact
  /// immutable value installed here; no asynchronous effect notifies directly.
  void _publishSnapshot() {
    _snapshot = _captureEditorSnapshot();
    super.notifyListeners();
  }

  FlarkEditorSnapshot _captureEditorSnapshot() {
    final projector = _captureSurfaceProjector();
    final capturedRows = List<FlarkEditorSnapshotRow>.unmodifiable(
      _viewportState.rows.map((row) {
        return FlarkEditorSnapshotRow(
          row: row,
          sourceUtf16: projector.surfaceSourceRange(row),
          editingPresentations: List<FlarkSurfaceRow>.unmodifiable(
            projector.surfaceRowsFor(row),
          ),
          viewPresentations: List<FlarkSurfaceRow>.unmodifiable(
            projector.surfaceRowsFor(row, includeEditingState: false),
          ),
          taskToggleable: _toggleableTaskRow(row) != null,
        );
      }),
    );
    return FlarkEditorSnapshot(
      sequence: _coordinator.nextSnapshotSequence(),
      status: _status,
      lastError: _lastError,
      interactionGeneration: _interactionGeneration,
      revision: _document.revision,
      sourceGeneration: _publishedSourceGeneration,
      sourceByteLength: _document.sourceByteLength,
      sourceUtf16Length: _document.sourceUtf16Length,
      pendingEdits: _coordinator.pendingEdits,
      canUndo:
          !_coordinator.historyReplayPending &&
          (_session.canUndo || _coordinator.pendingEdits > 0),
      canRedo: !_coordinator.historyReplayPending && _session.canRedo,
      semanticsCurrent: _viewportState.semanticCurrent,
      viewportPageIndex: _viewportPager.pageIndex,
      canPageForward: _viewportPager.canPageForward(
        semanticsCurrent: _viewportState.semanticCurrent,
        viewport: _viewportState.viewport,
      ),
      canPageBackward:
          _viewportState.semanticCurrent && _viewportPager.canPageBackward,
      pendingTableNavigationLocked: pendingTableNavigationLocked,
      visibleUtf16Start: _viewportState.visibleUtf16Start,
      visibleSource: _viewportState.visibleSource,
      canonicalSelectionBaseUtf16: _inputState.selectionBaseUtf16,
      canonicalSelectionExtentUtf16: _inputState.selectionExtentUtf16,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      inputValue: portableEditorInputValue(_inputState.value),
      activeOrdinal: _inputState.activeOrdinal,
      crossRowSelection: _inputState.crossRowSelection,
      rows: capturedRows,
    );
  }

  FlarkSurfaceProjector _captureSurfaceProjector() =>
      _viewportState.captureSurfaceProjector(
        pendingPresentation: _pendingPresentation,
        inputGlobalUtf16Start: _inputState.globalUtf16Start,
        inputValue: portableEditorInputValue(_inputState.value),
        activeOrdinal: _inputState.activeOrdinal,
        selectionBaseUtf16: _inputState.selectionBaseUtf16,
        selectionExtentUtf16: _inputState.selectionExtentUtf16,
        crossRowSelection: _inputState.crossRowSelection,
      );

  FlarkSurfaceRow surfaceRow(
    FlarkViewportRow row, {
    bool includeEditingState = true,
  }) => _captureSurfaceProjector().surfaceRow(
    row,
    includeEditingState: includeEditingState,
  );

  /// Ordered framework-neutral presentations currently replacing one stale
  /// certified row. Most rows return one surface; a structural edit that
  /// temporarily creates mixed block semantics may return a small set.
  List<FlarkSurfaceRow> surfaceRowsFor(
    FlarkViewportRow row, {
    bool includeEditingState = true,
  }) => _captureSurfaceProjector().surfaceRowsFor(
    row,
    includeEditingState: includeEditingState,
  );

  /// Exact source extent currently owned by one rendered row.
  FlarkSourceRange surfaceSourceRange(FlarkViewportRow row) =>
      _captureSurfaceProjector().surfaceSourceRange(row);

  FlarkSurfaceRow neutralSurfaceRow({
    required int globalUtf16Start,
    required String text,
    required int ordinal,
    bool includeEditingState = true,
  }) => _captureSurfaceProjector().neutralSurfaceRow(
    globalUtf16Start: globalUtf16Start,
    text: text,
    ordinal: ordinal,
    includeEditingState: includeEditingState,
  );

  void activateRow(
    FlarkViewportRow row,
    int globalUtf16Offset, {
    int? selectionExtent,
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    // A pointer/navigation selection is a new editing-context decision even
    // when it resolves to the same numeric caret. Do not let one-shot
    // delete-to-empty continuation authority survive that explicit choice.
    _inputState.abandonInlineContinuation();
    _inputState.setSemanticEditActive(
      row.semanticCapabilities.supportsSemanticEdit,
    );
    _coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.dependency,
      FlarkPendingPresentationPart.paragraphGap,
      FlarkPendingPresentationPart.caretBoundary,
    });
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    _abandonOversizedSelection();
    final range = _mapViewportRange(_activationRange(row));
    final text = _sliceVisibleUtf16(range.start, range.end);
    final selectionRepresented = _activateWindow(
      text: text,
      sourceStart: range.start,
      caret: globalUtf16Offset,
      selectionExtent: selectionExtent,
      ordinal: row.ordinal,
      affinity: affinity,
    );
    if (selectionExtent == null || selectionRepresented) {
      unawaited(_installCanonicalSelection(_selectionSnapshot()));
    } else {
      unawaited(selectOversizedRangeUtf16(globalUtf16Offset, selectionExtent));
    }
  }

  void activateNeutralLine({
    required String text,
    required int globalUtf16Start,
    required int globalUtf16Offset,
    required int ordinal,
    int? selectionExtent,
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    _inputState.abandonInlineContinuation();
    _inputState.setSemanticEditActive(false);
    _coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.dependency,
    });
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    _abandonOversizedSelection();
    final selectionRepresented = _activateWindow(
      text: text,
      sourceStart: globalUtf16Start,
      caret: globalUtf16Offset,
      selectionExtent: selectionExtent,
      ordinal: -ordinal - 1,
      affinity: affinity,
    );
    if (selectionExtent == null || selectionRepresented) {
      unawaited(_installCanonicalSelection(_selectionSnapshot()));
    } else {
      unawaited(selectOversizedRangeUtf16(globalUtf16Offset, selectionExtent));
    }
  }

  void extendSelectionTo(int globalUtf16Offset, {int? activeOrdinal}) {
    _inputState.abandonInlineContinuation();
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    final local = globalUtf16Offset - _inputState.globalUtf16Start;
    final remainsInActiveWindow =
        !_inputState.crossRowSelection &&
        local >= 0 &&
        local <= _inputState.value.text.length &&
        (activeOrdinal == null || activeOrdinal == _inputState.activeOrdinal);
    if (remainsInActiveWindow) {
      _inputState.replaceValue(
        _inputState.value.copyWith(
          selection: TextSelection(
            baseOffset: _inputState.value.selection.baseOffset,
            extentOffset: local,
            affinity: _inputState.value.selection.affinity,
            isDirectional: _inputState.value.selection.isDirectional,
          ),
          composing: TextRange.empty,
        ),
      );
      _inputState.extendCanonicalSelection(globalUtf16Offset);
      unawaited(_installCanonicalSelection(_selectionSnapshot()));
      _publishCommandInputState();
      return;
    }

    // A parser-authored projection proof belongs to the row that published
    // it. Once selection ownership leaves that active input window, fail the
    // provisional surface closed instead of allowing it to follow the caret
    // into another row while recertification is pending.
    _coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.dependency,
    });

    final visibleEnd =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    final start = math.min(_inputState.selectionBaseUtf16, globalUtf16Offset);
    final end = math.max(_inputState.selectionBaseUtf16, globalUtf16Offset);
    if (end - start > _maximumInputCodeUnits ||
        start < _viewportState.visibleUtf16Start ||
        end > visibleEnd) {
      final exactBase = _inputState.selectionBaseUtf16;
      final targetOrdinal =
          activeOrdinal ?? _surfaceOrdinalAt(globalUtf16Offset);
      _inputState.markOversizedSelection(
        base: exactBase,
        extent: globalUtf16Offset,
        activeOrdinal: targetOrdinal,
      );
      _restoreCollapsedInputWindow(
        globalUtf16Offset,
        preferredOrdinal: _inputState.activeOrdinal,
      );
      _inputState.markOversizedSelection(
        base: exactBase,
        extent: globalUtf16Offset,
        activeOrdinal: _inputState.activeOrdinal,
      );
      _publishCommandInputState();
      unawaited(selectOversizedRangeUtf16(exactBase, globalUtf16Offset));
      return;
    }
    final selection = TextSelection(
      baseOffset: _inputState.selectionBaseUtf16 - start,
      extentOffset: globalUtf16Offset - start,
      affinity: _inputState.value.selection.affinity,
      isDirectional: true,
    );
    _inputState.replaceWindow(
      globalUtf16Start: start,
      value: TextEditingValue(
        text: _sliceVisibleUtf16(start, end),
        selection: selection,
      ),
    );
    _inputState.retargetActiveOrdinal(
      activeOrdinal ?? _surfaceOrdinalAt(globalUtf16Offset),
    );
    _inputState.setCrossRowSelection(!selection.isCollapsed);
    _inputState.extendCanonicalSelection(globalUtf16Offset);
    unawaited(_installCanonicalSelection(_selectionSnapshot()));
    _publishCommandInputState();
  }

  void applyDeltas(List<TextEditingDelta> deltas) {
    final timing = _inputTransactions.beginCallback();
    try {
      _applyDeltas(deltas);
    } finally {
      _inputTransactions.finishCallback(timing);
    }
  }

  void _applyDeltas(List<TextEditingDelta> deltas) {
    if (_coordinator.historyReplayPending) {
      notifyListeners();
      return;
    }
    if (_captureSemanticSuccessors(deltas)) return;
    if (_captureLateSemanticSuccessors(deltas)) return;
    if (_capturePlatformDeltasBehindCertification(deltas)) return;
    final rejection = _validateDeltaBatch(deltas);
    if (rejection != FlarkInputResyncReason.none) {
      _resynchronize(rejection);
      return;
    }
    var observedValue = _inputState.value;
    for (final delta in deltas) {
      observedValue = delta.apply(observedValue);
    }
    if (_isCompositionCancelValue(observedValue)) {
      unawaited(cancelComposition());
      return;
    }
    _inputTransactions.beginPlatformMutation();
    try {
      if (_inputState.oversizedSelection) {
        _applyOversizedPlatformDeltas(deltas);
        notifyListeners();
        return;
      }
      final platformNewline = _isPlatformNewlineMutation(deltas);
      if (platformNewline &&
          _deferCommandUntilCertification(
            FlarkDeferredInputCommand.insertNewline,
            provisionalAfter: observedValue,
          )) {
        _inputTransactions.markNewlineTextObserved();
        return;
      }
      if (platformNewline && _queuePlatformSemanticNewline(deltas)) {
        _acceptPlatformWindowShadow(
          observedValue,
          globalStart: _inputState.globalUtf16Start,
        );
        return;
      }
      final platformDeleteBackward = _isPlatformDeleteBackwardMutation(deltas);
      final platformSelectionSupersededByProjection =
          platformDeleteBackward &&
          _platformInput.shadowText == _inputState.value.text &&
          _platformInput.shadowSelection != null &&
          _platformInput.shadowSelection != _inputState.value.selection;
      if (platformSelectionSupersededByProjection) {
        // The text service can issue another Backspace before adopting a
        // parser-normalized caret. Its oldText is still current, but its raw
        // deletion range belongs to the superseded selection geometry. Treat
        // the callback as the same visible command at the canonical caret and
        // reassert that canonical window; applying the raw range can delete a
        // hidden table delimiter/padding byte or reject an otherwise live
        // input connection.
        _inputTransactions.markBackspaceTextObserved();
        _deleteBackward(
          allowSemantic: true,
          platformTiming: _inputTransactions.activeTiming,
        );
        notifyListeners();
        return;
      }
      if (platformDeleteBackward &&
          _deferCommandUntilCertification(
            FlarkDeferredInputCommand.deleteBackward,
            provisionalAfter: observedValue,
          )) {
        _inputTransactions.markBackspaceTextObserved();
        return;
      }
      if (platformDeleteBackward &&
          _queuePlatformSemanticDeleteBackward(deltas)) {
        _acceptPlatformWindowShadow(
          observedValue,
          globalStart: _inputState.globalUtf16Start,
        );
        return;
      }
      final observedMutation = deltas.length == 1
          ? _mutationFor(deltas.single)
          : null;
      if (observedMutation != null &&
          _isPlatformSelectedDeletion(observedMutation) &&
          _mutationTouchesOnlyHiddenProjection(observedMutation)) {
        // A platform selection can span exact source that the rendered
        // projection collapses away (a block's trailing newline is the common
        // case). Preserve that hidden source and synchronously correct the
        // provisional platform value. Resynchronizing drops liveness, while
        // accepting the splice would make invisible Markdown user-deletable.
        notifyListeners();
        return;
      }
      if (platformNewline) {
        _inputTransactions.markNewlineTextObserved();
        _acceptPlatformWindowShadow(
          observedValue,
          globalStart: _inputState.globalUtf16Start,
        );
        insertNewline();
        return;
      }
      if (platformDeleteBackward &&
          _mutationTouchesOnlyHiddenProjection(_mutationFor(deltas.single)!)) {
        // The text service still sees the exact source window and can report
        // Backspace from an offset that a newly certified projection has made
        // non-navigable. Interpret that callback as the visible Backspace
        // command at the adjacent legal caret; never install a one-character
        // deletion of an invisible marker.
        _inputTransactions.markBackspaceTextObserved();
        _acceptPlatformWindowShadow(
          observedValue,
          globalStart: _inputState.globalUtf16Start,
        );
        _deleteBackward(
          allowSemantic: true,
          platformTiming: _inputTransactions.activeTiming,
        );
        notifyListeners();
        return;
      }
      if (platformDeleteBackward) {
        _inputTransactions.markBackspaceTextObserved();
      }
      var finalValue = _inputState.value;
      var mutatingDeltas = 0;
      var typingInput = true;
      var publishOptimistically = true;
      for (final delta in deltas) {
        finalValue = delta.apply(finalValue);
        if (_mutationFor(delta) != null) {
          mutatingDeltas += 1;
          typingInput = typingInput && delta is TextEditingDeltaInsertion;
        }
      }
      if (mutatingDeltas == 0) {
        _adoptPlatformSelectionOnlyValue(finalValue);
      } else {
        final before = _inputState.value.text;
        final after = finalValue.text;
        final mutation = _differenceMutation(before, after);
        if (mutation == null) {
          _breakTypingHistoryGroup();
          _inputState.replaceValue(finalValue);
          _trackCompositionWithoutMutation(finalValue.composing);
          _updateGlobalSelection();
          unawaited(_installCanonicalSelection(_selectionSnapshot()));
          notifyListeners();
          return;
        }
        if (platformDeleteBackward &&
            _mutationTouchesOnlyHiddenProjection(mutation)) {
          // Repeated source characters can make the value-level difference
          // choose a different, but textually equivalent, range from the
          // platform delta (table padding is the common case). Classify the
          // effective mutation too: hidden Markdown is never user-deletable,
          // so preserve it and execute the visible Backspace command at the
          // controller's normalized caret.
          _inputTransactions.markBackspaceTextObserved();
          _acceptPlatformWindowShadow(
            finalValue,
            globalStart: _platformInput.shadowWindowStart,
          );
          _deleteBackward(
            allowSemantic: true,
            platformTiming: _inputTransactions.activeTiming,
          );
          notifyListeners();
          return;
        }
        final acceptance = _acceptMutation(
          mutation,
          selection: finalValue.selection,
          composing: finalValue.composing,
          typingInput: typingInput,
          fullValue: finalValue.text.length <= _maximumInputCodeUnits
              ? finalValue
              : null,
        );
        if (!acceptance.accepted) {
          _resynchronize(FlarkInputResyncReason.rangeOutOfWindow);
          return;
        }
        publishOptimistically = acceptance.publishOptimistically;
      }
      if (publishOptimistically) notifyListeners();
      // A projected edit may normalize the controller selection while the
      // text service still owns finalValue's provisional selection. Publish
      // the canonical correction first, then retain the exact observed shadow
      // so a same-burst callback is classified in the coordinates it actually
      // used. The next host-originated notification or adopted callback
      // reconciles the two without retiring a live connection.
      _acceptPlatformWindowShadow(
        finalValue,
        globalStart: _platformInput.shadowWindowStart,
      );
    } finally {
      _inputTransactions.endPlatformMutation();
    }
  }

  void updateEditingValue(TextEditingValue value) {
    final timing = _inputTransactions.beginCallback();
    try {
      _updateEditingValue(value);
    } finally {
      _inputTransactions.finishCallback(timing);
    }
  }

  void _updateEditingValue(TextEditingValue value) {
    if (_coordinator.historyReplayPending) {
      notifyListeners();
      return;
    }
    if (_isCompositionCancelValue(value)) {
      unawaited(cancelComposition());
      return;
    }
    if (_captureSemanticSuccessorValue(value)) return;
    if (_capturePlatformValueBehindCertification(value)) return;
    if (value.text == _inputState.value.text) {
      _inputTransactions.lateSemantic = null;
    }
    _inputTransactions.beginPlatformMutation();
    try {
      if (_inputState.oversizedSelection) {
        _updateOversizedEditingValue(value);
        notifyListeners();
        return;
      }
      final platformNewline = _isPlatformNewlineValue(value);
      if (platformNewline &&
          _deferCommandUntilCertification(
            FlarkDeferredInputCommand.insertNewline,
            provisionalAfter: value,
          )) {
        _inputTransactions.markNewlineTextObserved();
        return;
      }
      if (platformNewline && _queuePlatformSemanticNewlineValue(value)) {
        _acceptPlatformWindowShadow(
          value,
          globalStart: _inputState.globalUtf16Start,
        );
        return;
      }
      final platformDeleteBackward = _isPlatformDeleteBackwardValue(value);
      if (platformDeleteBackward &&
          _deferCommandUntilCertification(
            FlarkDeferredInputCommand.deleteBackward,
            provisionalAfter: value,
          )) {
        _inputTransactions.markBackspaceTextObserved();
        return;
      }
      if (platformDeleteBackward &&
          _queuePlatformSemanticDeleteBackwardValue(value)) {
        _acceptPlatformWindowShadow(
          value,
          globalStart: _inputState.globalUtf16Start,
        );
        return;
      }
      final observedMutation = _differenceMutation(
        _inputState.value.text,
        value.text,
      );
      if (observedMutation != null &&
          _isPlatformSelectedDeletion(observedMutation) &&
          _mutationTouchesOnlyHiddenProjection(observedMutation)) {
        notifyListeners();
        return;
      }
      if (platformNewline) {
        _inputTransactions.markNewlineTextObserved();
        _acceptPlatformWindowShadow(
          value,
          globalStart: _inputState.globalUtf16Start,
        );
        _insertNewline(
          allowSemantic: true,
          platformTiming: _inputTransactions.activeTiming,
        );
        return;
      }
      if (platformDeleteBackward &&
          _mutationTouchesOnlyHiddenProjection(
            _differenceMutation(_inputState.value.text, value.text)!,
          )) {
        _inputTransactions.markBackspaceTextObserved();
        _acceptPlatformWindowShadow(
          value,
          globalStart: _inputState.globalUtf16Start,
        );
        _deleteBackward(
          allowSemantic: true,
          platformTiming: _inputTransactions.activeTiming,
        );
        notifyListeners();
        return;
      }
      if (platformDeleteBackward) {
        _inputTransactions.markBackspaceTextObserved();
      }
      _updateEditingValueFromPlatform(value);
    } finally {
      _inputTransactions.endPlatformMutation();
    }
  }

  void _updateEditingValueFromPlatform(TextEditingValue value) {
    if (value.text == _inputState.value.text) {
      _adoptPlatformSelectionOnlyValue(value);
      notifyListeners();
      return;
    }
    final mutation = _differenceMutation(_inputState.value.text, value.text);
    if (mutation == null) {
      _adoptPlatformSelectionOnlyValue(value);
      notifyListeners();
      return;
    }
    final selection = _inputState.value.selection;
    final typingInput =
        selection.isCollapsed &&
        mutation.start == selection.extentOffset &&
        mutation.end == selection.extentOffset &&
        mutation.replacement.isNotEmpty;
    final acceptance = _acceptMutation(
      mutation,
      selection: value.selection,
      composing: value.composing,
      fullValue: value.text.length <= _maximumInputCodeUnits ? value : null,
      typingInput: typingInput,
    );
    if (!acceptance.accepted) {
      _resynchronize(FlarkInputResyncReason.rangeOutOfWindow);
      return;
    }
    if (acceptance.publishOptimistically) notifyListeners();
    // Notification can publish a parser-normalized canonical caret. Retain
    // the exact selection that produced this full-value callback afterward,
    // because another callback already queued by the text service still uses
    // those provisional coordinates.
    _acceptPlatformWindowShadow(
      value,
      globalStart: _platformInput.shadowWindowStart,
    );
  }

  void _applyOversizedPlatformDeltas(List<TextEditingDelta> deltas) {
    var finalValue = _inputState.value;
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
      _adoptPlatformSelectionOnlyValue(finalValue);
      return;
    }
    final mutation = _differenceMutation(
      _inputState.value.text,
      finalValue.text,
    );
    if (mutation == null) {
      _adoptPlatformSelectionOnlyValue(finalValue);
      return;
    }
    _markOversizedPlatformCommand(mutation);
    _replaceSelection(mutation.replacement, typingInput: typingInput);
  }

  void _updateOversizedEditingValue(TextEditingValue value) {
    if (value.text == _inputState.value.text) {
      _adoptPlatformSelectionOnlyValue(value);
      return;
    }
    final mutation = _differenceMutation(_inputState.value.text, value.text);
    if (mutation == null) {
      _adoptPlatformSelectionOnlyValue(value);
      return;
    }
    _markOversizedPlatformCommand(mutation);
    _replaceSelection(mutation.replacement, typingInput: false);
  }

  void _markOversizedPlatformCommand(FlarkTextMutation mutation) {
    final caret = _inputState.value.selection.extentOffset;
    if (mutation.replacement == '\n') {
      _inputTransactions.markNewlineTextObserved();
    }
    if (mutation.replacement.isEmpty &&
        mutation.start < mutation.end &&
        mutation.end == caret) {
      _inputTransactions.markBackspaceTextObserved();
    }
  }

  void _adoptPlatformSelectionOnlyValue(TextEditingValue value) {
    _breakTypingHistoryGroup();
    value = _normalizeProjectedSelection(value);
    if (!_session.compositionActive && value.composing.isValid) {
      _rememberCompositionInputBase(_inputState.value);
    }
    _inputState.replaceValue(value);
    _trackCompositionWithoutMutation(value.composing);
    if (_inputState.oversizedSelection) {
      // The platform only sees a collapsed surrogate for an exact selection
      // that is larger than the bounded input window. Echoes and non-text
      // updates may synchronize that surrogate, but must never retarget the
      // Core-owned selection that the next mutation will consume.
      return;
    }
    _updateGlobalSelection();
    unawaited(_installCanonicalSelection(_selectionSnapshot()));
  }

  void replaceSelection(String replacement) =>
      _replaceSelection(replacement, typingInput: false);

  void _replaceSelection(
    String replacement, {
    required bool typingInput,
    FlarkPlatformInputTiming? platformTiming,
    bool publish = true,
  }) {
    if (_deferSemanticSuccessor(
      replacement: replacement,
      platformTiming: platformTiming,
    )) {
      return;
    }
    if (!typingInput) {
      _breakTypingHistoryGroup();
      _endCompositionHistoryGroup();
    }
    if (_inputState.oversizedSelection) {
      _replaceOversizedSelection(replacement);
      if (publish) _publishCommandInputState();
      return;
    }
    final selection = _inputState.value.selection;
    final start = math.min(selection.baseOffset, selection.extentOffset);
    final end = math.max(selection.baseOffset, selection.extentOffset);
    final caret = start + replacement.length;
    final acceptance = _acceptMutation(
      FlarkTextMutation(start, end, replacement),
      selection: TextSelection.collapsed(offset: caret),
      composing: TextRange.empty,
      typingInput: typingInput,
      platformTiming: platformTiming,
    );
    if (publish && acceptance.accepted) {
      // Command-originated edits have not already advanced the platform text
      // service. Publish the authoritative input window even when paint must
      // retain its prior certified surface until parsing catches up.
      _publishCommandInputState();
    }
  }

  /// Installs a canonical anchored selection larger than the bounded input
  /// window can represent. The platform sees only a collapsed active-extent
  /// surrogate; typing, paste, or deletion against it replaces the complete
  /// exact global selection atomically through the anchor-resolved range.
  Future<int> selectOversizedRangeUtf16(int base, int extent) async {
    _inputState.abandonInlineContinuation();
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    final length = sourceUtf16Length;
    final clampedBase = base.clamp(0, length);
    final clampedExtent = extent.clamp(0, length);
    final exactSelection = FlarkEditorSelectionSnapshot(
      TextSelection(baseOffset: clampedBase, extentOffset: clampedExtent),
      _inputState.activeOrdinal,
    );
    final generation = await _installCanonicalSelection(exactSelection);
    _inputState.markOversizedSelection(
      base: clampedBase,
      extent: clampedExtent,
      activeOrdinal: _inputState.activeOrdinal,
    );
    await _restoreHistorySelection(
      FlarkEditorSelectionSnapshot(
        TextSelection.collapsed(offset: clampedExtent),
        null,
      ),
    );
    _inputState.markOversizedSelection(
      base: clampedBase,
      extent: clampedExtent,
      activeOrdinal: _inputState.activeOrdinal,
    );
    notifyListeners();
    return generation;
  }

  /// Reads the complete authoritative Markdown source after every edit
  /// already accepted by this controller has settled in the Core session.
  Future<String> readSource() async {
    await _waitForMutationTail();
    return _document.readSource();
  }

  /// Follows successors that are admitted while an earlier semantic receipt
  /// is completing. Capturing one Future is insufficient because a native
  /// not-applicable result can enqueue its literal fallback from inside that
  /// completion.
  Future<void> _waitForMutationTail() async {
    while (true) {
      if (_coordinator.pendingEdits == 0 &&
          _coordinator.pendingSessionOnlyCommands == 0 &&
          _certificationDeferredInputPromotion == null) {
        return;
      }
      final observedEdit = _coordinator.editTail;
      final observedAdoption = _coordinator.sourceEditAdoptionTail;
      final observedDeferred = _certificationDeferredInputPromotion;
      await Future.wait([
        observedEdit,
        observedAdoption,
        if (observedDeferred != null) observedDeferred.future,
      ]);
      if (identical(observedEdit, _coordinator.editTail) &&
          identical(observedAdoption, _coordinator.sourceEditAdoptionTail) &&
          identical(observedDeferred, _certificationDeferredInputPromotion)) {
        return;
      }
    }
  }

  /// Deterministic test/debug barrier for the complete serialized mutation
  /// tail. Unlike polling [pendingEdits], this also waits for selection-only,
  /// history, and composition commands ordered through the same tail.
  @visibleForTesting
  Future<void> debugWaitForMutationSettled() => _waitForMutationTail();

  /// Deterministic test/debug barrier for a current-revision presentation.
  /// Active composition intentionally stops at mutation quiescence because
  /// parser certification is pinned until the composition ends.
  @visibleForTesting
  Future<void> debugWaitForPresentationSettled() async {
    await _waitForMutationTail();
    if (_session.compositionActive) return;
    if (_document.isOpening) {
      // A streamed open's parse task runs until the stream seals, so
      // awaiting it here would turn a presentation barrier into a
      // wait-for-the-whole-load. The settled presentation mid-load is the
      // published certified head for the current revision — including the
      // recertification an edit during admission produces.
      unawaited(continueParsing());
      while (_openingPublishedRevision != revision &&
          _document.isOpening &&
          !_closed &&
          !_session.compositionActive) {
        await (_openingPublication ??= Completer<void>()).future;
      }
      await _waitForMutationTail();
      return;
    }
    await continueParsing();
    final pageTask = _coordinator.pageTask;
    if (pageTask != null) await pageTask;
    await _waitForMutationTail();
  }

  /// Waits for parser authority that can safely complete one atomic edit
  /// publication at [generation]. A buffered document converges to Ready.
  /// A streamed-open document cannot do that until transport ends, so it
  /// instead waits for the current revision's certified head: every editable
  /// opening row is drawn from that same bounded head window.
  ///
  /// Keeping this distinction in one helper prevents command completion,
  /// history, and composition paths from accidentally turning "recertify the
  /// edited row" into "wait for the user to finish loading the document".
  Future<void> _awaitEditPublicationCertification(
    int generation, {
    required bool allowExactPending,
  }) async {
    while (_document.isOpening &&
        generation == _editGeneration &&
        !_closed &&
        !_session.compositionActive) {
      await _document.pump(workUnits: 512);
      if (!_document.isOpening ||
          generation != _editGeneration ||
          _closed ||
          _session.compositionActive) {
        break;
      }
      final probe = await _document.queryViewport(
        endByte: math.min(sourceByteLength, _openingHeadProbeBytes),
        maxRows: _viewportRowsPerPage,
      );
      final exactPendingWithoutPriorSemantics =
          allowExactPending &&
          probe.provesExactPendingDocument(
            documentRevision: revision,
            documentSourceByteLength: sourceByteLength,
            documentSourceUtf16Length: sourceUtf16Length,
          );
      final certified =
          probe.revision == revision &&
          ((probe.isCertified && probe.rows.isNotEmpty) ||
              probe.provesExactEmptyDocument(
                documentRevision: revision,
                documentSourceByteLength: sourceByteLength,
                documentSourceUtf16Length: sourceUtf16Length,
              ) ||
              exactPendingWithoutPriorSemantics);
      if (probe.continuation != 0) {
        await _document.releaseViewportContinuation(probe);
      }
      if (certified) return;
    }
    while (!_document.isReady &&
        generation == _editGeneration &&
        !_closed &&
        !_session.compositionActive) {
      await _document.pump(workUnits: 512);
    }
  }

  /// Installs the authority admitted by
  /// [_awaitEditPublicationCertification], then revalidates the installed
  /// viewport against the source that exists after the refresh. Opening
  /// appends intentionally preserve the edit revision, so generation and
  /// revision checks alone cannot prove that a pre-refresh pending viewport
  /// still covers the complete source.
  Future<void> _refreshEditPublicationAfterCertification(
    int generation, {
    required bool restoreInputWindow,
    Future<void> Function()? prepareForRefresh,
  }) async {
    final hadNoPriorSemanticRows = _viewportState.rows.isEmpty;
    while (generation == _editGeneration && !_closed) {
      await _awaitEditPublicationCertification(
        generation,
        allowExactPending: hadNoPriorSemanticRows,
      );
      if (generation != _editGeneration || _closed) return;

      final prepare = prepareForRefresh;
      if (prepare != null) await prepare();
      if (generation != _editGeneration || _closed) return;

      await _refreshViewport(
        restoreInputWindow: restoreInputWindow,
        expectedEditGeneration: generation,
        ensureActiveInputVisible: true,
        publish: false,
      );
      if (generation != _editGeneration || _closed) return;
      if (_document.isOpening &&
          _recordOpeningExactPublicationIfProven(
            hadNoPriorSemanticRows: hadNoPriorSemanticRows,
          )) {
        return;
      }
      if (!_document.isOpening &&
          _document.isReady &&
          _installedViewportProvesEditPublication(allowExactPending: false)) {
        return;
      }
      // No await may separate either proof above from its phase decision. A
      // streamed append can preserve revision while invalidating exact
      // pending coverage, and a stream seal can make a pending result
      // inadmissible even if the document becomes Ready immediately after the
      // query that produced it. In either case, loop through the new phase and
      // install fresh authority.
    }
  }

  /// Records a controller-owned refresh as the current streamed publication.
  /// The long-lived opening parse task deliberately refuses to publish while
  /// an edit is pending; the completing edit therefore owns this bookkeeping.
  void _recordOpeningEditPublication() {
    if (!_document.isOpening) return;
    _coordinator.recordOpeningPublication(revision);
    _openingPublication?.complete();
    _openingPublication = null;
  }

  /// Records a streamed publication only when the installed viewport proves
  /// the current source. Pending-neutral source may do so solely when the
  /// editor had no older semantic rows to retain; this prevents a live tail
  /// from replacing rendered Markdown with raw source while still keeping an
  /// initially empty document writable before transport seals.
  bool _recordOpeningExactPublicationIfProven({
    required bool hadNoPriorSemanticRows,
  }) {
    if (!_document.isOpening) return false;
    final proven = _installedViewportProvesEditPublication(
      allowExactPending: hadNoPriorSemanticRows,
    );
    if (proven) _recordOpeningEditPublication();
    return proven;
  }

  /// Whether the viewport actually installed in the controller proves a safe
  /// edit publication for the document's current phase and source lengths.
  /// Pending-neutral source is never authority after a stream seals.
  bool _installedViewportProvesEditPublication({
    required bool allowExactPending,
  }) {
    final viewport = _viewportState.viewport;
    if (viewport == null) return false;
    return viewport.provesEditPublication(
      documentRevision: revision,
      documentSourceByteLength: sourceByteLength,
      documentSourceUtf16Length: sourceUtf16Length,
      documentOpening: _document.isOpening,
      documentReady: _document.isReady,
      allowExactPending: allowExactPending,
    );
  }

  Future<FlarkSemanticTarget?> querySemanticTarget(FlarkInlineFact fact) =>
      _coordinator.afterEdits(() => _document.querySemanticTarget(fact));

  /// A user activation abandons the platform surrogate. The immediately
  /// queued ordinary selection replaces the canonical anchors in order.
  void _abandonOversizedSelection() {
    if (!_inputState.oversizedSelection) return;
    _inputState.clearOversizedSelection();
  }

  /// Resolves the exact core-owned selection after every queued edit or host
  /// selection replacement ahead of it has completed.
  Future<FlarkCoreSelectionSnapshot?> resolveCanonicalSelection() async {
    await _waitForMutationTail();
    return _session.resolveSelection();
  }

  /// Reads the complete selected source even when the platform input window
  /// carries only an active-extent surrogate.
  Future<String?> readSelectedText() async {
    if (!_inputState.crossRowSelection && !_inputState.oversizedSelection) {
      return selectedText;
    }
    final selection = await resolveCanonicalSelection();
    if (selection == null || selection.isCollapsed) return null;
    final start = math.min(selection.base, selection.extent);
    final end = math.max(selection.base, selection.extent);
    return _document.readSourceUtf16Range(start, end);
  }

  void _replaceOversizedSelection(String replacement) {
    final resolved = _selectionSnapshot();
    _inputState.clearOversizedSelection();
    final start = math.min(
      resolved.selection.baseOffset,
      resolved.selection.extentOffset,
    );
    final end = math.max(
      resolved.selection.baseOffset,
      resolved.selection.extentOffset,
    );
    final caret = start + replacement.length;
    final beforeSelection = FlarkEditorSelectionSnapshot(
      TextSelection(
        baseOffset: resolved.selection.baseOffset,
        extentOffset: resolved.selection.extentOffset,
        affinity: resolved.selection.affinity,
        isDirectional: resolved.selection.isDirectional,
      ),
      null,
    );
    final afterSelection = FlarkEditorSelectionSnapshot(
      TextSelection.collapsed(offset: caret),
      null,
    );
    _retainOptimisticRefreshAnchor(start, deriveFromInput: true);
    _inputState.setCanonicalSelection(caret, caret);
    _inputState.setCrossRowSelection(false);
    _inputState.retargetActiveOrdinal(_surfaceOrdinalAt(start));
    var windowStart = math.max(0, replacement.length - _maximumInputCodeUnits);
    final alignedWindow = scalarAlignedUtf16Window(
      replacement,
      windowStart,
      replacement.length,
    );
    windowStart = alignedWindow.start;
    final windowLength = alignedWindow.end - windowStart;
    _inputState.replaceWindow(
      globalUtf16Start: start + windowStart,
      value: TextEditingValue(
        text: replacement.substring(windowStart, alignedWindow.end),
        selection: TextSelection.collapsed(offset: windowLength),
      ),
    );
    _queueNativeEdit(
      start,
      end,
      replacement,
      beforeSelection: beforeSelection,
      afterSelection: afterSelection,
      coalesceTyping: false,
      compositionHistoryGroup: null,
      restoreSelectionAfterCommit: true,
    );
  }

  void deleteBackward() => _deleteBackward(allowSemantic: true);

  void _deleteBackward({
    required bool allowSemantic,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    if (allowSemantic &&
        _deferSemanticSuccessor(
          command: FlarkDeferredInputCommand.deleteBackward,
          platformTiming: platformTiming,
        )) {
      return;
    }
    if (allowSemantic &&
        _deferCommandUntilCertification(
          FlarkDeferredInputCommand.deleteBackward,
          platformTiming: platformTiming,
        )) {
      return;
    }
    if (_inputState.oversizedSelection) {
      replaceSelection('');
      return;
    }
    var selection = _inputState.value.selection;
    if (!selection.isCollapsed) {
      replaceSelection('');
      return;
    }
    if (allowSemantic &&
        _queueSemanticDeleteBackward(
          selection.extentOffset,
          platformTiming: platformTiming,
        )) {
      return;
    }
    if (_normalizeProjectedCommandSelection()) {
      selection = _inputState.value.selection;
      if (allowSemantic &&
          _queueSemanticDeleteBackward(
            selection.extentOffset,
            platformTiming: platformTiming,
          )) {
        return;
      }
    }
    if (_deleteProjectedVisible(
      backward: true,
      platformTiming: platformTiming,
    )) {
      return;
    }
    if (selection.extentOffset == 0) return;
    final end = selection.extentOffset;
    final cluster = FlarkCoreGraphemePolicy.previousClusterRange(
      _inputState.value.text,
      end,
    );
    if (cluster == null) return;
    _deleteLiteralCluster(cluster.$1, end, platformTiming: platformTiming);
  }

  void deleteForward() => _deleteForward(allowSemantic: true);

  void _deleteForward({
    required bool allowSemantic,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    if (allowSemantic &&
        _deferSemanticSuccessor(
          command: FlarkDeferredInputCommand.deleteForward,
          platformTiming: platformTiming,
        )) {
      return;
    }
    if (allowSemantic &&
        _deferCommandUntilCertification(
          FlarkDeferredInputCommand.deleteForward,
          platformTiming: platformTiming,
        )) {
      return;
    }
    if (_inputState.oversizedSelection) {
      replaceSelection('');
      return;
    }
    final selection = _inputState.value.selection;
    if (!selection.isCollapsed) {
      replaceSelection('');
      return;
    }
    final globalCaret = _inputState.globalUtf16Start + selection.extentOffset;
    final paragraphGap = _pendingPresentation.paragraphGap;
    final caretBoundary = _pendingPresentation.caretBoundary;
    final boundaryRowEnd =
        paragraphGap?.rowEndUtf16 ?? caretBoundary?.rowEndUtf16;
    final boundaryEnd = paragraphGap != null
        ? _committedGapEnd(paragraphGap)
        : caretBoundary != null
        ? _committedCaretBoundaryEnd(caretBoundary)
        : null;
    final editorBoundaryOwnsCaret =
        boundaryRowEnd != null &&
        boundaryEnd != null &&
        boundaryRowEnd <= globalCaret &&
        globalCaret <= boundaryEnd;
    if (editorBoundaryOwnsCaret &&
        _viewportState.rows.any(
          (row) => surfaceSourceRange(row).start == globalCaret,
        )) {
      // The parser-authored paragraph gap still owns this boundary even when
      // a fresh viewport also begins its successor row at the same source
      // offset. The bounded platform window can include successor text, but
      // Delete must not cross out of the editor-owned empty block.
      return;
    }
    if (allowSemantic &&
        _queueSemanticDeleteForward(
          selection.extentOffset,
          platformTiming: platformTiming,
        )) {
      return;
    }
    if (_deleteProjectedVisible(
      backward: false,
      platformTiming: platformTiming,
    )) {
      return;
    }
    final start = selection.extentOffset;
    if (start == _inputState.value.text.length) return;
    final cluster = FlarkCoreGraphemePolicy.nextClusterRange(
      _inputState.value.text,
      start,
    );
    if (cluster == null) return;
    _deleteLiteralCluster(start, cluster.$2, platformTiming: platformTiming);
  }

  void _deleteLiteralCluster(
    int start,
    int end, {
    FlarkPlatformInputTiming? platformTiming,
  }) {
    _breakTypingHistoryGroup();
    _endCompositionHistoryGroup();
    final acceptance = _acceptMutation(
      FlarkTextMutation(start, end, ''),
      selection: TextSelection.collapsed(offset: start),
      composing: TextRange.empty,
      typingInput: false,
      platformTiming: platformTiming,
      // A rendered row boundary has no glyph, but an explicit Backspace or
      // Delete command still owns its adjacent line ending. Other hidden
      // source (Markdown delimiters) remains protected by projection facts.
      editabilityProven: _inputState.value.text
          .substring(start, end)
          .codeUnits
          .every((unit) => unit == 0x0a || unit == 0x0d),
    );
    if (acceptance.accepted) {
      // Command-originated edits must synchronize the platform input window
      // independently of whether their new rendered surface is publishable.
      _publishCommandInputState();
    }
  }

  void insertNewline() => _insertNewline(allowSemantic: true);

  void _insertNewline({
    required bool allowSemantic,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    if (allowSemantic &&
        _deferSemanticSuccessor(
          command: FlarkDeferredInputCommand.insertNewline,
          platformTiming: platformTiming,
        )) {
      return;
    }
    if (allowSemantic &&
        _deferCommandUntilCertification(
          FlarkDeferredInputCommand.insertNewline,
          platformTiming: platformTiming,
        )) {
      return;
    }
    final selection = _inputState.value.selection;
    if (allowSemantic &&
        selection.isCollapsed &&
        _queueSemanticParagraphBreak(
          selection.extentOffset,
          platformTiming: platformTiming,
        )) {
      return;
    }
    _replaceSelection('\n', typingInput: false, platformTiming: platformTiming);
  }

  bool _isPlatformNewlineMutation(List<TextEditingDelta> deltas) {
    return _platformInput.isNewlineDeltaBatch(
      deltas,
      currentValue: _inputState.value,
    );
  }

  bool _queuePlatformSemanticNewline(List<TextEditingDelta> deltas) {
    _inputTransactions.lateSemantic = null;
    final provisionalMutation = _mutationFor(deltas.single)!;
    final provisionalAfter = deltas.single.apply(_inputState.value);
    _inputTransactions.pendingSemantic = FlarkPendingSemanticInput(
      base: _inputState.value,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      initialCallbackStartedEpochMicros:
          _inputTransactions.activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: _inputTransactions.activeTiming,
      provisionalMutation: provisionalMutation,
      provisionalAfter: provisionalAfter,
    );
    _inputTransactions.markNewlineTextObserved();
    final queued = _queueSemanticParagraphBreak(
      _inputState.value.selection.extentOffset,
      platformTiming: _inputTransactions.activeTiming,
    );
    if (!queued) {
      _inputTransactions.discardPendingSemantic();
      _inputTransactions.clearNewlineTextObservation();
    }
    return queued;
  }

  bool _isPlatformNewlineValue(TextEditingValue value) {
    return _platformInput.isNewlineValue(
      currentValue: _inputState.value,
      observedValue: value,
    );
  }

  bool _queuePlatformSemanticNewlineValue(TextEditingValue value) {
    _inputTransactions.lateSemantic = null;
    final selection = _inputState.value.selection;
    final provisionalMutation = FlarkTextMutation(
      math.min(selection.baseOffset, selection.extentOffset),
      math.max(selection.baseOffset, selection.extentOffset),
      '\n',
    );
    _inputTransactions.pendingSemantic = FlarkPendingSemanticInput(
      base: _inputState.value,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      initialCallbackStartedEpochMicros:
          _inputTransactions.activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: _inputTransactions.activeTiming,
      provisionalMutation: provisionalMutation,
      provisionalAfter: value,
    );
    _inputTransactions.markNewlineTextObserved();
    final queued = _queueSemanticParagraphBreak(
      _inputState.value.selection.extentOffset,
      platformTiming: _inputTransactions.activeTiming,
    );
    if (!queued) {
      _inputTransactions.discardPendingSemantic();
      _inputTransactions.clearNewlineTextObservation();
    }
    return queued;
  }

  bool _isPlatformDeleteBackwardMutation(List<TextEditingDelta> deltas) {
    return _platformInput.isDeleteBackwardDeltaBatch(
      deltas,
      currentValue: _inputState.value,
    );
  }

  bool _isPlatformSelectedDeletion(FlarkTextMutation mutation) {
    return _platformInput.isSelectedDeletion(
      mutation,
      currentSelection: _inputState.value.selection,
    );
  }

  bool _queuePlatformSemanticDeleteBackward(List<TextEditingDelta> deltas) {
    return _queueObservedPlatformSemanticDeleteBackward(
      provisionalMutation: _mutationFor(deltas.single)!,
      provisionalAfter: deltas.single.apply(_inputState.value),
    );
  }

  bool _isPlatformDeleteBackwardValue(TextEditingValue value) {
    return _platformInput.isDeleteBackwardValue(
      currentValue: _inputState.value,
      observedValue: value,
    );
  }

  bool _queuePlatformSemanticDeleteBackwardValue(TextEditingValue value) {
    return _queueObservedPlatformSemanticDeleteBackward(
      provisionalMutation: _selectionObservedMutation(
        _inputState.value,
        value,
      )!,
      provisionalAfter: value,
    );
  }

  bool _queueObservedPlatformSemanticDeleteBackward({
    required FlarkTextMutation provisionalMutation,
    required TextEditingValue provisionalAfter,
  }) {
    _inputTransactions.lateSemantic = null;
    _inputTransactions.pendingSemantic = FlarkPendingSemanticInput(
      base: _inputState.value,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      initialCallbackStartedEpochMicros:
          _inputTransactions.activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: _inputTransactions.activeTiming,
      provisionalMutation: provisionalMutation,
      provisionalAfter: provisionalAfter,
    );
    _inputTransactions.markBackspaceTextObserved();
    final queued = _queueSemanticDeleteBackward(
      _inputState.value.selection.extentOffset,
      platformTiming: _inputTransactions.activeTiming,
    );
    if (!queued) {
      _inputTransactions.discardPendingSemantic();
      _inputTransactions.clearBackspaceTextObservation();
    }
    return queued;
  }

  /// Adopts a platform action only when no preceding text observation already
  /// carried the newline. macOS deliberately emits both for one Return.
  void observePlatformNewlineAction({
    bool textObservationAlreadyApplied = false,
  }) {
    if (_inputTransactions.consumeNewlineAction(
      textObservationAlreadyApplied: textObservationAlreadyApplied,
    )) {
      return;
    }
    _runPlatformAction(
      (timing) => _insertNewline(allowSemantic: true, platformTiming: timing),
    );
  }

  /// Adopts a selector only when no preceding text observation already
  /// carried the same Backspace. Desktop embedders may emit both; mobile
  /// generally supplies only the deletion delta or full value.
  void observePlatformDeleteBackwardAction({
    bool textObservationAlreadyApplied = false,
  }) {
    if (_inputTransactions.consumeBackspaceSelector(
      textObservationAlreadyApplied: textObservationAlreadyApplied,
    )) {
      return;
    }
    _runPlatformAction(
      (timing) => _deleteBackward(allowSemantic: true, platformTiming: timing),
    );
  }

  void _runPlatformAction(void Function(FlarkPlatformInputTiming) action) {
    final timing = _inputTransactions.beginCallback();
    try {
      action(timing);
    } finally {
      _inputTransactions.finishCallback(timing);
    }
  }

  bool _captureSemanticSuccessors(List<TextEditingDelta> deltas) {
    final pending = _inputTransactions.pendingSemantic;
    if (pending == null) return false;
    if (!_reserveSemanticSuccessor(pending)) return true;
    final before = pending.provisionalTail;
    final rejection = _validateDeltaBatch(
      deltas,
      against: before,
      expectedTextSha256: flarkWindowTextSha256(before.text),
    );
    if (rejection != FlarkInputResyncReason.none) {
      _inputTransactions.discardPendingSemantic();
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
    final logical = _logicalSemanticSuccessor(
      before,
      after,
      observedMutation: deltas.length == 1 ? _mutationFor(deltas.single) : null,
    );
    if (logical != null) {
      pending.successors.add(
        _inputTransactions.reclassifyAfterCertification(logical),
      );
      pending.provisionalTail = after;
      _markObservedPlatformCommand(logical);
      _inputTransactions.observePendingSuccessors(pending);
      _acceptPlatformWindowShadow(
        after,
        globalStart: pending.inputGlobalUtf16Start,
      );
      return true;
    }
    if (pending.successors.isNotEmpty &&
        pending.successors.last is FlarkDeferredInputSuccessor) {
      _inputTransactions.discardPendingSemantic();
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    pending.successors.add(
      FlarkProvisionalInputBatch(
        before: before,
        after: after,
        typingInput: typingInput,
        platformTiming: _inputTransactions.activeTiming,
      ),
    );
    pending.provisionalTail = after;
    _inputTransactions.observePendingSuccessors(pending);
    _acceptPlatformWindowShadow(
      after,
      globalStart: pending.inputGlobalUtf16Start,
    );
    return true;
  }

  bool _captureLateSemanticSuccessors(List<TextEditingDelta> deltas) {
    final late = _inputTransactions.lateSemantic;
    if (late == null) return false;
    if (_platformShadowMatchesCurrentInput) {
      // The text service has adopted the committed source and canonical
      // selection. A later callback with that serialized oldText belongs to
      // the current row, even when the predecessor's provisional tail has the
      // same text but a pre-reconciliation caret.
      _inputTransactions.lateSemantic = null;
      return false;
    }
    final before = late.provisionalTail;
    if (before.text == _inputState.value.text &&
        before.selection == _inputState.value.selection &&
        before.composing == _inputState.value.composing) {
      // The text service has adopted the committed window exactly. Keeping
      // the predecessor's provisional lineage alive would make a fresh
      // Return look like an old successor and lose its own provisional
      // splice. Rapid typing would then replay that newline after the new
      // semantic receipt. Let the ordinary lane establish a fresh barrier.
      _inputTransactions.lateSemantic = null;
      return false;
    }
    final rejection = _validateDeltaBatch(
      deltas,
      against: before,
      expectedTextSha256: flarkWindowTextSha256(before.text),
    );
    if (rejection != FlarkInputResyncReason.none) {
      // The platform has adopted the committed window. Let the ordinary lane
      // validate this callback against that current window.
      _inputTransactions.lateSemantic = null;
      return false;
    }
    if (late.successorCount >= _maximumSemanticSuccessors) {
      _inputTransactions.lateSemantic = null;
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
    final logical = _logicalSemanticSuccessor(
      before,
      after,
      observedMutation: deltas.length == 1 ? _mutationFor(deltas.single) : null,
    );
    final holder = FlarkPendingSemanticInput(
      base: before,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      initialCallbackStartedEpochMicros:
          _inputTransactions.activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: _inputTransactions.activeTiming,
      provisionalAfter: before,
    );
    if (logical != null) {
      holder.successors.add(logical);
      _markObservedPlatformCommand(logical);
    } else {
      holder.successors.add(
        FlarkProvisionalInputBatch(
          before: before,
          after: after,
          typingInput: typingInput,
          platformTiming: _inputTransactions.activeTiming,
        ),
      );
    }
    late.provisionalTail = after;
    late.successorCount += 1;
    _acceptPlatformWindowShadow(
      after,
      globalStart: _platformInput.shadowWindowStart,
    );
    final priorGeneration = _editGeneration;
    _inputTransactions.beginPlatformMutation();
    try {
      _promoteSemanticSuccessorsWithMap(holder, late.reconciliation);
      if (_editGeneration == priorGeneration) notifyListeners();
    } finally {
      _inputTransactions.endPlatformMutation();
    }
    return true;
  }

  bool _captureSemanticSuccessorValue(TextEditingValue value) {
    final pending = _inputTransactions.pendingSemantic;
    if (pending == null) return false;
    if (!_reserveSemanticSuccessor(pending)) return true;
    if (!_validObservedValue(value)) {
      _inputTransactions.discardPendingSemantic();
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    final before = pending.provisionalTail;
    final mutation = _differenceMutation(before.text, value.text);
    final logical = _logicalSemanticSuccessor(before, value);
    if (logical != null) {
      pending.successors.add(
        _inputTransactions.reclassifyAfterCertification(logical),
      );
      pending.provisionalTail = value;
      _markObservedPlatformCommand(logical);
      _inputTransactions.observePendingSuccessors(pending);
      _acceptPlatformWindowShadow(
        value,
        globalStart: pending.inputGlobalUtf16Start,
      );
      return true;
    }
    if (pending.successors.isNotEmpty &&
        pending.successors.last is FlarkDeferredInputSuccessor) {
      _inputTransactions.discardPendingSemantic();
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    pending.successors.add(
      FlarkProvisionalInputBatch(
        before: before,
        after: value,
        typingInput:
            mutation != null &&
            mutation.start == mutation.end &&
            mutation.replacement.isNotEmpty,
        platformTiming: _inputTransactions.activeTiming,
      ),
    );
    pending.provisionalTail = value;
    _inputTransactions.observePendingSuccessors(pending);
    _acceptPlatformWindowShadow(
      value,
      globalStart: pending.inputGlobalUtf16Start,
    );
    return true;
  }

  bool _capturePlatformDeltasBehindCertification(
    List<TextEditingDelta> deltas,
  ) {
    if (!_publicationCertificationBarrierActive ||
        _inputTransactions.pendingSemantic != null ||
        _platformShadowMatchesCurrentInput) {
      return false;
    }
    final before = _platformShadowValue;
    if (before == null) {
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    final rejection = _validateDeltaBatch(
      deltas,
      against: before,
      expectedTextSha256: flarkWindowTextSha256(before.text),
    );
    if (rejection != FlarkInputResyncReason.none) {
      _resynchronize(rejection);
      return true;
    }
    var after = before;
    for (final delta in deltas) {
      after = delta.apply(after);
    }
    return _capturePlatformValueBehindCertification(
      after,
      observedMutation: deltas.length == 1 ? _mutationFor(deltas.single) : null,
      before: before,
    );
  }

  bool _capturePlatformValueBehindCertification(
    TextEditingValue value, {
    TextEditingValue? before,
    FlarkTextMutation? observedMutation,
  }) {
    if (!_publicationCertificationBarrierActive ||
        _inputTransactions.pendingSemantic != null ||
        _platformShadowMatchesCurrentInput) {
      return false;
    }
    final platformBefore = before ?? _platformShadowValue;
    if (platformBefore == null || !_validObservedValue(value)) {
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    final logical = _logicalSemanticSuccessor(
      platformBefore,
      value,
      observedMutation: observedMutation,
    );
    if (logical == null) {
      // The platform is editing a host-superseded window. Only a complete
      // logical command can be replayed safely after certification; diffing
      // this value against the unpublished current window can resurrect text
      // a preceding Delete/Backspace already removed.
      _resynchronize(FlarkInputResyncReason.unsupportedSuccessorObservation);
      return true;
    }
    final timing = _inputTransactions.activeTiming;
    final pending = FlarkPendingSemanticInput(
      base: _inputState.value,
      inputGlobalUtf16Start: _platformInput.shadowWindowStart,
      initialCallbackStartedEpochMicros:
          timing?.acceptedAtEpochMicros ??
          _inputTransactions.activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: timing,
      provisionalAfter: value,
    );
    pending.successors.add(
      _inputTransactions.reclassifyAfterCertification(logical),
    );
    _inputTransactions.pendingSemantic = pending;
    _beginCertificationDeferredInput();
    _markObservedPlatformCommand(logical);
    _inputTransactions.observePendingSuccessors(pending);
    _acceptPlatformWindowShadow(
      value,
      globalStart: _platformInput.shadowWindowStart,
    );
    return true;
  }

  TextEditingValue? get _platformShadowValue => _platformInput.shadowValue;

  bool _deferCommandUntilCertification(
    FlarkDeferredInputCommand command, {
    TextEditingValue? provisionalAfter,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    final staleViewportNeedsCommandCertification =
        !_viewportState.semanticCurrent;
    if ((!_publicationCertificationBarrierActive &&
            !staleViewportNeedsCommandCertification) ||
        _inputTransactions.pendingSemantic != null) {
      return false;
    }
    final timing = platformTiming ?? _inputTransactions.activeTiming;
    final pending = FlarkPendingSemanticInput(
      base: _inputState.value,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      initialCallbackStartedEpochMicros:
          timing?.acceptedAtEpochMicros ??
          _inputTransactions.activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: timing,
      provisionalAfter: provisionalAfter ?? _inputState.value,
    );
    pending.successors.add(
      FlarkDeferredInputSuccessor(
        command,
        reclassifyAfterCertification: true,
        platformTiming: timing,
      ),
    );
    _inputTransactions.pendingSemantic = pending;
    _beginCertificationDeferredInput();
    if (staleViewportNeedsCommandCertification) {
      _scheduleParsingAfterInput(immediate: true);
    }
    _inputTransactions.observePendingSuccessors(pending);
    if (provisionalAfter != null) {
      _acceptPlatformWindowShadow(
        provisionalAfter,
        globalStart: _inputState.globalUtf16Start,
      );
    }
    return true;
  }

  bool _deferSemanticSuccessor({
    FlarkDeferredInputCommand? command,
    String? replacement,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    final pending = _inputTransactions.pendingSemantic;
    if (pending != null) {
      if (!_reserveSemanticSuccessor(pending)) return true;
      pending.successors.add(
        FlarkDeferredInputSuccessor(
          command,
          replacement: replacement,
          reclassifyAfterCertification:
              _certificationDeferredInputActive && command != null,
          platformTiming: platformTiming ?? _inputTransactions.activeTiming,
        ),
      );
      _inputTransactions.observePendingSuccessors(pending);
      return true;
    }
    final late = _inputTransactions.lateSemantic;
    if (late == null || command == null || replacement != null) return false;
    if (_platformShadowMatchesCurrentInput) {
      // A committed publication has already replaced the provisional window
      // in the text service. A selector with no old-text lineage now belongs
      // to that current caret, not to the preceding semantic transaction.
      // Stale deltas remain distinguishable and continue through
      // _captureLateSemanticSuccessors using their explicit oldText.
      _inputTransactions.lateSemantic = null;
      return false;
    }
    if (late.successorCount >= _maximumSemanticSuccessors) {
      _inputTransactions.lateSemantic = null;
      _resynchronize(FlarkInputResyncReason.successorQueueOverflow);
      return true;
    }
    // Desktop selectors can arrive after the predecessor receipt committed
    // but before the text service adopted its new window. Preserve that
    // causal lineage: classify the command against current Rust semantics
    // instead of the receipt-backed/still-uncertified Flutter row cache.
    late.successorCount += 1;
    _inputTransactions.observeSuccessorCount(late.successorCount);
    _inputTransactions.lateSemantic = null;
    _promoteDeferredCommand(
      command,
      semanticAlreadyAttempted: false,
      platformTiming: platformTiming ?? _inputTransactions.activeTiming,
    );
    return true;
  }

  void _beginCertificationDeferredInput() {
    final pending = _inputTransactions.pendingSemantic;
    if (pending == null) {
      throw StateError('Certification-deferred input requires live lineage');
    }
    pending.certificationPromotion ??= Completer<void>();
  }

  void _promoteCertificationDeferredInput() {
    final pending = _inputTransactions.pendingSemantic;
    final promotion = pending?.certificationPromotion;
    if (pending != null) pending.certificationPromotion = null;
    try {
      _promoteUncommittedSemanticSuccessors();
    } finally {
      if (promotion != null && !promotion.isCompleted) promotion.complete();
    }
  }

  void _cancelCertificationDeferredInput() {
    final pending = _inputTransactions.pendingSemantic;
    final promotion = pending?.certificationPromotion;
    if (pending != null) pending.certificationPromotion = null;
    if (promotion != null && !promotion.isCompleted) promotion.complete();
  }

  FlarkDeferredInputSuccessor? _logicalSemanticSuccessor(
    TextEditingValue before,
    TextEditingValue after, {
    FlarkTextMutation? observedMutation,
  }) => _inputTransactions.classifySemanticSuccessor(
    before,
    after,
    mutation: observedMutation ?? _selectionObservedMutation(before, after),
  );

  FlarkTextMutation? _selectionObservedMutation(
    TextEditingValue before,
    TextEditingValue after,
  ) => _platformInput.selectionObservedMutation(before, after);

  void _markObservedPlatformCommand(FlarkDeferredInputSuccessor successor) {
    _inputTransactions.markObservedCommand(successor.command);
  }

  bool _reserveSemanticSuccessor(FlarkPendingSemanticInput pending) {
    if (_inputTransactions.reserveSemanticSuccessor(
      pending,
      maximum: _maximumSemanticSuccessors,
    )) {
      return true;
    }
    _resynchronize(FlarkInputResyncReason.successorQueueOverflow);
    return false;
  }

  bool _validObservedValue(TextEditingValue value) {
    return _platformInput.validObservedValue(
      value,
      maximumCodeUnits: _maximumInputCodeUnits,
    );
  }

  TextEditingValue _normalizeProjectedSelection(TextEditingValue value) {
    if (!value.selection.isValid || !value.composing.isCollapsed) return value;
    final row = _activeCachedRow();
    if (row == null) return value;
    if (value.selection.isCollapsed) {
      final globalCaret =
          _inputState.globalUtf16Start + value.selection.extentOffset;
      final structuralCaret = _structuralCanonicalCaretAt(globalCaret);
      if (structuralCaret != null) {
        final localCaret = structuralCaret - _inputState.globalUtf16Start;
        if (0 <= localCaret && localCaret <= value.text.length) {
          return value.copyWith(
            selection: TextSelection.collapsed(
              offset: localCaret,
              affinity: value.selection.affinity,
            ),
          );
        }
      }
      if (_exactTrailingWhitespaceRange(row, globalCaret) != null) return value;
      final dependency = _pendingPresentation.dependency?.authority;
      if (dependency is FlarkProjectionEditCellReceipt &&
          dependency.resultCaretUtf16 == globalCaret) {
        return value;
      }
      if (dependency is FlarkBoundedPendingPresentationPlanReceipt &&
          dependency.plan.triggerUtf16.start + dependency.prefixLength ==
              globalCaret) {
        // Every admitted prefix is parser-authored as one exact insertion
        // sequence. Its ordinary insertion result remains the canonical
        // source caret even when that clean step hides the entire prefix.
        return value;
      }
      for (final state in _pendingPresentation.structuralSurfaces) {
        final continuity = state.continuity;
        if (state.surface.rowOrdinal == row.ordinal &&
            continuity?.resultCaretUtf16 == globalCaret) {
          return value;
        }
      }
    }
    // A pending dependency surface carries parser-authored coordinates across
    // an optimistic edit. The predecessor-only surface can clamp a newly
    // advanced caret backward before the next same-burst command.
    final presentation = surfaceRow(row);
    if (!_surfaceHasProjection(presentation, row)) return value;

    int normalize(int localOffset) {
      final global = _inputState.globalUtf16Start + localOffset;
      final exactRow = _mappedExactRowRange(row);
      // The start and end of a source-owned block are canonical caret stops
      // even when the first or last Markdown marker is hidden by projection.
      // Projecting source zero through the first visible heading run would
      // otherwise move it across `# ` merely because parsing completed.
      if (global == exactRow.start || global == exactRow.end) {
        return localOffset;
      }
      final display = presentation.textOffsetForSourceOffset(
        global,
        affinity: portableTextAffinity(value.selection.affinity),
      );
      final upstream = presentation.sourceOffsetForTextOffset(
        display,
        affinity: FlarkTextAffinity.upstream,
      );
      final downstream = presentation.sourceOffsetForTextOffset(
        display,
        affinity: FlarkTextAffinity.downstream,
      );
      // Both sides of a hidden delimiter are real editing boundaries. Keep
      // the exact side already owned by the canonical selection; forcing the
      // current affinity here makes a fresh parser publication move a caret
      // across closing syntax and changes where the next rapid key lands.
      if (global == upstream || global == downstream) return localOffset;
      final normalizedGlobal = value.selection.affinity == TextAffinity.upstream
          ? upstream
          : downstream;
      return (normalizedGlobal - _inputState.globalUtf16Start).clamp(
        0,
        value.text.length,
      );
    }

    var selection = TextSelection(
      baseOffset: normalize(value.selection.baseOffset),
      extentOffset: normalize(value.selection.extentOffset),
      affinity: value.selection.affinity,
      isDirectional: value.selection.isDirectional,
    );
    if (!selection.isCollapsed) {
      final exactRow = _mappedExactRowRange(row);
      final globalBase = _inputState.globalUtf16Start + selection.baseOffset;
      final globalExtent =
          _inputState.globalUtf16Start + selection.extentOffset;
      final belongsToActiveRow =
          exactRow.start <= globalBase &&
          globalBase <= exactRow.end &&
          exactRow.start <= globalExtent &&
          globalExtent <= exactRow.end;
      if (belongsToActiveRow &&
          presentation.textOffsetForSourceOffset(
                globalBase,
                affinity: portableTextAffinity(selection.affinity),
              ) ==
              presentation.textOffsetForSourceOffset(
                globalExtent,
                affinity: portableTextAffinity(selection.affinity),
              )) {
        // Source-only delimiters and row terminators can give the platform a
        // non-empty selection whose endpoints occupy one rendered caret. It
        // has selected nothing the user can see, so retain the anchor as the
        // canonical caret. Otherwise the next insertion attempts to replace
        // hidden Markdown and is correctly rejected as an invalid splice.
        selection = TextSelection.collapsed(
          offset: selection.baseOffset,
          affinity: selection.affinity,
        );
      }
    }
    return selection == value.selection
        ? value
        : value.copyWith(selection: selection);
  }

  int? _structuralCanonicalCaretAt(int globalCaret) {
    for (final state in _pendingPresentation.structuralSurfaces) {
      final surface = state.surface;
      for (final cell in surface.projectionEditCells) {
        if (cell.triggerUtf16.start == cell.affectedUtf16.end &&
            cell.triggerUtf16.end == cell.affectedUtf16.end &&
            cell.affectedUtf16.start == globalCaret &&
            cell.affectedUtf16.end > globalCaret &&
            surface.presentation.globalUtf16Start == cell.affectedUtf16.end) {
          return cell.affectedUtf16.end;
        }
      }
    }
    return null;
  }

  FlarkSourceRange? _exactTrailingWhitespaceRange(
    FlarkViewportRow row,
    int globalCaret,
  ) =>
      _captureSurfaceProjector().exactTrailingWhitespaceRange(row, globalCaret);

  bool _normalizeProjectedCommandSelection() {
    final normalized = _normalizeProjectedSelection(_inputState.value);
    if (normalized.selection == _inputState.value.selection) return false;
    _breakTypingHistoryGroup();
    _inputState.replaceValue(normalized);
    _updateGlobalSelection();
    unawaited(_installCanonicalSelection(_selectionSnapshot()));
    return true;
  }

  bool _deleteProjectedVisible({
    required bool backward,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    final row = _activeCachedRow();
    if (row == null) return false;
    final presentation = surfaceRow(row);
    if (!presentation.active || !_surfaceHasProjection(presentation, row)) {
      return false;
    }
    final selection = _inputState.value.selection;
    if (!selection.isCollapsed) return false;
    final globalCaret = _inputState.globalUtf16Start + selection.extentOffset;
    final displayCaret = presentation.textOffsetForSourceOffset(
      globalCaret,
      affinity: portableTextAffinity(selection.affinity),
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
      affinity: FlarkTextAffinity.downstream,
    );
    final sourceEnd = presentation.sourceOffsetForTextOffset(
      cluster.$2,
      affinity: FlarkTextAffinity.upstream,
    );
    // The mapped grapheme can be separated from the raw caret by a hidden
    // inline delimiter. Delete the visible grapheme's exact source range and
    // cross that delimiter as one rendered caret stop; never delete the gap
    // itself. Structural row boundaries have no neighboring display cluster
    // here and continue to use their parser-authored semantic recipes.
    if (sourceStart >= sourceEnd) return true;
    final localStart = sourceStart - _inputState.globalUtf16Start;
    final localEnd = sourceEnd - _inputState.globalUtf16Start;
    if (localStart < 0 || localEnd > _inputState.value.text.length) {
      return false;
    }
    final acceptance = _acceptMutation(
      FlarkTextMutation(localStart, localEnd, ''),
      selection: TextSelection.collapsed(offset: localStart),
      composing: TextRange.empty,
      typingInput: false,
      platformTiming: platformTiming,
      editabilityProven: true,
    );
    if (!acceptance.accepted) return false;
    // This is a controller command, not a platform observation. The text
    // service needs the accepted value immediately even when rendering stays
    // behind a certification barrier.
    _publishCommandInputState();
    return true;
  }

  bool _mutationTouchesOnlyHiddenProjection(FlarkTextMutation mutation) {
    if (mutation.start == mutation.end) return false;
    final row = _activeCachedRow();
    if (row == null) return false;
    // Same-burst platform callbacks must be classified against the
    // transaction-authorized result surface, not predecessor coordinates.
    final presentation = surfaceRow(row);
    if (!_surfaceHasProjection(presentation, row)) return false;
    final sourceStart = _inputState.globalUtf16Start + mutation.start;
    final sourceEnd = _inputState.globalUtf16Start + mutation.end;
    final displayStart = presentation.textOffsetForSourceOffset(
      sourceStart,
      affinity: FlarkTextAffinity.downstream,
    );
    final displayEnd = presentation.textOffsetForSourceOffset(
      sourceEnd,
      affinity: FlarkTextAffinity.upstream,
    );
    return displayStart == displayEnd;
  }

  bool _surfaceHasProjection(
    FlarkSurfaceRow presentation,
    FlarkViewportRow row,
  ) {
    if (presentation.runs.isEmpty) return false;
    FlarkCoreCommittedPresentationSurfaceV1? committed;
    for (final state in _pendingPresentation.structuralSurfaces) {
      final surface = state.surface;
      if (surface.rowOrdinal == row.ordinal) {
        committed = surface;
        break;
      }
    }
    final activation =
        committed?.sourceUtf16 ?? _mapViewportRange(_activationRange(row));
    if (committed == null && !_rowSemanticsCurrent(activation)) return false;
    var sourceCursor = activation.start;
    for (final run in presentation.runs) {
      if (!run.sourceExact || run.sourceUtf16Start != sourceCursor) return true;
      sourceCursor = run.sourceUtf16End;
    }
    return sourceCursor != activation.end;
  }

  FlarkMutationAcceptance _acceptMutation(
    FlarkTextMutation mutation, {
    required TextSelection selection,
    required TextRange composing,
    TextEditingValue? fullValue,
    bool typingInput = false,
    FlarkPlatformInputTiming? platformTiming,
    bool editabilityProven = false,
  }) {
    final source = _inputState.value.text;
    if (mutation.start < 0 ||
        mutation.end < mutation.start ||
        mutation.end > source.length) {
      return const FlarkRejectedMutation();
    }
    // A platform composition may temporarily occupy a source position that
    // the last certified projection still describes as a hidden delimiter or
    // trailing newline. Once that range was accepted, later IME updates own
    // it authoritatively; rejecting the replacement against stale projection
    // facts drops real dead-key/CJK commits and forces a needless resync.
    final activeComposition = _inputState.value.composing;
    final replacesAcceptedComposition =
        activeComposition.isValid &&
        mutation.start >= activeComposition.start &&
        mutation.end <= activeComposition.end;
    if (!editabilityProven &&
        !replacesAcceptedComposition &&
        _mutationTouchesOnlyHiddenProjection(mutation)) {
      return const FlarkRejectedMutation();
    }
    final beforeSelection = _selectionSnapshot();
    var effectiveMutation = mutation;
    var effectiveSelection = selection;
    var effectiveComposing = composing;
    var effectiveFullValue = fullValue;
    final continuation = beforeSelection.inlineContinuation;
    final insertsAtContinuation =
        continuation != null &&
        mutation.start == mutation.end &&
        _inputState.globalUtf16Start + mutation.start ==
            continuation.caretUtf16 &&
        selection.isCollapsed &&
        selection.extentOffset ==
            mutation.start + mutation.replacement.length &&
        !composing.isValid;
    final candidateContinuationRewrite =
        continuation != null && insertsAtContinuation
        ? continuation.rewriteReplacement(mutation.replacement)
        : null;
    final continuationRewriteEnd = candidateContinuationRewrite == null
        ? mutation.end
        : mutation.end + candidateContinuationRewrite.replacedSuffixUtf16;
    final continuationRewrite =
        candidateContinuationRewrite != null &&
            (candidateContinuationRewrite.replacedSuffixUtf16 == 0 ||
                (continuationRewriteEnd <= source.length &&
                    source.substring(mutation.end, continuationRewriteEnd) ==
                        continuation!.suffix))
        ? candidateContinuationRewrite
        : null;
    if (continuationRewrite != null) {
      effectiveMutation = FlarkTextMutation(
        mutation.start,
        continuationRewriteEnd,
        continuationRewrite.replacement,
      );
      effectiveSelection = TextSelection.collapsed(
        offset: mutation.start + continuationRewrite.caretUtf16Offset,
        affinity: selection.affinity,
      );
      effectiveComposing = TextRange.empty;
      effectiveFullValue = null;
      // The recipe is parser-authored, but its successor projection is not
      // certified until Rust parses the wrapped source. Hold the prior atomic
      // frame rather than flashing delimiters or plain styling.
      _coordinator.beginPublicationBarrier();
      _inputState.restoreInlineContinuation(
        continuationRewrite.continuesOwner
            ? continuation!.materializedAtRevision(
                continuation.revision + 1,
                _inputState.globalUtf16Start + effectiveSelection.extentOffset,
              )
            : null,
      );
    } else if (continuation != null &&
        (insertsAtContinuation ||
            mutation.start != mutation.end ||
            mutation.replacement.isNotEmpty)) {
      // Whitespace and every nonmatching edit leave the emptied owner. Undo
      // still restores the continuation through beforeSelection.
      _inputState.abandonInlineContinuation();
    }
    final nextLength = replacementResultLength(
      source: source,
      start: effectiveMutation.start,
      end: effectiveMutation.end,
      replacement: effectiveMutation.replacement,
    );
    final removedText = source.substring(
      effectiveMutation.start,
      effectiveMutation.end,
    );
    final requiresStructuralCertification =
        effectiveMutation.replacement.contains('\n') ||
        effectiveMutation.replacement.contains('\r') ||
        removedText.contains('\n') ||
        removedText.contains('\r');
    if (!effectiveSelection.isValid ||
        effectiveSelection.baseOffset > nextLength ||
        effectiveSelection.extentOffset > nextLength) {
      return const FlarkRejectedMutation();
    }
    final inputStart = _inputState.globalUtf16Start;
    final wasCrossRowSelection = _inputState.crossRowSelection;
    final globalStart = inputStart + effectiveMutation.start;
    final globalEnd = inputStart + effectiveMutation.end;
    final preferredOrdinal = _preferredMutationOrdinal(
      globalStart,
      globalEnd,
      effectiveMutation.replacement,
    );
    final compositionHistoryGroup = _compositionGroupForMutation(
      effectiveComposing,
    );

    // Preserve an exact origin while this still carries the pre-edit input
    // text. A splice crossing a deep viewport start invalidates that page's
    // byte position, but an origin at or before the splice remains stable.
    _retainOptimisticRefreshAnchor(globalStart, deriveFromInput: true);

    if (nextLength <= _maximumInputCodeUnits) {
      _inputState.replaceValue(
        effectiveFullValue ??
            TextEditingValue(
              text: source.replaceRange(
                effectiveMutation.start,
                effectiveMutation.end,
                effectiveMutation.replacement,
              ),
              selection: effectiveSelection,
              composing: effectiveComposing,
            ),
      );
    } else {
      final window = boundedReplacementWindow(
        source: source,
        start: effectiveMutation.start,
        end: effectiveMutation.end,
        replacement: effectiveMutation.replacement,
        focus: effectiveSelection.extentOffset,
        maximumCodeUnits: _maximumInputCodeUnits,
      );
      final windowEnd = window.start + window.text.length;
      final localBase = (effectiveSelection.baseOffset - window.start).clamp(
        0,
        window.text.length,
      );
      final localExtent = (effectiveSelection.extentOffset - window.start)
          .clamp(0, window.text.length);
      final localComposing =
          effectiveComposing.isValid &&
              effectiveComposing.start >= window.start &&
              effectiveComposing.end <= windowEnd
          ? TextRange(
              start: effectiveComposing.start - window.start,
              end: effectiveComposing.end - window.start,
            )
          : TextRange.empty;
      _inputState.replaceWindow(
        globalUtf16Start: inputStart + window.start,
        value: TextEditingValue(
          text: window.text,
          selection: TextSelection(
            baseOffset: localBase,
            extentOffset: localExtent,
            affinity: effectiveSelection.affinity,
            isDirectional: effectiveSelection.isDirectional,
          ),
          composing: localComposing,
        ),
      );
    }
    _inputState.setCanonicalSelection(
      inputStart + effectiveSelection.baseOffset,
      inputStart + effectiveSelection.extentOffset,
    );
    _inputState.retargetActiveOrdinal(preferredOrdinal);
    _inputState.setCrossRowSelection(false);
    final afterSelection = _selectionSnapshot();
    final coalesceTyping =
        typingInput &&
        compositionHistoryGroup == null &&
        !effectiveComposing.isValid;
    if (!coalesceTyping) _breakTypingHistoryGroup();
    final publication = _queueNativeEdit(
      globalStart,
      globalEnd,
      effectiveMutation.replacement,
      beforeSelection: beforeSelection,
      afterSelection: afterSelection,
      coalesceTyping: coalesceTyping,
      compositionHistoryGroup: compositionHistoryGroup,
      compositionFinal:
          compositionHistoryGroup != null && !effectiveComposing.isValid,
      recenterAfterOptimisticEdit: wasCrossRowSelection,
      requiresStructuralCertification: requiresStructuralCertification,
      platformTiming: platformTiming ?? _inputTransactions.activeTiming,
    );
    return FlarkQueuedMutation(publication);
  }

  FlarkCoreSelectionSnapshot _coreSnapshot(
    FlarkEditorSelectionSnapshot snapshot,
  ) => FlarkCoreSelectionSnapshot(
    base: snapshot.selection.baseOffset,
    extent: snapshot.selection.extentOffset,
    affinity: snapshot.selection.affinity == TextAffinity.upstream
        ? FlarkCoreAffinity.upstream
        : FlarkCoreAffinity.downstream,
    // Adapter history restores only platform-specific visual metadata.
    // Core owns semantic continuation as a typed part of the canonical
    // selection and records it in the same history unit as the edit.
    adapterState: FlarkEditorSelectionSnapshot(
      snapshot.selection,
      snapshot.activeOrdinal,
    ),
    inlineContinuation: snapshot.inlineContinuation,
  );

  Future<int> _installCanonicalSelection(
    FlarkEditorSelectionSnapshot snapshot, {
    bool publish = true,
  }) {
    _inputState.restoreInlineContinuation(snapshot.inlineContinuation);
    final core = _coreSnapshot(snapshot);
    final operation = _coordinator.queueSessionCommand(
      () => _session.setSelectionUtf16(
        core.base,
        core.extent,
        affinity: core.affinity,
        adapterState: core.adapterState,
        inlineContinuation: core.inlineContinuation,
      ),
    );
    unawaited(
      operation
          .then((_) {
            if (!_closed && publish) notifyListeners();
          })
          .catchError((Object error, StackTrace stackTrace) {
            _lastError = error;
            _status = FlarkEditorStatus.faulted;
            notifyListeners();
          }),
    );
    return operation;
  }

  FlarkEditorSelectionSnapshot _adapterSnapshot(
    FlarkCoreSelectionSnapshot snapshot,
  ) {
    final adapter = snapshot.adapterState;
    return FlarkEditorSelectionSnapshot(
      TextSelection(
        baseOffset: snapshot.base,
        extentOffset: snapshot.extent,
        affinity: snapshot.affinity == FlarkCoreAffinity.upstream
            ? TextAffinity.upstream
            : TextAffinity.downstream,
        isDirectional:
            adapter is FlarkEditorSelectionSnapshot &&
            adapter.selection.isDirectional,
      ),
      adapter is FlarkEditorSelectionSnapshot ? adapter.activeOrdinal : null,
      inlineContinuation: snapshot.inlineContinuation,
    );
  }

  void _breakTypingHistoryGroup() => _session.breakTypingGroup();

  int? _compositionGroupForMutation(TextRange composing) {
    if (!_session.compositionActive && composing.isValid) {
      _rememberCompositionInputBase(_inputState.value);
    }
    final group = _session.compositionGroupForMutation(
      composingActive: composing.isValid,
    );
    if (!composing.isValid) _inputTransactions.clearCompositionInputBase();
    return group;
  }

  void _trackCompositionWithoutMutation(TextRange composing) {
    if (!_session.compositionActive && composing.isValid) {
      _rememberCompositionInputBase(
        _inputState.value.copyWith(composing: TextRange.empty),
      );
    }
    final compositionEnded = _session.trackCompositionWithoutMutation(
      composingActive: composing.isValid,
    );
    if (compositionEnded) {
      _inputTransactions.clearCompositionInputBase();
      _queueCompositionFinish();
      _scheduleParsingAfterInput();
    }
  }

  void _endCompositionHistoryGroup() {
    _inputTransactions.clearCompositionInputBase();
    final wasActive = _session.compositionActive;
    _session.endCompositionGroup();
    if (wasActive) _queueCompositionFinish();
  }

  void _queueCompositionFinish() {
    _coordinator.queueSessionCommand(_session.finishComposition);
  }

  void _rememberCompositionInputBase(TextEditingValue value) {
    _inputTransactions.rememberCompositionInputBase(
      windowStart: _inputState.globalUtf16Start,
      value: value,
    );
  }

  bool _isCompositionCancelValue(TextEditingValue value) {
    final base = _inputTransactions.compositionInputBase;
    if (base == null || !_session.compositionActive) return false;
    final expected = base.value;
    return value.composing == TextRange.empty &&
        base.windowStart == _inputState.globalUtf16Start &&
        value.text == expected.text &&
        value.selection == expected.selection;
  }

  Future<void> close() async {
    if (_closed) return;
    _coordinator.beginClosing();
    _parseTimer?.cancel();
    _parseTimer = null;
    if (_certificationDeferredInputActive) {
      _inputTransactions.discardPendingSemantic();
      _cancelCertificationDeferredInput();
    }
    await _waitForMutationTail();
    // Closing prevents new parser/page work from entering. Drain every
    // already-admitted effect before disposing either dependency: parser
    // adoption still inspects session state, while page work still owns
    // document continuations.
    final parserTask = _coordinator.parserTask;
    if (parserTask != null) await parserTask;
    final pageTask = _coordinator.pageTask;
    if (pageTask != null) await pageTask;
    // Let caller-visible continuations on a just-completed page future run
    // before close reports that all admitted page work has settled.
    await Future<void>.value();
    await _session.dispose();
    await _document.dispose();
    _coordinator.markDisposed();
  }

  Future<bool> _loadNextViewportPage() async {
    final current = _viewportState.viewport;
    if (current == null || !canPageForward) return false;
    try {
      final result = await _viewportPager.nextPage(current);
      if (result == null) return false;
      if (!_viewportPager.adopt(result)) {
        final cleanup = _viewportPager.discard(result);
        if (cleanup != null) await cleanup;
        return false;
      }
      _installViewport(result, restoreInputWindow: false);
      return true;
    } catch (error) {
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
      return false;
    }
  }

  Future<bool> _loadPreviousViewportPage() async {
    final current = _viewportState.viewport;
    if (current == null || !_viewportPager.canPageBackward) return false;
    try {
      final result = await _viewportPager.previousPage(current);
      if (result == null) return false;
      if (!_viewportPager.adopt(result)) {
        final cleanup = _viewportPager.discard(result);
        if (cleanup != null) await cleanup;
        return false;
      }
      _installViewport(result, restoreInputWindow: false);
      return true;
    } catch (error) {
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
      return false;
    }
  }

  bool _activateWindow({
    required String text,
    required int sourceStart,
    required int caret,
    int? selectionExtent,
    required int ordinal,
    required TextAffinity affinity,
  }) {
    final selectionRepresented = _inputState.activateWindow(
      text: text,
      sourceStart: sourceStart,
      caret: caret,
      selectionExtent: selectionExtent,
      ordinal: ordinal,
      affinity: affinity,
      maximumCodeUnits: _maximumInputCodeUnits,
    );
    notifyListeners();
    return selectionRepresented;
  }

  void _updateGlobalSelection() => _inputState.updateCanonicalFromLocal();

  FlarkEditorSelectionSnapshot _selectionSnapshot() =>
      FlarkEditorSelectionSnapshot(
        TextSelection(
          baseOffset: _inputState.selectionBaseUtf16,
          extentOffset: _inputState.selectionExtentUtf16,
          affinity: _inputState.value.selection.affinity,
          isDirectional: _inputState.value.selection.isDirectional,
        ),
        _inputState.activeOrdinal,
        inlineContinuation:
            _inputState.inlineContinuation != null &&
                // A rapid platform successor is admitted before its preceding
                // source transaction reaches the worker. Continuation authority
                // may therefore name one revision per already-pending edit plus
                // the edit currently being accepted, but never an arbitrary
                // future revision.
                _document.revision <=
                    _inputState.inlineContinuation!.revision &&
                _inputState.inlineContinuation!.revision <=
                    _document.revision + _coordinator.pendingEdits + 1 &&
                _inputState.selectionBaseUtf16 ==
                    _inputState.selectionExtentUtf16 &&
                _inputState.inlineContinuation?.caretUtf16 ==
                    _inputState.selectionExtentUtf16
            ? _inputState.inlineContinuation
            : null,
      );

  FlarkSourceRange _mappedExactRowRange(FlarkViewportRow row) =>
      _captureSurfaceProjector().mappedExactRowRange(row);

  int? _surfaceOrdinalAt(int globalUtf16Offset) {
    return _captureSurfaceProjector().surfaceOrdinalAt(
      rows: _viewportState.rows,
      globalUtf16Offset: globalUtf16Offset,
      sourceUtf16Length: sourceUtf16Length,
    );
  }

  bool _ensureActiveInputVisible() {
    final caret = _inputState.selectionExtentUtf16;
    final visibleEnd =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    if (_viewportState.visibleUtf16Start <= caret && caret <= visibleEnd) {
      return false;
    }
    final inputEnd =
        _inputState.globalUtf16Start + _inputState.value.text.length;
    if (caret < _inputState.globalUtf16Start || caret > inputEnd) return false;
    _viewportState.adoptUncertifiedSourceWindow(
      source: _inputState.value.text,
      startUtf16: _inputState.globalUtf16Start,
    );
    _clearPendingTaskChecks();
    _status = _idleStatus(current: _document.isReady);
    return true;
  }

  Future<void> _restoreHistorySelection(
    FlarkEditorSelectionSnapshot snapshot,
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
    final visibleEnd =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    if (selectionStart < _viewportState.visibleUtf16Start ||
        selectionEnd > visibleEnd) {
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
      final source = await _document.readSourceUtf16Range(
        windowStart,
        windowEnd,
      );
      _viewportState.adoptUncertifiedSourceWindow(
        source: source,
        startUtf16: windowStart,
      );
      _status = _idleStatus(current: _document.isReady);
    }
    _restoreSelectionSnapshot(snapshot);
  }

  void _restoreSelectionSnapshot(FlarkEditorSelectionSnapshot snapshot) {
    _inputState.restoreInlineContinuation(snapshot.inlineContinuation);
    final selection = snapshot.selection;
    final start = math.min(selection.baseOffset, selection.extentOffset);
    final end = math.max(selection.baseOffset, selection.extentOffset);
    final visibleEnd =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    if (!selection.isCollapsed &&
        start >= _viewportState.visibleUtf16Start &&
        end <= visibleEnd &&
        end - start <= _maximumInputCodeUnits) {
      FlarkViewportRow? containingRow;
      for (final row in _viewportState.rows) {
        final range = _mapViewportRange(_activationRange(row));
        if (range.start <= start && end <= range.end) {
          containingRow = row;
          break;
        }
      }
      if (containingRow != null) {
        final range = _mapViewportRange(_activationRange(containingRow));
        _inputState.replaceWindow(
          globalUtf16Start: range.start,
          value: TextEditingValue(
            text: _sliceVisibleUtf16(range.start, range.end),
            selection: TextSelection(
              baseOffset: selection.baseOffset - range.start,
              extentOffset: selection.extentOffset - range.start,
              affinity: selection.affinity,
              isDirectional: selection.isDirectional,
            ),
          ),
        );
        _inputState.setCanonicalSelection(
          selection.baseOffset,
          selection.extentOffset,
        );
        _inputState.retargetActiveOrdinal(containingRow.ordinal);
        _inputState.setCrossRowSelection(false);
        return;
      }
      _inputState.replaceWindow(
        globalUtf16Start: start,
        value: TextEditingValue(
          text: _sliceVisibleUtf16(start, end),
          selection: TextSelection(
            baseOffset: selection.baseOffset - start,
            extentOffset: selection.extentOffset - start,
            affinity: selection.affinity,
            isDirectional: selection.isDirectional,
          ),
        ),
      );
      _inputState.setCanonicalSelection(
        selection.baseOffset,
        selection.extentOffset,
      );
      _inputState.retargetActiveOrdinal(
        snapshot.activeOrdinal ?? _surfaceOrdinalAt(selection.extentOffset),
      );
      _inputState.setCrossRowSelection(true);
      return;
    }
    final caret = selection.extentOffset
        .clamp(0, math.max(sourceUtf16Length, visibleEnd))
        .toInt();
    _inputState.setCanonicalSelection(caret, caret);
    _restoreCollapsedInputWindow(
      caret,
      preferredOrdinal: snapshot.activeOrdinal,
    );
  }

  void _restoreCollapsedInputWindow(int caret, {int? preferredOrdinal}) {
    _inputState.installWindowPlan(
      FlarkEditorInputWindowPlanner.restoreCollapsed(
        viewportState: _viewportState,
        projector: _captureSurfaceProjector(),
        pendingPresentation: _pendingPresentation,
        caret: caret,
        sourceUtf16Length: sourceUtf16Length,
        maximumCodeUnits: _maximumInputCodeUnits,
        preferredOrdinal: preferredOrdinal,
      ),
    );
  }

  FlarkTextMutation? _mutationFor(TextEditingDelta delta) =>
      _platformInput.mutationFor(delta);

  FlarkQueuedEditPublication _queueNativeEdit(
    int start,
    int end,
    String replacement, {
    required FlarkEditorSelectionSnapshot beforeSelection,
    required FlarkEditorSelectionSnapshot afterSelection,
    required bool coalesceTyping,
    required int? compositionHistoryGroup,
    bool compositionFinal = false,
    bool recenterAfterOptimisticEdit = false,
    bool restoreSelectionAfterCommit = false,
    bool requiresStructuralCertification = false,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    final acceptanceWatch = Stopwatch()..start();
    final acceptedAtEpochMicros =
        platformTiming?.acceptedAtEpochMicros ??
        DateTime.now().microsecondsSinceEpoch;
    _retainOptimisticRefreshAnchor(start, deriveFromInput: false);
    final split = _pendingPresentation.paragraphGap;
    if (split != null &&
        (start < split.rowEndUtf16 || end > _committedGapEnd(split))) {
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.paragraphGap,
      });
    }
    final insertsNonLineEndingText =
        replacement.isNotEmpty &&
        !replacement.contains('\n') &&
        !replacement.contains('\r');
    final caretBoundary = _pendingPresentation.caretBoundary;
    var caretBoundaryStartsExactBlock = false;
    if (caretBoundary != null) {
      final boundaryEnd = _committedCaretBoundaryEnd(caretBoundary);
      final insertsInsideBlankBoundary =
          start == end &&
          caretBoundary.rowEndUtf16 <= start &&
          start <= boundaryEnd;
      final insertsOnlyLineEndings =
          replacement.isNotEmpty &&
          replacement.codeUnits.every((unit) => unit == 0x0a || unit == 0x0d);
      final preservesBlankBoundary =
          insertsInsideBlankBoundary && insertsOnlyLineEndings;
      caretBoundaryStartsExactBlock =
          insertsInsideBlankBoundary && insertsNonLineEndingText;
      if (!preservesBlankBoundary) {
        _coordinator.retirePendingPresentation(const {
          FlarkPendingPresentationPart.caretBoundary,
        });
      }
    }
    final structurals = _pendingPresentation.structuralSurfaces;
    final compositionUsesExactFallback = compositionHistoryGroup != null;
    final neutralInputStartsExactBlock =
        (_inputState.activeOrdinal ?? 0) < 0 &&
        start == end &&
        insertsNonLineEndingText;
    final editStartsExactFallback =
        compositionUsesExactFallback ||
        caretBoundaryStartsExactBlock ||
        neutralInputStartsExactBlock;
    FlarkProjectionEditCellReceipt? projectionReceipt;
    var structuralSuccessorRequiresCertification = false;
    if (!compositionUsesExactFallback &&
        editStartsExactFallback &&
        structurals.isNotEmpty) {
      // A structural Return can leave the caret in a parser-owned exact
      // successor island. Prefer the result edit-cell proof for the first
      // non-line-ending splice: it keeps the predecessor rendered while
      // authorizing the exact successor text. Falling straight to an
      // unproved local island here widens the stale owner row over both
      // blocks and briefly exposes the predecessor's Markdown markers.
      projectionReceipt = _advanceCommittedStructuralSurfaces(
        start,
        end,
        replacement,
      );
    }
    if (!compositionUsesExactFallback &&
        editStartsExactFallback &&
        projectionReceipt == null &&
        structurals.isEmpty &&
        caretBoundary != null) {
      // Certified rows retire the temporary structural surface, but its
      // parser-authored first-edit cell survives on the nonvisual boundary.
      // This keeps ordinary typing immediate without a Flutter Markdown
      // character allowlist; syntax-shaped prefixes fail closed naturally.
      projectionReceipt = _advanceCommittedCaretBoundary(
        caretBoundary,
        start,
        end,
        replacement,
      );
    }
    final editUsesExactFallback =
        editStartsExactFallback && projectionReceipt == null;
    final firstLf = _inputState.value.text.indexOf('\n');
    final firstCr = _inputState.value.text.indexOf('\r');
    final firstLineEnding = firstLf < 0
        ? firstCr
        : firstCr < 0
        ? firstLf
        : math.min(firstLf, firstCr);
    final lookaheadStart = firstLineEnding < 0
        ? _inputState.value.text.length
        : firstLineEnding +
              (firstCr == firstLineEnding &&
                      firstLineEnding + 1 < _inputState.value.text.length &&
                      _inputState.value.text.codeUnitAt(firstLineEnding + 1) ==
                          0x0a
                  ? 2
                  : 1);
    final exactFallbackHasStructuralLookahead =
        !compositionUsesExactFallback &&
        editUsesExactFallback &&
        lookaheadStart < _inputState.value.text.length;
    final exactFallbackHasCertifiedNeighbor =
        !compositionUsesExactFallback &&
        editUsesExactFallback &&
        // A retained caret boundary supplies exact source geometry for the
        // parser-less blank island. It may safely paint ordinary or
        // syntax-shaped input as local exact source while certified siblings
        // remain rendered. A generic neutral fallback has no such typed
        // partition and must retain the prior atomic frame.
        !caretBoundaryStartsExactBlock &&
        _viewportState.rows.any((row) {
          final range = surfaceSourceRange(row);
          return range.end <= start || end <= range.start;
        });
    if (editUsesExactFallback) {
      // Parser certification is intentionally pinned during composition, and
      // a neutral input island has no AST row yet. Neither state proves result
      // Markdown semantics. They do prove the exact local editing island, so
      // publish that island as source while mechanically unchanged siblings
      // stay rendered. This prevents both stale UTF-16 projection and a blank
      // row that appears to ignore the user's first character. The durable
      // caret-boundary receipt covers shared parser boundaries; the negative
      // neutral ordinal covers the same exact island after blank-line edits.
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
        FlarkPendingPresentationPart.structuralSurfaces,
      });
    } else if (projectionReceipt == null && structurals.isNotEmpty) {
      // Only a parser-proved structural surface may carry a typed edit cell
      // into its result revision. Exactly one matching cell may advance that
      // temporary presentation; every other successor fails closed until a
      // fresh parser publication arrives.
      projectionReceipt = _advanceCommittedStructuralSurfaces(
        start,
        end,
        replacement,
      );
      if (projectionReceipt == null) {
        // The semantic predecessor still proves the source partition that is
        // on screen, but it did not authorize this exact successor result.
        // Once those surfaces are retired there is no parser-owned geometry
        // for the new source. Keep the same-burst edit atomic through fresh
        // certification instead of publishing one raw window across blocks.
        structuralSuccessorRequiresCertification = true;
        _coordinator.retirePendingPresentation(const {
          FlarkPendingPresentationPart.dependency,
          FlarkPendingPresentationPart.structuralSurfaces,
        });
      }
    } else {
      projectionReceipt = _prepareProjectionContinuity(start, end, replacement);
    }
    // Exact source is always valid input state, but it is not necessarily the
    // rendered result: one scalar may change inline styling, join a lazy
    // continuation, or reinterpret sibling physical lines as a new block.
    // Publish optimistically only when Core supplied an exact authority for
    // this result. Otherwise keep the previous atomic surface publication
    // until the native parser certifies the successor revision.
    final lacksResultPresentationAuthority =
        !editUsesExactFallback &&
        projectionReceipt == null &&
        _pendingPresentation.dependency == null;
    final structuralOneShotRequiresCertification =
        !editUsesExactFallback &&
        structurals.isNotEmpty &&
        projectionReceipt != null &&
        !projectionReceipt.chainResultCell;
    _applyOptimisticViewportEdit(start, end, replacement);
    var committedAfterSelection = afterSelection;
    if (recenterAfterOptimisticEdit) {
      _restoreSelectionSnapshot(afterSelection);
    }
    // Projection continuity is bound only after the exact splice is known.
    // Normalize the result caret now, before publishing it back to the text
    // service and before recording the Core history selection, so a rapid
    // successor cannot target newly hidden syntax (for example table padding).
    final normalizedInput = _normalizeProjectedSelection(_inputState.value);
    if (normalizedInput.selection != _inputState.value.selection) {
      _inputState.replaceValue(normalizedInput);
      _updateGlobalSelection();
      committedAfterSelection = _selectionSnapshot();
    }
    final parserResultCaret = projectionReceipt?.resultCaretUtf16;
    final selectionFollowsEffectiveSplice =
        committedAfterSelection.selection.isCollapsed &&
        committedAfterSelection.selection.extentOffset ==
            start + replacement.length;
    if (parserResultCaret != null &&
        _inputState.value.selection.isCollapsed &&
        selectionFollowsEffectiveSplice) {
      final localCaret = parserResultCaret - _inputState.globalUtf16Start;
      if (localCaret >= 0 && localCaret <= _inputState.value.text.length) {
        _inputState.replaceValue(
          _inputState.value.copyWith(
            selection: TextSelection.collapsed(
              offset: localCaret,
              affinity: _inputState.value.selection.affinity,
            ),
          ),
        );
        _updateGlobalSelection();
        committedAfterSelection = _selectionSnapshot();
      }
    }
    _parseTimer?.cancel();
    _parseTimer = null;
    final command = _coordinator.admitCommand(
      FlarkEditorCommandKind.sourceEdit,
      publishSourceImmediately: true,
    );
    final operation = _coordinator.queueEdit<FlarkCoreEditReceipt>(() async {
      final receipt = await _session.applyEditUtf16(
        start,
        end,
        replacement,
        beforeSelection: _coreSnapshot(beforeSelection),
        afterSelection: _coreSnapshot(committedAfterSelection),
        coalesceTyping: coalesceTyping,
        compositionGroup: compositionHistoryGroup,
        compositionFinal: compositionFinal,
      );
      if (restoreSelectionAfterCommit) {
        await _restoreHistorySelection(committedAfterSelection);
      }
      return receipt;
    });
    acceptanceWatch.stop();
    final requiresParserCertification =
        _publicationCertificationBarrierActive ||
        structuralSuccessorRequiresCertification ||
        structuralOneShotRequiresCertification ||
        exactFallbackHasStructuralLookahead ||
        exactFallbackHasCertifiedNeighbor ||
        lacksResultPresentationAuthority ||
        (requiresStructuralCertification &&
            projectionReceipt == null &&
            !editUsesExactFallback);
    final publication = requiresParserCertification
        ? FlarkQueuedEditPublication.retainPublishedUntilCertified
        : FlarkQueuedEditPublication.publishOptimistically;
    if (publication.requiresParserCertification) {
      _coordinator.beginPublicationBarrier();
    }
    final completion = _completeQueuedEdit(
      operation,
      command,
      acceptedAtEpochMicros,
      acceptanceWatch.elapsedMicroseconds,
      platformTiming,
      publication: publication,
    );
    _coordinator.trackSourceAdoption(completion);
    unawaited(completion);
    return publication;
  }

  bool _queueSemanticParagraphBreak(
    int localCaret, {
    FlarkPlatformInputTiming? platformTiming,
  }) {
    if (!_inputState.value.selection.isCollapsed ||
        _publicationCertificationBarrierActive) {
      return false;
    }
    final globalCaret = _inputState.globalUtf16Start + localCaret;
    final dependency = _pendingPresentation.dependency;
    if (dependency?.authority.continueWith(
          startUtf16: globalCaret,
          endUtf16: globalCaret,
          replacement: '\n',
        ) !=
        null) {
      // A parser-authored pending sequence owns this exact newline. Keep it
      // on the ordinary source-edit lane so Core can select the supplied
      // result snapshot; a structural intent would race or discard that
      // stronger pre-edit authority.
      return false;
    }
    final row = _activeCachedRow();
    if (row?.semanticCapabilities.insertParagraphBreakAsLiteral ?? false) {
      return false;
    }
    final neutralCaret = (_inputState.activeOrdinal ?? 0) < 0;
    final editableRange = row?.editableUtf16;
    final parserOwnedEmbeddedLineStart =
        row != null &&
        row.semanticCapabilities.insertParagraphBreakAtPhysicalLineStart &&
        _isPhysicalLineStartInsideRow(row, globalCaret);
    final rowEligible =
        row != null &&
        (row.semanticCapabilities.insertParagraphBreak ||
            parserOwnedEmbeddedLineStart);
    if (row != null && _viewportState.semanticCurrent && !rowEligible) {
      return false;
    }
    if (!rowEligible && !_inputState.semanticEditActive && !neutralCaret) {
      return false;
    }
    if (neutralCaret) _inputState.setSemanticEditActive(true);
    if (rowEligible) {
      _inputState.setSemanticEditActive(true);
      final editable = editableRange == null
          ? null
          : _mapViewportRange(editableRange);
      final listItem = row.listItem;
      final listPrefix = listItem?.prefixUtf16;
      final atListMarkerEnd =
          listPrefix != null &&
          listItem != null &&
          globalCaret ==
              _mapViewportRange(listPrefix).start +
                  listItem.markerOffset +
                  listItem.markerText.length;
      if (editable != null &&
          (globalCaret < editable.start || globalCaret > editable.end)) {
        // A retained certified row can border the exact neutral island
        // created by the last semantic receipt. While recertification is
        // pending, the lane remains authoritative and Rust reclassifies the
        // current source at the anchor; the stale row range is not a gate.
        // A paragraph can own multiple physical source lines while exposing
        // only one primary editable range. An exact embedded line start is
        // still parser-owned; Rust's current-row context decides whether the
        // structural command is applicable at that boundary.
        if (_viewportState.semanticCurrent &&
            !parserOwnedEmbeddedLineStart &&
            !atListMarkerEnd) {
          return false;
        }
      }
    }
    _queueSemanticEdit(
      FlarkCoreEditIntentV1.insertParagraphBreak,
      fallbackWhenNotApplied: FlarkDeferredInputCommand.insertNewline,
      platformTiming: platformTiming,
    );
    return true;
  }

  bool _queueSemanticDeleteBackward(
    int localCaret, {
    FlarkPlatformInputTiming? platformTiming,
  }) {
    if (!_inputState.value.selection.isCollapsed ||
        _publicationCertificationBarrierActive) {
      return false;
    }
    final row = _activeCachedRow();
    final neutralLineStart =
        (_inputState.activeOrdinal ?? 0) < 0 && localCaret == 0;
    final retainedSemanticWindowStart =
        localCaret == 0 && _inputState.semanticEditActive;
    final retainedNeutralSemanticCaret =
        (_inputState.activeOrdinal ?? 0) < 0 && _inputState.semanticEditActive;
    final editableRange = row?.editableUtf16;
    final globalCaret = _inputState.globalUtf16Start + localCaret;
    final atInlineSemanticBoundary =
        row != null &&
        _isParserOwnedInlineBoundary(row, globalCaret, backward: true);
    final projectedStructuralRow =
        row?.semanticCapabilities.deleteBackwardAtProjectionStart ?? false;
    final rowEligible =
        row != null &&
        (row.semanticCapabilities.deleteBackwardAtEditableStart ||
            projectedStructuralRow ||
            row.semanticCapabilities.deleteBackwardAtPhysicalLineStart ||
            atInlineSemanticBoundary);
    if (row != null &&
        _viewportState.semanticCurrent &&
        !rowEligible &&
        !retainedSemanticWindowStart) {
      return false;
    }
    if (!rowEligible && !retainedNeutralSemanticCaret && !neutralLineStart) {
      if (!retainedSemanticWindowStart) return false;
    }
    if (neutralLineStart) _inputState.setSemanticEditActive(true);
    if (rowEligible) {
      _inputState.setSemanticEditActive(true);
      final editable = _mapViewportRange(editableRange!);
      final fencedPhysicalLineStart =
          row.semanticCapabilities.deleteBackwardAtPhysicalLineStart &&
          _isPhysicalLineStartInsideRow(row, globalCaret);
      final atStructuralSegmentStart =
          fencedPhysicalLineStart ||
          (projectedStructuralRow &&
              row.projectionSegments!.any(
                (segment) =>
                    _mapViewportRange(segment.sourceUtf16).start == globalCaret,
              )) ||
          (!_viewportState.semanticCurrent &&
              _pendingPresentation.structuralSurfaces.any((state) {
                final surface = state.surface;
                final runs = surface.presentation.runs;
                for (var index = 0; index < runs.length; index += 1) {
                  final run = runs[index];
                  if (run.sourceUtf16Start != globalCaret) continue;
                  final precedingEnd = index == 0
                      ? surface.sourceUtf16.start
                      : runs[index - 1].sourceUtf16End;
                  if (precedingEnd < globalCaret) return true;
                }
                return false;
              }));
      if (!atStructuralSegmentStart &&
          !atInlineSemanticBoundary &&
          globalCaret != editable.start &&
          (_viewportState.semanticCurrent || localCaret != 0) &&
          !retainedSemanticWindowStart) {
        return false;
      }
    }
    _queueSemanticEdit(
      FlarkCoreEditIntentV1.deleteBackward,
      fallbackWhenNotApplied: FlarkDeferredInputCommand.deleteBackward,
      platformTiming: platformTiming,
    );
    return true;
  }

  bool _queueSemanticDeleteForward(
    int localCaret, {
    FlarkPlatformInputTiming? platformTiming,
  }) {
    if (!_inputState.value.selection.isCollapsed ||
        _publicationCertificationBarrierActive) {
      return false;
    }
    final row = _activeCachedRow();
    final editableRange = row?.editableUtf16;
    if (row == null || editableRange == null) return false;
    final editable = _mapViewportRange(editableRange);
    final globalCaret = _inputState.globalUtf16Start + localCaret;
    final atInlineSemanticBoundary = _isParserOwnedInlineBoundary(
      row,
      globalCaret,
      backward: false,
    );
    final parserOwnedForwardStart =
        row.semanticCapabilities.deleteForwardAtEditableStart &&
        globalCaret == editable.start &&
        _rowSemanticsCurrent(editable);
    if (!parserOwnedForwardStart && !atInlineSemanticBoundary) {
      return false;
    }
    _inputState.setSemanticEditActive(true);
    _queueSemanticEdit(
      FlarkCoreEditIntentV1.deleteForward,
      fallbackWhenNotApplied: FlarkDeferredInputCommand.deleteForward,
      platformTiming: platformTiming,
    );
    return true;
  }

  /// Routes only from parser-authored capability. Rust revalidates the exact
  /// revision and owner closure when the semantic command executes.
  bool _isParserOwnedInlineBoundary(
    FlarkViewportRow row,
    int globalCaret, {
    required bool backward,
  }) {
    final visibleEnd =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    for (final fact in row.inlineFacts ?? const <FlarkInlineFact>[]) {
      if (!fact.supportsEmptyOwnerDelete) continue;
      final content = _mapViewportRange(fact.contentUtf16);
      final atBoundary = backward
          ? content.end == globalCaret
          : content.start == globalCaret;
      if (!atBoundary ||
          content.start < _viewportState.visibleUtf16Start ||
          content.end > visibleEnd) {
        continue;
      }
      return true;
    }
    return false;
  }

  bool _isPhysicalLineStartInsideRow(FlarkViewportRow row, int globalCaret) {
    final source = _mappedExactRowRange(row);
    if (globalCaret <= source.start || globalCaret >= source.end) return false;
    final previous = _sliceVisibleUtf16(globalCaret - 1, globalCaret);
    return previous == '\n' || previous == '\r';
  }

  void _queueSemanticEdit(
    FlarkCoreEditIntentV1 intent, {
    FlarkDeferredInputCommand? fallbackWhenNotApplied,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    _ensureSemanticInputBarrier(platformTiming: platformTiming);
    final admittedInput = _inputTransactions.pendingSemantic!;
    admittedInput.fallbackWhenNotApplied = fallbackWhenNotApplied;
    _breakTypingHistoryGroup();
    _parseTimer?.cancel();
    _parseTimer = null;
    final command = _coordinator.admitCommand(
      FlarkEditorCommandKind.semanticEdit,
    );
    final operation = _coordinator.afterEdits(
      () => _session.applyEditIntentOutcomeV1(
        intent,
        compositionActive: _session.compositionActive,
      ),
    );
    final completion = _completeSemanticEdit(operation, command, admittedInput);
    _coordinator.trackEdit(completion);
    unawaited(completion);
    // Queue admission changes no source, selection, or presentation. Publishing
    // here would stamp the retained pre-command frame with the new command
    // generation before the native receipt commits its source transaction.
    // The receipt (or failure) publishes the next observable state atomically.
  }

  /// Routes Tab/Shift-Tab through the same Rust-authoritative transaction
  /// seam as Return and structural deletion. Flutter only admits a currently
  /// certified simple list row; Rust decides whether the item can move and
  /// returns the exact indentation splice.
  bool handleListIndent({required bool outdent}) {
    if (_closed || _status == FlarkEditorStatus.faulted) return false;
    final row = _activeCachedRow();
    if (row == null || surfaceRow(row, includeEditingState: false).kind == 0) {
      return false;
    }
    final item = row.listItem;
    final editableUtf16 = row.editableUtf16;
    if (item == null || !item.simpleContinuation || editableUtf16 == null) {
      return false;
    }
    // Once Tab belongs to a list item it must not escape into Flutter focus
    // traversal, even while an incompatible composition/selection is active.
    if (!_inputState.value.selection.isCollapsed ||
        _session.compositionActive ||
        _inputTransactions.pendingSemantic != null ||
        _pendingPresentation.dependency != null ||
        _pendingPresentation.paragraphGap != null ||
        _pendingPresentation.structuralSurfaces.isNotEmpty) {
      return true;
    }
    final editable = _mapViewportRange(editableUtf16);
    final caret = _inputState.selectionExtentUtf16;
    if (caret < editable.start || caret > editable.end) return false;
    _inputState.setSemanticEditActive(true);
    _queueSemanticEdit(
      outdent
          ? FlarkCoreEditIntentV1.outdentListItem
          : FlarkCoreEditIntentV1.indentListItem,
    );
    return true;
  }

  /// Toggles a parser-certified GFM task checkbox without moving selection or
  /// asking Flutter to synthesize Markdown. The row contributes only a
  /// bounded target position; Rust returns the committed one-byte splice.
  FlarkViewportRow? _toggleableTaskRow(FlarkViewportRow row) {
    if (_closed ||
        _status == FlarkEditorStatus.faulted ||
        _session.compositionActive ||
        _inputTransactions.pendingSemantic != null ||
        _pendingPresentation.dependency != null ||
        _pendingPresentation.paragraphGap != null ||
        _pendingPresentation.structuralSurfaces.isNotEmpty ||
        (!_viewportState.semanticCurrent &&
            _pendingPresentation.taskChecks.isEmpty)) {
      return null;
    }
    FlarkViewportRow? current;
    for (final candidate in _viewportState.rows) {
      if (candidate.ordinal == row.ordinal) {
        current = candidate;
        break;
      }
    }
    final item = current?.listItem;
    final editable = current?.editableUtf16;
    if (current == null || item?.taskChecked == null || editable == null) {
      return null;
    }
    return current;
  }

  /// Whether the currently published parser result authorizes the task
  /// action. A retained edit-cell surface is presentation evidence only: it
  /// must never make an otherwise stale structural action discoverable.
  bool canToggleTaskChecked(FlarkViewportRow row) =>
      _toggleableTaskRow(row) != null;

  Future<bool> toggleTaskChecked(FlarkViewportRow row) {
    final current = _toggleableTaskRow(row);
    if (current == null) return Future<bool>.value(false);
    final editable = current.editableUtf16!;
    final target = _mapViewportRange(editable).start;
    _breakTypingHistoryGroup();
    _parseTimer?.cancel();
    _parseTimer = null;
    final command = _coordinator.admitCommand(
      FlarkEditorCommandKind.semanticAction,
    );
    final operation = _coordinator.afterEdits(
      () => _session.applySemanticActionV1(
        FlarkCoreSemanticActionV1.toggleTaskChecked,
        targetUtf16: target,
      ),
    );
    final completion = _completeTaskToggle(operation, command, current.ordinal);
    _coordinator.trackEdit(completion);
    notifyListeners();
    return completion;
  }

  Future<bool> _completeTaskToggle(
    Future<FlarkCoreEditIntentReceiptV1> operation,
    FlarkEditorCommandTicket command,
    int rowOrdinal,
  ) async {
    final generation = command.generation;
    try {
      final receipt = await operation;
      if (!receipt.hasCommit) {
        _coordinator.completeCommand(command);
        if (generation == _editGeneration) {
          _status = _idleStatus(current: _viewportState.semanticCurrent);
        }
        notifyListeners();
        return false;
      }
      final checked = switch (receipt.replacement) {
        'x' => true,
        ' ' => false,
        _ => throw StateError('Invalid task-toggle replacement receipt'),
      };
      if (generation != _editGeneration) {
        // A later optimistic edit is already the published source. The task
        // action committed first in the native command queue, but applying
        // its older coordinate splice or generation to that newer host
        // publication would tear source identity. The later edit's refresh
        // observes both commits atomically.
        _coordinator.completeCommand(command);
        return true;
      }
      if (!_applyLengthNeutralViewportReplacement(
        receipt.baseUtf16Start,
        receipt.baseUtf16End,
        receipt.replacement,
      )) {
        throw StateError('Task-toggle receipt fell outside its visible row');
      }
      _applyLengthNeutralInputReplacement(
        receipt.baseUtf16Start,
        receipt.baseUtf16End,
        receipt.replacement,
      );
      _coordinator.publishCommandSource(command);
      _setPendingTaskCheck(rowOrdinal, checked);
      // Publish the receipt-backed prefix immediately; parser recertification
      // may take several bounded pumps on a large document.
      notifyListeners();
      if (generation == _editGeneration) {
        await _refreshViewport(
          restoreInputWindow: false,
          expectedEditGeneration: generation,
          ensureActiveInputVisible: true,
        );
        if (generation == _editGeneration) _scheduleParsingAfterInput();
      }
      _coordinator.completeCommand(command);
      notifyListeners();
      return true;
    } catch (error) {
      _coordinator.failCommand(command, error);
      notifyListeners();
      return false;
    }
  }

  void _ensureSemanticInputBarrier({FlarkPlatformInputTiming? platformTiming}) {
    if (_inputTransactions.pendingSemantic != null) return;
    _inputTransactions.lateSemantic = null;
    final timing = platformTiming ?? _inputTransactions.activeTiming;
    _inputTransactions.pendingSemantic = FlarkPendingSemanticInput(
      base: _inputState.value,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      initialCallbackStartedEpochMicros:
          timing?.acceptedAtEpochMicros ??
          _inputTransactions.activeCallbackStartedEpochMicros ??
          DateTime.now().microsecondsSinceEpoch,
      platformTiming: timing,
      provisionalAfter: _inputState.value,
    );
    if (timing != null) {
      _inputTransactions.pendingSemantic!.initialCallbackMicros =
          timing.editorSyncMicros;
    }
  }

  Future<void> _completeSemanticEdit(
    Future<FlarkCoreEditIntentOutcomeV1> operation,
    FlarkEditorCommandTicket command,
    FlarkPendingSemanticInput admittedInput,
  ) async {
    final generation = command.generation;
    try {
      final outcome = await operation;
      final receipt = outcome.receipt;
      _debugLastSemanticReceiptDescription = jsonEncode({
        'disposition': receipt.disposition.name,
        'baseRevision': receipt.baseRevision,
        'resultRevision': receipt.resultRevision,
        'base': [receipt.baseUtf16Start, receipt.baseUtf16End],
        'replacement': receipt.replacement,
        'selection': receipt.resultSelectionUtf16,
      });
      final adoptionWatch = Stopwatch()..start();
      var requireParserCertification = _publicationCertificationBarrierActive;
      if (receipt.hasCommit) {
        final commandIsCurrent = _coordinator.publishCommandSource(command);
        if (commandIsCurrent) {
          // A semantic splice is new parser authority and is not constrained
          // by a predecessor literal envelope. Keep that envelope painted
          // while the command is merely in flight, then retire it atomically
          // with the committing receipt. A stale receipt is allowed to settle
          // its native command but cannot alter the newer command's host
          // presentation or continuation.
          _coordinator.retirePendingPresentation(const {
            FlarkPendingPresentationPart.dependency,
          });
          _inputState.restoreInlineContinuation(outcome.inlineContinuation);
          requireParserCertification =
              _adoptSemanticReceipt(command, receipt) ||
              requireParserCertification;
          if (requireParserCertification) {
            _coordinator.beginPublicationBarrier();
          } else {
            _promoteSemanticSuccessors(receipt);
          }
        }
      } else {
        _inputState.setSemanticEditActive(false);
        final pending = _inputTransactions.pendingSemantic;
        if (pending?.provisionalMutation != null) {
          _promoteUncommittedPlatformMutation();
        } else if (pending != null) {
          final fallback = pending.fallbackWhenNotApplied;
          if (fallback != null) {
            pending.successors.insert(
              0,
              FlarkDeferredInputSuccessor(
                fallback,
                semanticAlreadyAttempted: true,
                platformTiming: pending.platformTiming,
              ),
            );
          }
          _promoteUncommittedSemanticSuccessors();
        }
      }
      adoptionWatch.stop();
      if (admittedInput.provisionalMutation != null ||
          admittedInput.platformTiming != null) {
        final telemetry = receipt.telemetry;
        final performance = FlarkSemanticEditPerformance(
          sourceGeneration: generation,
          acceptedAtEpochMicros:
              admittedInput.initialCallbackStartedEpochMicros,
          platformCallbackMicros: admittedInput.initialCallbackMicros,
          coreQueueMicros: telemetry.coreQueueMicros,
          workerRoundTripMicros: telemetry.workerRoundTripMicros,
          workerQueueMicros: telemetry.workerQueueMicros,
          nativeFfiMicros: telemetry.nativeFfiMicros,
          coreAdoptionMicros: telemetry.coreAdoptionMicros,
          flutterReceiptAdoptionMicros: adoptionWatch.elapsedMicroseconds,
          callbackToReceiptMicros: math.max(
            0,
            DateTime.now().microsecondsSinceEpoch -
                admittedInput.initialCallbackStartedEpochMicros,
          ),
        );
        _performance.recordSemantic(performance);
      }
      if (generation != _editGeneration) {
        // A same-burst successor already owns the next observable source.
        // Publishing completion of this predecessor would pair that newer
        // source with predecessor or exact-fallback presentation. The
        // successor publishes after retaining proof or refreshing semantics.
        _coordinator.completeCommand(command);
        return;
      }
      if (!receipt.hasCommit) {
        _status = _idleStatus(current: _viewportState.semanticCurrent);
        _coordinator.completeCommand(command);
        notifyListeners();
        return;
      }
      if (requireParserCertification) {
        // A clear-only or absent transition changes authoritative source but
        // carries no result-surface geometry. Publishing a pending query here
        // would map stale row endings through the structural splice and can
        // flash an extra or missing blank line. Complete the bounded parser
        // handoff off-callback, then publish source, rows, and selection as
        // one transaction.
        await _refreshEditPublicationAfterCertification(
          generation,
          // This is the terminal atomic handoff for a semantic edit whose
          // provisional input could not safely publish. Fresh certified rows
          // must also canonicalize the platform window; otherwise an
          // interleaved delta can leave the entire predecessor viewport as
          // the input surrogate even after the caret moves to a successor
          // row, with no later parse task left to repair it.
          restoreInputWindow: true,
        );
        _coordinator.endPublicationBarrierForEdit(generation);
        // Successors observed behind a receipt with no safe result surface
        // are expressed in the platform's provisional coordinates. Promote
        // them only after the same bounded certification handoff that already
        // gates visual publication: the refreshed input window then supplies
        // the parser-canonical rendered caret (including source padding that
        // has no visible caret stop). Promoting earlier makes rapid typing land
        // differently from the identical settled command sequence.
        if (generation == _editGeneration && !_closed) {
          _promoteSemanticSuccessors(receipt);
        }
        if (generation != _editGeneration) {
          _coordinator.completeCommand(command);
          return;
        }
      } else {
        await _refreshViewport(
          restoreInputWindow: false,
          expectedEditGeneration: generation,
          ensureActiveInputVisible: true,
        );
        if (generation == _editGeneration) _scheduleParsingAfterInput();
      }
      _coordinator.completeCommand(command);
      notifyListeners();
    } catch (error) {
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
      });
      _inputTransactions.discardPendingSemantic();
      _coordinator.failCommand(command, error);
      notifyListeners();
    }
  }

  bool _adoptSemanticReceipt(
    FlarkEditorCommandTicket command,
    FlarkCoreEditIntentReceiptV1 receipt,
  ) {
    final transition = resolvePendingPresentationTransition(
      receipt: receipt,
      pendingPresentation: _pendingPresentation,
      activeOrdinal: _inputState.activeOrdinal,
      priorRows: _viewportState.rows
          .map(
            (row) => FlarkSurfaceProjector.corePresentationFromSurface(
              surfaceRow(row, includeEditingState: false),
              surfaceSourceRange(row),
            ),
          )
          .toList(growable: false),
    );
    final adoption = _coordinator.adoptCommittedPresentation(
      command: command,
      receipt: receipt,
      transition: transition,
    );
    if (adoption == null) return false;
    // The semantic receipt supplies a result-revision byte/UTF-16 pair even
    // when its structural splice crosses the cached page boundary. Preserve
    // that authoritative origin before the optimistic cache update can clear
    // the old viewport; the next query will rewind it to any enclosing row.
    _viewportPager.pinRefreshAnchor(
      FlarkViewportPageAnchor(
        byte: receipt.resultByteStart,
        utf16: receipt.resultUtf16Start,
      ),
    );
    _applyOptimisticViewportEdit(
      receipt.baseUtf16Start,
      receipt.baseUtf16End,
      receipt.replacement,
      preservesMappedRowFacts: false,
    );
    if (adoption.removedRowOrdinals.isNotEmpty) {
      _viewportState.removeRows(adoption.removedRowOrdinals);
    }
    final caret = receipt.resultSelectionUtf16;
    _inputState.setCanonicalSelection(caret, caret);
    _inputState.setCrossRowSelection(false);
    _inputState.clearOversizedSelection();
    _inputState.retargetActiveOrdinal(_surfaceOrdinalAt(caret));
    if (!_installCommittedSemanticInputWindow(receipt, caret)) {
      _restoreCollapsedInputWindow(
        caret,
        preferredOrdinal: _inputState.activeOrdinal,
      );
    }
    final normalized = _normalizeProjectedSelection(_inputState.value);
    if (normalized.selection != _inputState.value.selection) {
      _inputState.replaceValue(normalized);
      _updateGlobalSelection();
      unawaited(
        _installCanonicalSelection(_selectionSnapshot(), publish: false),
      );
    }
    return adoption.requiresParserCertification;
  }

  bool _installCommittedSemanticInputWindow(
    FlarkCoreEditIntentReceiptV1 receipt,
    int caret,
  ) {
    final pending = _inputTransactions.pendingSemantic;
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
      _inputState.replaceWindow(
        globalUtf16Start: resultWindowStart,
        value: pending.base.copyWith(
          selection: TextSelection.collapsed(offset: localCaret),
          composing: TextRange.empty,
        ),
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
    _inputState.replaceWindow(
      globalUtf16Start: pending.inputGlobalUtf16Start,
      value: TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: localCaret),
      ),
    );
    return true;
  }

  void _promoteSemanticSuccessors(FlarkCoreEditIntentReceiptV1 receipt) {
    final stopwatch = Stopwatch()..start();
    final pending = _inputTransactions.takePendingSemantic();
    try {
      if (pending == null) return;
      // Direct semantic commands may commit a structural splice that starts
      // outside the currently exposed bounded input window. Receipt adoption
      // has already installed the authoritative source/selection state; with
      // no provisional platform edit and no queued successor, there is
      // nothing to reconcile against that old window.
      if (pending.provisionalMutation == null && pending.successors.isEmpty) {
        return;
      }
      final reconciliation = FlarkInputReconciliationMap.forSemanticBarrier(
        pending: pending,
        receipt: receipt,
        canonicalResultSelectionUtf16: _inputState.selectionExtentUtf16,
        committedInputGlobalUtf16Start: _inputState.globalUtf16Start,
        committedInputLength: _inputState.value.text.length,
      );
      if (reconciliation == null) {
        _inputTransactions.completeDeferredHistorySuccessors(
          pending.successors,
          false,
        );
        _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
        return;
      }
      final resyncCount = _platformInput.resyncCount;
      _promoteSemanticSuccessorsWithMap(pending, reconciliation);
      if (pending.provisionalMutation != null &&
          _inputTransactions.pendingSemantic == null &&
          _platformInput.resyncCount == resyncCount) {
        _inputTransactions.lateSemantic = FlarkLateSemanticInput(
          provisionalTail: pending.provisionalTail,
          reconciliation: reconciliation,
          successorCount: pending.successors.length,
        );
      }
    } finally {
      stopwatch.stop();
      _inputTransactions.recordReconciliationMicros(
        stopwatch.elapsedMicroseconds,
      );
    }
  }

  void _promoteUncommittedSemanticSuccessors() {
    final stopwatch = Stopwatch()..start();
    final pending = _inputTransactions.takePendingSemantic();
    _inputTransactions.lateSemantic = null;
    try {
      if (pending == null) return;
      _promoteSemanticSuccessorsWithMap(
        pending,
        const FlarkInputReconciliationMap(
          fromStart: 0,
          fromEnd: 0,
          toStart: 0,
          toEnd: 0,
        ),
      );
    } finally {
      stopwatch.stop();
      _inputTransactions.recordReconciliationMicros(
        stopwatch.elapsedMicroseconds,
      );
    }
  }

  void _promoteUncommittedPlatformMutation() {
    final pending = _inputTransactions.takePendingSemantic();
    _inputTransactions.lateSemantic = null;
    if (pending == null || pending.provisionalMutation == null) return;
    final mutation = pending.provisionalMutation!;
    final provisional = TextEditingValue(
      text: pending.base.text.replaceRange(
        mutation.start,
        mutation.end,
        mutation.replacement,
      ),
      selection: TextSelection.collapsed(
        offset: mutation.start + mutation.replacement.length,
      ),
    );
    final acceptance = _acceptMutation(
      mutation,
      selection: provisional.selection,
      composing: provisional.composing,
      fullValue: provisional.text.length <= _maximumInputCodeUnits
          ? provisional
          : null,
      platformTiming: pending.platformTiming,
      editabilityProven: _editorOwnedBoundaryContains(
        pending.inputGlobalUtf16Start + mutation.start,
        pending.inputGlobalUtf16Start + mutation.end,
      ),
    );
    if (!acceptance.accepted) {
      _inputTransactions.completeDeferredHistorySuccessors(
        pending.successors,
        false,
      );
      _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
      return;
    }
    _promoteSemanticSuccessorsWithMap(
      pending,
      const FlarkInputReconciliationMap(
        fromStart: 0,
        fromEnd: 0,
        toStart: 0,
        toEnd: 0,
      ),
    );
  }

  void _promoteSemanticSuccessorsWithMap(
    FlarkPendingSemanticInput pending,
    FlarkInputReconciliationMap reconciliation,
  ) {
    for (var index = 0; index < pending.successors.length; index += 1) {
      final successor = pending.successors[index];
      if (successor is FlarkDeferredHistorySuccessor) {
        unawaited(
          _queueHistoryReplay(undoDirection: successor.undoDirection).then(
            (replayed) {
              if (!replayed) _finalizeDroppedHistoryInputWindow();
              successor.completion.complete(replayed);
            },
            onError: (Object error, StackTrace stackTrace) {
              successor.completion.completeError(error, stackTrace);
            },
          ),
        );
        continue;
      }
      if (successor case FlarkDeferredInputSuccessor(
        command: final command,
        replacement: final replacement,
        typingInput: final typingInput,
        semanticAlreadyAttempted: final semanticAlreadyAttempted,
        reclassifyAfterCertification: final reclassifyAfterCertification,
      )) {
        if (replacement != null) {
          _replaceSelection(
            replacement,
            typingInput: typingInput,
            platformTiming: successor.platformTiming,
            // This edit was accepted while the semantic predecessor was still
            // in flight. Its native receipt is the first point where source,
            // selection, and presentation can be published as one frame.
            publish: false,
          );
        } else if (reclassifyAfterCertification ||
            _publicationCertificationBarrierActive) {
          // A preceding successor in this same promotion pass can create a
          // new parser-certification barrier. Route every later command back
          // through the ordinary command gate so it is classified against
          // the certified result row, rather than queueing a semantic intent
          // with the predecessor's geometry and leaving the input shadow
          // stranded when that intent is not applicable.
          _promoteReclassifiedCommand(
            command!,
            platformTiming: successor.platformTiming,
          );
        } else {
          _promoteDeferredCommand(
            command!,
            semanticAlreadyAttempted: semanticAlreadyAttempted,
            platformTiming: successor.platformTiming,
          );
        }
        final nextBarrier = _inputTransactions.pendingSemantic;
        if (nextBarrier != null) {
          if (index + 1 < pending.successors.length) {
            nextBarrier.successors.addAll(pending.successors.skip(index + 1));
            nextBarrier.provisionalTail = pending.provisionalTail;
            _inputTransactions.observePendingSuccessors(nextBarrier);
          }
          return;
        }
        continue;
      }
      final batch = successor as FlarkProvisionalInputBatch;
      final selection = _mapProvisionalSelection(
        reconciliation,
        batch.after.selection,
      );
      final composing = _mapProvisionalRange(
        reconciliation,
        batch.after.composing,
      );
      if (selection == null || composing == null) {
        _inputTransactions.completeDeferredHistorySuccessors(
          pending.successors.skip(index),
          false,
        );
        _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
        return;
      }
      final mutation = _differenceMutation(batch.before.text, batch.after.text);
      if (mutation == null) {
        _breakTypingHistoryGroup();
        _inputState.replaceValue(
          _inputState.value.copyWith(
            selection: selection,
            composing: composing,
          ),
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
      final continuation = _inputState.inlineContinuation;
      final continuationLocalCaret = continuation == null
          ? null
          : continuation.caretUtf16 - _inputState.globalUtf16Start;
      final continuesAtCanonicalCaret =
          continuationLocalCaret != null &&
          batch.typingInput &&
          mutation.start == mutation.end &&
          batch.after.selection.isCollapsed &&
          !batch.after.composing.isValid &&
          0 <= continuationLocalCaret &&
          continuationLocalCaret <= _inputState.value.text.length;
      final promotedStart = continuesAtCanonicalCaret
          ? continuationLocalCaret
          : mappedStart;
      final promotedEnd = continuesAtCanonicalCaret
          ? continuationLocalCaret
          : mappedEnd;
      final promotedSelection = continuesAtCanonicalCaret
          ? TextSelection.collapsed(
              offset: continuationLocalCaret + mutation.replacement.length,
              affinity: selection.affinity,
            )
          : selection;
      final promotedComposing = continuesAtCanonicalCaret
          ? TextRange.empty
          : composing;
      final acceptance = promotedStart == null || promotedEnd == null
          ? null
          : _acceptMutation(
              FlarkTextMutation(
                promotedStart,
                promotedEnd,
                mutation.replacement,
              ),
              selection: promotedSelection,
              composing: promotedComposing,
              typingInput: batch.typingInput,
              platformTiming: batch.platformTiming,
            );
      if (acceptance?.accepted != true) {
        _inputTransactions.completeDeferredHistorySuccessors(
          pending.successors.skip(index),
          false,
        );
        _resynchronize(FlarkInputResyncReason.successorReconciliationFailed);
        return;
      }
    }
  }

  void _promoteReclassifiedCommand(
    FlarkDeferredInputCommand command, {
    FlarkPlatformInputTiming? platformTiming,
  }) {
    switch (command) {
      case FlarkDeferredInputCommand.deleteBackward:
        _deleteBackward(allowSemantic: true, platformTiming: platformTiming);
      case FlarkDeferredInputCommand.deleteForward:
        _deleteForward(allowSemantic: true, platformTiming: platformTiming);
      case FlarkDeferredInputCommand.insertNewline:
        _insertNewline(allowSemantic: true, platformTiming: platformTiming);
    }
  }

  void _promoteDeferredCommand(
    FlarkDeferredInputCommand command, {
    required bool semanticAlreadyAttempted,
    FlarkPlatformInputTiming? platformTiming,
  }) {
    if (!semanticAlreadyAttempted) {
      _inputState.setSemanticEditActive(true);
      _queueSemanticEdit(
        switch (command) {
          FlarkDeferredInputCommand.deleteBackward =>
            FlarkCoreEditIntentV1.deleteBackward,
          FlarkDeferredInputCommand.deleteForward =>
            FlarkCoreEditIntentV1.deleteForward,
          FlarkDeferredInputCommand.insertNewline =>
            FlarkCoreEditIntentV1.insertParagraphBreak,
        },
        fallbackWhenNotApplied: command,
        platformTiming: platformTiming,
      );
      return;
    }
    // A genuine Rust not-applicable result hands the command back to the
    // literal lane. Receipt adoption may retain a zero-offset fragment
    // beginning at the caret, so rebuild its exact input window while
    // preserving the active row's parser-certified projection. Falling back
    // to an artificial neutral row here would expose inline delimiters to the
    // next Backspace/Delete command in the same event-loop burst.
    _restoreCollapsedInputWindow(
      _inputState.selectionExtentUtf16,
      preferredOrdinal: _inputState.activeOrdinal,
    );
    switch (command) {
      case FlarkDeferredInputCommand.deleteBackward:
        _deleteBackward(allowSemantic: false, platformTiming: platformTiming);
      case FlarkDeferredInputCommand.deleteForward:
        _deleteForward(allowSemantic: false, platformTiming: platformTiming);
      case FlarkDeferredInputCommand.insertNewline:
        _insertNewline(allowSemantic: false, platformTiming: platformTiming);
    }
  }

  TextSelection? _mapProvisionalSelection(
    FlarkInputReconciliationMap map,
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
    FlarkInputReconciliationMap map,
    TextRange range,
  ) {
    if (range == TextRange.empty) return TextRange.empty;
    final start = map.mapOffset(range.start, downstream: true);
    final end = map.mapOffset(range.end, downstream: true);
    if (start == null || end == null) return null;
    return TextRange(start: start, end: end);
  }

  FlarkTextMutation? _differenceMutation(String before, String after) =>
      _platformInput.differenceMutation(before, after);

  int _committedGapEnd(FlarkCoreCommittedPresentationGapV1 split) {
    var end =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    final localStart = split.rowEndUtf16 - _viewportState.visibleUtf16Start;
    if (0 <= localStart && localStart < _viewportState.visibleSource.length) {
      final newline = _viewportState.visibleSource.indexOf('\n', localStart);
      if (newline >= 0) end = _viewportState.visibleUtf16Start + newline + 1;
    }
    for (final row in _viewportState.rows) {
      if (row.ordinal == split.rowOrdinal) continue;
      final start = surfaceSourceRange(row).start;
      if (start > split.rowEndUtf16) end = math.min(end, start);
    }
    return end;
  }

  int _committedCaretBoundaryEnd(FlarkPendingCaretBoundary boundary) {
    var end =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    for (final row in _viewportState.rows) {
      if (row.ordinal == boundary.rowOrdinal) continue;
      final start = surfaceSourceRange(row).start;
      if (start >= boundary.rowEndUtf16) end = math.min(end, start);
    }
    return end;
  }

  int? _committedCaretBoundaryInputEnd(FlarkPendingCaretBoundary boundary) {
    final localStart = boundary.rowEndUtf16 - _viewportState.visibleUtf16Start;
    if (localStart < 0 || localStart > _viewportState.visibleSource.length) {
      return null;
    }
    final newline = _viewportState.visibleSource.indexOf('\n', localStart);
    return newline == -1
        ? _viewportState.visibleUtf16Start + _viewportState.visibleSource.length
        : _viewportState.visibleUtf16Start + newline + 1;
  }

  bool _editorOwnedBoundaryContains(int start, int end) {
    if (start > end) return false;
    final gap = _pendingPresentation.paragraphGap;
    if (gap != null &&
        gap.rowEndUtf16 <= start &&
        end <= _committedGapEnd(gap)) {
      return true;
    }
    final boundary = _pendingPresentation.caretBoundary;
    final boundaryEnd = boundary == null
        ? null
        : _committedCaretBoundaryInputEnd(boundary);
    return boundary != null &&
        boundaryEnd != null &&
        boundary.rowEndUtf16 <= start &&
        end <= boundaryEnd;
  }

  FlarkProjectionEditCellReceipt? _prepareProjectionContinuity(
    int start,
    int end,
    String replacement,
  ) {
    final current = _pendingPresentation.dependency;
    if (current != null) {
      final successor = current.authority.continueWith(
        startUtf16: start,
        endUtf16: end,
        replacement: replacement,
      );
      if (successor != null) {
        final dependency = advancePendingDependencyPresentation(
          current: current,
          authority: successor,
          visibleSource: _viewportState.visibleSource,
          visibleUtf16Start: _viewportState.visibleUtf16Start,
          startUtf16: start,
          endUtf16: end,
          replacement: replacement,
        );
        if (dependency != null) {
          _coordinator.setPendingDependency(dependency);
          return successor is FlarkProjectionEditCellReceipt ? successor : null;
        }
      }
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
      });
      return null;
    }
    // Cached envelopes predate any optimistic edit. Only a fresh parser
    // publication can authorize another literal transaction.
    if (_viewportState.hasOptimisticEdits) {
      return null;
    }
    final row = _activeCachedRow();
    if (row == null) return null;
    final activation = _mapViewportRange(_activationRange(row));
    if (!_rowSemanticsCurrent(activation)) {
      return null;
    }
    final editable = _mapViewportRange(
      row.editableUtf16 ?? _activationRange(row),
    );
    final base = surfaceRow(row, includeEditingState: false);
    final authority = bindPendingDependencyAuthority(
      revision: revision,
      plans: row.pendingPresentationPlans,
      cells: row.projectionEditCells,
      envelopes: row.literalSafeEnvelopes,
      authorizedContentUtf16: editable,
      authorizedBlockUtf16: _mappedExactRowRange(row),
      startUtf16: start,
      endUtf16: end,
      replacement: replacement,
    );
    if (authority != null) {
      final dependency = bindPendingDependencyPresentation(
        rowOrdinal: row.ordinal,
        base: FlarkSurfaceProjector.corePresentationFromSurface(
          base,
          surfaceSourceRange(row),
        ),
        authority: authority,
        visibleSource: _viewportState.visibleSource,
        visibleUtf16Start: _viewportState.visibleUtf16Start,
        startUtf16: start,
        endUtf16: end,
        replacement: replacement,
      );
      if (dependency != null) {
        _coordinator.setPendingDependency(dependency);
        return authority is FlarkProjectionEditCellReceipt ? authority : null;
      }
    }
    _coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.dependency,
    });
    return null;
  }

  Future<void> _completeQueuedEdit(
    Future<FlarkCoreEditReceipt> operation,
    FlarkEditorCommandTicket command,
    int acceptedAtEpochMicros,
    int localEditorSyncMicros,
    FlarkPlatformInputTiming? platformTiming, {
    required FlarkQueuedEditPublication publication,
  }) async {
    final generation = command.generation;
    try {
      final receipt = await operation;
      final receiptAtEpochMicros = DateTime.now().microsecondsSinceEpoch;
      final adoptionWatch = Stopwatch()..start();
      void recordPerformance() {
        adoptionWatch.stop();
        final telemetry = receipt.telemetry;
        if (telemetry == null) return;
        _recordSourceEditPerformance(
          kind: FlarkSourceEditPerformanceKind.source,
          generation: generation,
          acceptedAtEpochMicros: acceptedAtEpochMicros,
          editorSyncMicros:
              platformTiming?.editorSyncMicros ?? localEditorSyncMicros,
          telemetry: telemetry,
          flutterReceiptAdoptionMicros: adoptionWatch.elapsedMicroseconds,
          acceptanceToReceiptMicros: math.max(
            0,
            receiptAtEpochMicros - acceptedAtEpochMicros,
          ),
        );
      }

      if (generation == _editGeneration) {
        if (publication.requiresParserCertification) {
          // This one-shot edit cell proves source placement and caret mapping,
          // but explicitly does not prove result block presentation. Drain the
          // bounded parser actor without publishing pending exact rows, then
          // install and publish the certified result atomically.
          await _refreshEditPublicationAfterCertification(
            generation,
            // Certification is the terminal publication for this edit.
            // Canonicalize the platform surrogate in the same transaction as
            // source and rows so delivery order cannot determine which row
            // remains editable after convergence.
            restoreInputWindow: true,
          );
          _coordinator.endPublicationBarrierForEdit(generation);
          if (generation == _editGeneration &&
              !_closed &&
              _certificationDeferredInputActive) {
            // Commands observed while this ordinary edit lacked result-row
            // authority were retained as bounded platform lineage, not
            // guessed from stale geometry. Reclassify them now against the
            // fresh certified rows. The promoted command owns the next
            // publication when it advances the generation.
            _promoteCertificationDeferredInput();
            if (generation != _editGeneration) {
              _coordinator.completeCommand(command);
              // The operation committed and yielded publication ownership to
              // its promoted successor. Its timing receipt remains valid and
              // must not disappear merely because the successor advanced the
              // controller generation before this completion published.
              recordPerformance();
              return;
            }
          }
        } else {
          final retainsProjectedTransition =
              _pendingPresentation.dependency != null ||
              _pendingPresentation.structuralSurfaces.any(
                (state) => state.continuity != null,
              );
          if (retainsProjectedTransition ||
              _canRetainOptimisticSurfaceAfterCommit()) {
            // The native edit is committed and the parser-authored continuity
            // surface, or the bounded optimistic cache, already pairs exact
            // result source with the best safe presentation. A pending viewport
            // query is strictly poorer authority: pending pages may carry no
            // rows and expand a short 32-row cache to the 16 KiB byte cap,
            // causing unrelated rendered rows to flash as source. Keep the
            // bounded publication and converge immediately. The touched row
            // still fails closed locally unless parser-authored continuity owns
            // it; unchanged rows retain only mapped predecessor facts.
            _scheduleParsingAfterInput(immediate: true);
          } else {
            final hadNoPriorSemanticRows = _viewportState.rows.isEmpty;
            await _refreshViewport(
              restoreInputWindow: false,
              expectedEditGeneration: generation,
              ensureActiveInputVisible: true,
            );
            if (generation == _editGeneration) {
              // A streamed-open refresh can legitimately publish an exact
              // pending-neutral row when no older semantic surface exists
              // (notably typing into a document just deleted to empty). That
              // is a current-revision presentation even though the unsealed
              // final paragraph is not yet parser-certified.
              _recordOpeningExactPublicationIfProven(
                hadNoPriorSemanticRows: hadNoPriorSemanticRows,
              );
              _scheduleParsingAfterInput();
            }
          }
        }
      }
      _coordinator.completeCommand(command);
      recordPerformance();
      notifyListeners();
    } catch (error) {
      if (_certificationDeferredInputActive) {
        _inputTransactions.discardPendingSemantic();
        _cancelCertificationDeferredInput();
      }
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
      });
      _coordinator.failCommand(command, error);
      notifyListeners();
    }
  }

  bool _canRetainOptimisticSurfaceAfterCommit() {
    if (_viewportState.rows.isEmpty ||
        _activeCachedRow() == null ||
        !_viewportState.allOptimisticEditsPreserveMappedRowFacts) {
      return false;
    }
    final visibleEnd =
        _viewportState.visibleUtf16Start + _viewportState.visibleSource.length;
    if (_inputState.selectionExtentUtf16 < _viewportState.visibleUtf16Start ||
        _inputState.selectionExtentUtf16 > visibleEnd) {
      return false;
    }
    return _viewportState.rows.every((row) {
      final mapped = _mapViewportRange(row.sourceUtf16);
      return _viewportState.visibleUtf16Start <= mapped.start &&
          mapped.end <= visibleEnd;
    });
  }

  Future<bool> undo() => _queueHistoryReplay(undoDirection: true);

  Future<bool> redo() => _queueHistoryReplay(undoDirection: false);

  /// Cancels the active platform composition through the authoritative native
  /// history unit. The composed source is rewound once, its redo inverse is
  /// discarded, and the precomposition source selection is restored.
  Future<bool> cancelComposition() {
    if (_closed ||
        _status == FlarkEditorStatus.faulted ||
        _coordinator.historyReplayPending ||
        !_session.compositionActive) {
      return Future<bool>.value(false);
    }
    _coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.dependency,
      FlarkPendingPresentationPart.paragraphGap,
      FlarkPendingPresentationPart.caretBoundary,
      FlarkPendingPresentationPart.structuralSurfaces,
    });
    _inputState.setSemanticEditActive(false);
    _breakTypingHistoryGroup();
    _parseTimer?.cancel();
    _parseTimer = null;
    final command = _coordinator.admitCommand(
      FlarkEditorCommandKind.compositionCancel,
    );
    final generation = command.generation;
    notifyListeners();

    final operation = _coordinator.queueEdit(() async {
      try {
        final outcome = await _session.cancelComposition();
        if (outcome == null) {
          _inputState.replaceValue(
            _inputState.value.copyWith(composing: TextRange.empty),
          );
          _scheduleParsingAfterInput();
          return true;
        }
        final restore = _adapterSnapshot(outcome.restoreSelection);
        _viewportState.clearOptimisticEdits();
        _clearPendingTaskChecks();
        await _refreshEditPublicationAfterCertification(
          generation,
          restoreInputWindow: false,
          prepareForRefresh: () => _restoreHistorySelection(restore),
        );
        // The pre-query restore supplies the exact deep source/byte anchor.
        // Reapply the same canonical selection after fresh rows install so
        // the platform input window expands to the certified active row.
        // Publish only after source, rows, and platform selection agree.
        await _restoreHistorySelection(restore);
        _scheduleParsingAfterInput();
        notifyListeners();
        return outcome is FlarkCoreHistoryReplayed;
      } finally {
        _inputTransactions.clearCompositionInputBase();
      }
    });
    unawaited(
      operation
          .then((cancelled) {
            _coordinator.completeCommand(command);
            if (!cancelled) {
              _status = _idleStatus(current: _viewportState.semanticCurrent);
            }
            notifyListeners();
          })
          .catchError((Object error, StackTrace stackTrace) {
            _coordinator.failCommand(command, error);
            notifyListeners();
          }),
    );
    return operation;
  }

  FlarkProjectionEditCellReceipt? _advanceCommittedStructuralSurfaces(
    int start,
    int end,
    String replacement,
  ) {
    final candidates =
        <
          ({
            int index,
            FlarkProjectionEditCellReceipt receipt,
            FlarkCorePresentationRow presentation,
          })
        >[];
    for (
      var index = 0;
      index < _pendingPresentation.structuralSurfaces.length;
      index++
    ) {
      final state = _pendingPresentation.structuralSurfaces[index];
      final surface = state.surface;
      if (!surface.projectionCurrent) continue;
      final authority =
          state.continuity?.continueWith(
            startUtf16: start,
            endUtf16: end,
            replacement: replacement,
          ) ??
          bindPendingDependencyAuthority(
            revision: revision,
            cells: surface.projectionEditCells,
            envelopes: const [],
            authorizedContentUtf16: surface.sourceUtf16,
            startUtf16: start,
            endUtf16: end,
            replacement: replacement,
          );
      final receipt = authority is FlarkProjectionEditCellReceipt
          ? authority
          : null;
      if (receipt == null) continue;
      final presentation = advancePendingPresentationRow(
        presentation: surface.presentation,
        authority: receipt,
        visibleSource: _viewportState.visibleSource,
        visibleUtf16Start: _viewportState.visibleUtf16Start,
        startUtf16: start,
        endUtf16: end,
        replacement: replacement,
      );
      if (presentation != null) {
        candidates.add((
          index: index,
          receipt: receipt,
          presentation: presentation,
        ));
      }
    }
    if (candidates.length != 1) return null;
    final matched = candidates.single;
    final states = [..._pendingPresentation.structuralSurfaces];
    final previous = states[matched.index].surface;
    final delta = replacement.length - (end - start);
    // The edit cell owns only the mutable closure inside this transitional
    // row. Advancing it must preserve the row's parser-authored block prefix
    // and terminal line ending; narrowing source ownership to the cell makes
    // those bytes reappear as neutral rows until the next parse publishes.
    final source = FlarkSourceRange(
      previous.sourceUtf16.start,
      previous.sourceUtf16.end + delta,
    );
    if (matched.receipt.affectedUtf16.start < source.start ||
        matched.receipt.affectedUtf16.end > source.end) {
      return null;
    }
    states[matched.index] = FlarkPendingStructuralSurface(
      // A one-shot receipt still proves the result of this edit. It consumes
      // future edit authority, not the transformed rendered surface. Keep the
      // result visible with no continuity so the next edit fails closed until
      // a fresh parser publication.
      continuity: matched.receipt.chainResultCell ? matched.receipt : null,
      surface: FlarkCoreCommittedPresentationSurfaceV1(
        rowOrdinal: previous.rowOrdinal,
        removedRowOrdinal: previous.removedRowOrdinal,
        sourceUtf16: source,
        projectionCurrent: true,
        role: previous.role,
        presentation: matched.presentation,
      ),
    );
    _coordinator.setPendingStructuralSurfaces(states);
    return matched.receipt;
  }

  FlarkProjectionEditCellReceipt? _advanceCommittedCaretBoundary(
    FlarkPendingCaretBoundary boundary,
    int start,
    int end,
    String replacement,
  ) {
    final authorized = boundary.authorizedContentUtf16;
    if (authorized == null || boundary.projectionEditCells.isEmpty) return null;
    final authority = bindPendingDependencyAuthority(
      revision: revision,
      cells: boundary.projectionEditCells,
      envelopes: const [],
      authorizedContentUtf16: authorized,
      startUtf16: start,
      endUtf16: end,
      replacement: replacement,
    );
    return authority is FlarkProjectionEditCellReceipt ? authority : null;
  }

  /// Commits the currently accepted composition prefix when the platform text
  /// client loses focus or closes its connection. Source is already native-
  /// authoritative; this only ends the composition history scope, clears the
  /// adapter range, and lets parsing converge.
  void commitActiveComposition() {
    if (_closed ||
        (!_session.compositionActive && !_inputState.value.composing.isValid)) {
      return;
    }
    _inputState.replaceValue(
      _inputState.value.copyWith(composing: TextRange.empty),
    );
    _trackCompositionWithoutMutation(TextRange.empty);
    notifyListeners();
  }

  Future<bool> _queueHistoryReplay({required bool undoDirection}) {
    final pendingSemantic = _inputTransactions.pendingSemantic;
    if (pendingSemantic != null) {
      if (!_reserveSemanticSuccessor(pendingSemantic)) {
        return Future<bool>.value(false);
      }
      final completion = Completer<bool>();
      pendingSemantic.successors.add(
        FlarkDeferredHistorySuccessor(
          undoDirection: undoDirection,
          completion: completion,
        ),
      );
      _inputTransactions.observePendingSuccessors(pendingSemantic);
      return completion.future;
    }
    if (_closed ||
        _status == FlarkEditorStatus.faulted ||
        _coordinator.historyReplayPending ||
        (!undoDirection && !_session.canRedo) ||
        (undoDirection &&
            !_session.canUndo &&
            _coordinator.pendingEdits == 0)) {
      return Future<bool>.value(false);
    }
    _parseTimer?.cancel();
    _parseTimer = null;
    final acceptedAtEpochMicros = DateTime.now().microsecondsSinceEpoch;
    final acceptanceWatch = Stopwatch()..start();
    final command = _coordinator.admitCommand(
      FlarkEditorCommandKind.historyReplay,
    );
    final generation = command.generation;
    acceptanceWatch.stop();
    final editorSyncMicros = acceptanceWatch.elapsedMicroseconds;

    final operation = _coordinator.afterEdits(() async {
      // The history boundary belongs after every input already admitted ahead
      // of this replay. Breaking the group synchronously here would
      // retroactively split a typing edit that is still waiting on the native
      // actor, so Undo would remove only that last character instead of the
      // complete rapid-typing unit. Edits admitted after this replay already
      // queue behind its completion and observe the new epoch.
      _breakTypingHistoryGroup();
      _endCompositionHistoryGroup();
      final outcome = undoDirection
          ? await _session.undo()
          : await _session.redo();
      // The native command tail can resolve before Flutter finishes adopting
      // the preceding receipt. Keep that receipt's projected presentation
      // through the serialized history call so any late predecessor
      // notification remains rendered. The history mutation has committed at
      // this point and emits no controller frame until the atomic certified
      // restore below, so retiring old authority here cannot expose source.
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
        FlarkPendingPresentationPart.paragraphGap,
        FlarkPendingPresentationPart.caretBoundary,
        FlarkPendingPresentationPart.structuralSurfaces,
      });
      // A predecessor semantic receipt can finish after this replay has been
      // admitted, observe the newer generation, and leave its provisional
      // input lineage plus certification barrier behind. The history outcome
      // has already replayed that native transaction and is now the sole
      // authority; retaining the superseded barrier would suppress the
      // restored publication indefinitely.
      _inputTransactions.discardPendingSemantic();
      _cancelCertificationDeferredInput();
      _inputTransactions.lateSemantic = null;
      _coordinator.endPublicationBarrier();
      _inputState.setSemanticEditActive(false);
      final receiptAtEpochMicros = DateTime.now().microsecondsSinceEpoch;
      final adoptionWatch = Stopwatch()..start();
      final resolvedSelection =
          outcome?.restoreSelection ?? await _session.resolveSelection();
      final restore = resolvedSelection == null
          ? _selectionSnapshot()
          : _adapterSnapshot(resolvedSelection);
      _viewportState.clearOptimisticEdits();
      _clearPendingTaskChecks();
      // History replay is one authoritative visual transaction. Do not
      // publish a pending exact-source viewport between the native replay and
      // its parser-certified result; retain the prior frame while bounded
      // parsing catches up, then adopt source, projection, and selection
      // together.
      await _refreshEditPublicationAfterCertification(
        generation,
        // Fresh rows own both the rendered caret stops and the platform
        // window. Restore them together so a downstream boundary
        // normalization cannot be followed by a stale history surrogate.
        restoreInputWindow: true,
        prepareForRefresh: () => _restoreHistorySelection(restore),
      );
      if (!restore.selection.isCollapsed) {
        // Installing certified rows may normalize a platform selection to its
        // active extent. Reapply an exact history range after that install so
        // undo of a replacement restores the original selection. A collapsed
        // caret deliberately stays under the fresh viewport's authority: it
        // may have advanced across a hidden Markdown boundary while retaining
        // the same rendered position.
        await _restoreHistorySelection(restore);
      }
      if (outcome == null || outcome is FlarkCoreHistoryDropped) {
        if (restore.selection.isCollapsed) {
          _finalizeDroppedHistoryInputWindow();
        }
        return false;
      }
      _scheduleParsingAfterInput();
      adoptionWatch.stop();
      if (outcome is FlarkCoreHistoryReplayed) {
        final telemetry = outcome.receipt.telemetry;
        if (telemetry == null) {
          throw StateError('Flark history replay omitted telemetry');
        }
        _recordSourceEditPerformance(
          kind: undoDirection
              ? FlarkSourceEditPerformanceKind.undo
              : FlarkSourceEditPerformanceKind.redo,
          generation: generation,
          acceptedAtEpochMicros: acceptedAtEpochMicros,
          editorSyncMicros: editorSyncMicros,
          telemetry: telemetry,
          flutterReceiptAdoptionMicros: adoptionWatch.elapsedMicroseconds,
          acceptanceToReceiptMicros: math.max(
            0,
            receiptAtEpochMicros - acceptedAtEpochMicros,
          ),
        );
      }
      notifyListeners();
      return true;
    });
    final completion = operation.then<bool>(
      (didReplay) {
        _coordinator.completeCommand(command);
        if (!didReplay) {
          _status = _idleStatus(current: _viewportState.semanticCurrent);
        }
        notifyListeners();
        return didReplay;
      },
      onError: (Object error, StackTrace stackTrace) {
        _coordinator.failCommand(command, error);
        notifyListeners();
        Error.throwWithStackTrace(error, stackTrace);
      },
    );
    // History replay is not settled until its public state is finalized.
    // Including that bookkeeping in both the returned Future and the edit
    // tail prevents callers from observing a certified source/selection with
    // transitional pending or semantic-lane state.
    _coordinator.trackEdit(completion);
    return completion;
  }

  void _finalizeDroppedHistoryInputWindow() {
    if (!_inputState.value.selection.isCollapsed) return;
    final caret = _inputState.selectionExtentUtf16;
    final boundary = _pendingPresentation.caretBoundary;
    // A replay queued behind a source edit can become invalid when that edit
    // clears the opposite history stack. The edit's current canonical caret is
    // still authoritative. A replay that reached the native history lane has
    // already retired its transition state, so rebuild its physical input row;
    // an admission-time drop can retain the typed boundary and its shared-edge
    // precedence.
    if (boundary == null) {
      _restoreNeutralInputWindow(caret);
    } else {
      _restoreCollapsedInputWindow(
        caret,
        preferredOrdinal: _inputState.activeOrdinal,
      );
    }
    _inputState.retargetActiveOrdinal(_surfaceOrdinalAt(caret));
  }

  void _recordSourceEditPerformance({
    required FlarkSourceEditPerformanceKind kind,
    required int generation,
    required int acceptedAtEpochMicros,
    required int editorSyncMicros,
    required FlarkCoreEditIntentTelemetryV1 telemetry,
    required int flutterReceiptAdoptionMicros,
    required int acceptanceToReceiptMicros,
  }) {
    _performance.recordSource(
      FlarkSourceEditPerformance(
        kind: kind,
        sourceGeneration: generation,
        acceptedAtEpochMicros: acceptedAtEpochMicros,
        editorSyncMicros: editorSyncMicros,
        coreQueueMicros: telemetry.coreQueueMicros,
        workerRoundTripMicros: telemetry.workerRoundTripMicros,
        workerQueueMicros: telemetry.workerQueueMicros,
        nativeFfiMicros: telemetry.nativeFfiMicros,
        coreAdoptionMicros: telemetry.coreAdoptionMicros,
        flutterReceiptAdoptionMicros: flutterReceiptAdoptionMicros,
        acceptanceToReceiptMicros: acceptanceToReceiptMicros,
      ),
    );
  }

  void _scheduleParsingAfterInput({bool immediate = false}) {
    if (_closed ||
        _session.compositionActive ||
        _status == FlarkEditorStatus.faulted) {
      return;
    }
    _parseTimer?.cancel();
    if (immediate) {
      _parseTimer = null;
      unawaited(continueParsing());
      return;
    }
    _parseTimer = Timer(_parseIdleDelay, () {
      _parseTimer = null;
      unawaited(continueParsing());
    });
  }

  Future<void> _finishParsing() async {
    try {
      _status = _idleStatus(current: false);
      // Streamed-open startup (RFC 029 A3): a document that is still
      // admitting source cannot pump to Ready first — that would discard the
      // certified head the whole path exists to serve. Instead, interleave
      // bounded pump slices with the bounded head-window certification probe
      // and publish through the ordinary viewport refresh path: the first
      // certified viewport makes the editor paint and accept input for the
      // certified region, and later publications happen only on genuine
      // certification upgrades (an adopted append rebinds the same certified
      // head, so republishing it would be a no-op; a mid-load edit's
      // recertification arrives at a new revision). Uncertified turns
      // publish nothing, so the last certified presentation stays painted
      // exactly as the projection-continuity machinery already guarantees
      // during recertification. When the stream seals, fall through to the
      // ordinary pump-to-ready convergence below.
      var openingPublishedCertifiedEnd = -1;
      Object? openingError;
      StackTrace? openingErrorStackTrace;
      if (_document.isOpening) {
        // A failed stream never seals, so the loop below would otherwise
        // keep serving the admitted prefix forever without surfacing the
        // typed failure the core layer is holding for it.
        unawaited(
          _document.openingSealed.then<void>(
            (_) {},
            onError: (Object error, StackTrace stackTrace) {
              openingError = error;
              openingErrorStackTrace = stackTrace;
            },
          ),
        );
      }
      while (_document.isOpening && !_closed && !_session.compositionActive) {
        if (openingError case final error?) {
          Error.throwWithStackTrace(error, openingErrorStackTrace!);
        }
        await _document.pump(workUnits: 512);
        if (_closed || _session.compositionActive) return;
        if (!_document.isOpening) break;
        final probe = await _document.queryViewport(
          endByte: math.min(sourceByteLength, _openingHeadProbeBytes),
          maxRows: _viewportRowsPerPage,
        );
        // A semantic row query answers with whole-page certification; the
        // per-range breakdown belongs to live-projection queries and is
        // always empty here. During a streamed open the runtime clamps the
        // page to the certified head, so a certified answer carrying rows is
        // exactly the publishable event, and its last row's end is the
        // certified frontier that decides whether a later turn upgraded.
        final certified = probe.isCertified && probe.rows.isNotEmpty;
        final certifiedEnd = certified ? probe.rows.last.sourceBytes.end : 0;
        final upgraded =
            probe.revision != _openingPublishedRevision ||
            certifiedEnd > openingPublishedCertifiedEnd;
        if (probe.continuation != 0) {
          await _document.releaseViewportContinuation(probe);
        }
        // An in-flight edit owns its own refresh; publishing around it would
        // race the optimistic window against a not-yet-committed splice.
        if (!certified || !upgraded || _coordinator.pendingEdits != 0) continue;
        final stamp = _coordinator.stamp;
        await _refreshViewport(
          restoreInputWindow: true,
          expectedEditGeneration: stamp.editGeneration,
          ensureActiveInputVisible: true,
        );
        if (!_coordinator.accepts(stamp)) continue;
        _coordinator.recordOpeningPublication(probe.revision);
        openingPublishedCertifiedEnd = certifiedEnd;
        _openingPublication?.complete();
        _openingPublication = null;
      }
      // Leaving the opening loop for any reason — sealed, closed, faulted,
      // or a composition taking over — releases every presentation barrier
      // waiting on the next publication; none is coming from this loop.
      _openingPublication?.complete();
      _openingPublication = null;
      if (_closed || _session.compositionActive) return;
      if (_status == FlarkEditorStatus.streaming) {
        // The stream sealed; the post-seal parse converges below.
        _status = FlarkEditorStatus.parsing;
        notifyListeners();
      }
      parseLoop:
      while (!_closed && !_session.compositionActive) {
        // An older idle parser task can survive into a newer edit generation.
        // Wait for that generation's native edit tail before pumping/querying;
        // otherwise it can publish a shorter pre-commit source window beside
        // the optimistic input for the same generation.
        final stamp = _coordinator.stamp;
        final editBarrier = _coordinator.editTail;
        final adoptionBarrier = _coordinator.sourceEditAdoptionTail;
        await Future.wait([editBarrier, adoptionBarrier]);
        if (_closed || _session.compositionActive) return;
        if (!_coordinator.accepts(stamp) ||
            !identical(editBarrier, _coordinator.editTail) ||
            !identical(adoptionBarrier, _coordinator.sourceEditAdoptionTail)) {
          continue;
        }
        while (!_document.isReady && !_closed) {
          await _document.pump(workUnits: 512);
          if (_session.compositionActive) return;
          if (!_coordinator.accepts(stamp)) continue parseLoop;
        }
        if (_closed || _session.compositionActive) return;
        if (!_coordinator.accepts(stamp)) continue;
        await _refreshViewport(
          restoreInputWindow: true,
          expectedEditGeneration: stamp.editGeneration,
          ensureActiveInputVisible: true,
        );
        if (_coordinator.accepts(stamp) && _certificationDeferredInputActive) {
          // Receipt-backed dependency or structural rows can be safe to paint
          // without being current command semantics. A Return/Delete/
          // Backspace observed on that surface waits here, then reclassifies
          // against the certified row partition instead of falling back
          // literally in provisional coordinates.
          _promoteCertificationDeferredInput();
        }
        if (_coordinator.accepts(stamp)) return;
        // A newer edit arrived while the parser/query task was in flight.
        // This same task must converge on that generation: a later idle timer
        // may already have joined the runtime's single parser task.
      }
    } catch (error) {
      if (_certificationDeferredInputActive) {
        _inputTransactions.discardPendingSemantic();
        _cancelCertificationDeferredInput();
      }
      _lastError = error;
      _status = FlarkEditorStatus.faulted;
      notifyListeners();
    } finally {
      // No publication can follow this task; release any barrier waiting on
      // one, including on the faulted and early-return paths.
      _openingPublication?.complete();
      _openingPublication = null;
    }
  }

  void _retainOptimisticRefreshAnchor(
    int editStart, {
    required bool deriveFromInput,
  }) {
    _viewportPager.retainRefreshAnchorForEdit(
      editStart: editStart,
      deriveFromInput: deriveFromInput,
      currentViewport: _viewportState.viewport,
      inputGlobalUtf16Start: _inputState.globalUtf16Start,
      inputText: _inputState.value.text,
    );
  }

  Future<void> _refreshViewport({
    required bool restoreInputWindow,
    int? expectedEditGeneration,
    bool ensureActiveInputVisible = false,
    bool publish = true,
  }) async {
    final previous = _viewportState.viewport;
    final result = await _viewportPager.refresh(
      FlarkViewportRefreshRequest(
        previousViewport: previous,
        visibleUtf16Start: _viewportState.visibleUtf16Start,
        visibleSource: _viewportState.visibleSource,
        optimisticEditsStartAtOrAfterPreviousStart:
            previous == null ||
            _viewportState.allOptimisticEditsStartAtOrAfter(
              previous.coveredUtf16.start,
            ),
        caretUtf16: _inputState.selectionExtentUtf16,
        ensureCaretVisible: ensureActiveInputVisible,
        expectedEditGeneration: expectedEditGeneration,
      ),
    );
    if (result == null) return;
    if (!_viewportPager.adopt(result)) {
      final cleanup = _viewportPager.discard(result);
      if (cleanup != null) await cleanup;
      return;
    }
    _installViewport(
      result,
      restoreInputWindow: restoreInputWindow,
      ensureActiveInputVisible: ensureActiveInputVisible,
      publish: publish,
    );
  }

  // Installation is synchronous so page index, rows, visible source, and
  // certification can never be observed in a torn half-installed state.
  void _installViewport(
    FlarkViewportPageResult result, {
    required bool restoreInputWindow,
    bool ensureActiveInputVisible = false,
    bool publish = true,
  }) {
    final viewport = result.viewport;
    final installation = _viewportState.install(viewport, result.source);
    final retainsExistingSurface = installation.retainsExistingSurface;
    final installsFreshRows = installation.installsFreshRows;
    final installsCertifiedSurface = installation.installsCertifiedSurface;
    if (!retainsExistingSurface) {
      // Async parsing, paging, and history restoration can replace source
      // mapping without admitting a new key command. Invalidate hits from the
      // previous layout before the replacement is exposed to listeners.
      _coordinator.recordInteraction();
    }
    if (!retainsExistingSurface &&
        viewport.revision != _publishedDocumentRevision) {
      // History replay and composition cancellation adopt their new source
      // through this atomic viewport publication rather than an optimistic
      // local splice. Advance the paint generation in the same synchronous
      // install that replaces the visible source, never before its query.
      _coordinator.installViewportRevision(viewport.revision);
    }
    _viewportPager.observeInstallation(
      viewport: viewport,
      installation: installation,
      caretUtf16: _inputState.selectionExtentUtf16,
    );
    if (installsCertifiedSurface) _clearPendingTaskChecks();
    // A streamed open's head page is typically mixed — certified head rows
    // ahead of pending-exact tail — so the first-certified receipt keys on
    // published rows inside any certified range, not on the whole-viewport
    // certification that only a converged parse restores.
    if (installsFreshRows &&
        !_firstCertifiedPublication.isCompleted &&
        (viewport.isCertified ||
            viewport.certificationRanges.any(
              (range) => range.isCertified && range.sourceBytes.length > 0,
            ))) {
      _firstCertifiedPublicationEpochMicros =
          DateTime.now().microsecondsSinceEpoch;
      _firstCertifiedPublication.complete();
    }
    if (certifiedViewportSupersedesPendingDependency(
      viewport: viewport,
      pendingPresentation: _pendingPresentation,
    )) {
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
      });
    }
    final supersededParagraphGap = _viewportState.semanticCurrent
        ? _pendingPresentation.paragraphGap
        : null;
    final supersededStructuralCaretBoundary = _viewportState.semanticCurrent
        ? caretBoundaryForStructuralSurfaces(
            _pendingPresentation.structuralSurfaces,
          )
        : null;
    if (_viewportState.semanticCurrent) {
      // Certified rows supersede the visual transition partition. The AST
      // still cannot represent which side owns a caret in the resulting blank
      // source gap, so promote that one fact into a nonvisual boundary receipt
      // before retiring the visual gap and structural surfaces.
      if (supersededParagraphGap != null) {
        _coordinator.setPendingCaretBoundary(
          FlarkPendingCaretBoundary.fromGap(
            supersededParagraphGap,
            // The gap owns durable shared-edge geometry; the structural
            // successor owns the parser-authored first-edit cell. Parser
            // certification supersedes their visual surfaces, not either
            // half of that typed interaction authority.
            editAuthority:
                supersededStructuralCaretBoundary ??
                _pendingPresentation.caretBoundary,
          ),
        );
      } else if (supersededStructuralCaretBoundary != null) {
        _coordinator.setPendingCaretBoundary(supersededStructuralCaretBoundary);
      }
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.paragraphGap,
        FlarkPendingPresentationPart.structuralSurfaces,
      });
    }
    _status = _idleStatus(current: _viewportState.semanticCurrent);
    if (installsFreshRows) {
      // Input-window restoration must route through the fresh row partition.
      // The prior ordinal can be a neutral placeholder retained during a
      // semantic transition; using it here expands the platform window to the
      // whole physical list line and exposes its hidden prefix.
      _inputState.retargetActiveOrdinal(
        _surfaceOrdinalAt(_inputState.selectionExtentUtf16),
      );
    }
    if (restoreInputWindow) {
      if (supersededParagraphGap != null &&
          !_certifiedRowHasNonemptyInputAt(_inputState.selectionExtentUtf16) &&
          _restoreCommittedParagraphGapInputWindow(
            _inputState.selectionExtentUtf16,
            gap: supersededParagraphGap,
          )) {
        // The certified viewport supersedes the gap's rendering ownership,
        // but the same atomic handoff must use its exact result-line extent to
        // reconcile the platform input window. Rebuilding from the newly
        // parsed row alone can exclude a hidden continuation prefix and turn
        // a correct `- \n` input cell into an empty one.
      } else if (!ensureActiveInputVisible || !_ensureActiveInputVisible()) {
        _restoreInputWindow();
      }
    } else if (ensureActiveInputVisible) {
      if (!_ensureActiveInputVisible()) {
        _inputState.retargetActiveOrdinal(
          _surfaceOrdinalAt(_inputState.selectionExtentUtf16),
        );
      }
    }
    if (installsFreshRows) {
      // Row ordinals belong to one viewport publication. The same numeric
      // ordinal can survive while its source range changes, so existence is
      // not proof that it still owns the canonical caret. Re-resolve after
      // restoration too because an intentionally retained empty paragraph
      // gap installs a neutral input surrogate without changing the parser-
      // owned active row.
      _inputState.retargetActiveOrdinal(
        _surfaceOrdinalAt(_inputState.selectionExtentUtf16),
      );
    }
    var certifiedBoundaryCaretChanged = false;
    if (installsFreshRows &&
        _viewportState.semanticCurrent &&
        !_inputState.crossRowSelection &&
        !_inputState.oversizedSelection &&
        _inputState.value.selection.isCollapsed &&
        _inputState.value.selection.affinity == TextAffinity.downstream) {
      final canonicalCaret = _certifiedDownstreamBoundaryCaretAt(
        _inputState.selectionExtentUtf16,
      );
      if (canonicalCaret != null &&
          canonicalCaret != _inputState.selectionExtentUtf16) {
        _inputState.setCanonicalSelection(canonicalCaret, canonicalCaret);
        _inputState.retargetActiveOrdinal(_surfaceOrdinalAt(canonicalCaret));
        _restoreCollapsedInputWindow(
          canonicalCaret,
          preferredOrdinal: _inputState.activeOrdinal,
        );
        certifiedBoundaryCaretChanged = true;
      }
    }
    final mayRestoreInputWindow =
        restoreInputWindow || ensureActiveInputVisible;
    if (installsFreshRows &&
        mayRestoreInputWindow &&
        (_inputState.activeOrdinal ?? 0) < 0) {
      _restoreNeutralInputWindow(_inputState.selectionExtentUtf16);
    }
    // A fresh parse owns presentation, never the canonical source selection.
    // Platform-originated selections are normalized when observed and
    // parser-authorized edits apply their declared result caret at admission.
    // Re-normalizing here would make a clean publication move an authored
    // caret through newly hidden syntax (for example from the end of a just-
    // typed opening fence to the code body) and change where the next key
    // lands.
    if (certifiedBoundaryCaretChanged) {
      unawaited(_installCanonicalSelection(_selectionSnapshot()));
    }
    if (publish) notifyListeners();
  }

  bool _certifiedRowHasNonemptyInputAt(int caret) {
    if (!_viewportState.semanticCurrent) return false;
    for (final row in _viewportState.rows) {
      final activation = _mapViewportRange(_activationRange(row));
      if (activation.length > 0 &&
          activation.start <= caret &&
          caret <= activation.end) {
        return true;
      }
    }
    return false;
  }

  int? _certifiedDownstreamBoundaryCaretAt(int globalCaret) {
    if ((_inputState.activeOrdinal ?? 0) >= 0) return null;
    var hasPredecessor = false;
    FlarkSourceRange? successor;
    for (final row in _viewportState.rows) {
      final source = surfaceSourceRange(row);
      if (source.end <= globalCaret) hasPredecessor = true;
      if (source.start <= globalCaret) continue;
      if (successor == null || source.start < successor.start) {
        successor = source;
      }
    }
    if (!hasPredecessor || successor == null) return null;
    final padding = _sliceVisibleUtf16(globalCaret, successor.start);
    if (padding.isEmpty ||
        !padding.codeUnits.every((unit) => unit == 0x20 || unit == 0x09)) {
      return null;
    }
    // A certified inter-row boundary is a rendered caret stop; horizontal
    // source padding excluded by both adjacent rows is not. Requiring a real
    // predecessor preserves document-leading indentation as an exact caret
    // side, so cut/paste at the first rendered glyph remains lossless. The
    // inter-row case notably occurs when a rapid paragraph split turns a
    // former table continuation into the next parser-owned row.
    return successor.start;
  }

  void _restoreInputWindow() {
    final previousComposing = _inputState.value.composing;
    final composingStart = previousComposing.isValid
        ? _inputState.globalUtf16Start + previousComposing.start
        : null;
    final composingEnd = previousComposing.isValid
        ? _inputState.globalUtf16Start + previousComposing.end
        : null;
    _restoreInputWindowBody();
    if (composingStart == null || composingEnd == null) return;
    final windowEnd =
        _inputState.globalUtf16Start + _inputState.value.text.length;
    if (_inputState.globalUtf16Start <= composingStart &&
        composingEnd <= windowEnd) {
      _inputState.replaceValue(
        _inputState.value.copyWith(
          composing: TextRange(
            start: composingStart - _inputState.globalUtf16Start,
            end: composingEnd - _inputState.globalUtf16Start,
          ),
        ),
      );
    }
  }

  void _restoreInputWindowBody() {
    if (_inputState.crossRowSelection) {
      _restoreSelectionSnapshot(_selectionSnapshot());
      return;
    }
    if ((_inputState.activeOrdinal ?? 0) < 0) {
      if (_restoreCommittedParagraphGapInputWindow(
        _inputState.selectionExtentUtf16,
      )) {
        return;
      }
      if (_restoreCommittedCaretBoundaryInputWindow(
        _inputState.selectionExtentUtf16,
      )) {
        return;
      }
      _restoreNeutralInputWindow(_inputState.selectionExtentUtf16);
      return;
    }
    _restoreCollapsedInputWindow(
      _inputState.selectionExtentUtf16.clamp(0, sourceUtf16Length),
      preferredOrdinal: _inputState.activeOrdinal,
    );
  }

  bool _restoreCommittedParagraphGapInputWindow(
    int caret, {
    FlarkCoreCommittedPresentationGapV1? gap,
  }) {
    gap ??= _pendingPresentation.paragraphGap;
    final plan = FlarkEditorInputWindowPlanner.paragraphGap(
      viewportState: _viewportState,
      projector: _captureSurfaceProjector(),
      gap: gap,
      caret: caret,
      maximumCodeUnits: _maximumInputCodeUnits,
    );
    if (plan == null) return false;
    _inputState.installWindowPlan(plan);
    return true;
  }

  bool _restoreCommittedCaretBoundaryInputWindow(
    int caret, {
    FlarkPendingCaretBoundary? boundary,
  }) {
    boundary ??= _pendingPresentation.caretBoundary;
    final plan = FlarkEditorInputWindowPlanner.caretBoundary(
      viewportState: _viewportState,
      boundary: boundary,
      caret: caret,
      maximumCodeUnits: _maximumInputCodeUnits,
    );
    if (plan == null) return false;
    _inputState.installWindowPlan(plan);
    return true;
  }

  void _restoreNeutralInputWindow(int caret) {
    _inputState.installWindowPlan(
      FlarkEditorInputWindowPlanner.neutralLine(
        viewportState: _viewportState,
        caret: caret,
        maximumCodeUnits: _maximumInputCodeUnits,
      ),
    );
  }

  String _sliceVisibleUtf16(int globalStart, int globalEnd) =>
      _viewportState.sliceVisibleUtf16(globalStart, globalEnd);

  void _applyOptimisticViewportEdit(
    int globalStart,
    int globalEnd,
    String replacement, {
    bool preservesMappedRowFacts = true,
  }) {
    final adoption = _viewportState.applyOptimisticEdit(
      globalStart: globalStart,
      globalEnd: globalEnd,
      replacement: replacement,
      fallbackSource: _inputState.value.text,
      fallbackUtf16Start: _inputState.globalUtf16Start,
      focusUtf16: _inputState.selectionExtentUtf16,
      maximumVisibleCodeUnits: _maximumInputCodeUnits,
      preservesMappedRowFacts: preservesMappedRowFacts,
    );
    if (adoption.disposition ==
        FlarkOptimisticViewportEditDisposition.replacedByBoundedWindow) {
      _viewportPager.resetPagePath();
    }
    if (adoption.disposition !=
        FlarkOptimisticViewportEditDisposition.retainedMappedSurface) {
      _inputState.retargetActiveOrdinal(
        _surfaceOrdinalAt(_inputState.selectionExtentUtf16),
      );
    }
  }

  bool _applyLengthNeutralViewportReplacement(
    int globalStart,
    int globalEnd,
    String replacement,
  ) => _viewportState.applyLengthNeutralReplacement(
    globalStart: globalStart,
    globalEnd: globalEnd,
    replacement: replacement,
  );

  void _applyLengthNeutralInputReplacement(
    int globalStart,
    int globalEnd,
    String replacement,
  ) {
    final windowStart = _inputState.globalUtf16Start;
    final windowEnd = windowStart + _inputState.value.text.length;
    if (globalEnd <= windowStart || globalStart >= windowEnd) return;
    if (globalStart < windowStart ||
        globalEnd > windowEnd ||
        replacement.length != globalEnd - globalStart) {
      throw StateError('Task-toggle receipt crossed the input-window edge');
    }
    final localStart = globalStart - windowStart;
    final localEnd = globalEnd - windowStart;
    _inputState.replaceValue(
      _inputState.value.copyWith(
        text: _inputState.value.text.replaceRange(
          localStart,
          localEnd,
          replacement,
        ),
        selection: _inputState.value.selection,
        composing: _inputState.value.composing,
      ),
    );
  }

  FlarkSourceRange _mapViewportRange(FlarkSourceRange base) =>
      _viewportState.mapRange(base);

  FlarkViewportRow? _activeCachedRow() {
    final activeOrdinal = _inputState.activeOrdinal;
    if (activeOrdinal == null) return null;
    for (final candidate in _viewportState.rows) {
      if (candidate.ordinal == activeOrdinal) return candidate;
    }
    return null;
  }

  int? _preferredMutationOrdinal(
    int startUtf16,
    int endUtf16,
    String replacement,
  ) {
    final direct = _surfaceOrdinalAt(startUtf16);
    if (_viewportState.rows.any((row) => row.ordinal == direct)) return direct;

    final continuity = _pendingPresentation.dependency;
    if (continuity != null) {
      final authorizedSuccessor = continuity.authority.continueWith(
        startUtf16: startUtf16,
        endUtf16: endUtf16,
        replacement: replacement,
      );
      if (authorizedSuccessor != null) return continuity.rowOrdinal;
    }

    // The normal surface lookup owns the final cached row's exact end, but a
    // carried publication can temporarily extend beyond the predecessor row.
    // Preserve that row only when its parser publication explicitly authorizes
    // this exact boundary edit. This is authority routing, not Markdown logic.
    final row = _activeCachedRow();
    if (row == null) return direct;
    final activation = _mapViewportRange(_activationRange(row));
    if (!_rowSemanticsCurrent(activation)) return direct;
    final editable = _mapViewportRange(
      row.editableUtf16 ?? _activationRange(row),
    );
    final authority = bindPendingDependencyAuthority(
      revision: revision,
      plans: row.pendingPresentationPlans,
      cells: row.projectionEditCells,
      envelopes: row.literalSafeEnvelopes,
      authorizedContentUtf16: editable,
      authorizedBlockUtf16: _mappedExactRowRange(row),
      startUtf16: startUtf16,
      endUtf16: endUtf16,
      replacement: replacement,
    );
    return authority == null ? direct : row.ordinal;
  }

  FlarkSourceRange _activationRange(FlarkViewportRow row) {
    return _captureSurfaceProjector().activationRange(row);
  }

  bool _rowSemanticsCurrent(FlarkSourceRange mappedSource) =>
      _captureSurfaceProjector().rowSemanticsCurrent(mappedSource);
}
