import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../projection/projection.dart';
import '../render_plan/render_plan.dart';
import 'sovereign_text_delta_adapter.dart';

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

final class FlarkFlutterController extends ChangeNotifier {
  FlarkFlutterController({
    required FlarkEditorRuntime runtime,
    FlarkProjection? projection,
    FlarkRenderPlan? renderPlan,
    FlarkTextDeltaAdapter textDeltaAdapter = const FlarkTextDeltaAdapter(),
    FlarkProjectedTextEditAdapter projectedTextEditAdapter =
        const FlarkProjectedTextEditAdapter(),
  }) : _runtime = runtime,
       _projection =
           projection ??
           FlarkProjection(textLength: runtime.state.document.length),
       _renderPlan = renderPlan ?? _staleRenderPlan(runtime.state.revision),
       _renderPlanRevision = renderPlan == null ? null : runtime.state.revision,
       _textDeltaAdapter = textDeltaAdapter,
       _projectedTextEditAdapter = projectedTextEditAdapter;

  factory FlarkFlutterController.fromMarkdown(
    String markdown, {
    FlarkExtensionSet? extensions,
  }) {
    return FlarkFlutterController(
      runtime: FlarkEditorRuntime.fromMarkdown(
        markdown,
        extensions: extensions ?? FlarkMarkdownEditingExtensions.standard(),
      ),
    );
  }

  FlarkEditorRuntime _runtime;
  FlarkProjection _projection;
  FlarkRenderPlan _renderPlan;
  int? _renderPlanRevision;
  FlarkProjectionPrediction? _lastProjectionPrediction;
  final FlarkTextDeltaAdapter _textDeltaAdapter;
  final FlarkProjectedTextEditAdapter _projectedTextEditAdapter;
  final StreamController<FlarkControllerEvent> _events =
      StreamController<FlarkControllerEvent>.broadcast();

  FlarkEditorRuntime get runtime => _runtime;

  Stream<FlarkControllerEvent> get events => _events.stream;

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
