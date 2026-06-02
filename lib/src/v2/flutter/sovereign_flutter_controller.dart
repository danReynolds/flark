import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../core/core.dart';
import '../markdown/markdown.dart';
import '../projection/projection.dart';
import '../render_plan/render_plan.dart';
import 'sovereign_text_delta_adapter.dart';

enum SovereignControllerEventKind {
  runtimeChanged,
  selectionChanged,
  projectionPredicted,
  parseAdopted,
  undo,
  redo,
}

final class SovereignControllerEvent {
  const SovereignControllerEvent({
    required this.kind,
    required this.revision,
    required this.previousRevision,
    required this.markdownChanged,
    required this.selectionChanged,
  });

  final SovereignControllerEventKind kind;
  final int revision;
  final int previousRevision;
  final bool markdownChanged;
  final bool selectionChanged;
}

final class SovereignFlutterController extends ChangeNotifier {
  SovereignFlutterController({
    required SovereignEditorRuntime runtime,
    SovereignProjection? projection,
    SovereignRenderPlan? renderPlan,
    SovereignTextDeltaAdapter textDeltaAdapter =
        const SovereignTextDeltaAdapter(),
    SovereignProjectedTextEditAdapter projectedTextEditAdapter =
        const SovereignProjectedTextEditAdapter(),
  }) : _runtime = runtime,
       _projection =
           projection ??
           SovereignProjection(textLength: runtime.state.document.length),
       _renderPlan = renderPlan ?? _staleRenderPlan(runtime.state.revision),
       _renderPlanRevision = renderPlan == null ? null : runtime.state.revision,
       _textDeltaAdapter = textDeltaAdapter,
       _projectedTextEditAdapter = projectedTextEditAdapter;

  factory SovereignFlutterController.fromMarkdown(
    String markdown, {
    SovereignExtensionSet? extensions,
  }) {
    return SovereignFlutterController(
      runtime: SovereignEditorRuntime.fromMarkdown(
        markdown,
        extensions: extensions ?? SovereignMarkdownEditingExtensions.standard(),
      ),
    );
  }

  SovereignEditorRuntime _runtime;
  SovereignProjection _projection;
  SovereignRenderPlan _renderPlan;
  int? _renderPlanRevision;
  SovereignProjectionPrediction? _lastProjectionPrediction;
  final SovereignTextDeltaAdapter _textDeltaAdapter;
  final SovereignProjectedTextEditAdapter _projectedTextEditAdapter;
  final StreamController<SovereignControllerEvent> _events =
      StreamController<SovereignControllerEvent>.broadcast();

  SovereignEditorRuntime get runtime => _runtime;

  Stream<SovereignControllerEvent> get events => _events.stream;

  SovereignEditorState get state => _runtime.state;

  String get markdown => state.markdown;

  SovereignSelection get selection => state.selection;

  SovereignProjection get projection => _projection;

  SovereignRenderPlan get renderPlan => _renderPlan;

  SovereignProjectionPrediction? get lastProjectionPrediction {
    return _lastProjectionPrediction;
  }

  bool get hasAuthoritativeRenderPlan {
    return _renderPlanRevision == state.revision;
  }

  SovereignEditorRuntimeResult dispatch<TPayload>({
    required SovereignCommand<TPayload> command,
    required TPayload payload,
  }) {
    final result = _runtime.dispatch(command: command, payload: payload);
    _adoptRuntimeResult(result);
    return result;
  }

  SovereignEditorRuntimeResult applyTransaction(
    SovereignTransaction transaction,
  ) {
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
    SovereignMapAffinity fallbackInsertionAffinity =
        SovereignMapAffinity.downstream,
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
    SovereignSelection displaySelection, {
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
  }) {
    final sourceSelection = projection.displaySelectionToSource(
      displaySelection,
      affinity: affinity,
    );
    return applySelection(sourceSelection, userEvent: 'selection.projected');
  }

  bool applySelection(
    SovereignSelection sourceSelection, {
    String userEvent = 'selection',
  }) {
    sourceSelection.validate(state.document.length);
    if (sourceSelection == selection) return false;
    applyTransaction(
      SovereignTransaction(
        operations: const [],
        selectionAfter: sourceSelection,
        metadata: SovereignTransactionMetadata(
          intent: SovereignTransactionIntent.selection,
          userEvent: userEvent,
          addToHistory: false,
        ),
      ),
    );
    return true;
  }

  SovereignEditorRuntimeResult undo() {
    final result = _runtime.undo();
    _adoptRuntimeResult(result, eventKind: SovereignControllerEventKind.undo);
    return result;
  }

  SovereignEditorRuntimeResult redo() {
    final result = _runtime.redo();
    _adoptRuntimeResult(result, eventKind: SovereignControllerEventKind.redo);
    return result;
  }

  bool applyParseResult(SovereignMarkdownParseResult parseResult) {
    if (parseResult.revision != state.revision ||
        parseResult.sourceTextLength != state.document.length) {
      return false;
    }

    final nextProjection = SovereignProjection.fromParseResult(parseResult);
    _projection = nextProjection;
    final baseRenderPlan = SovereignRenderPlan.fromParseResult(
      parseResult: parseResult,
      projection: nextProjection,
    );
    _renderPlan = applySovereignRenderPlanExtensions(
      renderPlan: baseRenderPlan,
      parseResult: parseResult,
      projection: nextProjection,
      extensions: _runtime.extensions,
    );
    _renderPlanRevision = parseResult.revision;
    _lastProjectionPrediction = null;
    _emitEvent(
      kind: SovereignControllerEventKind.parseAdopted,
      previousState: state,
    );
    notifyListeners();
    return true;
  }

  void _adoptRuntimeResult(
    SovereignEditorRuntimeResult result, {
    SovereignControllerEventKind? eventKind,
  }) {
    if (identical(result.runtime, _runtime)) return;

    final transaction = result.commandResult.transaction;
    final previousProjection = _projection;
    final previousRenderPlan = _renderPlan;
    final previousState = state;
    _runtime = result.runtime;
    if (transaction == null) {
      _projection = SovereignProjection(textLength: state.document.length);
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

  SovereignControllerEventKind _eventKindForRuntimeChange(
    SovereignEditorState previousState,
  ) {
    if (previousState.markdown == state.markdown &&
        previousState.selection != state.selection) {
      return SovereignControllerEventKind.selectionChanged;
    }
    if (_lastProjectionPrediction != null) {
      return SovereignControllerEventKind.projectionPredicted;
    }
    return SovereignControllerEventKind.runtimeChanged;
  }

  void _emitEvent({
    required SovereignControllerEventKind kind,
    required SovereignEditorState previousState,
  }) {
    if (_events.isClosed) return;
    _events.add(
      SovereignControllerEvent(
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

  static SovereignRenderPlan _staleRenderPlan(int revision) {
    return SovereignRenderPlan(
      blocks: const [],
      metadata: {'revision': revision, 'stale': true},
    );
  }

  static SovereignRenderPlan _predictRenderPlan({
    required SovereignRenderPlan previousRenderPlan,
    required SovereignTransaction transaction,
    required SovereignProjection projection,
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
