import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import '../../test/support/live_editor_scenario.dart';
import '../../test/support/live_editor_scenario_executor.dart';
import '../../test/support/live_editor_scenario_surface_driver.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const configuredLibrary = String.fromEnvironment('FLARK_V4_LIBRARY_PATH');
  const configuredScenario = String.fromEnvironment('FLARK_SCENARIO_PATH');

  testWidgets(
    'canonical live-editor scenario plans run through the mounted surface',
    (tester) async {
      final libraryPath = configuredLibrary.isNotEmpty
          ? configuredLibrary
          : File(
              '../../../native/comrak_bridge/target/release/libflark_abi.dylib',
            ).absolute.path;
      final scenarioFiles = configuredScenario.isNotEmpty
          ? [File(configuredScenario)]
          : (Directory('../test/scenarios')
                  .listSync()
                  .whereType<File>()
                  .where((file) => file.path.endsWith('.json'))
                  .toList()
                ..sort((left, right) => left.path.compareTo(right.path)));
      for (final scenarioFile in scenarioFiles) {
        final plans = const LiveEditorScenarioCompiler().compile(
          jsonDecode(scenarioFile.readAsStringSync()) as Map<String, Object?>,
        );
        for (final plan in plans) {
          final result = await executeLiveEditorScenario(
            plan,
            FlutterSurfaceLiveEditorScenarioDriver(
              libraryPath: libraryPath,
              tester: tester,
            ),
          );
          print('FLARK_SCENARIO_RESULT ${jsonEncode(result.toJson())}');
        }
      }
    },
  );
}
