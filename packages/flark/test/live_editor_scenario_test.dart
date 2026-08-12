import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario.dart';
import 'support/live_editor_scenario_runner.dart';

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
        () => runLiveEditorScenario(
          scenario,
          schedule,
          libraryPath: libraryPath!,
        ),
        skip: libraryPath == null,
      );
    }
  }
}
