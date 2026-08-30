import 'editor_coordinator.dart';
import 'editor_viewport_pager.dart';
import 'editor_viewport_state.dart';
import 'models.dart';
import 'pending_presentation.dart';
import 'pending_presentation_evolution.dart';
import 'presentation.dart';
import 'viewport_installation.dart';

/// Portable result of atomically adopting one queried viewport publication.
final class FlarkEditorViewportAdoption {
  const FlarkEditorViewportAdoption({
    required this.viewport,
    required this.installation,
    required this.supersededParagraphGap,
    required this.hasFirstCertifiedEvidence,
  });

  final FlarkViewport viewport;
  final FlarkViewportInstallationPlan installation;

  /// Gap geometry retired by this certified publication. A host may use it
  /// once to rebuild its bounded platform input before ordinary row geometry
  /// takes over.
  final FlarkCoreCommittedPresentationGapV1? supersededParagraphGap;

  /// Whether this publication contains the first paintable certified row
  /// evidence. Timing and completion primitives remain host concerns.
  final bool hasFirstCertifiedEvidence;
}

/// Atomically advances portable viewport publication state.
///
/// The pager receipt, bounded source/rows, source publication generation,
/// navigation origin, and certified retirement of provisional presentation
/// all advance in this one synchronous transaction. The adopter owns no host
/// input value, notification, timer, or layout state.
final class FlarkEditorViewportAdopter {
  const FlarkEditorViewportAdopter({
    required FlarkEditorCoordinator coordinator,
    required FlarkEditorViewportPager pager,
    required FlarkEditorViewportState state,
  }) : _coordinator = coordinator,
       _pager = pager,
       _state = state;

  final FlarkEditorCoordinator _coordinator;
  final FlarkEditorViewportPager _pager;
  final FlarkEditorViewportState _state;

  FlarkEditorViewportAdoption? adopt(
    FlarkViewportPageResult result, {
    required int caretUtf16,
  }) {
    if (!_pager.adopt(result)) return null;
    final viewport = result.viewport;
    final installation = _state.install(viewport, result.source);
    if (!installation.retainsExistingSurface) {
      _coordinator.recordInteraction();
      if (viewport.revision != _coordinator.publishedDocumentRevision) {
        _coordinator.installViewportRevision(viewport.revision);
      }
    }
    _pager.observeInstallation(
      viewport: viewport,
      installation: installation,
      caretUtf16: caretUtf16,
    );
    if (installation.installsCertifiedSurface) {
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.taskChecks,
      });
    }
    if (certifiedViewportSupersedesPendingDependency(
      viewport: viewport,
      pendingPresentation: _coordinator.pendingPresentation,
    )) {
      _coordinator.retirePendingPresentation(const {
        FlarkPendingPresentationPart.dependency,
      });
    }

    final pending = _coordinator.pendingPresentation;
    final supersededParagraphGap = _state.semanticCurrent
        ? pending.paragraphGap
        : null;
    final supersededStructuralCaretBoundary = _state.semanticCurrent
        ? caretBoundaryForStructuralSurfaces(pending.structuralSurfaces)
        : null;
    if (_state.semanticCurrent) {
      if (supersededParagraphGap != null) {
        _coordinator.setPendingCaretBoundary(
          FlarkPendingCaretBoundary.fromGap(
            supersededParagraphGap,
            editAuthority:
                supersededStructuralCaretBoundary ?? pending.caretBoundary,
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

    final hasFirstCertifiedEvidence =
        installation.installsFreshRows &&
        (viewport.isCertified ||
            viewport.certificationRanges.any(
              (range) => range.isCertified && range.sourceBytes.length > 0,
            ));
    return FlarkEditorViewportAdoption(
      viewport: viewport,
      installation: installation,
      supersededParagraphGap: supersededParagraphGap,
      hasFirstCertifiedEvidence: hasFirstCertifiedEvidence,
    );
  }

  Future<void>? discard(FlarkViewportPageResult result) =>
      _pager.discard(result);
}
