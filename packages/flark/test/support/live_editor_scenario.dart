import 'dart:convert';

import 'package:crypto/crypto.dart';

const liveEditorScenarioSchemaVersion = 1;

enum LiveEditorScenarioKey { enter, backspace, delete, undo, redo }

enum LiveEditorScenarioBarrier { editSettled, paintSettled }

sealed class LiveEditorScenarioOperation {
  const LiveEditorScenarioOperation();

  Map<String, Object?> toJson();

  factory LiveEditorScenarioOperation.fromJson(Map<String, Object?> json) {
    final reader = _ObjectReader(json, 'plan.operation');
    final op = reader.requiredString('op');
    switch (op) {
      case 'insertText':
        reader.expectKeys(const {'op', 'text', 'cadenceMs'});
        return LiveEditorInsertText(
          text: reader.requiredString('text'),
          cadence: Duration(
            milliseconds: reader.optionalNonNegativeInt('cadenceMs') ?? 0,
          ),
        );
      case 'key':
        reader.expectKeys(const {'op', 'key'});
        return LiveEditorKeyOperation(
          key: _enumByName(
            LiveEditorScenarioKey.values,
            reader.requiredString('key'),
            '${reader.path}.key',
          ),
        );
      case 'selectSourceRange':
        reader.expectKeys(const {'op', 'baseUtf16', 'extentUtf16'});
        return LiveEditorSelectSourceRange(
          baseUtf16: reader.requiredNonNegativeInt('baseUtf16'),
          extentUtf16: reader.requiredNonNegativeInt('extentUtf16'),
        );
      case 'pasteText':
        reader.expectKeys(const {'op', 'text'});
        return LiveEditorPasteText(text: reader.requiredString('text'));
      case 'toggleTaskAtUtf16':
        reader.expectKeys(const {'op', 'targetUtf16'});
        return LiveEditorToggleTaskAtUtf16(
          targetUtf16: reader.requiredNonNegativeInt('targetUtf16'),
        );
      case 'pause':
        reader.expectKeys(const {'op', 'milliseconds'});
        return LiveEditorPause(
          duration: Duration(
            milliseconds: reader.requiredNonNegativeInt('milliseconds'),
          ),
        );
      case 'await':
        reader.expectKeys(const {'op', 'barrier'});
        return LiveEditorAwait(
          barrier: _enumByName(
            LiveEditorScenarioBarrier.values,
            reader.requiredString('barrier'),
            '${reader.path}.barrier',
          ),
        );
      case 'checkpoint':
        reader.expectKeys(const {
          'op',
          'id',
          'source',
          'selectionBaseUtf16',
          'selectionExtentUtf16',
        });
        return LiveEditorCheckpoint(
          id: reader.requiredString('id'),
          source: reader.requiredString('source'),
          selectionBaseUtf16: reader.requiredNonNegativeInt(
            'selectionBaseUtf16',
          ),
          selectionExtentUtf16: reader.requiredNonNegativeInt(
            'selectionExtentUtf16',
          ),
        );
      default:
        throw FormatException('${reader.path}.op is unsupported: $op');
    }
  }
}

final class LiveEditorInsertText extends LiveEditorScenarioOperation {
  const LiveEditorInsertText({required this.text, required this.cadence});

  final String text;
  final Duration cadence;

  @override
  Map<String, Object?> toJson() => {
    'op': 'insertText',
    'text': text,
    'cadenceMs': cadence.inMilliseconds,
  };
}

final class LiveEditorKeyOperation extends LiveEditorScenarioOperation {
  const LiveEditorKeyOperation({required this.key});

  final LiveEditorScenarioKey key;

  @override
  Map<String, Object?> toJson() => {'op': 'key', 'key': key.name};
}

final class LiveEditorSelectSourceRange extends LiveEditorScenarioOperation {
  const LiveEditorSelectSourceRange({
    required this.baseUtf16,
    required this.extentUtf16,
  });

  final int baseUtf16;
  final int extentUtf16;

  @override
  Map<String, Object?> toJson() => {
    'op': 'selectSourceRange',
    'baseUtf16': baseUtf16,
    'extentUtf16': extentUtf16,
  };
}

final class LiveEditorPasteText extends LiveEditorScenarioOperation {
  const LiveEditorPasteText({required this.text});

  final String text;

  @override
  Map<String, Object?> toJson() => {'op': 'pasteText', 'text': text};
}

final class LiveEditorToggleTaskAtUtf16 extends LiveEditorScenarioOperation {
  const LiveEditorToggleTaskAtUtf16({required this.targetUtf16});

