import 'dart:convert';
import 'dart:io';

final class LiveEditorScenario {
  const LiveEditorScenario({
    required this.id,
    required this.description,
    required this.initialSource,
    required this.activation,
    required this.steps,
    required this.schedules,
    required this.expectation,
  });

  factory LiveEditorScenario.fromJson(Map<String, Object?> json) {
    if (json['schemaVersion'] != 1) {
      throw FormatException(
        'unsupported live-editor scenario schema ${json['schemaVersion']}',
      );
    }
    return LiveEditorScenario(
      id: json['id']! as String,
      description: json['description']! as String,
      initialSource: json['initialSource']! as String,
      activation: ScenarioActivation.fromJson(
        json['activation']! as Map<String, Object?>,
      ),
      steps: [
        for (final value in json['steps']! as List<Object?>)
          ScenarioStep.fromJson(value! as Map<String, Object?>),
      ],
      schedules: [
        for (final value in json['schedules']! as List<Object?>)
          ScenarioSchedule.fromJson(value! as Map<String, Object?>),
      ],
      expectation: ScenarioExpectation.fromJson(
        json['expect']! as Map<String, Object?>,
      ),
    );
  }

  static LiveEditorScenario load(File file) => LiveEditorScenario.fromJson(
    jsonDecode(file.readAsStringSync()) as Map<String, Object?>,
  );

  final String id;
  final String description;
  final String initialSource;
  final ScenarioActivation activation;
  final List<ScenarioStep> steps;
  final List<ScenarioSchedule> schedules;
  final ScenarioExpectation expectation;
}

final class ScenarioActivation {
  const ScenarioActivation({
    required this.needle,
    required this.utf16OffsetInNeedle,
  });

  factory ScenarioActivation.fromJson(Map<String, Object?> json) =>
      ScenarioActivation(
        needle: json['needle']! as String,
        utf16OffsetInNeedle: json['utf16OffsetInNeedle']! as int,
      );

  final String needle;
  final int utf16OffsetInNeedle;

  int resolve(String source) {
    final start = source.indexOf(needle);
    if (start < 0) throw StateError('activation needle not found: $needle');
    return start + utf16OffsetInNeedle;
  }
}

final class ScenarioStep {
  const ScenarioStep({
    required this.type,
    this.text,
    this.intervalMs,
    this.scheduleKey,
  });

  factory ScenarioStep.fromJson(Map<String, Object?> json) => ScenarioStep(
    type: json['type']! as String,
    text: json['text'] as String?,
    intervalMs: json['intervalMs'] as int?,
    scheduleKey: json['key'] as String?,
  );

  final String type;
  final String? text;
  final int? intervalMs;
  final String? scheduleKey;
}

final class ScenarioSchedule {
  const ScenarioSchedule({required this.id, required this.delaysMs});

  factory ScenarioSchedule.fromJson(Map<String, Object?> json) =>
      ScenarioSchedule(
        id: json['id']! as String,
        delaysMs: {
          for (final entry in json.entries)
            if (entry.key != 'id') entry.key: entry.value! as int,
        },
      );

  final String id;
  final Map<String, int> delaysMs;
}

final class ScenarioExpectation {
  const ScenarioExpectation({
    required this.source,
    required this.caretUtf16,
    required this.resyncCount,
    required this.faulted,
    required this.forbiddenSurfaceSubstrings,
  });

  factory ScenarioExpectation.fromJson(Map<String, Object?> json) =>
      ScenarioExpectation(
        source: json['source']! as String,
        caretUtf16: json['caretUtf16']! as int,
        resyncCount: json['resyncCount']! as int,
        faulted: json['faulted']! as bool,
        forbiddenSurfaceSubstrings: [
          for (final value
              in json['forbiddenSurfaceSubstrings']! as List<Object?>)
            value! as String,
        ],
      );

  final String source;
  final int caretUtf16;
  final int resyncCount;
  final bool faulted;
  final List<String> forbiddenSurfaceSubstrings;
}
