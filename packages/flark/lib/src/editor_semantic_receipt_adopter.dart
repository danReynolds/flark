import 'editor_command_executor.dart';
import 'editor_coordinator.dart';
import 'editor_session.dart';
import 'editor_text.dart';
import 'editor_viewport_pager.dart';
import 'editor_viewport_state.dart';
import 'pending_presentation.dart';
import 'pending_presentation_evolution.dart';
import 'surface_projector.dart';
import 'viewport_navigation.dart';

/// Host-neutral facts needed to adopt one committed semantic edit receipt.
final class FlarkEditorSemanticReceiptAdoptionRequest {
  const FlarkEditorSemanticReceiptAdoptionRequest({
    required this.execution,
    required this.outcome,
    required this.inputGlobalUtf16Start,
    required this.inputValue,
    required this.activeOrdinal,
    required this.selectionBaseUtf16,
    required this.selectionExtentUtf16,
    required this.crossRowSelection,
  });

  final FlarkEditorCommandExecution<FlarkCoreEditIntentOutcomeV1> execution;
  final FlarkCoreEditIntentOutcomeV1 outcome;
  final int inputGlobalUtf16Start;
  final FlarkEditorInputValue inputValue;
  final int? activeOrdinal;
  final int selectionBaseUtf16;
  final int selectionExtentUtf16;
  final bool crossRowSelection;
}

/// Portable publication state produced by one current semantic receipt.
final class FlarkEditorSemanticReceiptAdoption {
  const FlarkEditorSemanticReceiptAdoption({
    required this.caretUtf16,
    required this.inlineContinuation,
    required this.requiresParserCertification,
  });

  final int caretUtf16;
  final FlarkCoreInlineContinuationV1? inlineContinuation;
  final bool requiresParserCertification;
}

/// Atomically applies a committed semantic receipt to portable editor state.
///
/// Rust remains the source and Markdown authority. This boundary validates the
/// command generation, advances the coordinator's pending presentation, and
/// applies the exact receipt splice to the bounded viewport and navigation
/// state. It owns no host input object, callback, timer, or outward publish.
final class FlarkEditorSemanticReceiptAdopter {
  const FlarkEditorSemanticReceiptAdopter({
    required FlarkEditorCoordinator coordinator,
    required FlarkEditorCommandExecutor commands,
    required FlarkEditorViewportState viewportState,
    required FlarkEditorViewportPager viewportPager,
    required int maximumVisibleCodeUnits,
  }) : _coordinator = coordinator,
       _commands = commands,
       _viewportState = viewportState,
       _viewportPager = viewportPager,
       _maximumVisibleCodeUnits = maximumVisibleCodeUnits;

  final FlarkEditorCoordinator _coordinator;
  final FlarkEditorCommandExecutor _commands;
  final FlarkEditorViewportState _viewportState;
  final FlarkEditorViewportPager _viewportPager;
  final int _maximumVisibleCodeUnits;

  FlarkEditorSemanticReceiptAdoption? adopt(
    FlarkEditorSemanticReceiptAdoptionRequest request,
  ) {
    final receipt = request.outcome.receipt;
    if (!receipt.hasCommit) {
      throw ArgumentError('Only a committed semantic receipt can be adopted');
    }
    if (_maximumVisibleCodeUnits <= 0) {
      throw StateError('The visible source bound must be positive');
    }
    if (!_commands.publishSource(request.execution)) return null;

    // A semantic splice carries fresh Rust authority and cannot continue a
    // predecessor literal edit proof. Retire it before capturing the rows
    // used to resolve the receipt's structural transition.
    _coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.dependency,
    });
    final projector = _viewportState.captureSurfaceProjector(
      pendingPresentation: _coordinator.pendingPresentation,
      inputGlobalUtf16Start: request.inputGlobalUtf16Start,
      inputValue: request.inputValue,
      activeOrdinal: request.activeOrdinal,
      selectionBaseUtf16: request.selectionBaseUtf16,
      selectionExtentUtf16: request.selectionExtentUtf16,
      crossRowSelection: request.crossRowSelection,
    );
    final transition = resolvePendingPresentationTransition(
      receipt: receipt,
      pendingPresentation: _coordinator.pendingPresentation,
      activeOrdinal: request.activeOrdinal,
      priorRows: _viewportState.rows
          .map(
            (row) => FlarkSurfaceProjector.corePresentationFromSurface(
              projector.surfaceRow(row, includeEditingState: false),
              projector.surfaceSourceRange(row),
            ),
          )
          .toList(growable: false),
    );
    final presentation = _commands.adoptCommittedPresentation(
      request.execution,
      receipt: receipt,
      transition: transition,
    );
    if (presentation == null) {
      throw StateError(
        'A current semantic receipt became stale during adoption',
      );
    }

    // The result byte/UTF-16 pair remains authoritative even when the splice
    // crosses the cached page. Pin it before a bounded fallback can replace
    // the old viewport and its navigation path.
    _viewportPager.pinRefreshAnchor(
      FlarkViewportPageAnchor(
        byte: receipt.resultByteStart,
        utf16: receipt.resultUtf16Start,
      ),
    );
    final viewport = _viewportState.applyOptimisticEdit(
      globalStart: receipt.baseUtf16Start,
      globalEnd: receipt.baseUtf16End,
      replacement: receipt.replacement,
      fallbackSource: request.inputValue.text,
      fallbackUtf16Start: request.inputGlobalUtf16Start,
      focusUtf16: request.selectionExtentUtf16,
      maximumVisibleCodeUnits: _maximumVisibleCodeUnits,
      preservesMappedRowFacts: false,
    );
    if (viewport.disposition ==
        FlarkOptimisticViewportEditDisposition.replacedByBoundedWindow) {
      _viewportPager.resetPagePath();
    }
    if (presentation.removedRowOrdinals.isNotEmpty) {
      _viewportState.removeRows(presentation.removedRowOrdinals);
    }
    return FlarkEditorSemanticReceiptAdoption(
      caretUtf16: receipt.resultSelectionUtf16,
      inlineContinuation: request.outcome.inlineContinuation,
      requiresParserCertification: presentation.requiresParserCertification,
    );
  }
}