  final int targetUtf16;

  @override
  Map<String, Object?> toJson() => {
    'op': 'toggleTaskAtUtf16',
    'targetUtf16': targetUtf16,
  };
}

final class LiveEditorPause extends LiveEditorScenarioOperation {
  const LiveEditorPause({required this.duration});

  final Duration duration;

  @override
  Map<String, Object?> toJson() => {
    'op': 'pause',
    'milliseconds': duration.inMilliseconds,
  };
}

final class LiveEditorAwait extends LiveEditorScenarioOperation {
  const LiveEditorAwait({required this.barrier});

  final LiveEditorScenarioBarrier barrier;

  @override
  Map<String, Object?> toJson() => {'op': 'await', 'barrier': barrier.name};
}

final class LiveEditorCheckpoint extends LiveEditorScenarioOperation {
  const LiveEditorCheckpoint({
    required this.id,
    required this.source,
    required this.selectionBaseUtf16,
    required this.selectionExtentUtf16,
  });

  final String id;
  final String source;
  final int selectionBaseUtf16;
  final int selectionExtentUtf16;

  @override
  Map<String, Object?> toJson() => {
    'op': 'checkpoint',
    'id': id,
    'source': source,
    'selectionBaseUtf16': selectionBaseUtf16,
    'selectionExtentUtf16': selectionExtentUtf16,
  };
}

final class LiveEditorScenarioExpectation {
  const LiveEditorScenarioExpectation({
    required this.source,
    required this.selectionBaseUtf16,
    required this.selectionExtentUtf16,
    required this.resyncCount,
    required this.faulted,
    required this.settledPresentationNeverContains,
    required this.paintedPresentationNeverContains,
  });

  factory LiveEditorScenarioExpectation.fromJson(Map<String, Object?> json) {
    final reader = _ObjectReader(json, 'plan.expect');
    reader.expectKeys(const {
      'source',
      'selectionBaseUtf16',
      'selectionExtentUtf16',
      'resyncCount',
      'faulted',
      'settledPresentationNeverContains',
      'paintedPresentationNeverContains',
    });
    return LiveEditorScenarioExpectation(
      source: reader.requiredString('source'),
      selectionBaseUtf16: reader.requiredNonNegativeInt('selectionBaseUtf16'),
      selectionExtentUtf16: reader.requiredNonNegativeInt(
        'selectionExtentUtf16',
      ),
      resyncCount: reader.requiredNonNegativeInt('resyncCount'),
      faulted: reader.requiredBool('faulted'),
      settledPresentationNeverContains: reader.requiredStringList(
        'settledPresentationNeverContains',
      ),
      paintedPresentationNeverContains: reader.requiredStringList(
        'paintedPresentationNeverContains',
      ),
    );
  }

  final String source;
  final int selectionBaseUtf16;
  final int selectionExtentUtf16;
  final int resyncCount;
  final bool faulted;
  final List<String> settledPresentationNeverContains;
  final List<String> paintedPresentationNeverContains;

  Map<String, Object?> toJson() => {
    'source': source,
    'selectionBaseUtf16': selectionBaseUtf16,
    'selectionExtentUtf16': selectionExtentUtf16,
    'resyncCount': resyncCount,
    'faulted': faulted,
    'settledPresentationNeverContains': settledPresentationNeverContains,
    'paintedPresentationNeverContains': paintedPresentationNeverContains,
  };
}

final class LiveEditorScenarioPlan {
  LiveEditorScenarioPlan({
    required this.id,
    required this.caseId,
    required this.description,
    required this.initialSource,
    required this.activationUtf16,
    required List<LiveEditorScenarioOperation> operations,
    required this.expectation,
    String? planHash,
  }) : operations = List.unmodifiable(operations),
       planHash = planHash ?? '' {
    _validate();
    final computedHash = computeHash();
    if (this.planHash.isNotEmpty && this.planHash != computedHash) {
      throw FormatException(
        'plan hash mismatch for $qualifiedId: '
        '${this.planHash} != $computedHash',
      );
    }
    this.planHash = computedHash;
  }

