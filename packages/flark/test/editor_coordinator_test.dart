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

    coordinator.admitEditingCommand();
    expect(coordinator.accepts(initial), isFalse);
  });

  test('an older completion cannot clear a newer publication barrier', () {
    final coordinator = FlarkEditorCoordinator();
    final firstGeneration = coordinator.admitEditingCommand();
    coordinator.beginPublicationBarrier();

    final secondGeneration = coordinator.admitEditingCommand();
    coordinator.beginPublicationBarrier();

    expect(coordinator.endPublicationBarrierForEdit(firstGeneration), isFalse);
    expect(coordinator.publicationCertificationBarrierActive, isTrue);
    expect(coordinator.endPublicationBarrierForEdit(secondGeneration), isTrue);
    expect(coordinator.publicationCertificationBarrierActive, isFalse);
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

  test('pending edit accounting has one owner and cannot underflow', () {
    final coordinator = FlarkEditorCoordinator();

    coordinator.beginPendingEdit();
    coordinator.beginPendingEdit();
    expect(coordinator.pendingEdits, 2);
    coordinator.endPendingEdit();
    coordinator.endPendingEdit();
    expect(coordinator.pendingEdits, 0);
    expect(coordinator.endPendingEdit, throwsStateError);
  });

  test('history replay is exclusive and coordinator-owned', () {
    final coordinator = FlarkEditorCoordinator();

    coordinator.beginHistoryReplay();
    expect(coordinator.historyReplayPending, isTrue);
    expect(coordinator.beginHistoryReplay, throwsStateError);
    coordinator.endHistoryReplay();
    expect(coordinator.historyReplayPending, isFalse);
    expect(coordinator.endHistoryReplay, throwsStateError);

    coordinator.beginClosing();
    expect(coordinator.beginHistoryReplay, throwsStateError);
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
