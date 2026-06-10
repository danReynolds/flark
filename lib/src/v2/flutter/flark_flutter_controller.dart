import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../markdown/source/flark_markdown_fenced_code_scanner.dart';
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
    // Only backend/profile/debounce changes need a scheduler restart. The
    // error callback is read through a stable forwarder, so swapping it in
    // place keeps the debounce timer running — widget rebuilds that pass a
    // fresh inline closure must not restart parsing every frame.
    final restartNeeded =
        parseBackend != null || parseProfile != null || parseDebounce != null;
    if (parseBackend != null) _parseBackend = parseBackend;
    if (parseProfile != null) _parseProfile = parseProfile;
    if (parseDebounce != null) _parseDebounce = parseDebounce;
    if (onParseError != null || clearOnParseError) {
      _onParseError = onParseError;
    }
    if (_parseScheduler == null || !restartNeeded) return;
    final wasStarted = _parseStarted;
    _parseScheduler!.dispose();
    _parseScheduler = null;
    _parseStarted = false;
    if (wasStarted) ensureParsing();
  }

  /// Parses until the current revision has an authoritative render plan,
  /// bypassing the debounce window.
  ///
  /// Resolves immediately when the plan is already authoritative, and chains
  /// onto an in-flight parse instead of silently returning, so the returned
  /// future means "the plan is current". If [ensureParsing] has not been
  /// called, this performs a one-shot parse without installing a background
  /// debounce loop, so advanced widgets that drive parsing per structural
  /// edit do not leak pending timers. Errors are routed to the configured
  /// parse-error callback rather than thrown.
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
      // A stable forwarder, so configureParsing can swap the callback
      // without restarting the scheduler.
      onError: (error, stackTrace) => _onParseError?.call(error, stackTrace),
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

  /// Whether [renderPlan] is renderable by block-based surfaces.
  ///
  /// True for an authoritative plan of the current revision, and for a
  /// non-empty predicted plan mapped through recent edits. False only when the
  /// plan is a stale placeholder (or a prediction emptied of blocks), in which
  /// case surfaces fall back to plain projected text until the next parse.
  bool get hasUsableRenderPlan {
    assert(
      _renderPlan.blocks.isEmpty ||
          _renderPlan.fidelity != FlarkRenderPlanFidelity.stale,
      'Stale render plans must not carry blocks.',
    );
    return hasAuthoritativeRenderPlan || _renderPlan.blocks.isNotEmpty;
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
    if (_disposed) return false;
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

    final documentTransactions = [
      for (final transaction in result.appliedTransactions)
        if (transaction.changesDocument) transaction,
    ];
    final previousProjection = _projection;
    final previousRenderPlan = _renderPlan;
    final previousState = state;
    _runtime = result.runtime;
    if (result.appliedTransactions.isEmpty) {
      // The runtime changed without telling us how (no applied transactions).
      // There is nothing to map the projection or render plan through, so
      // reset both and let the next parse rebuild them.
      _projection = FlarkProjection(textLength: state.document.length);
      _lastProjectionPrediction = null;
      _renderPlan = _staleRenderPlan(state.revision);
      _renderPlanRevision = null;
    } else if (documentTransactions.isEmpty) {
      _projection = previousProjection;
      _lastProjectionPrediction = null;
    } else if (documentTransactions.length == 1) {
      final transaction = documentTransactions.single;
      final prediction = previousProjection.predictAfter(
        transaction,
        textLengthAfter: state.document.length,
      );
      final structuralPrediction = _predictStructuralRenderPlan(
        markdown: state.markdown,
        revision: state.revision,
        projection: prediction.projection,
        previousRenderPlan: previousRenderPlan,
        transaction: transaction,
      );
      _projection = structuralPrediction?.projection ?? prediction.projection;
      _lastProjectionPrediction = structuralPrediction == null
          ? prediction
          : null;
      _renderPlan =
          structuralPrediction?.renderPlan ??
          _predictRenderPlan(
            previousRenderPlan: previousRenderPlan,
            transaction: transaction,
            projection: _projection,
            revision: state.revision,
            textLengthAfter: state.document.length,
          );
      _renderPlanRevision = null;
    } else {
      // Several transactions applied atomically (a grouped undo/redo entry).
      // Map the projection and render plan through each in order; the
      // intermediate text length steps by each transaction's net delta.
      var projection = previousProjection;
      var renderPlan = previousRenderPlan;
      var textLength = previousState.document.length;
      var touchedSensitiveRange = false;
      FlarkSourceRange? invalidatedRange;
      FlarkProjectionPrediction? prediction;
      for (final transaction in documentTransactions) {
        textLength += _transactionNetDelta(transaction);
        prediction = projection.predictAfter(
          transaction,
          textLengthAfter: textLength,
        );
        touchedSensitiveRange =
            touchedSensitiveRange || prediction.touchedProjectionSensitiveRange;
        final stepInvalidated = prediction.invalidatedRange;
        if (stepInvalidated != null) {
          invalidatedRange =
              invalidatedRange?.union(stepInvalidated) ?? stepInvalidated;
        }
        projection = prediction.projection;
        renderPlan = _predictRenderPlan(
          previousRenderPlan: renderPlan,
          transaction: transaction,
          projection: projection,
          revision: state.revision,
          textLengthAfter: textLength,
        );
      }
      assert(
        textLength == state.document.length,
        'Applied transactions must net to the new document length.',
      );
      _projection = projection;
      // Merge the per-step prediction metadata: a consumer of
      // lastProjectionPrediction must see the union of what the grouped
      // transactions touched, not just the final step's view.
      _lastProjectionPrediction = prediction == null
          ? null
          : FlarkProjectionPrediction(
              projection: projection,
              touchedProjectionSensitiveRange: touchedSensitiveRange,
              invalidatedRange: invalidatedRange,
            );
      _renderPlan = renderPlan;
      _renderPlanRevision = null;
    }
    _emitEvent(
      kind: eventKind ?? _eventKindForRuntimeChange(previousState),
      previousState: previousState,
    );
    notifyListeners();
  }

  static int _transactionNetDelta(FlarkTransaction transaction) {
    var delta = 0;
    for (final operation in transaction.operations) {
      delta += operation.delta;
    }
    return delta;
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
      metadata: {'revision': revision},
      fidelity: FlarkRenderPlanFidelity.stale,
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

  static _PredictedStructuralRenderPlan? _predictStructuralRenderPlan({
    required String markdown,
    required int revision,
    required FlarkProjection projection,
    required FlarkRenderPlan previousRenderPlan,
    required FlarkTransaction transaction,
  }) {
    if (markdown.isEmpty) return null;
    final context = _predictiveCodeFenceContext(
      markdown: markdown,
      transaction: transaction,
    );
    if (context == null || !_canPredictCodeFence(markdown, context)) {
      return null;
    }
    final markerRanges = _predictiveCodeFenceMarkerRanges(markdown, context);
    final structuralProjection = FlarkProjection(
      textLength: markdown.length,
      hiddenRanges: [
        for (final hiddenRange in projection.hiddenRanges)
          if (!_overlapsAny(hiddenRange.range, markerRanges)) hiddenRange,
        for (final markerRange in markerRanges)
          FlarkHiddenRange(
            range: markerRange,
            kind: FlarkHiddenRangeKind.markdownMarker,
          ),
      ],
      replacementRanges: projection.replacementRanges,
      ambiguityZones: projection.ambiguityZones,
    );
    final predictedPreviousRenderPlan = previousRenderPlan
        .predictThroughTransaction(
          transaction: transaction,
          projection: structuralProjection,
          revision: revision,
          textLengthAfter: markdown.length,
        );
    final blockEnd = context.closingLineEnd ?? markdown.length;
    final predictedCodeBlock = FlarkRenderBlock(
      kind: FlarkMarkdownBlockKind.codeBlock,
      type: 'codeBlock',
      sourceRange: FlarkSourceRange(context.openingLineStart, blockEnd),
      displayRange: FlarkSourceRange(
        structuralProjection.sourceToDisplayOffset(context.openingLineStart),
        structuralProjection.sourceToDisplayOffset(blockEnd),
      ),
      styleToken: FlarkRenderTextStyleToken.body,
      inlineRuns: const [],
      children: const [],
      codeBlock: FlarkRenderCodeBlockDescriptor(language: context.language),
    );
    final predictedBlocks = [
      for (final block in predictedPreviousRenderPlan.blocks)
        if (!_rangesOverlap(block.sourceRange, predictedCodeBlock.sourceRange))
          block,
      predictedCodeBlock,
    ];
    return _PredictedStructuralRenderPlan(
      projection: structuralProjection,
      renderPlan: FlarkRenderPlan(
        blocks: predictedBlocks,
        metadata: {
          ...predictedPreviousRenderPlan.metadata,
          'revision': revision,
        },
        fidelity: FlarkRenderPlanFidelity.predicted,
      ),
    );
  }

  static FlarkMarkdownFencedCodeContext? _predictiveCodeFenceContext({
    required String markdown,
    required FlarkTransaction transaction,
  }) {
    // One fence scan per predicted edit; both probes query the shared layout
    // so the prediction cannot disagree with the policy layer's fence model.
    final layout = FlarkMarkdownFenceLayout.scan(markdown);
    final insertedContext = _insertedCodeFenceContext(
      markdown: markdown,
      transaction: transaction,
      layout: layout,
    );
    if (insertedContext != null) return insertedContext;
    return layout.contextAt(markdown.length);
  }

  static FlarkMarkdownFencedCodeContext? _insertedCodeFenceContext({
    required String markdown,
    required FlarkTransaction transaction,
    required FlarkMarkdownFenceLayout layout,
  }) {
    var delta = 0;
    final operations = [...transaction.operations]
      ..sort((left, right) {
        final startCompare = left.replacedRange.start.compareTo(
          right.replacedRange.start,
        );
        if (startCompare != 0) return startCompare;
        return left.replacedRange.end.compareTo(right.replacedRange.end);
      });

    for (final operation in operations) {
      final insertedStart = (operation.replacedRange.start + delta).clamp(
        0,
        markdown.length,
      );
      final insertedEnd = (insertedStart + operation.insertedLength).clamp(
        insertedStart,
        markdown.length,
      );
      delta += operation.delta;
      final insertedRange = FlarkSourceRange(insertedStart, insertedEnd);
      var lineStart = FlarkMarkdownFencedCodeScanner.lineStartForOffset(
        markdown,
        insertedStart,
      );
      while (lineStart <= insertedEnd && lineStart < markdown.length) {
        final context = layout.openerAt(lineStart);
        if (context != null &&
            _rangesOverlap(
              insertedRange,
              FlarkSourceRange(
                context.openingLineStart,
                context.openingLineEndWithBreak,
              ),
            )) {
          return context;
        }

        final next = FlarkMarkdownFencedCodeScanner.lineEndWithBreak(
          markdown,
          lineStart,
        );
        if (next <= lineStart || next >= markdown.length) break;
        lineStart = next;
      }
    }

    return null;
  }

  static bool _canPredictCodeFence(
    String markdown,
    FlarkMarkdownFencedCodeContext context,
  ) {
    if (context.openingLineEndWithBreak <= context.openingLineStart ||
        context.bodyStart > markdown.length) {
      return false;
    }
    if (context.isClosed) return context.closingLineEnd != null;
    return context.openingLineEndWithBreak < markdown.length ||
        markdown.endsWith('\n');
  }

  static List<FlarkSourceRange> _predictiveCodeFenceMarkerRanges(
    String markdown,
    FlarkMarkdownFencedCodeContext context,
  ) {
    final ranges = <FlarkSourceRange>[
      FlarkSourceRange(context.openingLineStart, context.bodyStart),
    ];
    final closingLineStart = context.closingLineStart;
    final closingLineEnd = context.closingLineEnd;
    if (closingLineStart != null && closingLineEnd != null) {
      var closingHiddenStart = closingLineStart;
      if (closingHiddenStart > context.bodyStart &&
          _isLineBreakBefore(markdown, closingHiddenStart)) {
        closingHiddenStart -= 1;
      }
      ranges.add(FlarkSourceRange(closingHiddenStart, closingLineEnd));
    }
    return ranges;
  }

  static bool _isLineBreakBefore(String markdown, int offset) {
    if (offset <= 0 || offset > markdown.length) return false;
    final codeUnit = markdown.codeUnitAt(offset - 1);
    return codeUnit == 0x0A || codeUnit == 0x0D;
  }

  static bool _rangesOverlap(FlarkSourceRange left, FlarkSourceRange right) {
    return left.start < right.end && right.start < left.end;
  }

  static bool _overlapsAny(
    FlarkSourceRange range,
    Iterable<FlarkSourceRange> others,
  ) {
    for (final other in others) {
      if (_rangesOverlap(range, other)) return true;
    }
    return false;
  }
}

final class _PredictedStructuralRenderPlan {
  const _PredictedStructuralRenderPlan({
    required this.projection,
    required this.renderPlan,
  });

  final FlarkProjection projection;
  final FlarkRenderPlan renderPlan;
}
