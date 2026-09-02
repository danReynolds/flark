import 'editor_coordinator.dart';
import 'editor_text.dart';
import 'models.dart';
import 'surface_projection.dart';

/// One parser row and every render-facing fact captured for the same editor
/// snapshot. Layout never asks mutable coordinator state to reconstruct row
/// ownership after this object is created.
final class FlarkEditorSnapshotRow {
  FlarkEditorSnapshotRow({
    required this.row,
    required this.sourceUtf16,
    required List<FlarkSurfaceRow> editingPresentations,
    required List<FlarkSurfaceRow> viewPresentations,
    required this.taskToggleable,
  }) : editingPresentations = List.unmodifiable(editingPresentations),
       viewPresentations = List.unmodifiable(viewPresentations);

  final FlarkViewportRow row;
  final FlarkSourceRange sourceUtf16;
  final List<FlarkSurfaceRow> editingPresentations;
  final List<FlarkSurfaceRow> viewPresentations;
  final bool taskToggleable;

  List<FlarkSurfaceRow> presentations({required bool includeEditingState}) =>
      includeEditingState ? editingPresentations : viewPresentations;
}

/// Immutable bounded editor state sealed at one outward notification.
///
/// Layout, paint, hit testing, and semantics retain this exact value until a
/// newer snapshot is published. It is a host-facing projection, not a second
/// document model: canonical source and Markdown meaning remain owned by the
/// native runtime.
final class FlarkEditorSnapshot {
  FlarkEditorSnapshot({
    required this.sequence,
    required this.status,
    required this.lastError,
    required this.interactionGeneration,
    required this.revision,
    required this.sourceGeneration,
    required this.sourceByteLength,
    required this.sourceUtf16Length,
    required this.pendingEdits,
    required this.canUndo,
    required this.canRedo,
    required this.semanticsCurrent,
    required this.viewportPageIndex,
    required this.canPageForward,
    required this.canPageBackward,
    required this.pendingTableNavigationLocked,
    required this.visibleUtf16Start,
    required this.visibleSource,
    required this.canonicalSelectionBaseUtf16,
    required this.canonicalSelectionExtentUtf16,
    required this.inputGlobalUtf16Start,
    required this.inputValue,
    required this.activeOrdinal,
    required this.crossRowSelection,
    required List<FlarkEditorSnapshotRow> rows,
  }) : rows = List.unmodifiable(rows);

  final int sequence;
  final FlarkEditorStatus status;
  final Object? lastError;
  final int interactionGeneration;
  final int revision;
  final int sourceGeneration;
  final int sourceByteLength;
  final int sourceUtf16Length;
  final int pendingEdits;
  final bool canUndo;
  final bool canRedo;
  final bool semanticsCurrent;
  final int viewportPageIndex;
  final bool canPageForward;
  final bool canPageBackward;
  final bool pendingTableNavigationLocked;
  final int visibleUtf16Start;
  final String visibleSource;
  final int canonicalSelectionBaseUtf16;
  final int canonicalSelectionExtentUtf16;
  final int inputGlobalUtf16Start;
  final FlarkEditorInputValue inputValue;
  final int? activeOrdinal;
  final bool crossRowSelection;
  final List<FlarkEditorSnapshotRow> rows;

  int get canonicalCaretUtf16 => canonicalSelectionExtentUtf16;

  FlarkSurfaceRow neutralSurfaceRow({
    required int globalUtf16Start,
    required String text,
    required int ordinal,
    bool includeEditingState = true,
  }) => FlarkSurfaceProjection.neutralRow(
    visibleUtf16Start: visibleUtf16Start,
    visibleSource: visibleSource,
    inputGlobalUtf16Start: inputGlobalUtf16Start,
    inputValue: inputValue,
    activeOrdinal: activeOrdinal,
    canonicalSelectionBaseUtf16: canonicalSelectionBaseUtf16,
    canonicalSelectionExtentUtf16: canonicalSelectionExtentUtf16,
    crossRowSelection: crossRowSelection,
    globalUtf16Start: globalUtf16Start,
    text: text,
    ordinal: ordinal,
    includeEditingState: includeEditingState,
  );
}
