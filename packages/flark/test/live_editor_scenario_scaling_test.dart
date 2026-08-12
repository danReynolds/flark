import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario.dart';
import 'support/live_editor_scenario_runner.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  final countSpecification =
      Platform.environment['FLARK_SCENARIO_SCALE_COUNTS'];
  final counts = countSpecification
      ?.split(',')
      .map(int.parse)
      .toList(growable: false);
  final skipReason = libraryPath == null
      ? 'FLARK_V4_LIBRARY_PATH is required'
      : counts == null
      ? 'set FLARK_SCENARIO_SCALE_COUNTS to opt into the scaling experiment'
      : false;

  test('portable scenario corpus scaling experiment', () async {
    for (final count in counts!) {
      final corpusWatch = Stopwatch()..start();
      final caseMicros = <int>[];
      for (var index = 0; index < count; index += 1) {
        final scenario = _scalingScenario(index);
        try {
          final result = await runLiveEditorScenario(
            scenario,
            scenario.schedules.single,
            libraryPath: libraryPath!,
            emitResult: false,
          );
          caseMicros.add(result.elapsed.inMicroseconds);
        } catch (error, stackTrace) {
          Error.throwWithStackTrace(
            StateError('scaling case $index of $count failed: $error'),
            stackTrace,
          );
        }
      }
      corpusWatch.stop();
      caseMicros.sort();
      final result = <String, Object?>{
        'runner': 'headless',
        'cases': count,
        'elapsedMs': corpusWatch.elapsedMilliseconds,
        'casesPerSecond':
            count /
            (corpusWatch.elapsedMicroseconds / Duration.microsecondsPerSecond),
        'caseP50Ms':
            _percentile(caseMicros, 0.50) / Duration.microsecondsPerMillisecond,
        'caseP95Ms':
            _percentile(caseMicros, 0.95) / Duration.microsecondsPerMillisecond,
        'caseMaxMs': caseMicros.last / Duration.microsecondsPerMillisecond,
      };
      print('FLARK_SCENARIO_SCALE_RESULT ${jsonEncode(result)}');
    }
  }, skip: skipReason);
}

LiveEditorScenario _scalingScenario(int index) {
  final beforeCaret = '# Scale $index\n\nAlpha beta';
  const afterCaret = ' gamma.\n';
  final initialSource = '$beforeCaret$afterCaret';
  final expectedSource = '${beforeCaret}X\n\nY$afterCaret';
  return LiveEditorScenario(
    id: 'scale-$index',
    description: 'Synthetic portable transaction case for runner scaling.',
    initialSource: initialSource,
    activation: const ScenarioActivation(
      needle: 'beta',
      utf16OffsetInNeedle: 4,
    ),
    steps: const <ScenarioStep>[
      ScenarioStep(type: 'typeText', text: 'X', intervalMs: 0),
      ScenarioStep(type: 'pressReturn'),
      ScenarioStep(type: 'typeText', text: 'Y', intervalMs: 0),
      ScenarioStep(type: 'waitForIdle'),
    ],
    schedules: const <ScenarioSchedule>[
      ScenarioSchedule(id: 'default', delaysMs: <String, int>{}),
    ],
    expectation: ScenarioExpectation(
      source: expectedSource,
      caretUtf16: beforeCaret.length + 4,
      resyncCount: 0,
      faulted: false,
      forbiddenSurfaceSubstrings: const <String>['<empty>'],
    ),
  );
}

int _percentile(List<int> sortedValues, double percentile) {
  final index = ((sortedValues.length - 1) * percentile).round();
  return sortedValues[index];
}
