import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario_executor.dart';
import 'support/live_editor_scenario_runner.dart';
import 'support/live_editor_scenario_scaling_fixture.dart';

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
        final plan = scalingScenarioPlan(index);
        try {
          final result = await executeLiveEditorScenario(
            plan,
            NoWindowLiveEditorScenarioDriver(libraryPath: libraryPath!),
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
        'runner': 'no-window',
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

int _percentile(List<int> sortedValues, double percentile) {
  final index = ((sortedValues.length - 1) * percentile).round();
  return sortedValues[index];
}
