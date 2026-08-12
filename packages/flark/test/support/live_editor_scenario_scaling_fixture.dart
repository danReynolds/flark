import 'live_editor_scenario.dart';

LiveEditorScenarioPlan scalingScenarioPlan(int index) {
  final beforeCaret = '# Scale $index\n\nAlpha beta';
  const afterCaret = ' gamma.\n';
  final initialSource = '$beforeCaret$afterCaret';
  final expectedSource = '${beforeCaret}X\n\nY$afterCaret';
  return LiveEditorScenarioPlan(
    id: 'scale-case',
    caseId: 'case-$index',
    description: 'Synthetic portable transaction case for runner scaling.',
    initialSource: initialSource,
    activationUtf16: beforeCaret.length,
    operations: const [
      LiveEditorInsertText(text: 'X', cadence: Duration.zero),
      LiveEditorKeyOperation(key: LiveEditorScenarioKey.enter),
      LiveEditorInsertText(text: 'Y', cadence: Duration.zero),
      LiveEditorAwait(barrier: LiveEditorScenarioBarrier.editSettled),
    ],
    expectation: LiveEditorScenarioExpectation(
      source: expectedSource,
      selectionBaseUtf16: beforeCaret.length + 4,
      selectionExtentUtf16: beforeCaret.length + 4,
      resyncCount: 0,
      faulted: false,
      settledPresentationNeverContains: const ['<empty>'],
      paintedPresentationNeverContains: const ['<empty>'],
    ),
  );
}
