import 'dart:async';

import 'document.dart';
import 'pending_presentation.dart';
import 'presentation.dart';

enum FlarkEditorStatus {
  opening,

  /// A streamed open is still admitting source. The editor remains live while
  /// its parser-certified head is exposed and editable.
  streaming,
  parsing,
  ready,
  editing,
  faulted,
  disposed,
}

/// The editor behavior admitted through one coordinated command lifetime.
///
/// This is deliberately a closed editor-specific set, not an extensible event
/// or command bus. It lets the coordinator enforce the few lifetimes whose
/// overlap matters to source publication and history correctness.
enum FlarkEditorCommandKind {
  sourceEdit,
  semanticEdit,
  semanticAction,
  historyReplay,
  compositionCancel,
}

/// Identity for one admitted editor command.
///
/// A ticket can be completed exactly once and only by the coordinator that
/// issued it. The generation is the public receipt lineage used to reject a
/// stale asynchronous result.
final class FlarkEditorCommandTicket {
  FlarkEditorCommandTicket._({
    required FlarkEditorCoordinator owner,
    required this.kind,
    required this.generation,
  }) : _owner = owner;

  final FlarkEditorCoordinator _owner;
  final FlarkEditorCommandKind kind;
  final int generation;
  bool _settled = false;
}

/// Identity attached to asynchronous editor effects.
///
/// Results may be adopted only while their edit generation remains current.
/// Interaction generation is carried separately because selection/navigation
/// can invalidate user intent without advancing the canonical source.
final class FlarkEditorStamp {
  const FlarkEditorStamp({
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

  final FlarkEditorStamp stamp;
}

/// Sole owner of host-neutral editor coordination and publication lineage.
///
/// Markdown and canonical source remain in the native runtime. This object
/// owns the temporal identity, command serialization, pending-presentation
/// state, and bounded-work admission shared by every frontend.
final class FlarkEditorCoordinator {
  FlarkEditorStatus _status = FlarkEditorStatus.opening;
  Object? _lastError;
  bool _closed = false;
  int _editGeneration = 0;
  int _interactionGeneration = 0;
  int _publishedSourceGeneration = 0;
  int _publishedDocumentRevision = 0;
  FlarkPublicationPhase _publicationPhase = const FlarkPublicationIdle();
  int _snapshotSequence = 0;
  FlarkPendingPresentationSnapshot _pendingPresentation =
      const FlarkPendingPresentationSnapshot.empty();
  int _openingPublishedRevision = -1;
  final Set<FlarkEditorCommandTicket> _activeCommands = {};
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
  FlarkPendingPresentationSnapshot get pendingPresentation =>
      _pendingPresentation;
  int get openingPublishedRevision => _openingPublishedRevision;
  int get pendingEdits => _activeCommands.length;
  bool get historyReplayPending => _activeCommands.any(
    (command) =>
        command.kind == FlarkEditorCommandKind.historyReplay ||
        command.kind == FlarkEditorCommandKind.compositionCancel,
  );
  Future<void> get editTail => _editTail;
  Future<void> get sourceEditAdoptionTail => _sourceEditAdoptionTail;
  int get pendingSessionOnlyCommands => _pendingSessionOnlyCommands;
  Future<void>? get parserTask => _parserTask;
  Future<bool>? get pageTask => _pageTask;

  FlarkEditorStamp get stamp => FlarkEditorStamp(
    editGeneration: _editGeneration,
    interactionGeneration: _interactionGeneration,
  );

  bool accepts(
    FlarkEditorStamp stamp, {
    bool requireInteraction = false,
    bool allowClosing = false,
  }) =>
      (allowClosing || !_closed) &&
      stamp.editGeneration == _editGeneration &&
      (!requireInteraction ||
          stamp.interactionGeneration == _interactionGeneration);

  FlarkEditorCommandTicket admitCommand(
    FlarkEditorCommandKind kind, {
    bool publishSourceImmediately = false,
  }) {
    if (_closed) throw StateError('A closed editor cannot admit a command');
    if (historyReplayPending) {
      throw StateError('History replay is already pending');
    }
    if (publishSourceImmediately && kind != FlarkEditorCommandKind.sourceEdit) {
      throw ArgumentError(
        'Only a source edit can publish at command admission',
      );
    }
    _interactionGeneration += 1;
    final generation = ++_editGeneration;
    _status = FlarkEditorStatus.editing;
    if (publishSourceImmediately) {
      _publishedSourceGeneration = generation;
    }
    final ticket = FlarkEditorCommandTicket._(
      owner: this,
      kind: kind,
      generation: generation,
    );
    _activeCommands.add(ticket);
    return ticket;
  }

  bool publishCommandSource(FlarkEditorCommandTicket ticket) {
    _requireActiveCommand(ticket);
    if (ticket.generation != _editGeneration) return false;
    _publishedSourceGeneration = ticket.generation;
    return true;
  }

  void completeCommand(FlarkEditorCommandTicket ticket) {
    _requireActiveCommand(ticket);
    ticket._settled = true;
    _activeCommands.remove(ticket);
  }

  void failCommand(FlarkEditorCommandTicket ticket, Object error) {
    completeCommand(ticket);
    recordFault(error);
  }

  void _requireActiveCommand(FlarkEditorCommandTicket ticket) {
    if (!identical(ticket._owner, this)) {
      throw StateError('Editor command belongs to another coordinator');
    }
    if (ticket._settled || !_activeCommands.contains(ticket)) {
      throw StateError('Editor command is already complete');
    }
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
    if (_activeCommands.isNotEmpty) {
      throw StateError('Cannot dispose an editor with active commands');
    }
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

  int nextSnapshotSequence() => ++_snapshotSequence;

  void retirePendingPresentation(Set<FlarkPendingPresentationPart> parts) {
    _pendingPresentation = _pendingPresentation.retire(parts);
  }

  void setPendingDependency(FlarkPendingDependencyPresentation? dependency) {
    _pendingPresentation = _pendingPresentation.withDependency(dependency);
  }

  void setPendingCaretBoundary(FlarkPendingCaretBoundary? boundary) {
    _pendingPresentation = _pendingPresentation.withCaretBoundary(boundary);
  }

  void setPendingStructuralSurfaces(
    List<FlarkPendingStructuralSurface> surfaces,
  ) {
    _pendingPresentation = _pendingPresentation.withStructuralSurfaces(
      surfaces,
    );
  }

  void setPendingTaskCheck(int rowOrdinal, bool checked) {
    _pendingPresentation = _pendingPresentation.withTaskCheck(
      rowOrdinal,
      checked,
    );
  }

  FlarkPendingPresentationAdoption? adoptCommittedPresentation({
    required FlarkEditorCommandTicket command,
    required FlarkCoreEditIntentReceiptV1 receipt,
    required FlarkCoreCommittedPresentationTransitionV1? transition,
  }) {
    _requireActiveCommand(command);
    if (command.kind != FlarkEditorCommandKind.semanticEdit) {
      throw StateError('Only a semantic edit can adopt committed presentation');
    }
    if (command.generation != _editGeneration) return null;
    final adoption = _pendingPresentation.adoptCommittedTransition(
      receipt: receipt,
      transition: transition,
    );
    _pendingPresentation = adoption.snapshot;
    return adoption;
  }
}
