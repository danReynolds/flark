import 'dart:async';

import 'surface_projection.dart';

enum FlarkEditorStatus {
  opening,

  /// A streamed open is still admitting source. The editor remains live while
  /// its parser-certified head is painted and editable.
  streaming,
  parsing,
  ready,
  editing,
  faulted,
  disposed,
}

/// Identity attached to asynchronous editor effects.
///
/// Results may be adopted only while their edit generation remains current.
/// Interaction generation is carried separately because selection/navigation
/// can invalidate user intent without advancing the canonical source.
final class FlarkRuntimeStamp {
  const FlarkRuntimeStamp({
    required this.editGeneration,
    required this.interactionGeneration,
  });

  final int editGeneration;
  final int interactionGeneration;
}

sealed class FlarkPublicationPhase {
  const FlarkPublicationPhase();
}

final class FlarkPublicationIdle extends FlarkPublicationPhase {
  const FlarkPublicationIdle();
}

final class FlarkPublicationAwaitingCertification
    extends FlarkPublicationPhase {
  const FlarkPublicationAwaitingCertification(this.stamp);

  final FlarkRuntimeStamp stamp;
}

/// Sole owner of controller-wide lifecycle and publication lineage.
///
/// Markdown semantics remain in Core. Platform input shadow state lives in
/// [FlarkPlatformInputBridge]. This object owns the temporal identity that
/// joins their asynchronous results into one publishable editor state.
final class FlarkEditorRuntimeState {
  FlarkEditorStatus _status = FlarkEditorStatus.opening;
  Object? _lastError;
  bool _closed = false;
  int _editGeneration = 0;
  int _interactionGeneration = 0;
  int _publishedSourceGeneration = 0;
  int _publishedDocumentRevision = 0;
  FlarkPublicationPhase _publicationPhase = const FlarkPublicationIdle();
  FlarkSurfacePublication? _surfacePublication;
  int _surfacePublicationSequence = 0;
  int _openingPublishedRevision = -1;
  int _pendingEdits = 0;
  bool _historyReplayPending = false;
  Future<void> _editTail = Future<void>.value();
  Future<void> _sourceEditAdoptionTail = Future<void>.value();
  int _pendingSessionOnlyCommands = 0;
  Future<void>? _parserTask;
  Future<bool>? _pageTask;

  FlarkEditorStatus get status => _status;
  Object? get lastError => _lastError;
  bool get closed => _closed;
  int get editGeneration => _editGeneration;
  int get interactionGeneration => _interactionGeneration;
  int get publishedSourceGeneration => _publishedSourceGeneration;
  int get publishedDocumentRevision => _publishedDocumentRevision;
  bool get publicationCertificationBarrierActive =>
      _publicationPhase is FlarkPublicationAwaitingCertification;
  FlarkPublicationPhase get publicationPhase => _publicationPhase;
  FlarkSurfacePublication? get surfacePublication => _surfacePublication;
  int get openingPublishedRevision => _openingPublishedRevision;
  int get pendingEdits => _pendingEdits;
  bool get historyReplayPending => _historyReplayPending;
  Future<void> get editTail => _editTail;
  Future<void> get sourceEditAdoptionTail => _sourceEditAdoptionTail;
  int get pendingSessionOnlyCommands => _pendingSessionOnlyCommands;
  Future<void>? get parserTask => _parserTask;
  Future<bool>? get pageTask => _pageTask;

  FlarkRuntimeStamp get stamp => FlarkRuntimeStamp(
    editGeneration: _editGeneration,
    interactionGeneration: _interactionGeneration,
  );

  bool accepts(
    FlarkRuntimeStamp stamp, {
    bool requireInteraction = false,
    bool allowClosing = false,
  }) =>
      (allowClosing || !_closed) &&
      stamp.editGeneration == _editGeneration &&
      (!requireInteraction ||
          stamp.interactionGeneration == _interactionGeneration);

  int admitEditingCommand() {
    _interactionGeneration += 1;
    return ++_editGeneration;
  }

  void beginPendingEdit() {
    if (_closed) throw StateError('A closed editor cannot admit an edit');
    _pendingEdits += 1;
  }

  void endPendingEdit() {
    if (_pendingEdits == 0) {
      throw StateError('Pending edit accounting underflow');
    }
    _pendingEdits -= 1;
  }

  void beginHistoryReplay() {
    if (_closed) throw StateError('A closed editor cannot replay history');
    if (_historyReplayPending) {
      throw StateError('History replay is already pending');
    }
    _historyReplayPending = true;
  }

  void endHistoryReplay() {
    if (!_historyReplayPending) {
      throw StateError('No history replay is pending');
    }
    _historyReplayPending = false;
  }

