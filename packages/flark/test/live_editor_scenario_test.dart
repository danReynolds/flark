import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  final configuredScenario = Platform.environment['FLARK_SCENARIO_PATH'];
  final scenarioFiles = configuredScenario == null
      ? Directory('test/scenarios')
            .listSync()
            .whereType<File>()
            .where((file) => file.path.endsWith('.json'))
            .toList()
      : [File(configuredScenario)];

  for (final scenarioFile in scenarioFiles) {
    final scenario = LiveEditorScenario.load(scenarioFile);
    for (final schedule in scenario.schedules) {
      test(
        '${scenario.id} [headless/${schedule.id}]',
        () => _runScenario(scenario, schedule, libraryPath: libraryPath!),
        skip: libraryPath == null,
      );
    }
  }
}

Future<void> _runScenario(
  LiveEditorScenario scenario,
  ScenarioSchedule schedule, {
  required String libraryPath,
}) async {
  final watch = Stopwatch()..start();
  final controller = await FlarkEditorController.open(
    scenario.initialSource,
    libraryPath: libraryPath,
  );
  await controller.continueParsing();
  final frames = <String>[];

  void captureFrame() {
    final presentations = controller.rows
        .map(controller.surfaceRow)
        .toList(growable: false);
    final frame = presentations.isEmpty
        ? '<empty>'
        : presentations.map((row) => row.text).join('\n');
    if (frames.isEmpty || frames.last != frame) frames.add(frame);
  }

  captureFrame();
  final frameSampler = Timer.periodic(
    const Duration(milliseconds: 8),
    (_) => captureFrame(),
  );

  try {
    final caret = scenario.activation.resolve(scenario.initialSource);
    final row = controller.rows.firstWhere(
      (candidate) =>
          candidate.editableUtf16 != null &&
          candidate.editableUtf16!.start <= caret &&
          caret <= candidate.editableUtf16!.end,
    );
    controller.activateRow(row, caret);
    var platformValue = controller.inputValue;
    var retainPlatformShadow = false;

    for (final step in scenario.steps) {
      switch (step.type) {
        case 'typeText':
          platformValue = await _typeText(
            controller,
            platformValue,
            step.text!,
            interval: Duration(milliseconds: step.intervalMs ?? 0),
            followControllerUpdates: !retainPlatformShadow,
          );
        case 'pressReturn':
          platformValue = _pressReturn(controller, platformValue);
          retainPlatformShadow = true;
        case 'scheduleDelay':
          final delay = schedule.delaysMs[step.scheduleKey];
          if (delay == null) {
            throw StateError(
              'schedule ${schedule.id} does not define ${step.scheduleKey}',
            );
          }
          await Future<void>.delayed(Duration(milliseconds: delay));
          if (delay > 0) {
            platformValue = controller.inputValue;
            retainPlatformShadow = false;
          }
        case 'waitForIdle':
          await _settle(controller);
          platformValue = controller.inputValue;
          retainPlatformShadow = false;
        default:
          throw StateError('unsupported scenario step ${step.type}');
      }
    }
    await _settle(controller);
    captureFrame();
    watch.stop();

    final expectation = scenario.expectation;
    expect(await controller.readSource(), expectation.source);
    expect(controller.globalCaretOffset, expectation.caretUtf16);
    expect(controller.resyncCount, expectation.resyncCount);
    expect(controller.status == FlarkEditorStatus.faulted, expectation.faulted);
    expect(controller.lastError, isNull);
    // Controller states can change more often than Flutter paints. The native
    // adapter checks every actual frame; this fast lane only requires the
    // settled render-ready presentation to be clean.
    final settledPresentation = frames.last;
    for (final forbidden in expectation.forbiddenSurfaceSubstrings) {
      expect(
        settledPresentation.contains(forbidden),
        isFalse,
        reason: 'settled presentation contained "$forbidden"',
      );
    }
    print(
      'FLARK_SCENARIO_RESULT ${jsonEncode(<String, Object?>{'id': scenario.id, 'runner': 'headless', 'schedule': schedule.id, 'elapsedMs': watch.elapsedMilliseconds, 'frames': frames.length, 'revision': controller.revision, 'resyncs': controller.resyncCount, 'passed': true})}',
    );
  } finally {
    frameSampler.cancel();
    await controller.close();
  }
}

TextEditingValue _pressReturn(
  FlarkEditorController controller,
  TextEditingValue before,
) {
  final selection = before.selection;
  final start = selection.start;
  final end = selection.end;
  final delta = start == end
      ? TextEditingDeltaInsertion(
          oldText: before.text,
          textInserted: '\n',
          insertionOffset: start,
          selection: TextSelection.collapsed(offset: start + 1),
          composing: TextRange.empty,
        )
      : TextEditingDeltaReplacement(
          oldText: before.text,
          replacementText: '\n',
          replacedRange: TextRange(start: start, end: end),
          selection: TextSelection.collapsed(offset: start + 1),
          composing: TextRange.empty,
        );
  controller.applyDeltas([delta]);
  controller.observePlatformNewlineAction();
  return delta.apply(before);
}

Future<TextEditingValue> _typeText(
  FlarkEditorController controller,
  TextEditingValue initialValue,
  String text, {
  required Duration interval,
  required bool followControllerUpdates,
}) async {
  var platformValue = initialValue;
  var follow = followControllerUpdates;
  for (final rune in text.runes) {
    final character = String.fromCharCode(rune);
    final before = follow ? controller.inputValue : platformValue;
    final offset = before.selection.extentOffset;
    final delta = TextEditingDeltaInsertion(
      oldText: before.text,
      textInserted: character,
      insertionOffset: offset,
      selection: TextSelection.collapsed(offset: offset + character.length),
      composing: TextRange.empty,
    );
    controller.applyDeltas([delta]);
    platformValue = delta.apply(before);
    if (interval > Duration.zero) {
      await Future<void>.delayed(interval);
    }
    follow = true;
  }
  return platformValue;
}

Future<void> _settle(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 1));
  }
  if (controller.pendingEdits == 0) {
    await controller.continueParsing();
  }
  if (controller.pendingEdits != 0) {
    throw StateError('scenario did not settle before its 5 second deadline');
  }
  if (controller.lastError case final error?) throw error;
}
