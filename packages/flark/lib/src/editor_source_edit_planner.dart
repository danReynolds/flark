import 'dart:math' as math;

import 'editor_coordinator.dart';
import 'editor_text.dart';
import 'editor_viewport_state.dart';
import 'models.dart';
import 'pending_presentation.dart';
import 'pending_presentation_evolution.dart';
import 'presentation.dart';
import 'projection_continuity.dart';
import 'surface_projector.dart';

/// The only two presentation outcomes of an accepted source edit.
enum FlarkQueuedEditPublication {
  publishOptimistically,
  retainPublishedUntilCertified;

  bool get publishesOptimistically => this == publishOptimistically;
  bool get requiresParserCertification => this == retainPublishedUntilCertified;
}

/// Host-neutral facts needed to evolve parser-authorized presentation across
/// one exact source splice.
final class FlarkEditorSourceEditPlanningRequest {
  const FlarkEditorSourceEditPlanningRequest({
    required this.revision,
    required this.startUtf16,
    required this.endUtf16,
    required this.replacement,
    required this.inputGlobalUtf16Start,
    required this.inputValue,
    required this.activeOrdinal,
    required this.selectionBaseUtf16,
    required this.selectionExtentUtf16,
    required this.crossRowSelection,
    required this.compositionUsesExactFallback,
    required this.requiresStructuralCertification,
  });

  final int revision;
  final int startUtf16;
  final int endUtf16;
  final String replacement;
  final int inputGlobalUtf16Start;
  final FlarkEditorInputValue inputValue;
  final int? activeOrdinal;
  final int selectionBaseUtf16;
  final int selectionExtentUtf16;
  final bool crossRowSelection;
  final bool compositionUsesExactFallback;
  final bool requiresStructuralCertification;
}

/// Complete pending-presentation decision for one source splice.
final class FlarkEditorSourceEditPlan {
  const FlarkEditorSourceEditPlan({
    required this.publication,
    required this.projectionReceipt,
    required this.usesExactFallback,
  });

  final FlarkQueuedEditPublication publication;
  final FlarkProjectionEditCellReceipt? projectionReceipt;
  final bool usesExactFallback;
}

/// Evolves parser-authorized pending presentation for ordinary source edits.
///
/// This planner owns no source, viewport, or host-input state. It updates the
/// coordinator's one pending-presentation snapshot synchronously and returns
/// the exact publication requirement the host must honor before invoking the
/// native command.
final class FlarkEditorSourceEditPlanner {
  const FlarkEditorSourceEditPlanner({
    required FlarkEditorCoordinator coordinator,
    required FlarkEditorViewportState viewportState,
  }) : _coordinator = coordinator,
       _viewportState = viewportState;

  final FlarkEditorCoordinator _coordinator;
  final FlarkEditorViewportState _viewportState;