  void recordInteraction() {
    _interactionGeneration += 1;
  }

  void transitionStatus(FlarkEditorStatus status) {
    if (_status == FlarkEditorStatus.disposed &&
        status != FlarkEditorStatus.disposed) {
      throw StateError('A disposed editor cannot re-enter the runtime');
    }
    _status = status;
  }

  void recordFault(Object error) {
    _lastError = error;
    _status = FlarkEditorStatus.faulted;
  }

  void setLastError(Object? error) {
    _lastError = error;
  }

  void beginClosing() {
    _closed = true;
  }

  void markDisposed() {
    _closed = true;
    _status = FlarkEditorStatus.disposed;
  }

  void beginPublicationBarrier() {
    _publicationPhase = FlarkPublicationAwaitingCertification(stamp);
  }

  void endPublicationBarrier() {
    _publicationPhase = const FlarkPublicationIdle();
  }

  /// Ends only the barrier created by [editGeneration]. An older async
  /// completion can therefore never clear a newer edit's publication gate.
  bool endPublicationBarrierForEdit(int editGeneration) {
    final phase = _publicationPhase;
    if (phase is! FlarkPublicationAwaitingCertification ||
        phase.stamp.editGeneration != editGeneration ||
        editGeneration != _editGeneration ||
        _closed) {
      return false;
    }
    _publicationPhase = const FlarkPublicationIdle();
    return true;
  }

  void publishSourceGeneration(int generation) {
    if (generation > _editGeneration) {
      throw StateError('Cannot publish a future edit generation');
    }
    _publishedSourceGeneration = generation;
  }

  void installViewportRevision(int revision) {
    _publishedDocumentRevision = revision;
    _publishedSourceGeneration = _editGeneration;
  }

  void recordOpeningPublication(int revision) {
    _openingPublishedRevision = revision;
  }

  Future<T> afterEdits<T>(Future<T> Function() operation) =>
      _editTail.then((_) => operation());

  void trackEdit<T>(Future<T> operation) {
    _editTail = operation
        .then<void>((_) {})
        .catchError((Object _, StackTrace _) {});
  }

  Future<T> queueEdit<T>(Future<T> Function() operation) {
    final queued = afterEdits(operation);
    trackEdit(queued);
    return queued;
  }

  Future<T> queueSessionCommand<T>(Future<T> Function() operation) {
    _pendingSessionOnlyCommands += 1;
    final queued = afterEdits(() async {
      try {
        return await operation();
      } finally {
        _pendingSessionOnlyCommands -= 1;
      }
    });
    trackEdit(queued);
    return queued;
  }

  void trackSourceAdoption(Future<void> completion) {
    final prior = _sourceEditAdoptionTail;
    _sourceEditAdoptionTail = Future.wait([
      prior,
      completion,
    ]).then<void>((_) {}).catchError((Object _, StackTrace _) {});
  }

  Future<void> runParser(Future<void> Function() operation) {
    final active = _parserTask;
    if (active != null) return active;
    // The in-flight slot and joined Future must settle in the operation's
    // completion turn. An asynchronous completer can leave a finished parser
    // looking active across Flutter's runAsync boundary, so the next caller
    // joins a task whose delivery is stranded on the outer event loop.
    final completion = Completer<void>.sync();
    final started = completion.future;
    _parserTask = started;
    Future<void>.sync(operation).then<void>(
      (_) {
        if (identical(_parserTask, started)) _parserTask = null;
        completion.complete();
      },
      onError: (Object error, StackTrace stackTrace) {
        if (identical(_parserTask, started)) _parserTask = null;
        completion.completeError(error, stackTrace);
      },
    );
    return started;
  }

  Future<bool> runPage(Future<bool> Function() operation) {
    final active = _pageTask;
    if (active != null) return active;
    // Keep page admission and caller-visible settlement atomic for the same
    // reason as the parser single-flight above.
    final completion = Completer<bool>.sync();
    final started = completion.future;
    _pageTask = started;
    Future<bool>.sync(operation).then<void>(
      (result) {
        if (identical(_pageTask, started)) _pageTask = null;
        completion.complete(result);
      },
      onError: (Object error, StackTrace stackTrace) {
        if (identical(_pageTask, started)) _pageTask = null;
        completion.completeError(error, stackTrace);
      },
    );
    return started;
  }

  int nextSurfacePublicationSequence() => ++_surfacePublicationSequence;

  void installSurfacePublication(FlarkSurfacePublication publication) {
    if (publication.sequence != _surfacePublicationSequence) {
      throw StateError('Surface publication sequence was not reserved');
    }
    _surfacePublication = publication;
  }
}
