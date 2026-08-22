import 'models.dart';
import 'presentation.dart';
import 'projection_continuity.dart';

/// Independently typed parts of one pending-presentation publication.
enum FlarkPendingPresentationPart {
  dependency,
  paragraphGap,
  structuralSurfaces,
  taskChecks,
}

/// One pre-edit dependency proof paired with its exact result presentation.
///
/// [presentation] is framework-neutral. Frontends may add selection and paint
/// geometry, but the source mapping, styles, block shell, and affected-island
/// consequence stay bound to [authority] and [rowOrdinal].
final class FlarkPendingDependencyPresentation {
  const FlarkPendingDependencyPresentation({
    required this.rowOrdinal,
    required this.authority,
    required this.presentation,
  });

  final int rowOrdinal;
  final FlarkPendingDependencyAuthority authority;
  final FlarkCorePresentationRow presentation;

  int get resultRevision => authority.resultRevision;
  FlarkSourceRange get affectedUtf16 => authority.affectedUtf16;
  bool get presentsExactIsland => authority.presentsExactIsland;
}

/// One committed structural surface and any parser-authored successor proof
/// carried by that surface.
final class FlarkPendingStructuralSurface {
  const FlarkPendingStructuralSurface({required this.surface, this.continuity});

  final FlarkCoreCommittedPresentationSurfaceV1 surface;
  final FlarkProjectionEditCellReceipt? continuity;
}

/// The only host-visible pending-presentation authority state.
///
/// Pre-edit dependency proofs and committed structural receipts remain typed
/// inputs with different admission rules. Once admitted, their current
/// presentation, paragraph gap, and semantic-action overlays share this one
/// immutable state and therefore one retirement/supersession lifecycle.
final class FlarkPendingPresentationSnapshot {
  FlarkPendingPresentationSnapshot({
    this.dependency,
    this.paragraphGap,
    List<FlarkPendingStructuralSurface> structuralSurfaces = const [],
    Map<int, bool> taskChecks = const {},
  }) : structuralSurfaces = List.unmodifiable(structuralSurfaces),
       taskChecks = Map.unmodifiable(taskChecks);

  const FlarkPendingPresentationSnapshot.empty()
    : dependency = null,
      paragraphGap = null,
      structuralSurfaces = const [],
      taskChecks = const {};

  final FlarkPendingDependencyPresentation? dependency;
  final FlarkCoreCommittedPresentationGapV1? paragraphGap;
  final List<FlarkPendingStructuralSurface> structuralSurfaces;
  final Map<int, bool> taskChecks;

  bool get isEmpty =>
      dependency == null &&
      paragraphGap == null &&
      structuralSurfaces.isEmpty &&
      taskChecks.isEmpty;

  bool get hasPresentationAuthority =>
      dependency != null ||
      structuralSurfaces.isNotEmpty ||
      taskChecks.isNotEmpty;

  FlarkPendingPresentationSnapshot withDependency(
    FlarkPendingDependencyPresentation? value,
  ) => FlarkPendingPresentationSnapshot(
    dependency: value,
    paragraphGap: paragraphGap,
    structuralSurfaces: structuralSurfaces,
    taskChecks: taskChecks,
  );

  FlarkPendingPresentationSnapshot withParagraphGap(
    FlarkCoreCommittedPresentationGapV1? value,
  ) => FlarkPendingPresentationSnapshot(
    dependency: dependency,
    paragraphGap: value,
    structuralSurfaces: structuralSurfaces,
    taskChecks: taskChecks,
  );

  FlarkPendingPresentationSnapshot withStructuralSurfaces(
    List<FlarkPendingStructuralSurface> value,
  ) => FlarkPendingPresentationSnapshot(
    dependency: dependency,
    paragraphGap: paragraphGap,
    structuralSurfaces: value,
    taskChecks: taskChecks,
  );

  FlarkPendingPresentationSnapshot withTaskCheck(
    int rowOrdinal,
    bool checked,
  ) => FlarkPendingPresentationSnapshot(
    dependency: dependency,
    paragraphGap: paragraphGap,
    structuralSurfaces: structuralSurfaces,
    taskChecks: {...taskChecks, rowOrdinal: checked},
  );

  /// Retires selected authority through the single snapshot lifecycle.
  FlarkPendingPresentationSnapshot retire(
    Set<FlarkPendingPresentationPart> parts,
  ) {
    if (parts.isEmpty) return this;
    return FlarkPendingPresentationSnapshot(
      dependency: parts.contains(FlarkPendingPresentationPart.dependency)
          ? null
          : dependency,
      paragraphGap: parts.contains(FlarkPendingPresentationPart.paragraphGap)
          ? null
          : paragraphGap,
      structuralSurfaces:
          parts.contains(FlarkPendingPresentationPart.structuralSurfaces)
          ? const []
          : structuralSurfaces,
      taskChecks: parts.contains(FlarkPendingPresentationPart.taskChecks)
          ? const {}
          : taskChecks,
    );
  }

  FlarkPendingPresentationSnapshot clear() =>
      retire(FlarkPendingPresentationPart.values.toSet());
}
