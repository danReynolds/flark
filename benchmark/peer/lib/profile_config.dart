import 'dart:io';

import 'profile_fixture.dart';

enum CompetitorScenario {
  coldOpen('cold-open'),
  sustainedTyping('sustained-typing'),
  localInsertDelete('local-insert-delete'),
  paste32Kib('paste-32kib');

  const CompetitorScenario(this.wireName);
  final String wireName;

  static CompetitorScenario parse(String value) => values.firstWhere(
    (candidate) => candidate.wireName == value,
    orElse: () => throw FormatException('Unknown scenario: $value'),
  );
}

enum EditLocation {
  start,
  middle,
  end;

  static EditLocation parse(String value) => values.firstWhere(
    (candidate) => candidate.name == value,
    orElse: () => throw FormatException('Unknown edit location: $value'),
  );
}

final class ProfileConfig {
  const ProfileConfig({
    required this.protocolId,
    required this.scenario,
    required this.targetBytes,
    required this.sizeTierId,
    required this.location,
    required this.runIndex,
    required this.orderIndex,
    required this.processRunId,
    required this.outputPath,
    required this.exportPath,
    required this.nonClaimRun,
    required this.typingWarmups,
    required this.typingSamples,
    required this.typingCadenceHz,
    required this.localWarmupPairs,
    required this.localSamplePairs,
    required this.pasteWarmups,
    required this.pasteSamples,
    required this.inputTimeoutSeconds,
  });

  factory ProfileConfig.fromEnvironment(Map<String, String> environment) {
    const compiledProtocolId = String.fromEnvironment(
      'COMPETITOR_PROTOCOL_ID',
      defaultValue: 'missing',
    );
    final scenario = CompetitorScenario.parse(
      _required(environment, 'COMPETITOR_SCENARIO'),
    );
    final targetBytes = int.parse(
      _required(environment, 'COMPETITOR_TARGET_BYTES'),
    );
    final nonClaimRun = environment['COMPETITOR_NONCLAIM_RUN'] == '1';
    const fixedTiers = <int, String>{
      1048576: '1mib',
      5242880: '5mib',
      10485760: '10mib',
    };
    if (!nonClaimRun && !fixedTiers.containsKey(targetBytes)) {
      throw FormatException(
        'Claim-mode target must be 1, 5, or 10 MiB; got $targetBytes',
      );
    }
    if (!nonClaimRun && compiledProtocolId != competitorProtocolId) {
      throw StateError(
        'Runner was compiled for "$compiledProtocolId", not '
        '"$competitorProtocolId"',
      );
    }
    final inputTimeoutSeconds = _count(
      environment,
      'COMPETITOR_INPUT_TIMEOUT_SECONDS',
      60,
    );
    if (!nonClaimRun && inputTimeoutSeconds != 60) {
      throw const FormatException(
        'Claim-mode input timeout must be 60 seconds',
      );
    }

    return ProfileConfig(
      protocolId: compiledProtocolId,
      scenario: scenario,
      targetBytes: targetBytes,
      sizeTierId: fixedTiers[targetBytes] ?? 'nonclaim-${targetBytes}b',
      location: EditLocation.parse(
        environment['COMPETITOR_LOCATION'] ?? 'start',
      ),
      runIndex: int.parse(environment['COMPETITOR_RUN_INDEX'] ?? '0'),
      orderIndex: int.parse(environment['COMPETITOR_ORDER_INDEX'] ?? '0'),
      processRunId: environment['COMPETITOR_PROCESS_RUN_ID'],
      outputPath: environment['COMPETITOR_OUTPUT_PATH'],
      exportPath: environment['COMPETITOR_EXPORT_PATH'],
      nonClaimRun: nonClaimRun,
      typingWarmups: _count(environment, 'COMPETITOR_TYPING_WARMUPS', 20),
      typingSamples: _count(environment, 'COMPETITOR_TYPING_SAMPLES', 200),
      typingCadenceHz: _count(environment, 'COMPETITOR_TYPING_HZ', 10),
      localWarmupPairs: _count(
        environment,
        'COMPETITOR_LOCAL_WARMUP_PAIRS',
        10,
      ),
      localSamplePairs: _count(
        environment,
        'COMPETITOR_LOCAL_SAMPLE_PAIRS',
        100,
      ),
      pasteWarmups: _count(environment, 'COMPETITOR_PASTE_WARMUPS', 2),
      pasteSamples: _count(environment, 'COMPETITOR_PASTE_SAMPLES', 20),
      inputTimeoutSeconds: inputTimeoutSeconds,
    );
  }

  final String protocolId;
  final CompetitorScenario scenario;
  final int targetBytes;
  final String sizeTierId;
  final EditLocation location;
  final int runIndex;
  final int orderIndex;
  final String? processRunId;
  final String? outputPath;
  final String? exportPath;
  final bool nonClaimRun;
  final int typingWarmups;
  final int typingSamples;
  final int typingCadenceHz;
  final int localWarmupPairs;
  final int localSamplePairs;
  final int pasteWarmups;
  final int pasteSamples;
  final int inputTimeoutSeconds;

  Duration get inputTimeout => Duration(seconds: inputTimeoutSeconds);

  bool get usesProtocolCounts =>
      typingWarmups == 20 &&
      typingSamples == 200 &&
      typingCadenceHz == 10 &&
      localWarmupPairs == 10 &&
      localSamplePairs == 100 &&
      pasteWarmups == 2 &&
      pasteSamples == 20 &&
      inputTimeoutSeconds == 60;

  bool get completionEnvelopeConfigurationEligible =>
      !nonClaimRun &&
      protocolId == competitorProtocolId &&
      usesProtocolCounts &&
      processRunId != null &&
      processRunId!.isNotEmpty &&
      outputPath != null &&
      outputPath!.isNotEmpty &&
      exportPath != null &&
      exportPath!.isNotEmpty;

  Map<String, Object?> toJson() => <String, Object?>{
    'protocolId': protocolId,
    'scenario': scenario.wireName,
    'targetBytes': targetBytes,
    'sizeTierId': sizeTierId,
    'location': location.name,
    'runIndex': runIndex,
    'orderIndex': orderIndex,
    'processRunId': processRunId,
    'nonClaimRun': nonClaimRun,
    'typingWarmups': typingWarmups,
    'typingSamples': typingSamples,
    'typingCadenceHz': typingCadenceHz,
    'localWarmupPairs': localWarmupPairs,
    'localSamplePairs': localSamplePairs,
    'pasteWarmups': pasteWarmups,
    'pasteSamples': pasteSamples,
    'inputTimeoutSeconds': inputTimeoutSeconds,
    'completionEnvelopeConfigurationEligible':
        completionEnvelopeConfigurationEligible,
  };
}

String _required(Map<String, String> environment, String key) {
  final value = environment[key];
  if (value == null || value.isEmpty) {
    throw FormatException('Missing required environment variable $key');
  }
  return value;
}

int _count(Map<String, String> environment, String key, int defaultValue) {
  final parsed = int.parse(environment[key] ?? '$defaultValue');
  if (parsed < 0) {
    throw FormatException('$key must be nonnegative; got $parsed');
  }
  return parsed;
}

ProfileConfig loadProfileConfig() =>
    ProfileConfig.fromEnvironment(Platform.environment);
