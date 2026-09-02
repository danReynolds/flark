import 'dart:async';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  test('stale effects cannot cross edit or interaction generations', () {
    final coordinator = FlarkEditorCoordinator();
    final initial = coordinator.stamp;

    coordinator.recordInteraction();
    expect(coordinator.accepts(initial), isTrue);
    expect(coordinator.accepts(initial, requireInteraction: true), isFalse);

    coordinator.admitCommand(FlarkEditorCommandKind.sourceEdit);
    expect(coordinator.accepts(initial), isFalse);
  });

  test('an older completion cannot clear a newer publication barrier', () {
    final coordinator = FlarkEditorCoordinator();
    final first = coordinator.admitCommand(FlarkEditorCommandKind.sourceEdit);
    coordinator.beginPublicationBarrier();

    final second = coordinator.admitCommand(FlarkEditorCommandKind.sourceEdit);
    coordinator.beginPublicationBarrier();

    expect(coordinator.endPublicationBarrierForEdit(first.generation), isFalse);
    expect(coordinator.publicationCertificationBarrierActive, isTrue);
    expect(coordinator.endPublicationBarrierForEdit(second.generation), isTrue);
    expect(coordinator.publicationCertificationBarrierActive, isFalse);
  });

  test('a stale command cannot relabel the published source', () {
    final coordinator = FlarkEditorCoordinator();
    final first = coordinator.admitCommand(FlarkEditorCommandKind.semanticEdit);
    final second = coordinator.admitCommand(
      FlarkEditorCommandKind.sourceEdit,
      publishSourceImmediately: true,
    );

    expect(coordinator.publishCommandSource(first), isFalse);
    expect(coordinator.publishedSourceGeneration, second.generation);
    expect(
      () => coordinator.admitCommand(
        FlarkEditorCommandKind.semanticEdit,
        publishSourceImmediately: true,
      ),
      throwsArgumentError,
    );
  });

  test('a stale semantic receipt cannot replace pending presentation', () {
    final coordinator = FlarkEditorCoordinator();
    final boundary = FlarkPendingCaretBoundary(rowOrdinal: 2, rowEndUtf16: 4);
    coordinator.setPendingCaretBoundary(boundary);
    final semantic = coordinator.admitCommand(
      FlarkEditorCommandKind.semanticEdit,
    );
    coordinator.admitCommand(
      FlarkEditorCommandKind.sourceEdit,
      publishSourceImmediately: true,
    );

    final adoption = coordinator.adoptCommittedPresentation(
      command: semantic,
      receipt: _receipt(
        transition: FlarkCoreEditPresentationTransitionV1.mergeParagraph,
      ),
      transition: FlarkCoreCommittedPresentationTransitionV1(
        clearPriorGap: true,
      ),
    );

    expect(adoption, isNull);
    expect(coordinator.pendingPresentation.caretBoundary, same(boundary));
  });

  test('disposed is a terminal coordinator status', () {
    final coordinator = FlarkEditorCoordinator()..markDisposed();

    expect(
      () => coordinator.transitionStatus(FlarkEditorStatus.ready),
      throwsStateError,
    );
  });

  test('closing blocks new effects but can drain admitted page work', () {
    final coordinator = FlarkEditorCoordinator();
    final admitted = coordinator.stamp;

    coordinator.beginClosing();

    expect(coordinator.accepts(admitted), isFalse);
    expect(coordinator.accepts(admitted, allowClosing: true), isTrue);
  });

  test('edit operations have one serialized coordinator tail', () async {
    final coordinator = FlarkEditorCoordinator();
    final firstStarted = Completer<void>();
    final releaseFirst = Completer<void>();
    final events = <String>[];

    final first = coordinator.queueEdit(() async {
      events.add('first-start');
      firstStarted.complete();
      await releaseFirst.future;
      events.add('first-end');
    });
    final second = coordinator.queueEdit(() async {
      events.add('second');
    });

    await firstStarted.future;
    expect(events, ['first-start']);
    releaseFirst.complete();
    await Future.wait([first, second]);
    expect(events, ['first-start', 'first-end', 'second']);
  });

  test('parser work is single-flight', () async {
    final coordinator = FlarkEditorCoordinator();
    final started = Completer<void>();
    final release = Completer<void>();
    var starts = 0;

    Future<void> parse() async {
      starts += 1;
      started.complete();
      await release.future;
    }

    final first = coordinator.runParser(parse);
    await started.future;
    final joined = coordinator.runParser(() async {
      starts += 100;
    });
    expect(identical(first, joined), isTrue);
    release.complete();
    await joined;
    expect(starts, 1);
    expect(coordinator.parserTask, isNull);
  });

  test('completed page work reopens admission', () async {
    final coordinator = FlarkEditorCoordinator();
    var callerSettled = false;

    final page = coordinator.runPage(() async => true);
    final caller = page.whenComplete(() => callerSettled = true);
    await coordinator.pageTask;
    await caller;

    expect(callerSettled, isTrue);
    expect(coordinator.pageTask, isNull);
    expect(await coordinator.runPage(() async => false), isFalse);
  });

  test('command tickets own pending accounting and complete exactly once', () {
    final coordinator = FlarkEditorCoordinator();

    final first = coordinator.admitCommand(FlarkEditorCommandKind.sourceEdit);
    final second = coordinator.admitCommand(
      FlarkEditorCommandKind.semanticEdit,
    );
    expect(coordinator.pendingEdits, 2);
    coordinator.completeCommand(first);
    coordinator.completeCommand(second);
    expect(coordinator.pendingEdits, 0);
    expect(() => coordinator.completeCommand(second), throwsStateError);
  });

  test('history command lifetime is exclusive and coordinator-owned', () {
    final coordinator = FlarkEditorCoordinator();

    final history = coordinator.admitCommand(
      FlarkEditorCommandKind.historyReplay,
    );
    expect(coordinator.historyReplayPending, isTrue);
    expect(
      () => coordinator.admitCommand(FlarkEditorCommandKind.historyReplay),
      throwsStateError,
    );
    expect(
      () => coordinator.admitCommand(FlarkEditorCommandKind.sourceEdit),
      throwsStateError,
      reason: 'history is exclusive against every later command',
    );
    coordinator.completeCommand(history);
    expect(coordinator.historyReplayPending, isFalse);
    expect(() => coordinator.completeCommand(history), throwsStateError);

    coordinator.beginClosing();
    expect(
      () => coordinator.admitCommand(FlarkEditorCommandKind.historyReplay),
      throwsStateError,
    );
  });

  test('disposal rejects a leaked command lifetime', () {
    final coordinator = FlarkEditorCoordinator();
    final command = coordinator.admitCommand(FlarkEditorCommandKind.sourceEdit);

    expect(coordinator.markDisposed, throwsStateError);
    coordinator.completeCommand(command);
    coordinator.markDisposed();
    expect(coordinator.status, FlarkEditorStatus.disposed);
  });

  test('pending presentation is coordinator-owned', () {
    final coordinator = FlarkEditorCoordinator();

    coordinator.setPendingTaskCheck(7, true);

    expect(coordinator.pendingPresentation.taskChecks, {7: true});
    coordinator.retirePendingPresentation(const {
      FlarkPendingPresentationPart.taskChecks,
    });
    expect(coordinator.pendingPresentation.isEmpty, isTrue);
  });
}

FlarkCoreEditIntentReceiptV1 _receipt({
  required FlarkCoreEditPresentationTransitionV1 transition,
}) => FlarkCoreEditIntentReceiptV1(
  disposition: FlarkCoreEditIntentDispositionV1.applied,
  baseRevision: 1,
  resultRevision: 2,
  baseByteStart: 4,
  baseByteEnd: 5,
  baseUtf16Start: 4,
  baseUtf16End: 5,
  resultByteStart: 4,
  resultByteEnd: 4,
  resultUtf16Start: 4,
  resultUtf16End: 4,
  replacement: '',
  resultSelectionUtf16: 4,
  resultSourceByteLength: 9,
  resultSourceUtf16Length: 9,
  historyToken: null,
  parserPending: true,
  presentationProven: true,
  logicalEditId: 1,
  requestDigest: 1,
  telemetry: const FlarkCoreEditIntentTelemetryV1(
    coreQueueMicros: 0,
    workerRoundTripMicros: 0,
    workerQueueMicros: 0,
    nativeFfiMicros: 0,
    coreAdoptionMicros: 0,
  ),
  presentationTransition: transition,
);
