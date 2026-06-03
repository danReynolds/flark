import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../projection/projection.dart';
import '../render_plan/render_plan.dart';
import 'flark_parse_scheduler.dart';
import 'flark_text_delta_adapter.dart';

enum FlarkControllerEventKind {
  runtimeChanged,
  selectionChanged,
  projectionPredicted,
  parseAdopted,
  undo,
  redo,
}

final class FlarkControllerEvent {
  const FlarkControllerEvent({
    required this.kind,
    required this.revision,
    required this.previousRevision,
    required this.markdownChanged,
    required this.selectionChanged,
  });

  final FlarkControllerEventKind kind;
  final int revision;
  final int previousRevision;
  final bool markdownChanged;
  final bool selectionChanged;
}

/// Controller for shared Markdown editor and preview state.
///
/// ## Observing changes
///
/// There is one semantic event model: the [events] stream. Each logical change
/// emits exactly one [FlarkControllerEvent]. For the common cases prefer the
/// typed projections [markdownChanges] and [selectionChanges] instead of
/// hand-filtering event kinds.
///
/// This class is also a [ChangeNotifier]; `addListener` is the low-level
/// "something changed, rebuild" signal used by the editor/preview widgets to
/// resync. Application code should observe [events] (or its projections) rather
/// than `addListener` — they fire together, but only [events] carries what
/// changed.
final class FlarkFlutterController extends ChangeNotifier {
  FlarkFlutterController({
    required FlarkEditorRuntime runtime,
    FlarkProjection? projection,
    FlarkRenderPlan? renderPlan,
    FlarkMarkdownParseBackend? parseBackend,
    FlarkMarkdownProfile parseProfile = FlarkMarkdownProfile.commonMarkGfm,
    Duration parseDebounce = const Duration(milliseconds: 80),
    void Function(Object error, StackTrace stackTrace)? onParseError,
    FlarkTextDeltaAdapter textDeltaAdapter = const FlarkTextDeltaAdapter(),
    FlarkProjectedTextEditAdapter projectedTextEditAdapter =
        const FlarkProjectedTextEditAdapter(),
  }) : _runtime = runtime,
       _projection =
           projection ??
           FlarkProjection(textLength: runtime.state.document.length),
       _renderPlan = renderPlan ?? _staleRenderPlan(runtime.state.revision),
       _renderPlanRevision = renderPlan == null ? null : runtime.state.revision,
       _parseBackend = parseBackend,
       _parseProfile = parseProfile,
       _parseDebounce = parseDebounce,
       _onParseError = onParseError,
       _textDeltaAdapter = textDeltaAdapter,
       _projectedTextEditAdapter = projectedTextEditAdapter;

  factory FlarkFlutterController.fromMarkdown(
    String markdown, {
    FlarkExtensionSet? extensions,
    FlarkMarkdownParseBackend? parseBackend,
    FlarkMarkdownProfile parseProfile = FlarkMarkdownProfile.commonMarkGfm,
    Duration parseDebounce = const Duration(milliseconds: 80),
    void Function(Object error, StackTrace stackTrace)? onParseError,
  }) {
    return FlarkFlutterController(
      runtime: FlarkEditorRuntime.fromMarkdown(
        markdown,
        extensions: extensions ?? FlarkMarkdownEditingExtensions.standard(),
      ),
      parseBackend: parseBackend,
      parseProfile: parseProfile,
      parseDebounce: parseDebounce,
      onParseError: onParseError,
    );
  }

  FlarkEditorRuntime _runtime;
  FlarkProjection _projection;
  FlarkRenderPlan _renderPlan;
  int? _renderPlanRevision;
  FlarkProjectionPrediction? _lastProjectionPrediction;
  FlarkMarkdownParseBackend? _parseBackend;
  FlarkMarkdownProfile _parseProfile;
  Duration _parseDebounce;
  void Function(Object error, StackTrace stackTrace)? _onParseError;
  FlarkParseScheduler? _parseScheduler;
  bool _parseStarted = false;
  int _parseSurfaceCount = 0;
  bool _disposed = false;
  final FlarkTextDeltaAdapter _textDeltaAdapter;
  final FlarkProjectedTextEditAdapter _projectedTextEditAdapter;
  final StreamController<FlarkControllerEvent> _events =
      StreamController<FlarkControllerEvent>.broadcast();

