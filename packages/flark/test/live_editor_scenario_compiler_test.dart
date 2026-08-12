import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_scenario.dart';

void main() {
  const compiler = LiveEditorScenarioCompiler();

  test('compiler expands schedules into stable hashed plans', () {
    final plans = compiler.compile(_fixture());
    expect(plans.map((plan) => plan.caseId), ['immediate', 'delayed']);
    expect(plans.first.activationUtf16, 6);
    expect(plans.first.operations[1], isA<LiveEditorPause>());
    expect(
      (plans.first.operations[1] as LiveEditorPause).duration,
      Duration.zero,
    );
    expect(
      (plans.last.operations[1] as LiveEditorPause).duration,
      const Duration(milliseconds: 7),
    );
    expect(plans.first.planHash, hasLength(64));
    expect(plans.first.planHash, isNot(plans.last.planHash));
    final roundTrip = decodeLiveEditorScenarioPlanBundle(
      encodeLiveEditorScenarioPlanBundle(plans),
    );
    expect(
      roundTrip.map((plan) => plan.planHash),
      plans.map((plan) => plan.planHash),
    );
  });

  test('compiler rejects unknown fields at every boundary', () {
    final fixture = _fixture();
    fixture['mystery'] = true;
    expect(() => compiler.compile(fixture), throwsFormatException);

    final stepFixture = _fixture();
    final firstStep =
        (stepFixture['steps']! as List<Object?>).first as Map<String, Object?>;
    firstStep['mystery'] = true;
    expect(() => compiler.compile(stepFixture), throwsFormatException);
  });

  test('compiler rejects ambiguous activation contracts', () {
    final missingOccurrence = _fixture();
    (missingOccurrence['activation']! as Map<String, Object?>).remove(
      'occurrence',
    );
    expect(() => compiler.compile(missingOccurrence), throwsFormatException);

    final missingNeedle = _fixture();
    (missingNeedle['activation']! as Map<String, Object?>)['occurrence'] = 9;
    expect(() => compiler.compile(missingNeedle), throwsFormatException);
  });

  test('compiler rejects offsets that split surrogate pairs', () {
    final fixture = _fixture(initialSource: 'A😀B');
    fixture['activation'] = <String, Object?>{
      'needle': '😀',
      'occurrence': 0,
      'utf16OffsetInNeedle': 1,
    };
    expect(() => compiler.compile(fixture), throwsFormatException);
  });

  test('plan hash detects canonical plan tampering', () {
    final plan = compiler.compile(_fixture()).first;
    final decoded =
        jsonDecode(jsonEncode(plan.toJson())) as Map<String, Object?>;
    decoded['activationUtf16'] = plan.activationUtf16 + 1;
    expect(
      () => LiveEditorScenarioPlan.fromJson(decoded),
      throwsFormatException,
    );
  });

  test('compiler rejects invalid and duplicate checkpoints', () {
    final invalidOffset = _fixture();
    (invalidOffset['steps']! as List<Object?>).insert(1, <String, Object?>{
      'type': 'checkpoint',
      'id': 'after-edit',
      'source': 'A😀B',
      'caretUtf16': 2,
    });
    expect(() => compiler.compile(invalidOffset), throwsFormatException);

    final duplicate = _fixture();
    final steps = duplicate['steps']! as List<Object?>;
    for (var index = 0; index < 2; index += 1) {
      steps.insert(1, <String, Object?>{
        'type': 'checkpoint',
        'id': 'same-id',
        'source': 'Alpha beta.',
        'caretUtf16': 0,
      });
    }
    expect(() => compiler.compile(duplicate), throwsFormatException);
  });
}

Map<String, Object?> _fixture({String initialSource = 'Alpha beta.'}) => {
  'schemaVersion': 1,
  'id': 'compiler-case',
  'description': 'Compiler contract fixture.',
  'initialSource': initialSource,
  'activation': <String, Object?>{
    'needle': initialSource == 'Alpha beta.' ? 'beta' : initialSource,
    'occurrence': 0,
    'utf16OffsetInNeedle': 0,
  },
  'steps': <Object?>[
    <String, Object?>{'type': 'typeText', 'text': 'X', 'intervalMs': 0},
    <String, Object?>{'type': 'scheduleDelay', 'key': 'delayMs'},
    <String, Object?>{'type': 'waitForIdle'},
  ],
  'schedules': <Object?>[
    <String, Object?>{'id': 'immediate', 'delayMs': 0},
    <String, Object?>{'id': 'delayed', 'delayMs': 7},
  ],
  'expect': <String, Object?>{
    'source': initialSource,
    'caretUtf16': 0,
    'resyncCount': 0,
    'faulted': false,
    'forbiddenSurfaceSubstrings': <Object?>[],
    'paintedSurfaceNeverContains': <Object?>[],
  },
};
