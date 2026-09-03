import 'editor_text.dart';
import 'models.dart';
import 'optimistic_range_map.dart';
import 'pending_presentation.dart';
import 'surface_projector.dart';
import 'viewport_installation.dart';

enum FlarkOptimisticViewportEditDisposition {
  retainedMappedSurface,
  replacedByInputWindow,
  replacedByBoundedWindow,
}

/// Result of applying one optimistic source splice to the bounded viewport.
///
/// The disposition tells the runtime whether page navigation must also be
/// reset. It carries no callback and cannot mutate host input state.
final class FlarkOptimisticViewportEditAdoption {
  const FlarkOptimisticViewportEditAdoption(this.disposition);

  final FlarkOptimisticViewportEditDisposition disposition;
}

/// Sole owner of the bounded source and presentation currently available to
/// an editor host.
///
/// Native querying, page navigation, input restoration, and publication stay
/// outside this state machine. Its job is narrower: viewport, rows, source,
/// certification, and optimistic coordinate mapping are adopted together so
/// a host cannot observe a torn combination of those values.
final class FlarkEditorViewportState {
  FlarkViewport? _viewport;
  List<FlarkViewportRow> _rows = const [];
  List<FlarkCertificationRange> _certificationRanges = const [];
  String _visibleSource = '';
  int _visibleUtf16Start = 0;
  bool _semanticCurrent = false;
  bool _certificationRevisionCurrent = false;
  final FlarkOptimisticRangeMap _optimisticEdits = FlarkOptimisticRangeMap();

  FlarkViewport? get viewport => _viewport;
  List<FlarkViewportRow> get rows => _rows;
  List<FlarkCertificationRange> get certificationRanges => _certificationRanges;
  String get visibleSource => _visibleSource;
  int get visibleUtf16Start => _visibleUtf16Start;
  int get visibleUtf16End => _visibleUtf16Start + _visibleSource.length;
  bool get semanticCurrent => _semanticCurrent;
  bool get certificationRevisionCurrent => _certificationRevisionCurrent;
  bool get hasOptimisticEdits => _optimisticEdits.isNotEmpty;
  bool get allOptimisticEditsPreserveMappedRowFacts =>
      _optimisticEdits.every((edit) => edit.preservesMappedRowFacts);

  bool allOptimisticEditsStartAtOrAfter(int utf16Offset) =>
      _optimisticEdits.every((edit) => edit.start >= utf16Offset);

  void clearOptimisticEdits() => _optimisticEdits.clear();

  FlarkSourceRange mapRange(FlarkSourceRange base) =>
      _optimisticEdits.mapRange(base);

  /// Captures a deterministic projector from this state's owned source,
  /// certification, and optimistic mapping plus caller-owned input facts.
  FlarkSurfaceProjector captureSurfaceProjector({
    required FlarkPendingPresentationSnapshot pendingPresentation,
    required int inputGlobalUtf16Start,
    required FlarkEditorInputValue inputValue,
    required int? activeOrdinal,
    required int selectionBaseUtf16,
    required int selectionExtentUtf16,
    required bool crossRowSelection,
  }) => FlarkSurfaceProjector(
    pendingPresentation: pendingPresentation,
    visibleUtf16Start: _visibleUtf16Start,
    visibleSource: _visibleSource,
    inputGlobalUtf16Start: inputGlobalUtf16Start,
    inputValue: inputValue,
    activeOrdinal: activeOrdinal,
    selectionBaseUtf16: selectionBaseUtf16,
    selectionExtentUtf16: selectionExtentUtf16,
    crossRowSelection: crossRowSelection,
    semanticViewportCurrent: _semanticCurrent,
    certificationRevisionCurrent: _certificationRevisionCurrent,
    certificationRanges: _certificationRanges,
    optimisticRanges: _optimisticEdits,
  );

  /// Atomically adopts one queried viewport and its exact bounded source.
  FlarkViewportInstallationPlan install(FlarkViewport viewport, String source) {
    final installation = FlarkViewportInstallationPlan.evaluate(
      viewport: viewport,
      source: source,
      previousVisibleUtf16Start: _visibleUtf16Start,
      previousVisibleSource: _visibleSource,
      mappedCachedRowRanges: _rows.map(
        (row) => _optimisticEdits.mapRange(row.sourceUtf16),
      ),
    );
    _viewport = viewport;
    if (installation.installsFreshRows) {
      _rows = List.unmodifiable(viewport.rows);
    } else if (!installation.retainsExistingSurface) {
      _rows = const [];
    }
    if (!installation.retainsExistingSurface) {
      _visibleSource = source;
      _visibleUtf16Start = viewport.coveredUtf16.start;
    }
    _certificationRanges = List.unmodifiable(viewport.certificationRanges);
    _certificationRevisionCurrent = viewport.certificationRanges.isNotEmpty;
    _semanticCurrent = installation.installsCertifiedSurface;
    if (installation.installsCertifiedSurface) _optimisticEdits.clear();
    return installation;
  }