  FlarkEditorRuntime get runtime => _runtime;

  /// Whether the controller-owned background parser is running (debounced
  /// re-parsing in response to edits).
  bool get isParsing => _parseStarted;

  /// Starts the controller-owned background parser if it is not already running.
  ///
  /// This is idempotent. Editor and preview surfaces that share one controller
  /// all call this, but a single parser is created per controller — one
  /// document is parsed once, regardless of how many widgets observe it.
  ///
  /// The default Comrak backend is resolved lazily here (not at construction),
  /// so headless controllers created for tests or server-side render plans do
  /// not require the native bridge until a surface actually starts parsing.
  void ensureParsing() {
    if (_disposed) return;
    _ensureScheduler().start();
    _parseStarted = true;
  }

  /// Registers an editing surface that needs background parsing.
  ///
  /// The controller keeps a single parser running while at least one surface is
  /// attached, and stops it when the last surface detaches (see
  /// [detachParsingSurface]). Widgets call this in `initState`; app code that
  /// drives a controller without a widget can call [ensureParsing] instead.
  void attachParsingSurface() {
    if (_disposed) return;
    _parseSurfaceCount += 1;
    ensureParsing();
  }

  /// Detaches a surface previously registered with [attachParsingSurface].
  ///
  /// When the last attached surface detaches, the background parser is stopped
  /// so a controller observed only by disposed widgets does not keep timers
  /// pending. The controller and its current render plan remain usable.
  void detachParsingSurface() {
    if (_parseSurfaceCount > 0) _parseSurfaceCount -= 1;
    if (_parseSurfaceCount > 0 || !_parseStarted) return;
    _parseScheduler?.dispose();
    _parseScheduler = null;
    _parseStarted = false;
  }

  /// Reconfigures the controller-owned parser, restarting it if running.
  ///
  /// Only non-null arguments override existing configuration. Pass
  /// [clearOnParseError] to drop a previously configured error callback.
  void configureParsing({
    FlarkMarkdownParseBackend? parseBackend,
    FlarkMarkdownProfile? parseProfile,
    Duration? parseDebounce,
    void Function(Object error, StackTrace stackTrace)? onParseError,
    bool clearOnParseError = false,
  }) {
    if (parseBackend != null) _parseBackend = parseBackend;
    if (parseProfile != null) _parseProfile = parseProfile;
    if (parseDebounce != null) _parseDebounce = parseDebounce;
    if (onParseError != null || clearOnParseError) {
      _onParseError = onParseError;
    }
    if (_parseScheduler == null) return;
    final wasStarted = _parseStarted;
    _parseScheduler!.dispose();
    _parseScheduler = null;
    _parseStarted = false;
    if (wasStarted) ensureParsing();
  }

  /// Immediately parses the current revision, bypassing the debounce window.
  ///
  /// If [ensureParsing] has not been called, this performs a one-shot parse
  /// without installing a background debounce loop, so advanced widgets that
  /// drive parsing per structural edit do not leak pending timers. Errors are
  /// routed to the configured parse-error callback rather than thrown.
  Future<void> parseNow() async {
    if (_disposed) return;
    final scheduler = _ensureScheduler();
    try {
      await scheduler.parseNow();
    } catch (error, stackTrace) {
      _onParseError?.call(error, stackTrace);
    }
  }

  FlarkParseScheduler _ensureScheduler() {
    return _parseScheduler ??= FlarkParseScheduler(
      controller: this,
      backend: _parseBackend ?? FlarkNativeComrakParseBackend.requiredDefault(),
      profile: _parseProfile,
      debounce: _parseDebounce,
      onError: _onParseError,
    );
  }

