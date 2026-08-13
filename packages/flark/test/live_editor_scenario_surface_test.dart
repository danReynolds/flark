import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario.dart';
import 'support/live_editor_scenario_executor.dart';
import 'support/live_editor_scenario_surface_driver.dart';

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

  for (final scenarioFile in scenarioFiles) {
    final plans = const LiveEditorScenarioCompiler().compile(
      jsonDecode(scenarioFile.readAsStringSync()) as Map<String, Object?>,
    );
    for (final plan in plans) {
      testWidgets('${plan.qualifiedId} [flutter-surface]', (tester) async {
        final result = await tester.runAsync(
          () => executeLiveEditorScenario(
            plan,
            FlutterSurfaceLiveEditorScenarioDriver(
              libraryPath: libraryPath!,
              tester: tester,
            ),
          ),
        );
        print('FLARK_SCENARIO_RESULT ${jsonEncode(result!.toJson())}');
        if (Platform.environment['FLARK_SCENARIO_TRACE'] == '1') {
          final trace = <String, Object?>{
            'presentations': result.snapshot.paintedPresentations,
            'renderPlanHashes': result.snapshot.paintedRenderPlanHashes,
            'visualStateHashes': result.snapshot.paintedVisualStateHashes,
          };
          print('FLARK_SURFACE_TRACE ${jsonEncode(trace)}');
        }
      }, skip: libraryPath == null);
    }
  }
}
