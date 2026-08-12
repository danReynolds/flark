import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import '../../test/support/live_editor_scenario_executor.dart';
import '../../test/support/live_editor_scenario_scaling_fixture.dart';
import '../../test/support/live_editor_scenario_surface_driver.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const countSpecification = String.fromEnvironment(
    'FLARK_SCENARIO_SCALE_COUNTS',
  );

  testWidgets(
    'portable scenario corpus surface scaling experiment',
    (tester) async {
      const configuredLibrary = String.fromEnvironment('FLARK_V4_LIBRARY_PATH');
      final libraryPath = configuredLibrary.isNotEmpty
          ? configuredLibrary
          : File(
              '../../../native/comrak_bridge/target/release/libflark_abi.dylib',
            ).absolute.path;

      for (final count in _parseCounts(countSpecification)) {
        final corpusWatch = Stopwatch()..start();
        final caseMicros = <int>[];
        for (var index = 0; index < count; index += 1) {
          if (index > 0 && index % 25 == 0) {
            print(
              'FLARK_SCENARIO_SCALE_PROGRESS ${jsonEncode(<String, Object?>{'cases': count, 'completed': index, 'elapsedMs': corpusWatch.elapsedMilliseconds})}',
            );
          }
          try {
            final result = await executeLiveEditorScenario(
              scalingScenarioPlan(index),
              FlutterSurfaceLiveEditorScenarioDriver(
                libraryPath: libraryPath,
                tester: tester,
              ),
            );
            caseMicros.add(result.elapsed.inMicroseconds);
          } catch (error, stackTrace) {
            Error.throwWithStackTrace(
              StateError('surface scaling case $index failed: $error'),
              stackTrace,
            );
          }
        }
        corpusWatch.stop();
        caseMicros.sort();
        final result = <String, Object?>{
          'runner': 'flutter-surface/${Platform.operatingSystem}',
          'cases': count,
          'elapsedMs': corpusWatch.elapsedMilliseconds,
          'casesPerSecond':
              count /
              (corpusWatch.elapsedMicroseconds /
                  Duration.microsecondsPerSecond),
          'caseP50Ms':
              _percentile(caseMicros, 0.50) /
              Duration.microsecondsPerMillisecond,
          'caseP95Ms':
              _percentile(caseMicros, 0.95) /
              Duration.microsecondsPerMillisecond,
          'caseMaxMs': caseMicros.last / Duration.microsecondsPerMillisecond,
        };
        print('FLARK_SCENARIO_SCALE_RESULT ${jsonEncode(result)}');
      }
    },
    skip: countSpecification.isEmpty,
  );
}

List<int> _parseCounts(String specification) {
  final counts = specification.split(',').map(int.parse).toList();
  if (counts.isEmpty || counts.any((count) => count <= 0)) {
    throw FormatException('scenario scale counts must all be positive');
  }
  return counts;
}

int _percentile(List<int> sortedValues, double percentile) {
  final index = ((sortedValues.length - 1) * percentile).round();
  return sortedValues[index];
}
