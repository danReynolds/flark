import 'document.dart';
import 'models.dart';
import 'surface_projector.dart';

/// Host actions that may require a parser-owned structural source edit.
enum FlarkEditorSemanticCommand {
  insertParagraphBreak,
  deleteBackward,
  deleteForward,
}

/// Immutable host-neutral facts observed when one semantic command is offered.
final class FlarkEditorSemanticCommandPlanningRequest {
  const FlarkEditorSemanticCommandPlanningRequest({
    required this.command,
    required this.projector,
    required this.row,
    required this.localCaretUtf16,
    required this.semanticEditActive,
    required this.publicationCertificationBarrierActive,
  });

  final FlarkEditorSemanticCommand command;
  final FlarkSurfaceProjector projector;
  final FlarkViewportRow? row;
  final int localCaretUtf16;

  /// Whether a preceding semantic receipt still owns the retained input island.
  final bool semanticEditActive;

  /// Whether source publication is deliberately held for parser certification.
  final bool publicationCertificationBarrierActive;
}

/// A parser-capability-backed semantic command admitted for native execution.
final class FlarkEditorSemanticCommandAdmission {
  const FlarkEditorSemanticCommandAdmission(this.intent);

  final FlarkCoreEditIntentV1 intent;
}

/// Chooses the semantic command lane without interpreting Markdown syntax.
///
/// Rust authors the row and inline capabilities and revalidates the exact
/// revision, caret, and owner before changing source. This planner owns the
/// portable boundary policy that decides whether a host Return, Backspace, or
/// Delete should be offered to that authority instead of applied literally.
final class FlarkEditorSemanticCommandPlanner {
  const FlarkEditorSemanticCommandPlanner();

  FlarkEditorSemanticCommandAdmission? plan(
    FlarkEditorSemanticCommandPlanningRequest request,
  ) {
    final input = request.projector.inputValue;
    if (!input.selection.isCollapsed ||
        request.localCaretUtf16 != input.selection.extentOffset ||
        request.publicationCertificationBarrierActive ||
        request.localCaretUtf16 < 0 ||
        request.localCaretUtf16 > input.text.length) {
      return null;
    }
    final row = request.row;
    if (row != null && row.ordinal != request.projector.activeOrdinal) {
      return null;
    }
    return switch (request.command) {
      FlarkEditorSemanticCommand.insertParagraphBreak => _planParagraphBreak(
        request,
      ),
      FlarkEditorSemanticCommand.deleteBackward => _planDeleteBackward(request),
      FlarkEditorSemanticCommand.deleteForward => _planDeleteForward(request),
    };
  }

  FlarkEditorSemanticCommandAdmission? _planParagraphBreak(
    FlarkEditorSemanticCommandPlanningRequest request,
  ) {
    final projector = request.projector;
    final globalCaret =
        projector.inputGlobalUtf16Start + request.localCaretUtf16;
    final dependency = projector.pendingPresentation.dependency;
    if (dependency?.authority.continueWith(
          startUtf16: globalCaret,
          endUtf16: globalCaret,
          replacement: '\n',
        ) !=
        null) {
      // The parser supplied a stronger exact result for this literal newline.
      return null;
    }

    final row = request.row;
    if (row?.semanticCapabilities.insertParagraphBreakAsLiteral ?? false) {
      return null;
    }
    final neutralCaret = (projector.activeOrdinal ?? 0) < 0;
    final parserOwnedEmbeddedLineStart =
        row != null &&
        row.semanticCapabilities.insertParagraphBreakAtPhysicalLineStart &&
        _isPhysicalLineStartInsideRow(projector, row, globalCaret);
    final rowEligible =
        row != null &&
        (row.semanticCapabilities.insertParagraphBreak ||
            parserOwnedEmbeddedLineStart);
    if (row != null && projector.semanticViewportCurrent && !rowEligible) {
      return null;
    }
    if (!rowEligible && !request.semanticEditActive && !neutralCaret) {
      return null;
    }

    if (rowEligible) {
      final editableRange = row.editableUtf16;
      final editable = editableRange == null
          ? null
          : projector.optimisticRanges.mapRange(editableRange);
      final listItem = row.listItem;
      final listPrefix = listItem?.prefixUtf16;
      final atListMarkerEnd =
          listPrefix != null &&
          listItem != null &&
          globalCaret ==
              projector.optimisticRanges.mapRange(listPrefix).start +
                  listItem.markerOffset +
                  listItem.markerText.length;
      if (editable != null &&
          (globalCaret < editable.start || globalCaret > editable.end) &&
          projector.semanticViewportCurrent &&
          !parserOwnedEmbeddedLineStart &&
          !atListMarkerEnd) {
        return null;
      }
    }
    return const FlarkEditorSemanticCommandAdmission(
      FlarkCoreEditIntentV1.insertParagraphBreak,
    );
  }