  factory LiveEditorScenarioPlan.fromJson(Map<String, Object?> json) {
    final reader = _ObjectReader(json, 'plan');
    reader.expectKeys(const {
      'schema',
      'id',
      'caseId',
      'description',
      'initialSource',
      'activationUtf16',
      'operations',
      'expect',
      'planHash',
    });
    if (reader.requiredString('schema') != 'flark.live-editor-plan/v1') {
      throw FormatException('plan.schema is unsupported');
    }
    return LiveEditorScenarioPlan(
      id: reader.requiredString('id'),
      caseId: reader.requiredString('caseId'),
      description: reader.requiredString('description'),
      initialSource: reader.requiredString('initialSource'),
      activationUtf16: reader.requiredNonNegativeInt('activationUtf16'),
      operations: reader
          .requiredObjectList('operations')
          .map(LiveEditorScenarioOperation.fromJson)
          .toList(growable: false),
      expectation: LiveEditorScenarioExpectation.fromJson(
        reader.requiredObject('expect'),
      ),
      planHash: reader.requiredString('planHash'),
    );
  }

  final String id;
  final String caseId;
  final String description;
  final String initialSource;
  final int activationUtf16;
  final List<LiveEditorScenarioOperation> operations;
  final LiveEditorScenarioExpectation expectation;
  late String planHash;

  String get qualifiedId => '$id/$caseId';

  Map<String, Object?> toJson({bool includeHash = true}) => {
    'schema': 'flark.live-editor-plan/v1',
    'id': id,
    'caseId': caseId,
    'description': description,
    'initialSource': initialSource,
    'activationUtf16': activationUtf16,
    'operations': operations.map((operation) => operation.toJson()).toList(),
    'expect': expectation.toJson(),
    if (includeHash) 'planHash': planHash,
  };

  String computeHash() {
    final canonical = _canonicalJson(toJson(includeHash: false));
    return sha256.convert(utf8.encode(canonical)).toString();
  }

  void _validate() {
    final idPattern = RegExp(r'^[a-z0-9]+(?:-[a-z0-9]+)*$');
    if (!idPattern.hasMatch(id)) {
      throw FormatException('plan.id is not a stable kebab-case id: $id');
    }
    if (!idPattern.hasMatch(caseId)) {
      throw FormatException(
        'plan.caseId is not a stable kebab-case id: $caseId',
      );
    }
    if (description.isEmpty) {
      throw const FormatException('plan.description must not be empty');
    }
    if (operations.isEmpty) {
      throw const FormatException('plan.operations must not be empty');
    }
    _validateUtf16Offset(initialSource, activationUtf16, 'activationUtf16');
    final checkpointIds = <String>{};
    for (final operation in operations) {
      if (operation case LiveEditorCheckpoint()) {
        if (!idPattern.hasMatch(operation.id)) {
          throw FormatException(
            'checkpoint id is not stable kebab-case: ${operation.id}',
          );
        }
        if (!checkpointIds.add(operation.id)) {
          throw FormatException('duplicate checkpoint id: ${operation.id}');
        }
        _validateUtf16Offset(
          operation.source,
          operation.selectionBaseUtf16,
          'checkpoint.${operation.id}.selectionBaseUtf16',
        );
        _validateUtf16Offset(
          operation.source,
          operation.selectionExtentUtf16,
          'checkpoint.${operation.id}.selectionExtentUtf16',
        );
      }
    }
    _validateUtf16Offset(
      expectation.source,
      expectation.selectionBaseUtf16,
      'expect.selectionBaseUtf16',
    );
    _validateUtf16Offset(
      expectation.source,
      expectation.selectionExtentUtf16,
      'expect.selectionExtentUtf16',
    );
  }
}

final class LiveEditorScenarioCompiler {
  const LiveEditorScenarioCompiler();

  List<LiveEditorScenarioPlan> compile(Map<String, Object?> json) {
    final reader = _ObjectReader(json, 'scenario');
    reader.expectKeys(const {
      'schemaVersion',
      'id',
      'description',
      'initialSource',
      'activation',
      'steps',
      'schedules',
      'expect',
      'runnerHints',
    });
    if (reader.requiredInt('schemaVersion') !=
        liveEditorScenarioSchemaVersion) {
      throw FormatException(
        'scenario.schemaVersion must be $liveEditorScenarioSchemaVersion',
      );
    }
    final id = reader.requiredString('id');
    final description = reader.requiredString('description');
    final initialSource = reader.requiredString('initialSource');
    final activation = _compileActivation(
      reader.requiredObject('activation'),
      initialSource,
    );
    final steps = reader.requiredObjectList('steps');
    if (steps.isEmpty) {
      throw FormatException('scenario.steps must not be empty');
    }
    final schedules = reader.requiredObjectList('schedules');
    if (schedules.isEmpty) {
      throw FormatException('scenario.schedules must not be empty');
    }
    final expectation = _compileExpectation(reader.requiredObject('expect'));
    if (reader.optionalObject('runnerHints') case final runnerHints?) {
      _validateRunnerHints(runnerHints);
    }

    final referencedParameters = <String>{};
    for (final step in steps) {
      if (step['type'] == 'scheduleDelay' && step['key'] is String) {
        referencedParameters.add(step['key']! as String);
      }
    }

    final seenCases = <String>{};
    return [
      for (var index = 0; index < schedules.length; index += 1)
        _compileCase(
          id: id,
          description: description,
          initialSource: initialSource,
          activationUtf16: activation,
          steps: steps,
          schedule: schedules[index],
          scheduleIndex: index,
          referencedParameters: referencedParameters,
          expectation: expectation,
          seenCases: seenCases,
        ),
    ];
  }