  Stream<FlarkControllerEvent> get events => _events.stream;

  /// Emits the current [markdown] whenever the document text changes.
  ///
  /// A typed projection of [events] for the most common observation case —
  /// selection-only changes do not emit here.
  Stream<String> get markdownChanges =>
      events.where((event) => event.markdownChanged).map((_) => markdown);

  /// Emits the current [selection] whenever it changes.
  ///
  /// A typed projection of [events]; document edits that also move the caret
  /// emit here as well as on [markdownChanges].
  Stream<FlarkSelection> get selectionChanges =>
      events.where((event) => event.selectionChanged).map((_) => selection);

  FlarkEditorState get state => _runtime.state;

  String get markdown => state.markdown;

  FlarkSelection get selection => state.selection;

  FlarkProjection get projection => _projection;

  FlarkRenderPlan get renderPlan => _renderPlan;

  FlarkProjectionPrediction? get lastProjectionPrediction {
    return _lastProjectionPrediction;
  }

  bool get hasAuthoritativeRenderPlan {
    return _renderPlanRevision == state.revision;
  }

  FlarkEditorRuntimeResult dispatch<TPayload>({
    required FlarkCommand<TPayload> command,
    required TPayload payload,
  }) {
    final result = _runtime.dispatch(command: command, payload: payload);
    _adoptRuntimeResult(result);
    return result;
  }

  FlarkEditorRuntimeResult applyTransaction(FlarkTransaction transaction) {
    final result = _runtime.applyTransaction(transaction);
    _adoptRuntimeResult(result);
    return result;
  }

  bool applyTextEditingDelta(TextEditingDelta delta) {
    final transaction = _textDeltaAdapter.transactionFromDelta(
      delta,
      currentMarkdown: markdown,
    );
    if (transaction == null) return false;
    applyTransaction(transaction);
    return true;
  }

  bool applyProjectedTextEdit({
    required String oldDisplayText,
    required String newDisplayText,
    int? undoGroupId,
    FlarkMapAffinity fallbackInsertionAffinity = FlarkMapAffinity.downstream,
  }) {
    final transaction = _projectedTextEditAdapter.transactionFromDisplayEdit(
      currentMarkdown: markdown,
      projection: projection,
      oldDisplayText: oldDisplayText,
      newDisplayText: newDisplayText,
      sourceSelectionBefore: selection,
      undoGroupId: undoGroupId,
      fallbackInsertionAffinity: fallbackInsertionAffinity,
    );
    if (transaction == null) return false;
    applyTransaction(transaction);
    return true;
  }

  bool applyProjectedSelection(
    FlarkSelection displaySelection, {
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
  }) {
    final sourceSelection = projection.displaySelectionToSource(
      displaySelection,
      affinity: affinity,
    );
    return applySelection(sourceSelection, userEvent: 'selection.projected');
  }

  bool applySelection(
    FlarkSelection sourceSelection, {
    String userEvent = 'selection',
  }) {
    sourceSelection.validate(state.document.length);
    if (sourceSelection == selection) return false;
    applyTransaction(
      FlarkTransaction(
        operations: const [],
        selectionAfter: sourceSelection,
        metadata: FlarkTransactionMetadata(
          intent: FlarkTransactionIntent.selection,
          userEvent: userEvent,
          addToHistory: false,
        ),
      ),
    );
    return true;
  }

  FlarkEditorRuntimeResult undo() {
    final result = _runtime.undo();
    _adoptRuntimeResult(result, eventKind: FlarkControllerEventKind.undo);
    return result;
  }

  FlarkEditorRuntimeResult redo() {
    final result = _runtime.redo();
    _adoptRuntimeResult(result, eventKind: FlarkControllerEventKind.redo);
    return result;
  }