  FlarkEditorSourceEditPlan plan(FlarkEditorSourceEditPlanningRequest request) {
    if (request.revision < 0 ||
        request.startUtf16 < 0 ||
        request.endUtf16 < request.startUtf16) {
      throw ArgumentError('Source edit planning ranges must be valid');
    }
    final start = request.startUtf16;
    final end = request.endUtf16;
    final replacement = request.replacement;
    final split = _pending.paragraphGap;
    if (split != null &&
        (start < split.rowEndUtf16 ||
            end > committedGapEnd(split, _projector(request)))) {
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.paragraphGap,
      });
    }

    final insertsNonLineEndingText =
        replacement.isNotEmpty &&
        !replacement.contains('\n') &&
        !replacement.contains('\r');
    final caretBoundary = _pending.caretBoundary;
    var caretBoundaryStartsExactBlock = false;
    if (caretBoundary != null) {
      final boundaryEnd = committedCaretBoundaryEnd(
        caretBoundary,
        _projector(request),
      );
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

    final structurals = _pending.structuralSurfaces;
    final neutralInputStartsExactBlock =
        (request.activeOrdinal ?? 0) < 0 &&
        start == end &&
        insertsNonLineEndingText;
    final editStartsExactFallback =
        request.compositionUsesExactFallback ||
        caretBoundaryStartsExactBlock ||
        neutralInputStartsExactBlock;
    FlarkProjectionEditCellReceipt? projectionReceipt;
    var structuralSuccessorRequiresCertification = false;
    if (!request.compositionUsesExactFallback &&
        editStartsExactFallback &&
        structurals.isNotEmpty) {
      projectionReceipt = _advanceCommittedStructuralSurfaces(request);
    }
    if (!request.compositionUsesExactFallback &&
        editStartsExactFallback &&
        projectionReceipt == null &&
        structurals.isEmpty &&
        caretBoundary != null) {
      projectionReceipt = _advanceCommittedCaretBoundary(
        caretBoundary,
        request,
      );
    }

    final editUsesExactFallback =
        editStartsExactFallback && projectionReceipt == null;
    final inputText = request.inputValue.text;
    final firstLf = inputText.indexOf('\n');
    final firstCr = inputText.indexOf('\r');
    final firstLineEnding = firstLf < 0
        ? firstCr
        : firstCr < 0
        ? firstLf
        : math.min(firstLf, firstCr);
    final lookaheadStart = firstLineEnding < 0
        ? inputText.length
        : firstLineEnding +
              (firstCr == firstLineEnding &&
                      firstLineEnding + 1 < inputText.length &&
                      inputText.codeUnitAt(firstLineEnding + 1) == 0x0a
                  ? 2
                  : 1);
    final exactFallbackHasStructuralLookahead =
        !request.compositionUsesExactFallback &&
        editUsesExactFallback &&
        lookaheadStart < inputText.length;
    final exactFallbackHasCertifiedNeighbor =
        !request.compositionUsesExactFallback &&
        editUsesExactFallback &&
        !caretBoundaryStartsExactBlock &&
        _viewportState.rows.any((row) {
          final range = _projector(request).surfaceSourceRange(row);
          return range.end <= start || end <= range.start;
        });

    if (editUsesExactFallback) {
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
        FlarkPendingPresentationPart.structuralSurfaces,
      });
    } else if (projectionReceipt == null && structurals.isNotEmpty) {
      projectionReceipt = _advanceCommittedStructuralSurfaces(request);
      if (projectionReceipt == null) {
        structuralSuccessorRequiresCertification = true;
        _coordinator.retirePendingPresentation(const {
          FlarkPendingPresentationPart.dependency,
          FlarkPendingPresentationPart.structuralSurfaces,
        });
      }
    } else {
      projectionReceipt = _prepareProjectionContinuity(request);
    }

    final lacksResultPresentationAuthority =
        !editUsesExactFallback &&
        projectionReceipt == null &&
        _pending.dependency == null;
    final structuralOneShotRequiresCertification =
        !editUsesExactFallback &&
        structurals.isNotEmpty &&
        projectionReceipt != null &&
        !projectionReceipt.chainResultCell;
    final requiresParserCertification =
        _coordinator.publicationCertificationBarrierActive ||
        structuralSuccessorRequiresCertification ||
        structuralOneShotRequiresCertification ||
        exactFallbackHasStructuralLookahead ||
        exactFallbackHasCertifiedNeighbor ||
        lacksResultPresentationAuthority ||
        (request.requiresStructuralCertification &&
            projectionReceipt == null &&
            !editUsesExactFallback);
    return FlarkEditorSourceEditPlan(
      publication: requiresParserCertification
          ? FlarkQueuedEditPublication.retainPublishedUntilCertified
          : FlarkQueuedEditPublication.publishOptimistically,
      projectionReceipt: projectionReceipt,
      usesExactFallback: editUsesExactFallback,
    );
  }

  FlarkPendingPresentationSnapshot get _pending =>
      _coordinator.pendingPresentation;

  FlarkSurfaceProjector _projector(
    FlarkEditorSourceEditPlanningRequest request,
  ) => _viewportState.captureSurfaceProjector(
    pendingPresentation: _pending,
    inputGlobalUtf16Start: request.inputGlobalUtf16Start,
    inputValue: request.inputValue,
    activeOrdinal: request.activeOrdinal,
    selectionBaseUtf16: request.selectionBaseUtf16,
    selectionExtentUtf16: request.selectionExtentUtf16,
    crossRowSelection: request.crossRowSelection,
  );

  /// Exact input boundary owned by a committed paragraph gap.
  int committedGapEnd(
    FlarkCoreCommittedPresentationGapV1 split,
    FlarkSurfaceProjector projector,
  ) {
    var end = _viewportState.visibleUtf16End;
    final localStart = split.rowEndUtf16 - _viewportState.visibleUtf16Start;
    if (0 <= localStart && localStart < _viewportState.visibleSource.length) {
      final newline = _viewportState.visibleSource.indexOf('\n', localStart);
      if (newline >= 0) end = _viewportState.visibleUtf16Start + newline + 1;
    }
    for (final row in _viewportState.rows) {
      if (row.ordinal == split.rowOrdinal) continue;
      final start = projector.surfaceSourceRange(row).start;
      if (start > split.rowEndUtf16) end = math.min(end, start);
    }
    return end;
  }

  /// Exact source boundary owned by a retained structural caret handoff.
  int committedCaretBoundaryEnd(
    FlarkPendingCaretBoundary boundary,
    FlarkSurfaceProjector projector,
  ) {
    var end = _viewportState.visibleUtf16End;
    for (final row in _viewportState.rows) {
      if (row.ordinal == boundary.rowOrdinal) continue;
      final start = projector.surfaceSourceRange(row).start;
      if (start >= boundary.rowEndUtf16) end = math.min(end, start);
    }
    return end;
  }

  /// Whether one source range is wholly owned by a pending non-AST editing
  /// boundary rather than a certified parser row.
  bool editorOwnedBoundaryContains(
    int start,
    int end,
    FlarkSurfaceProjector projector,
  ) {
    if (start > end) return false;
    final gap = _pending.paragraphGap;
    if (gap != null &&
        gap.rowEndUtf16 <= start &&
        end <= committedGapEnd(gap, projector)) {
      return true;
    }
    final boundary = _pending.caretBoundary;
    final boundaryEnd = boundary == null
        ? null
        : _committedCaretBoundaryInputEnd(boundary);
    return boundary != null &&
        boundaryEnd != null &&
        boundary.rowEndUtf16 <= start &&
        end <= boundaryEnd;
  }

  int? _committedCaretBoundaryInputEnd(FlarkPendingCaretBoundary boundary) {
    final localStart = boundary.rowEndUtf16 - _viewportState.visibleUtf16Start;
    if (localStart < 0 || localStart > _viewportState.visibleSource.length) {
      return null;
    }
    final newline = _viewportState.visibleSource.indexOf('\n', localStart);
    return newline == -1
        ? _viewportState.visibleUtf16End
        : _viewportState.visibleUtf16Start + newline + 1;
  }

  FlarkProjectionEditCellReceipt? _prepareProjectionContinuity(
    FlarkEditorSourceEditPlanningRequest request,
  ) {
    final current = _pending.dependency;
    if (current != null) {
      final successor = current.authority.continueWith(
        startUtf16: request.startUtf16,
        endUtf16: request.endUtf16,
        replacement: request.replacement,
      );
      if (successor != null) {
        final dependency = advancePendingDependencyPresentation(
          current: current,
          authority: successor,
          visibleSource: _viewportState.visibleSource,
          visibleUtf16Start: _viewportState.visibleUtf16Start,
          startUtf16: request.startUtf16,
          endUtf16: request.endUtf16,
          replacement: request.replacement,
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
    if (_viewportState.hasOptimisticEdits) return null;
    final row = _activeRow(request.activeOrdinal);
    if (row == null) return null;
    final projector = _projector(request);
    final activation = _viewportState.mapRange(projector.activationRange(row));
    if (!projector.rowSemanticsCurrent(activation)) return null;
    final editable = _viewportState.mapRange(
      row.editableUtf16 ?? projector.activationRange(row),
    );
    final base = projector.surfaceRow(row, includeEditingState: false);
    final authority = bindPendingDependencyAuthority(
      revision: request.revision,
      plans: row.pendingPresentationPlans,
      cells: row.projectionEditCells,
      envelopes: row.literalSafeEnvelopes,
      authorizedContentUtf16: editable,
      authorizedBlockUtf16: projector.mappedExactRowRange(row),
      startUtf16: request.startUtf16,
      endUtf16: request.endUtf16,
      replacement: request.replacement,
    );
    if (authority != null) {
      final dependency = bindPendingDependencyPresentation(
        rowOrdinal: row.ordinal,
        base: FlarkSurfaceProjector.corePresentationFromSurface(
          base,
          projector.surfaceSourceRange(row),
        ),
        authority: authority,
        visibleSource: _viewportState.visibleSource,
        visibleUtf16Start: _viewportState.visibleUtf16Start,
        startUtf16: request.startUtf16,
        endUtf16: request.endUtf16,
        replacement: request.replacement,
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

  FlarkProjectionEditCellReceipt? _advanceCommittedStructuralSurfaces(
    FlarkEditorSourceEditPlanningRequest request,
  ) {
    final candidates =
        <
          ({
            int index,
            FlarkProjectionEditCellReceipt receipt,
            FlarkCorePresentationRow presentation,
          })
        >[];
    for (var index = 0; index < _pending.structuralSurfaces.length; index++) {
      final state = _pending.structuralSurfaces[index];
      final surface = state.surface;
      if (!surface.projectionCurrent) continue;
      final authority =
          state.continuity?.continueWith(
            startUtf16: request.startUtf16,
            endUtf16: request.endUtf16,
            replacement: request.replacement,
          ) ??
          bindPendingDependencyAuthority(
            revision: request.revision,
            cells: surface.projectionEditCells,
            envelopes: const [],
            authorizedContentUtf16: surface.sourceUtf16,
            startUtf16: request.startUtf16,
            endUtf16: request.endUtf16,
            replacement: request.replacement,
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
        startUtf16: request.startUtf16,
        endUtf16: request.endUtf16,
        replacement: request.replacement,
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
    final states = [..._pending.structuralSurfaces];
    final previous = states[matched.index].surface;
    final delta =
        request.replacement.length - (request.endUtf16 - request.startUtf16);
    final source = FlarkSourceRange(
      previous.sourceUtf16.start,
      previous.sourceUtf16.end + delta,
    );
    if (matched.receipt.affectedUtf16.start < source.start ||
        matched.receipt.affectedUtf16.end > source.end) {
      return null;
    }
    states[matched.index] = FlarkPendingStructuralSurface(
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
    FlarkEditorSourceEditPlanningRequest request,
  ) {
    final authorized = boundary.authorizedContentUtf16;
    if (authorized == null || boundary.projectionEditCells.isEmpty) return null;
    final authority = bindPendingDependencyAuthority(
      revision: request.revision,
      cells: boundary.projectionEditCells,
      envelopes: const [],
      authorizedContentUtf16: authorized,
      startUtf16: request.startUtf16,
      endUtf16: request.endUtf16,
      replacement: request.replacement,
    );
    return authority is FlarkProjectionEditCellReceipt ? authority : null;
  }

  FlarkViewportRow? _activeRow(int? ordinal) {
    if (ordinal == null) return null;
    for (final row in _viewportState.rows) {
      if (row.ordinal == ordinal) return row;
    }
    return null;
  }
}
