import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario.dart';
import 'support/live_editor_scenario_executor.dart';
import 'support/live_editor_scenario_macos_driver.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  final appExecutable = Platform.environment['FLARK_SCENARIO_APP_EXECUTABLE'];
  final configuredScenario = Platform.environment['FLARK_SCENARIO_PATH'];
  final scenarioFiles = configuredScenario == null
      ? [
          File('test/scenarios/paragraph_split_rapid_successor.json'),
          File('test/scenarios/list_return_backspace_successor.json'),
          File('test/scenarios/cross_block_pointer_replace_history.json'),
          File('test/scenarios/select_all_replace_history.json'),
          File('test/scenarios/select_all_type_history.json'),
          File('test/scenarios/clipboard_copy_cut_history.json'),
          File('test/scenarios/paragraph_list_boundary_newline.json'),
          File('test/scenarios/paragraph_join_backspace_successor.json'),
          File('test/scenarios/simple_list_continue_exit_type.json'),
          File('test/scenarios/unicode_grapheme_delete_successor.json'),
          File('test/scenarios/projected_inline_rapid_typing.json'),
          File('test/scenarios/scroll_preserves_selection.json'),
          File('test/scenarios/styled_selection_replace_history.json'),
          File('test/scenarios/multiblock_paste_history.json'),
          File('test/scenarios/task_checkbox_toggle_history.json'),
        ]
      : [File(configuredScenario)];
  final skipReason = libraryPath == null
      ? 'FLARK_V4_LIBRARY_PATH is not configured'
      : appExecutable == null
      ? 'FLARK_SCENARIO_APP_EXECUTABLE is not configured'
      : false;
  final plans = [
    for (final scenarioFile in scenarioFiles)
      ...const LiveEditorScenarioCompiler().compile(
        jsonDecode(scenarioFile.readAsStringSync()) as Map<String, Object?>,
      ),
  ];
  final driver = skipReason == false
      ? MacosNativeLiveEditorScenarioDriver(
          appExecutable: appExecutable!,
          libraryPath: libraryPath!,
          actuatorScript: File(
            'tool/live_editor_scenario_macos.swift',
          ).absolute.path,
        )
      : null;

  tearDownAll(() => driver?.close());

  for (final plan in plans) {
    test('${plan.qualifiedId} [macos-native]', () async {
      try {
        final result = await executeLiveEditorScenario(plan, driver!);
        print('FLARK_SCENARIO_RESULT ${jsonEncode(result.toJson())}');
      } catch (_) {
        stderr.writeln('FLARK_NATIVE_RECEIPT ${driver!.debugLastReceipt}');
        rethrow;
      }
    }, skip: skipReason);
  }
}
