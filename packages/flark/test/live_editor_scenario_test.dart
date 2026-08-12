import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario.dart';
import 'support/live_editor_scenario_executor.dart';
import 'support/live_editor_scenario_runner.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  final configuredScenario = Platform.environment['FLARK_SCENARIO_PATH'];
  final scenarioFiles = configuredScenario == null
      ? (Directory('test/scenarios')
            .listSync()
            .whereType<File>()
            .where((file) => file.path.endsWith('.json'))
            .toList()
          ..sort((left, right) => left.path.compareTo(right.path)))
      : [File(configuredScenario)];
  const compiler = LiveEditorScenarioCompiler();

  for (final scenarioFile in scenarioFiles) {
    final plans = compiler.compile(
      jsonDecode(scenarioFile.readAsStringSync()) as Map<String, Object?>,
    );
    for (final plan in plans) {
      test('${plan.id} [no-window/${plan.caseId}]', () async {
        final result = await executeLiveEditorScenario(
          plan,
          NoWindowLiveEditorScenarioDriver(libraryPath: libraryPath!),
        );
        print('FLARK_SCENARIO_RESULT ${jsonEncode(result.toJson())}');
      }, skip: libraryPath == null);
    }
  }
}