  int _compileActivation(Map<String, Object?> json, String source) {
    final reader = _ObjectReader(json, 'scenario.activation');
    reader.expectKeys(const {'needle', 'occurrence', 'utf16OffsetInNeedle'});
    final needle = reader.requiredString('needle');
    if (needle.isEmpty) {
      throw FormatException('scenario.activation.needle must not be empty');
    }
    final occurrence = reader.requiredNonNegativeInt('occurrence');
    final offset = reader.requiredNonNegativeInt('utf16OffsetInNeedle');
    _validateUtf16Offset(needle, offset, 'activation.utf16OffsetInNeedle');
    var start = -1;
    var searchFrom = 0;
    for (var index = 0; index <= occurrence; index += 1) {
      start = source.indexOf(needle, searchFrom);
      if (start < 0) {
        throw FormatException(
          'scenario.activation occurrence $occurrence was not found',
        );
      }
      searchFrom = start + needle.length;
    }
    return start + offset;
  }

  LiveEditorScenarioExpectation _compileExpectation(Map<String, Object?> json) {
    final reader = _ObjectReader(json, 'scenario.expect');
    reader.expectKeys(const {
      'source',
      'caretUtf16',
      'resyncCount',
      'faulted',
      'forbiddenSurfaceSubstrings',
      'paintedSurfaceNeverContains',
    });
    final caret = reader.requiredNonNegativeInt('caretUtf16');
    return LiveEditorScenarioExpectation(
      source: reader.requiredString('source'),
      selectionBaseUtf16: caret,
      selectionExtentUtf16: caret,
      resyncCount: reader.requiredNonNegativeInt('resyncCount'),
      faulted: reader.requiredBool('faulted'),
      settledPresentationNeverContains: reader.requiredStringList(
        'forbiddenSurfaceSubstrings',
      ),
      paintedPresentationNeverContains:
          reader.optionalStringList('paintedSurfaceNeverContains') ?? const [],
    );
  }

  LiveEditorScenarioPlan _compileCase({
    required String id,
    required String description,
    required String initialSource,
    required int activationUtf16,
    required List<Map<String, Object?>> steps,
    required Map<String, Object?> schedule,
    required int scheduleIndex,
    required Set<String> referencedParameters,
    required LiveEditorScenarioExpectation expectation,
    required Set<String> seenCases,
  }) {
    final reader = _ObjectReader(
      schedule,
      'scenario.schedules[$scheduleIndex]',
    );
    final caseId = reader.requiredString('id');
    if (!seenCases.add(caseId)) {
      throw FormatException('duplicate scenario case id: $caseId');
    }
    reader.expectKeys({'id', ...referencedParameters});
    final parameters = {
      for (final parameter in referencedParameters)
        parameter: reader.requiredNonNegativeInt(parameter),
    };
    final operations = <LiveEditorScenarioOperation>[];
    for (var index = 0; index < steps.length; index += 1) {
      operations.add(
        _compileStep(steps[index], index: index, parameters: parameters),
      );
    }
    if (operations.last is! LiveEditorAwait) {
      operations.add(
        const LiveEditorAwait(barrier: LiveEditorScenarioBarrier.editSettled),
      );
    }
    return LiveEditorScenarioPlan(
      id: id,
      caseId: caseId,
      description: description,
      initialSource: initialSource,
      activationUtf16: activationUtf16,
      operations: operations,
      expectation: expectation,
    );
  }