  bool applyParseResult(FlarkMarkdownParseResult parseResult) {
    if (parseResult.revision != state.revision ||
        parseResult.sourceTextLength != state.document.length) {
      return false;
    }

    final nextProjection = FlarkProjection.fromParseResult(parseResult);
    _projection = nextProjection;
    final baseRenderPlan = FlarkRenderPlan.fromParseResult(
      parseResult: parseResult,
      projection: nextProjection,
    );
    _renderPlan = applyFlarkRenderPlanExtensions(
      renderPlan: baseRenderPlan,
      parseResult: parseResult,
      projection: nextProjection,
      extensions: _runtime.extensions,
    );
    _renderPlanRevision = parseResult.revision;
    _lastProjectionPrediction = null;
    _emitEvent(
      kind: FlarkControllerEventKind.parseAdopted,
      previousState: state,
    );
    notifyListeners();
    return true;
  }

  void _adoptRuntimeResult(
    FlarkEditorRuntimeResult result, {
    FlarkControllerEventKind? eventKind,
  }) {
    if (identical(result.runtime, _runtime)) return;

    final transaction = result.commandResult.transaction;
    final previousProjection = _projection;
    final previousRenderPlan = _renderPlan;
    final previousState = state;
    _runtime = result.runtime;
    if (transaction == null) {
      _projection = FlarkProjection(textLength: state.document.length);
      _lastProjectionPrediction = null;
      _renderPlan = _staleRenderPlan(state.revision);
      _renderPlanRevision = null;
    } else if (!transaction.changesDocument) {
      _projection = previousProjection;
      _lastProjectionPrediction = null;
    } else {
      final prediction = previousProjection.predictAfter(
        transaction,
        textLengthAfter: state.document.length,
      );
      _projection = prediction.projection;
      _lastProjectionPrediction = prediction;
      _renderPlan = _predictRenderPlan(
        previousRenderPlan: previousRenderPlan,
        transaction: transaction,
        projection: _projection,
        revision: state.revision,
        textLengthAfter: state.document.length,
      );
      _renderPlanRevision = null;
    }
    _emitEvent(
      kind: eventKind ?? _eventKindForRuntimeChange(previousState),
      previousState: previousState,
    );
    notifyListeners();
  }

  FlarkControllerEventKind _eventKindForRuntimeChange(
    FlarkEditorState previousState,
  ) {
    if (previousState.markdown == state.markdown &&
        previousState.selection != state.selection) {
      return FlarkControllerEventKind.selectionChanged;
    }
    if (_lastProjectionPrediction != null) {
      return FlarkControllerEventKind.projectionPredicted;
    }
    return FlarkControllerEventKind.runtimeChanged;
  }

  void _emitEvent({
    required FlarkControllerEventKind kind,
    required FlarkEditorState previousState,
  }) {
    if (_events.isClosed) return;
    _events.add(
      FlarkControllerEvent(
        kind: kind,
        revision: state.revision,
        previousRevision: previousState.revision,
        markdownChanged: previousState.markdown != state.markdown,
        selectionChanged: previousState.selection != state.selection,
      ),
    );
  }

  @override
  void dispose() {
    _disposed = true;
    _parseScheduler?.dispose();
    _parseScheduler = null;
    _events.close();
    super.dispose();
  }

  static FlarkRenderPlan _staleRenderPlan(int revision) {
    return FlarkRenderPlan(
      blocks: const [],
      metadata: {'revision': revision, 'stale': true},
    );
  }

  static FlarkRenderPlan _predictRenderPlan({
    required FlarkRenderPlan previousRenderPlan,
    required FlarkTransaction transaction,
    required FlarkProjection projection,
    required int revision,
    required int textLengthAfter,
  }) {
    if (previousRenderPlan.blocks.isEmpty) return _staleRenderPlan(revision);
    return previousRenderPlan.predictThroughTransaction(
      transaction: transaction,
      projection: projection,
      revision: revision,
      textLengthAfter: textLengthAfter,
    );
  }
}
