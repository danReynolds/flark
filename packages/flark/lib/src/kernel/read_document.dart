part of 'editor.dart';

/// The shared rendering facts, without editing commands or history.
abstract interface class FlarkDocumentState {
  FlarkEditorSnapshot get snapshot;
  String get source;
  FlarkSelection get selection;
  int get revision;
  bool get sourceMode;
  FlarkDocument get document;
  Projection get projection;
  CodeEditingDelegate? get codeEditing;
}

FlarkEditorSnapshot _projectSnapshot(
  FlarkParseBackend backend,
  String text,
  FlarkSelection selected,
  ProjectionOptions options,
  FlarkLiveLimits liveLimits,
  int syncLimit, {
  bool forceSourceMode = false,
  bool rejectDeviation = false,
  RenderModel? parsed,
}) {
  if (forceSourceMode ||
      !_withinLiveByteLimit(text, syncLimit) ||
      !liveLimits._admitsSource(text)) {
    return FlarkSourceSnapshot._(text, selected);
  }
  try {
    final model = parsed ?? backend.parse(text);
    if (!liveLimits._admitsModel(model)) {
      return FlarkSourceSnapshot._(text, selected);
    }
    return FlarkLiveSnapshot._(
      projectFlarkDocument(text, model, selected, options),
    );
  } on FlarkParseException catch (error) {
    if (error.code != FlarkParseException.extractionDeviationCode ||
        rejectDeviation) {
      rethrow;
    }
    return FlarkSourceSnapshot._(text, selected);
  }
}

/// Per-instance read-only projection. It owns no history, editing commands,
/// composition state or code indentation service. The caller owns the backend.
final class FlarkReadDocument implements FlarkDocumentState {
  FlarkReadDocument(this._backend, String markdown) {
    update(markdown);
  }
  final FlarkParseBackend _backend;
  FlarkEditorSnapshot? _snapshot;
  int _revision = 0;
  @override
  FlarkEditorSnapshot get snapshot => _snapshot!;
  @override
  String get source => snapshot.source;
  @override
  FlarkSelection get selection => snapshot.selection;
  @override
  int get revision => _revision;
  @override
  bool get sourceMode => snapshot is FlarkSourceSnapshot;
  @override
  FlarkDocument get document => (snapshot as FlarkLiveSnapshot).document;
  @override
  Projection get projection => document.projection;
  @override
  CodeEditingDelegate? get codeEditing => null;

  bool update(String markdown) {
    if (_snapshot?.source == markdown) return false;
    validateFlarkSource(markdown);
    final next = _projectSnapshot(
      _backend,
      markdown,
      const FlarkSelection.collapsed(0),
      const ProjectionOptions(),
      const FlarkLiveLimits(),
      FlarkEditor.defaultSyncLimit,
    );
    _snapshot = next;
    _revision++;
    return true;
  }

  bool select(FlarkSelection selection) {
    final current = snapshot;
    final next = switch (current) {
      FlarkLiveSnapshot(:final document) => FlarkLiveSnapshot._(
        document.withSelection(selection),
      ),
      FlarkSourceSnapshot() => current._withSelection(selection),
    };
    if (next.selection == current.selection) return false;
    _snapshot = next;
    _revision++;
    return true;
  }
}