  LiveEditorScenarioOperation _compileStep(
    Map<String, Object?> json, {
    required int index,
    required Map<String, int> parameters,
  }) {
    final reader = _ObjectReader(json, 'scenario.steps[$index]');
    final type = reader.requiredString('type');
    switch (type) {
      case 'typeText':
        reader.expectKeys(const {'type', 'text', 'intervalMs'});
        return LiveEditorInsertText(
          text: reader.requiredString('text'),
          cadence: Duration(
            milliseconds: reader.optionalNonNegativeInt('intervalMs') ?? 0,
          ),
        );
      case 'pressReturn':
        reader.expectKeys(const {'type'});
        return const LiveEditorKeyOperation(key: LiveEditorScenarioKey.enter);
      case 'pressBackspace':
        reader.expectKeys(const {'type'});
        return const LiveEditorKeyOperation(
          key: LiveEditorScenarioKey.backspace,
        );
      case 'pressDelete':
        reader.expectKeys(const {'type'});
        return const LiveEditorKeyOperation(key: LiveEditorScenarioKey.delete);
      case 'undo':
        reader.expectKeys(const {'type'});
        return const LiveEditorKeyOperation(key: LiveEditorScenarioKey.undo);
      case 'redo':
        reader.expectKeys(const {'type'});
        return const LiveEditorKeyOperation(key: LiveEditorScenarioKey.redo);
      case 'selectSourceRange':
        reader.expectKeys(const {'type', 'baseUtf16', 'extentUtf16'});
        return LiveEditorSelectSourceRange(
          baseUtf16: reader.requiredNonNegativeInt('baseUtf16'),
          extentUtf16: reader.requiredNonNegativeInt('extentUtf16'),
        );
      case 'pasteText':
        reader.expectKeys(const {'type', 'text'});
        return LiveEditorPasteText(text: reader.requiredString('text'));
      case 'toggleTaskAtUtf16':
        reader.expectKeys(const {'type', 'targetUtf16'});
        return LiveEditorToggleTaskAtUtf16(
          targetUtf16: reader.requiredNonNegativeInt('targetUtf16'),
        );
      case 'scheduleDelay':
        reader.expectKeys(const {'type', 'key'});
        final key = reader.requiredString('key');
        final milliseconds = parameters[key];
        if (milliseconds == null) {
          throw FormatException('${reader.path}.key is not scheduled: $key');
        }
        return LiveEditorPause(duration: Duration(milliseconds: milliseconds));
      case 'waitForIdle':
        reader.expectKeys(const {'type'});
        return const LiveEditorAwait(
          barrier: LiveEditorScenarioBarrier.editSettled,
        );
      case 'checkpoint':
        reader.expectKeys(const {
          'type',
          'id',
          'source',
          'caretUtf16',
          'selectionBaseUtf16',
          'selectionExtentUtf16',
        });
        final caret = reader.optionalNonNegativeInt('caretUtf16');
        final base = reader.optionalNonNegativeInt('selectionBaseUtf16');
        final extent = reader.optionalNonNegativeInt('selectionExtentUtf16');
        if (caret == null && (base == null || extent == null)) {
          throw FormatException(
            '${reader.path} requires caretUtf16 or both selection offsets',
          );
        }
        if (caret != null && (base != null || extent != null)) {
          throw FormatException(
            '${reader.path} cannot mix caret and ranged selection',
          );
        }
        return LiveEditorCheckpoint(
          id: reader.requiredString('id'),
          source: reader.requiredString('source'),
          selectionBaseUtf16: caret ?? base!,
          selectionExtentUtf16: caret ?? extent!,
        );
      default:
        throw FormatException('${reader.path}.type is unsupported: $type');
    }
  }

  void _validateRunnerHints(Map<String, Object?> json) {
    final reader = _ObjectReader(json, 'scenario.runnerHints');
    reader.expectKeys(const {'macos'});
    final macos = _ObjectReader(
      reader.requiredObject('macos'),
      'scenario.runnerHints.macos',
    );
    macos.expectKeys(const {
      'windowWidth',
      'windowHeight',
      'activationX',
      'activationY',
    });
    for (final key in const [
      'windowWidth',
      'windowHeight',
      'activationX',
      'activationY',
    ]) {
      macos.requiredNonNegativeInt(key);
    }
  }
}

String encodeLiveEditorScenarioPlanBundle(
  Iterable<LiveEditorScenarioPlan> plans,
) => _canonicalJson({
  'schema': 'flark.live-editor-plan-bundle/v1',
  'plans': plans.map((plan) => plan.toJson()).toList(),
});

