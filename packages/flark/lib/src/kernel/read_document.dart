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

/// Per-instance read-only projection. It owns no history, editing commands,
/// composition state or code indentation service. The caller owns the backend.
final class FlarkReadDocument implements FlarkDocumentState {
  FlarkReadDocument(
    this._backend,
    String markdown, {
    this.syncLimit = FlarkEditor.defaultSyncLimit,
    this.liveLimits = const FlarkLiveLimits(),
  }) {
    update(markdown);
  }
  final FlarkParseBackend _backend;

  /// The UTF-8 size and shape rendered live; beyond either, source mode.
  final int syncLimit;
  final FlarkLiveLimits liveLimits;
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
  FlarkDocument get document => switch (snapshot) {
    FlarkLiveSnapshot(:final document) => document,
    FlarkSourceSnapshot() => throw StateError(
      'FlarkReadDocument has no parsed document in source mode',
    ),
  };
  @override
  Projection get projection => document.projection;
  @override
  CodeEditingDelegate? get codeEditing => null;

  /// Show [markdown]. Text outside the editing contract (a bare CR or an
  /// unpaired surrogate) and content the parser refuses are still this
  /// document, so they show as source rather than fail: a read-only view
  /// has no edit to reject, and the next text the parser takes renders live
  /// again. The editor refuses such text instead, because it must keep a
  /// parsable source.
  bool update(String markdown) {
    if (_snapshot?.source == markdown) return false;
    final current = _snapshot;
    FlarkEditorSnapshot next;
    try {
      validateFlarkSource(markdown);
      next = _projectSnapshot(
        _backend,
        markdown,
        const FlarkSelection.collapsed(0),
        const ProjectionOptions(editableDelimiterRows: false),
        liveLimits,
        syncLimit,
        previous: current is FlarkLiveSnapshot ? current.document : null,
      );
    } on FormatException {
      next = FlarkSourceSnapshot._(markdown, const FlarkSelection.collapsed(0));
    } on FlarkParseException {
      next = FlarkSourceSnapshot._(markdown, const FlarkSelection.collapsed(0));
    }
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