  FlarkEditorSemanticCommandAdmission? _planDeleteBackward(
    FlarkEditorSemanticCommandPlanningRequest request,
  ) {
    final projector = request.projector;
    final row = request.row;
    final neutralLineStart =
        (projector.activeOrdinal ?? 0) < 0 && request.localCaretUtf16 == 0;
    final retainedSemanticWindowStart =
        request.localCaretUtf16 == 0 && request.semanticEditActive;
    final retainedNeutralSemanticCaret =
        (projector.activeOrdinal ?? 0) < 0 && request.semanticEditActive;
    final globalCaret =
        projector.inputGlobalUtf16Start + request.localCaretUtf16;
    final atInlineSemanticBoundary =
        row != null &&
        _isParserOwnedInlineBoundary(
          projector,
          row,
          globalCaret,
          backward: true,
        );
    final projectedStructuralRow =
        row?.semanticCapabilities.deleteBackwardAtProjectionStart ?? false;
    final rowEligible =
        row != null &&
        (row.semanticCapabilities.deleteBackwardAtEditableStart ||
            projectedStructuralRow ||
            row.semanticCapabilities.deleteBackwardAtPhysicalLineStart ||
            atInlineSemanticBoundary);
    if (row != null &&
        projector.semanticViewportCurrent &&
        !rowEligible &&
        !retainedSemanticWindowStart) {
      return null;
    }
    if (!rowEligible &&
        !retainedNeutralSemanticCaret &&
        !neutralLineStart &&
        !retainedSemanticWindowStart) {
      return null;
    }

    if (rowEligible) {
      final editableRange = row.editableUtf16;
      if (editableRange == null) return null;
      final editable = projector.optimisticRanges.mapRange(editableRange);
      final fencedPhysicalLineStart =
          row.semanticCapabilities.deleteBackwardAtPhysicalLineStart &&
          _isPhysicalLineStartInsideRow(projector, row, globalCaret);
      final projectionSegments = row.projectionSegments;
      final atStructuralSegmentStart =
          fencedPhysicalLineStart ||
          (projectedStructuralRow &&
              projectionSegments != null &&
              projectionSegments.any(
                (segment) =>
                    projector.optimisticRanges
                        .mapRange(segment.sourceUtf16)
                        .start ==
                    globalCaret,
              )) ||
          (!projector.semanticViewportCurrent &&
              _isPendingStructuralRunGap(projector, globalCaret));
      if (!atStructuralSegmentStart &&
          !atInlineSemanticBoundary &&
          globalCaret != editable.start &&
          (projector.semanticViewportCurrent || request.localCaretUtf16 != 0) &&
          !retainedSemanticWindowStart) {
        return null;
      }
    }
    return const FlarkEditorSemanticCommandAdmission(
      FlarkCoreEditIntentV1.deleteBackward,
    );
  }

  FlarkEditorSemanticCommandAdmission? _planDeleteForward(
    FlarkEditorSemanticCommandPlanningRequest request,
  ) {
    final projector = request.projector;
    final row = request.row;
    final editableRange = row?.editableUtf16;
    if (row == null || editableRange == null) return null;
    final editable = projector.optimisticRanges.mapRange(editableRange);
    final globalCaret =
        projector.inputGlobalUtf16Start + request.localCaretUtf16;
    final atInlineSemanticBoundary = _isParserOwnedInlineBoundary(
      projector,
      row,
      globalCaret,
      backward: false,
    );
    final parserOwnedForwardStart =
        row.semanticCapabilities.deleteForwardAtEditableStart &&
        globalCaret == editable.start &&
        projector.rowSemanticsCurrent(editable);
    if (!parserOwnedForwardStart && !atInlineSemanticBoundary) return null;
    return const FlarkEditorSemanticCommandAdmission(
      FlarkCoreEditIntentV1.deleteForward,
    );
  }

  bool _isParserOwnedInlineBoundary(
    FlarkSurfaceProjector projector,
    FlarkViewportRow row,
    int globalCaret, {
    required bool backward,
  }) {
    final visibleEnd =
        projector.visibleUtf16Start + projector.visibleSource.length;
    for (final fact in row.inlineFacts ?? const <FlarkInlineFact>[]) {
      if (!fact.supportsEmptyOwnerDelete) continue;
      final content = projector.optimisticRanges.mapRange(fact.contentUtf16);
      final atBoundary = backward
          ? content.end == globalCaret
          : content.start == globalCaret;
      if (!atBoundary ||
          content.start < projector.visibleUtf16Start ||
          content.end > visibleEnd) {
        continue;
      }
      return true;
    }
    return false;
  }

  bool _isPhysicalLineStartInsideRow(
    FlarkSurfaceProjector projector,
    FlarkViewportRow row,
    int globalCaret,
  ) {
    final source = projector.mappedExactRowRange(row);
    if (globalCaret <= source.start || globalCaret >= source.end) return false;
    final previousLocal = globalCaret - projector.visibleUtf16Start - 1;
    if (previousLocal < 0 || previousLocal >= projector.visibleSource.length) {
      return false;
    }
    final previous = projector.visibleSource.codeUnitAt(previousLocal);
    return previous == 0x0a || previous == 0x0d;
  }

  bool _isPendingStructuralRunGap(
    FlarkSurfaceProjector projector,
    int globalCaret,
  ) {
    for (final state in projector.pendingPresentation.structuralSurfaces) {
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
    }
    return false;
  }
}