  /// Installs an exact host-visible source window with no parser presentation.
  ///
  /// The prior native viewport may remain as a query anchor, but its rows and
  /// certification can no longer describe the new window.
  void adoptUncertifiedSourceWindow({
    required String source,
    required int startUtf16,
    bool clearViewport = false,
  }) {
    if (startUtf16 < 0) {
      throw ArgumentError.value(
        startUtf16,
        'startUtf16',
        'must be nonnegative',
      );
    }
    if (clearViewport) _viewport = null;
    _rows = const [];
    _certificationRanges = const [];
    _visibleSource = source;
    _visibleUtf16Start = startUtf16;
    _semanticCurrent = false;
    _certificationRevisionCurrent = false;
    _optimisticEdits.clear();
  }

  void removeRows(Set<int> ordinals) {
    if (ordinals.isEmpty) return;
    _rows = List.unmodifiable(
      _rows.where((row) => !ordinals.contains(row.ordinal)),
    );
  }

  /// Applies an optimistic splice without letting source, rows, and mapping
  /// advance independently.
  FlarkOptimisticViewportEditAdoption applyOptimisticEdit({
    required int globalStart,
    required int globalEnd,
    required String replacement,
    required String fallbackSource,
    required int fallbackUtf16Start,
    required int focusUtf16,
    required int maximumVisibleCodeUnits,
    bool preservesMappedRowFacts = true,
  }) {
    if (maximumVisibleCodeUnits <= 0) {
      throw ArgumentError.value(
        maximumVisibleCodeUnits,
        'maximumVisibleCodeUnits',
        'must be positive',
      );
    }
    _semanticCurrent = false;
    _certificationRevisionCurrent = false;
    _certificationRanges = const [];
    final localStart = globalStart - _visibleUtf16Start;
    final localEnd = globalEnd - _visibleUtf16Start;
    if (localStart < 0 ||
        localEnd < localStart ||
        localEnd > _visibleSource.length) {
      adoptUncertifiedSourceWindow(
        source: fallbackSource,
        startUtf16: fallbackUtf16Start,
        clearViewport: true,
      );
      return const FlarkOptimisticViewportEditAdoption(
        FlarkOptimisticViewportEditDisposition.replacedByInputWindow,
      );
    }
    final nextLength = replacementResultLength(
      source: _visibleSource,
      start: localStart,
      end: localEnd,
      replacement: replacement,
    );
    if (nextLength > maximumVisibleCodeUnits) {
      final window = boundedReplacementWindow(
        source: _visibleSource,
        start: localStart,
        end: localEnd,
        replacement: replacement,
        focus: focusUtf16 - _visibleUtf16Start,
        maximumCodeUnits: maximumVisibleCodeUnits,
      );
      final nextStart = _visibleUtf16Start + window.start;
      adoptUncertifiedSourceWindow(
        source: window.text,
        startUtf16: nextStart,
        clearViewport: true,
      );
      return const FlarkOptimisticViewportEditAdoption(
        FlarkOptimisticViewportEditDisposition.replacedByBoundedWindow,
      );
    }
    _visibleSource = _visibleSource.replaceRange(
      localStart,
      localEnd,
      replacement,
    );
    _optimisticEdits.add(
      FlarkOptimisticViewportEdit(
        start: globalStart,
        end: globalEnd,
        replacementLength: replacement.length,
        preservesMappedRowFacts: preservesMappedRowFacts,
      ),
    );
    return const FlarkOptimisticViewportEditAdoption(
      FlarkOptimisticViewportEditDisposition.retainedMappedSurface,
    );
  }

  bool applyLengthNeutralReplacement({
    required int globalStart,
    required int globalEnd,
    required String replacement,
  }) {
    if (globalEnd < globalStart ||
        replacement.length != globalEnd - globalStart) {
      return false;
    }
    final localStart = globalStart - _visibleUtf16Start;
    final localEnd = globalEnd - _visibleUtf16Start;
    if (localStart < 0 ||
        localEnd < localStart ||
        localEnd > _visibleSource.length) {
      return false;
    }
    _semanticCurrent = false;
    _certificationRevisionCurrent = false;
    _certificationRanges = const [];
    _visibleSource = _visibleSource.replaceRange(
      localStart,
      localEnd,
      replacement,
    );
    return true;
  }

  String sliceVisibleUtf16(int globalStart, int globalEnd) {
    final start = (globalStart - _visibleUtf16Start).clamp(
      0,
      _visibleSource.length,
    );
    final end = (globalEnd - _visibleUtf16Start).clamp(
      start,
      _visibleSource.length,
    );
    return _visibleSource.substring(start, end);
  }
}