List<LiveEditorScenarioPlan> decodeLiveEditorScenarioPlanBundle(String input) {
  final decoded = jsonDecode(input);
  if (decoded is! Map<String, Object?>) {
    throw const FormatException('scenario plan bundle must be an object');
  }
  final reader = _ObjectReader(decoded, 'bundle');
  reader.expectKeys(const {'schema', 'plans'});
  if (reader.requiredString('schema') != 'flark.live-editor-plan-bundle/v1') {
    throw const FormatException('unsupported scenario plan bundle schema');
  }
  return reader
      .requiredObjectList('plans')
      .map(LiveEditorScenarioPlan.fromJson)
      .toList(growable: false);
}

T _enumByName<T extends Enum>(List<T> values, String name, String path) {
  for (final value in values) {
    if (value.name == name) return value;
  }
  throw FormatException('$path is unsupported: $name');
}

void _validateUtf16Offset(String text, int offset, String path) {
  if (offset < 0 || offset > text.length) {
    throw FormatException('$path is outside 0..${text.length}: $offset');
  }
  if (offset > 0 && offset < text.length) {
    final previous = text.codeUnitAt(offset - 1);
    final next = text.codeUnitAt(offset);
    if (previous >= 0xD800 &&
        previous <= 0xDBFF &&
        next >= 0xDC00 &&
        next <= 0xDFFF) {
      throw FormatException('$path splits a UTF-16 surrogate pair: $offset');
    }
  }
}

String _canonicalJson(Object? value) {
  Object? canonicalize(Object? candidate) {
    if (candidate is Map) {
      final keys = candidate.keys.cast<String>().toList()..sort();
      return <String, Object?>{
        for (final key in keys) key: canonicalize(candidate[key]),
      };
    }
    if (candidate is List) return candidate.map(canonicalize).toList();
    return candidate;
  }

  return jsonEncode(canonicalize(value));
}

final class _ObjectReader {
  const _ObjectReader(this.value, this.path);

  final Map<String, Object?> value;
  final String path;

  void expectKeys(Set<String> allowed) {
    final unknown = value.keys.where((key) => !allowed.contains(key)).toList();
    if (unknown.isNotEmpty) {
      throw FormatException(
        '$path contains unknown fields: ${unknown.join(', ')}',
      );
    }
  }

  Object? _required(String key) {
    if (!value.containsKey(key)) {
      throw FormatException('$path.$key is required');
    }
    return value[key];
  }

  String requiredString(String key) {
    final candidate = _required(key);
    if (candidate is! String) {
      throw FormatException('$path.$key must be a string');
    }
    return candidate;
  }

  int requiredInt(String key) {
    final candidate = _required(key);
    if (candidate is! int) {
      throw FormatException('$path.$key must be an integer');
    }
    return candidate;
  }

  int requiredNonNegativeInt(String key) {
    final candidate = requiredInt(key);
    if (candidate < 0) throw FormatException('$path.$key must be non-negative');
    return candidate;
  }

  int? optionalNonNegativeInt(String key) {
    if (!value.containsKey(key)) return null;
    return requiredNonNegativeInt(key);
  }

  bool requiredBool(String key) {
    final candidate = _required(key);
    if (candidate is! bool) {
      throw FormatException('$path.$key must be a boolean');
    }
    return candidate;
  }

  Map<String, Object?> requiredObject(String key) {
    final candidate = _required(key);
    if (candidate is! Map<String, Object?>) {
      throw FormatException('$path.$key must be an object');
    }
    return candidate;
  }

  Map<String, Object?>? optionalObject(String key) {
    if (!value.containsKey(key)) return null;
    return requiredObject(key);
  }

  List<Map<String, Object?>> requiredObjectList(String key) {
    final candidate = _required(key);
    if (candidate is! List) {
      throw FormatException('$path.$key must be an array');
    }
    return [
      for (var index = 0; index < candidate.length; index += 1)
        if (candidate[index] is Map<String, Object?>)
          candidate[index]! as Map<String, Object?>
        else
          throw FormatException('$path.$key[$index] must be an object'),
    ];
  }

  List<String> requiredStringList(String key) {
    final candidate = _required(key);
    if (candidate is! List) {
      throw FormatException('$path.$key must be an array');
    }
    return [
      for (var index = 0; index < candidate.length; index += 1)
        if (candidate[index] is String)
          candidate[index]! as String
        else
          throw FormatException('$path.$key[$index] must be a string'),
    ];
  }

  List<String>? optionalStringList(String key) {
    if (!value.containsKey(key)) return null;
    return requiredStringList(key);
  }
}
