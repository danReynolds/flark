import 'dart:async';

import 'live_editor_scenario.dart';

final class LiveEditorScenarioSnapshot {
  const LiveEditorScenarioSnapshot({
    required this.source,
    required this.selectionBaseUtf16,
    required this.selectionExtentUtf16,
    required this.resyncCount,
    required this.faulted,
    required this.lastError,
    required this.settledPresentation,
    required this.paintedPresentations,
    required this.revision,
  });

  final String source;
  final int selectionBaseUtf16;
  final int selectionExtentUtf16;
  final int resyncCount;
  final bool faulted;
  final Object? lastError;
  final String settledPresentation;
  final List<String> paintedPresentations;
  final int revision;
}

abstract interface class LiveEditorScenarioDriver {
  String get name;

  bool get observesPaint;

  Future<void> start(LiveEditorScenarioPlan plan);

  Future<void> activateAtUtf16(int offset);

  Future<void> insertText(String text, {required Duration cadence});

  Future<void> pressKey(LiveEditorScenarioKey key);

  Future<void> selectSourceRange({required int base, required int extent});

  Future<void> pasteText(String text);

  Future<void> toggleTaskAtUtf16(int targetUtf16);

  Future<void> pause(Duration duration);

  Future<void> awaitBarrier(LiveEditorScenarioBarrier barrier);

  Future<LiveEditorScenarioSnapshot> snapshot();

  Future<void> stop();
}

final class LiveEditorScenarioExecutionResult {
  const LiveEditorScenarioExecutionResult({
    required this.plan,
    required this.driverName,
    required this.elapsed,
    required this.snapshot,
  });

  final LiveEditorScenarioPlan plan;
  final String driverName;
  final Duration elapsed;
  final LiveEditorScenarioSnapshot snapshot;

  Map<String, Object?> toJson() => {
    'id': plan.id,
    'caseId': plan.caseId,
    'planHash': plan.planHash,
    'runner': driverName,
    'elapsedMs': elapsed.inMilliseconds,
    'revision': snapshot.revision,
    'resyncs': snapshot.resyncCount,
    'paintSamples': snapshot.paintedPresentations.length,
    'passed': true,
  };
}

final class LiveEditorScenarioFailure implements Exception {
  const LiveEditorScenarioFailure(this.message);

  final String message;

  @override
  String toString() => 'LiveEditorScenarioFailure: $message';
}

Future<LiveEditorScenarioExecutionResult> executeLiveEditorScenario(
  LiveEditorScenarioPlan plan,
  LiveEditorScenarioDriver driver,
) async {
  final watch = Stopwatch()..start();
  await driver.start(plan);
  try {
    await driver.activateAtUtf16(plan.activationUtf16);
    for (final operation in plan.operations) {
      switch (operation) {
        case LiveEditorInsertText():
          await driver.insertText(operation.text, cadence: operation.cadence);
        case LiveEditorKeyOperation():
          await driver.pressKey(operation.key);
        case LiveEditorSelectSourceRange():
          await driver.selectSourceRange(
            base: operation.baseUtf16,
            extent: operation.extentUtf16,
          );
        case LiveEditorPasteText():
          await driver.pasteText(operation.text);
        case LiveEditorToggleTaskAtUtf16():
          await driver.toggleTaskAtUtf16(operation.targetUtf16);
        case LiveEditorPause():
          await driver.pause(operation.duration);
        case LiveEditorAwait():
          await driver.awaitBarrier(operation.barrier);
        case LiveEditorCheckpoint():
          await driver.awaitBarrier(LiveEditorScenarioBarrier.editSettled);
          final checkpoint = await driver.snapshot();
          _equal(
            plan,
            'checkpoint.${operation.id}.source',
            operation.source,
            checkpoint.source,
          );
          _equal(
            plan,
            'checkpoint.${operation.id}.selectionBaseUtf16',
            operation.selectionBaseUtf16,
            checkpoint.selectionBaseUtf16,
          );
          _equal(
            plan,
            'checkpoint.${operation.id}.selectionExtentUtf16',
            operation.selectionExtentUtf16,
            checkpoint.selectionExtentUtf16,
          );
          if (checkpoint.faulted || checkpoint.lastError != null) {
            throw LiveEditorScenarioFailure(
              '${plan.qualifiedId} checkpoint ${operation.id} faulted: '
              '${checkpoint.lastError}',
            );
          }
      }
    }
    await driver.awaitBarrier(LiveEditorScenarioBarrier.editSettled);
    final snapshot = await driver.snapshot();
    _assertExpectation(plan, driver, snapshot);
    watch.stop();
    return LiveEditorScenarioExecutionResult(
      plan: plan,
      driverName: driver.name,
      elapsed: watch.elapsed,
      snapshot: snapshot,
    );
  } finally {
    await driver.stop();
  }
}

void _assertExpectation(
  LiveEditorScenarioPlan plan,
  LiveEditorScenarioDriver driver,
  LiveEditorScenarioSnapshot actual,
) {
  final expected = plan.expectation;
  _equal(plan, 'source', expected.source, actual.source);
  _equal(
    plan,
    'selection.baseUtf16',
    expected.selectionBaseUtf16,
    actual.selectionBaseUtf16,
  );
  _equal(
    plan,
    'selection.extentUtf16',
    expected.selectionExtentUtf16,
    actual.selectionExtentUtf16,
  );
  _equal(plan, 'resyncCount', expected.resyncCount, actual.resyncCount);
  _equal(plan, 'faulted', expected.faulted, actual.faulted);
  if (actual.lastError != null) {
    throw LiveEditorScenarioFailure(
      '${plan.qualifiedId} produced an unexpected error: ${actual.lastError}',
    );
  }
  for (final forbidden in expected.settledPresentationNeverContains) {
    if (actual.settledPresentation.contains(forbidden)) {
      throw LiveEditorScenarioFailure(
        '${plan.qualifiedId} settled presentation contained "$forbidden"',
      );
    }
  }
  if (expected.paintedPresentationNeverContains.isNotEmpty &&
      !driver.observesPaint) {
    return;
  }
  for (final forbidden in expected.paintedPresentationNeverContains) {
    for (
      var index = 0;
      index < actual.paintedPresentations.length;
      index += 1
    ) {
      if (actual.paintedPresentations[index].contains(forbidden)) {
        throw LiveEditorScenarioFailure(
          '${plan.qualifiedId} painted presentation $index contained '
          '"$forbidden"',
        );
      }
    }
  }
}

void _equal(
  LiveEditorScenarioPlan plan,
  String field,
  Object? expected,
  Object? actual,
) {
  if (expected == actual) return;
  throw LiveEditorScenarioFailure(
    '${plan.qualifiedId} $field differed:\n'
    'expected: $expected\n'
    'actual:   $actual',
  );
}
