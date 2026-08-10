import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

const peerSuiteProtocolId = 'm0-mac-competitor-profile-v1';
const peerSuiteId = 'm0-mac-two-peer-suite-v1';
const peerSuiteSchemaVersion = 1;
const protocolIdleSeconds = 300;
const protocolProcessCount = 234;

const protocolPeers = <String>['flutter_quill', 'super_editor'];
const protocolSizes = <int>[1048576, 5242880, 10485760];
const protocolLocations = <String>['start', 'middle', 'end'];
const protocolWorkloads = <String>[
  'cold-open',
  'sustained-typing',
  'local-insert-delete',
  'paste-32kib',
];

const frozenOrdinaryProseCycle =
    'Ordinary prose opens with a clear sentence and a small **bold** run.\n'
    'It continues with _emphasis_, `code`, and a direct '
    '[link](https://example.invalid/).\n\n';
const frozenTypingCycle = 'abcdefghijklmnopqrstuvwxyz0123456789';

final _frozenFixtureCache = <int, String>{};

String frozenOrdinaryProseExact(int targetBytes) =>
    _frozenFixtureCache.putIfAbsent(targetBytes, () {
      if (targetBytes < 0) {
        throw ArgumentError.value(targetBytes, 'targetBytes');
      }
      final cycles = targetBytes ~/ frozenOrdinaryProseCycle.length;
      final remainder = targetBytes % frozenOrdinaryProseCycle.length;
      return List.filled(cycles, frozenOrdinaryProseCycle).join() +
          frozenOrdinaryProseCycle.substring(0, remainder);
    });

String frozenExpectedFinalSource(PeerSuiteEntry entry, {required String peer}) {
  if (!protocolPeers.contains(peer)) {
    throw ArgumentError.value(peer, 'peer');
  }
  final fixture = frozenOrdinaryProseExact(entry.targetBytes);
  String insertAt(String payload) {
    final offset = switch (entry.location) {
      'start' => 0,
      'middle' => fixture.length ~/ 2,
      'end' => fixture.length,
      _ => throw StateError('Unknown location ${entry.location}'),
    };
    return '${fixture.substring(0, offset)}$payload'
        '${fixture.substring(offset)}';
  }

  if (entry.workload == 'sustained-typing') {
    final typed = List.generate(
      220,
      (index) => frozenTypingCycle[index % frozenTypingCycle.length],
    ).join();
    return insertAt(typed);
  }
  return fixture;
}

/// The next competitor-derived probe is mechanical and remains distinct from
/// Flark's fixed 10 MiB product target.
int? nextCompetitorTierBytes(int? completedTierBytes) =>
    switch (completedTierBytes) {
      1048576 => 5242880,
      5242880 => 10485760,
      10485760 => 20971520,
      _ => null,
    };

final class PeerSuiteEntry {
  const PeerSuiteEntry({
    required this.id,
    required this.groupIndex,
    required this.orderSlot,
    required this.peer,
    required this.workload,
    required this.targetBytes,
    required this.location,
    required this.replicate,
  });

  factory PeerSuiteEntry.fromJson(Object? value) {
    final json = _map(value, 'plan entry');
    return PeerSuiteEntry(
      id: _string(json, 'id'),
      groupIndex: _int(json, 'groupIndex'),
      orderSlot: _int(json, 'orderSlot'),
      peer: _string(json, 'peer'),
      workload: _string(json, 'workload'),
      targetBytes: _int(json, 'targetBytes'),
      location: _string(json, 'location'),
      replicate: _int(json, 'replicate'),
    );
  }

  final String id;
  final int groupIndex;
  final int orderSlot;
  final String peer;
  final String workload;
  final int targetBytes;
  final String location;
  final int replicate;

  String get caseKey =>
      '$groupIndex:$workload:$targetBytes:$location:$replicate';

  Map<String, Object?> toJson() => <String, Object?>{
    'id': id,
    'groupIndex': groupIndex,
    'orderSlot': orderSlot,
    'peer': peer,
    'workload': workload,
    'targetBytes': targetBytes,
    'location': location,
    'replicate': replicate,
  };
}

final class PeerSuitePlan {
  const PeerSuitePlan({required this.entries});

  factory PeerSuitePlan.protocol() {
    final entries = <PeerSuiteEntry>[];
    var globalSlot = 0;
    for (var group = 0; group < 3; group += 1) {
      final sizes = <int>[
        ...protocolSizes.skip(group),
        ...protocolSizes.take(group),
      ];
      final cases = <_PeerCase>[];

      // Thirty cold starts per peer and tier, split evenly over the three
      // independently idled run groups.
      for (var withinGroup = 0; withinGroup < 10; withinGroup += 1) {
        final replicate = group * 10 + withinGroup;
        final rotatedSizes = <int>[
          ...sizes.skip(withinGroup % sizes.length),
          ...sizes.take(withinGroup % sizes.length),
        ];
        for (final size in rotatedSizes) {
          cases.add(
            _PeerCase(
              workload: 'cold-open',
              targetBytes: size,
              location: 'middle',
              replicate: replicate,
            ),
          );
        }
      }

      // One typing run per group is the protocol's three-run denominator.
      for (final size in sizes) {
        cases.add(
          _PeerCase(
            workload: 'sustained-typing',
            targetBytes: size,
            location: 'end',
            replicate: group,
          ),
        );
      }

      // The three groups rotate start/middle/end. Thus each size and peer gets
      // one fresh process at every required location for both local and paste.
      for (final workload in const ['local-insert-delete', 'paste-32kib']) {
        for (final size in sizes) {
          cases.add(
            _PeerCase(
              workload: workload,
              targetBytes: size,
              location: protocolLocations[group],
              replicate: group,
            ),
          );
        }
      }

      for (var caseIndex = 0; caseIndex < cases.length; caseIndex += 1) {
        final peerOrder = (caseIndex + group).isEven
            ? protocolPeers
            : protocolPeers.reversed.toList(growable: false);
        for (final peer in peerOrder) {
          final current = cases[caseIndex];
          final id =
              'g$group-${globalSlot.toString().padLeft(3, '0')}-'
              '${current.workload}-${current.targetBytes}b-'
              '${current.location}-r${current.replicate}-$peer';
          entries.add(
            PeerSuiteEntry(
              id: id,
              groupIndex: group,
              orderSlot: globalSlot,
              peer: peer,
              workload: current.workload,
              targetBytes: current.targetBytes,
              location: current.location,
              replicate: current.replicate,
            ),
          );
          globalSlot += 1;
        }
      }
    }
    return PeerSuitePlan(entries: entries);
  }

  factory PeerSuitePlan.fromJson(Object? value) {
    final json = _map(value, 'suite plan');
    return PeerSuitePlan(
      entries: _list(
        json,
        'entries',
      ).map(PeerSuiteEntry.fromJson).toList(growable: false),
    );
  }

  final List<PeerSuiteEntry> entries;

  String get sha256 => sha256Text(canonicalJson(toJson()));

  Map<String, Object?> toJson() => <String, Object?>{
    'schemaVersion': peerSuiteSchemaVersion,
    'suiteId': peerSuiteId,
    'protocolId': peerSuiteProtocolId,
    'processCount': entries.length,
    'runGroupCount': 3,
    'idleSecondsBeforeEachRunGroup': protocolIdleSeconds,
    'orderPolicy':
        'three-row size Latin square; adjacent peer pair per exact case; '
        'peer-first alternates by case and group',
    'entries': entries.map((entry) => entry.toJson()).toList(),
  };
}

final class PeerProcessEvidence {
  const PeerProcessEvidence({
    required this.evidenceId,
    required this.planEntryId,
    required this.processId,
    required this.startedAtUtc,
    required this.finishedAtUtc,
    required this.exitCode,
    required this.timedOut,
    required this.argv,
    required this.cwd,
    required this.environmentOverrides,
    required this.resultPath,
    required this.resultSha256,
    required this.stdoutPath,
    required this.stdoutSha256,
    required this.stderrPath,
    required this.stderrSha256,
  });

  factory PeerProcessEvidence.fromJson(Object? value) {
    final json = _map(value, 'process evidence');
    return PeerProcessEvidence(
      evidenceId: _string(json, 'evidenceId'),
      planEntryId: _string(json, 'planEntryId'),
      processId: _int(json, 'processId'),
      startedAtUtc: DateTime.parse(_string(json, 'startedAtUtc')).toUtc(),
      finishedAtUtc: DateTime.parse(_string(json, 'finishedAtUtc')).toUtc(),
      exitCode: _int(json, 'exitCode'),
      timedOut: _bool(json, 'timedOut'),
      argv: _list(json, 'argv').map((value) => '$value').toList(),
      cwd: _string(json, 'cwd'),
      environmentOverrides: _map(
        json['environmentOverrides'],
        'environmentOverrides',
      ).map((key, value) => MapEntry(key, '$value')),
      resultPath: _string(json, 'resultPath'),
      resultSha256: _nullableString(json['resultSha256']),
      stdoutPath: _string(json, 'stdoutPath'),
      stdoutSha256: _nullableString(json['stdoutSha256']),
      stderrPath: _string(json, 'stderrPath'),
      stderrSha256: _nullableString(json['stderrSha256']),
    );
  }

  final String evidenceId;
  final String planEntryId;
  final int processId;
  final DateTime startedAtUtc;
  final DateTime finishedAtUtc;
  final int exitCode;
  final bool timedOut;
  final List<String> argv;
  final String cwd;
  final Map<String, String> environmentOverrides;
  final String resultPath;
  final String? resultSha256;
  final String stdoutPath;
  final String? stdoutSha256;
  final String stderrPath;
  final String? stderrSha256;

  Map<String, Object?> toJson() => <String, Object?>{
    'evidenceId': evidenceId,
    'planEntryId': planEntryId,
    'processId': processId,
    'startedAtUtc': startedAtUtc.toIso8601String(),
    'finishedAtUtc': finishedAtUtc.toIso8601String(),
    'exitCode': exitCode,
    'timedOut': timedOut,
    'argv': argv,
    'cwd': cwd,
    'environmentOverrides': environmentOverrides,
    'resultPath': resultPath,
    'resultSha256': resultSha256,
    'stdoutPath': stdoutPath,
    'stdoutSha256': stdoutSha256,
    'stderrPath': stderrPath,
    'stderrSha256': stderrSha256,
  };
}

final class RunGroupEvidence {
  const RunGroupEvidence({
    required this.groupIndex,
    required this.idleStartedAtUtc,
    required this.idleFinishedAtUtc,
    required this.firstProcessStartedAtUtc,
    required this.lastProcessFinishedAtUtc,
  });

  factory RunGroupEvidence.fromJson(Object? value) {
    final json = _map(value, 'run-group evidence');
    DateTime parse(String key) => DateTime.parse(_string(json, key)).toUtc();
    return RunGroupEvidence(
      groupIndex: _int(json, 'groupIndex'),
      idleStartedAtUtc: parse('idleStartedAtUtc'),
      idleFinishedAtUtc: parse('idleFinishedAtUtc'),
      firstProcessStartedAtUtc: parse('firstProcessStartedAtUtc'),
      lastProcessFinishedAtUtc: parse('lastProcessFinishedAtUtc'),
    );
  }

  final int groupIndex;
  final DateTime idleStartedAtUtc;
  final DateTime idleFinishedAtUtc;
  final DateTime firstProcessStartedAtUtc;
  final DateTime lastProcessFinishedAtUtc;

  int get observedIdleMillis =>
      idleFinishedAtUtc.difference(idleStartedAtUtc).inMilliseconds;

  Map<String, Object?> toJson() => <String, Object?>{
    'groupIndex': groupIndex,
    'idleStartedAtUtc': idleStartedAtUtc.toIso8601String(),
    'idleFinishedAtUtc': idleFinishedAtUtc.toIso8601String(),
    'observedIdleMillis': observedIdleMillis,
    'firstProcessStartedAtUtc': firstProcessStartedAtUtc.toIso8601String(),
    'lastProcessFinishedAtUtc': lastProcessFinishedAtUtc.toIso8601String(),
  };
}

final class PeerSuiteAssessment {
  const PeerSuiteAssessment({
    required this.completionEnvelopeEligible,
    required this.completionEnvelopeBlockers,
    required this.performanceClaimEligible,
    required this.performanceClaimBlockers,
    required this.completedTierByPeer,
    required this.cohortCompletedTierBytes,
    required this.nextCompetitorTierBytes,
    required this.processesValidated,
  });

  final bool completionEnvelopeEligible;
  final List<String> completionEnvelopeBlockers;
  final bool performanceClaimEligible;
  final List<String> performanceClaimBlockers;
  final Map<String, int?> completedTierByPeer;
  final int? cohortCompletedTierBytes;
  final int? nextCompetitorTierBytes;
  final int processesValidated;

  Map<String, Object?> toJson() => <String, Object?>{
    'completionEnvelopeEligible': completionEnvelopeEligible,
    'completionEnvelopeBlockers': completionEnvelopeBlockers,
    'mayResolveCompetitorDerivedSizeTiers':
        completionEnvelopeEligible && cohortCompletedTierBytes != null,
    'performanceClaimEligible': performanceClaimEligible,
    'performanceClaimBlockers': performanceClaimBlockers,
    // The aggregate resolves only the scoped Flutter SDK boundary, not a
    // public market-wide editor claim.
    'claimEligible': false,
    'claimScope': 'leading-relevant-embeddable-flutter-editor-sdk-cohort-only',
    'completedTierByPeer': completedTierByPeer,
    'cohortCompletedTierBytes': cohortCompletedTierBytes,
    'nextCompetitorTierBytes': nextCompetitorTierBytes,
    'nextTierRule': '1MiB->5MiB, 5MiB->10MiB, 10MiB->20MiB',
    'flarkFixedTenMiBTargetUnaffected': true,
    'processesValidated': processesValidated,
  };
}

final class PeerSuiteValidator {
  const PeerSuiteValidator() : _testFixtures = null;

  /// Keeps adversarial contract tests small. Production callers must use the
  /// default constructor, which always derives the real 1/5/10 MiB bytes from
  /// [frozenOrdinaryProseExact].
  PeerSuiteValidator.testOnly(Map<int, String> fixtures)
    : _testFixtures = Map.unmodifiable(fixtures);

  final Map<int, String>? _testFixtures;

  String _fixtureFor(int bytes) =>
      _testFixtures?[bytes] ?? frozenOrdinaryProseExact(bytes);

  String _expectedFinal(PeerSuiteEntry entry, String peer) {
    if (_testFixtures == null) {
      return frozenExpectedFinalSource(entry, peer: peer);
    }
    final fixture = _fixtureFor(entry.targetBytes);
    final offset = switch (entry.location) {
      'start' => 0,
      'middle' => fixture.length ~/ 2,
      'end' => fixture.length,
      _ => throw StateError('Unknown location ${entry.location}'),
    };
    String insert(String payload) =>
        '${fixture.substring(0, offset)}$payload${fixture.substring(offset)}';
    if (entry.workload == 'sustained-typing') {
      return insert(
        List.generate(
          220,
          (index) => frozenTypingCycle[index % frozenTypingCycle.length],
        ).join(),
      );
    }
    if (!protocolPeers.contains(peer)) throw ArgumentError.value(peer, 'peer');
    return fixture;
  }

  PeerSuiteAssessment validate({
    required PeerSuitePlan plan,
    required List<PeerProcessEvidence> processes,
    required List<RunGroupEvidence> runGroups,
    required bool exclusiveMachineAttested,
    required bool dryRun,
  }) {
    final completion = <String>[];
    final performance = <String>[];
    final canonicalPlan = PeerSuitePlan.protocol();
    _validatePlan(plan, canonicalPlan, completion);

    if (dryRun) {
      completion.add(
        'Dry-run mode launches no peer processes and cannot resolve the '
        'competitor completion envelope.',
      );
      performance.add('Dry-run mode records no profile measurements.');
      return PeerSuiteAssessment(
        completionEnvelopeEligible: false,
        completionEnvelopeBlockers: _dedupe(completion),
        performanceClaimEligible: false,
        performanceClaimBlockers: _dedupe(performance),
        completedTierByPeer: const {
          'flutter_quill': null,
          'super_editor': null,
        },
        cohortCompletedTierBytes: null,
        nextCompetitorTierBytes: null,
        processesValidated: 0,
      );
    }

    if (!exclusiveMachineAttested) {
      completion.add(
        'Exclusive-machine use was not explicitly attested for the suite.',
      );
    }
    performance.add(
      'Cross-process cold-open and input-to-paint p50/p90/p99/max '
      'distributions are not materialized by coordinator v1; performance '
      'claim promotion remains locked.',
    );
    _validateRunGroups(plan, processes, runGroups, completion);

    final entriesById = <String, PeerSuiteEntry>{
      for (final entry in plan.entries) entry.id: entry,
    };
    final evidenceByEntry = <String, PeerProcessEvidence>{};
    final evidenceIds = <String>{};
    final processInstances = <String>{};
    final resultPaths = <String>{};
    final stdoutPaths = <String>{};
    final stderrPaths = <String>{};
    final exportPaths = <String>{};
    final rawEvidencePaths = <String>{};
    final superEditorProfileInstances = <String>{};
    final fixtureHashesBySize = <int, Set<String>>{};
    final pasteStateShapesByCase = <String, Map<String, String>>{};
    final completedCases = <String>{};
    var longestSynchronousSpanMissing = false;

    for (final evidence in processes) {
      final entry = entriesById[evidence.planEntryId];
      if (entry == null) {
        completion.add(
          'Unexpected process evidence for ${evidence.planEntryId}.',
        );
        continue;
      }
      if (evidenceByEntry.containsKey(entry.id)) {
        completion.add('Duplicate process evidence for ${entry.id}.');
      } else {
        evidenceByEntry[entry.id] = evidence;
      }
      _requireUnique(
        evidenceIds,
        evidence.evidenceId,
        'evidence ID',
        entry.id,
        completion,
      );
      _requireUnique(
        processInstances,
        '${evidence.processId}@${evidence.startedAtUtc.toIso8601String()}',
        'process instance',
        entry.id,
        completion,
      );
      _requireUnique(
        resultPaths,
        File(evidence.resultPath).absolute.path,
        'result path',
        entry.id,
        completion,
      );
      _requireUnique(
        stdoutPaths,
        File(evidence.stdoutPath).absolute.path,
        'stdout path',
        entry.id,
        completion,
      );
      _requireUnique(
        stderrPaths,
        File(evidence.stderrPath).absolute.path,
        'stderr path',
        entry.id,
        completion,
      );

      final processCompletion = <String>[];
      final processPerformance = <String>[];
      _validateOuterProcess(evidence, entry, processCompletion);
      final payload = _loadAndValidateResult(
        evidence,
        entry,
        processCompletion,
      );
      if (payload != null) {
        final pasteStateShape = _validatePayload(
          payload: payload,
          evidence: evidence,
          entry: entry,
          processCompletion: processCompletion,
          processPerformance: processPerformance,
          exportPaths: exportPaths,
          rawEvidencePaths: rawEvidencePaths,
          superEditorProfileInstances: superEditorProfileInstances,
          fixtureHashesBySize: fixtureHashesBySize,
        );
        if (pasteStateShape != null) {
          pasteStateShapesByCase.putIfAbsent(
            entry.caseKey,
            () => <String, String>{},
          )[entry.peer] = pasteStateShape;
        }
      }
      if (processCompletion.isEmpty) {
        completedCases.add(entry.id);
      } else {
        completion.addAll(
          processCompletion.map((value) => '${entry.id}: $value'),
        );
      }
      for (final value in processPerformance) {
        if (value.contains('longest synchronous span')) {
          longestSynchronousSpanMissing = true;
        } else {
          performance.add('${entry.id}: $value');
        }
      }
    }

    for (final entry in plan.entries) {
      if (!evidenceByEntry.containsKey(entry.id)) {
        completion.add('Missing process evidence for ${entry.id}.');
      }
    }
    final checkedPasteCases = <String>{};
    for (final entry in plan.entries.where(
      (candidate) => candidate.workload == 'paste-32kib',
    )) {
      if (!checkedPasteCases.add(entry.caseKey)) continue;
      final shapes = pasteStateShapesByCase[entry.caseKey];
      if (shapes == null || !shapes.keys.toSet().containsAll(protocolPeers)) {
        completion.add(
          '${entry.caseKey}: paired paste-state evidence is incomplete.',
        );
      } else if (shapes.values.toSet().length != 1) {
        completion.add(
          '${entry.caseKey}: peer paste-state byte/hash shapes differ.',
        );
      }
    }
    for (final entry in plan.entries) {
      final hashes = fixtureHashesBySize[entry.targetBytes];
      if (hashes != null && hashes.length > 1) {
        completion.add(
          'Fixture SHA-256 differs across peers/runs at '
          '${entry.targetBytes} bytes.',
        );
      }
    }

    final completedTierByPeer = <String, int?>{
      for (final peer in protocolPeers)
        peer: _largestCompletedTier(
          peer: peer,
          plan: plan,
          completedEntryIds: completedCases,
        ),
    };
    final peerTiers = completedTierByPeer.values.whereType<int>().toList();
    final cohortTier = peerTiers.length == protocolPeers.length
        ? peerTiers.reduce((left, right) => left < right ? left : right)
        : null;

    if (completion.isNotEmpty) {
      performance.insert(
        0,
        'The completion envelope is not eligible, so the performance claim '
        'cannot be eligible.',
      );
    }
    if (longestSynchronousSpanMissing) {
      performance.add(
        'Longest synchronous span capture is missing; this blocks a '
        'performance claim but not completion-envelope resolution.',
      );
    }

    return PeerSuiteAssessment(
      completionEnvelopeEligible: completion.isEmpty,
      completionEnvelopeBlockers: _dedupe(completion),
      performanceClaimEligible: completion.isEmpty && performance.isEmpty,
      performanceClaimBlockers: _dedupe(performance),
      completedTierByPeer: completedTierByPeer,
      cohortCompletedTierBytes: cohortTier,
      nextCompetitorTierBytes: nextCompetitorTierBytes(cohortTier),
      processesValidated: evidenceByEntry.length,
    );
  }

  void _validatePlan(
    PeerSuitePlan actual,
    PeerSuitePlan expected,
    List<String> blockers,
  ) {
    if (actual.entries.length != protocolProcessCount) {
      blockers.add(
        'Plan contains ${actual.entries.length} processes; '
        '$protocolProcessCount are required.',
      );
    }
    if (canonicalJson(actual.toJson()) != canonicalJson(expected.toJson())) {
      blockers.add(
        'Plan does not match the frozen three-group Latin-square '
        'interleaving.',
      );
    }
    for (var index = 0; index + 1 < actual.entries.length; index += 2) {
      final first = actual.entries[index];
      final second = actual.entries[index + 1];
      if (first.caseKey != second.caseKey ||
          {first.peer, second.peer}.length != protocolPeers.length) {
        blockers.add(
          'Order slots $index/${index + 1} are not an adjacent two-peer '
          'comparison of one exact case.',
        );
      }
    }
  }

  void _validateRunGroups(
    PeerSuitePlan plan,
    List<PeerProcessEvidence> processes,
    List<RunGroupEvidence> groups,
    List<String> blockers,
  ) {
    final groupIndexes = groups.map((group) => group.groupIndex).toSet();
    if (groups.length != 3 ||
        groupIndexes.length != 3 ||
        !groupIndexes.containsAll(const {0, 1, 2})) {
      blockers.add('Exactly run groups 0, 1, and 2 must be recorded.');
    }
    final entryById = <String, PeerSuiteEntry>{
      for (final entry in plan.entries) entry.id: entry,
    };
    final processByGroup = <int, List<PeerProcessEvidence>>{};
    for (final process in processes) {
      final group = entryById[process.planEntryId]?.groupIndex;
      if (group != null) {
        processByGroup.putIfAbsent(group, () => []).add(process);
      }
    }
    for (final group in groups) {
      if (group.observedIdleMillis < protocolIdleSeconds * 1000) {
        blockers.add(
          'Run group ${group.groupIndex} observed only '
          '${group.observedIdleMillis} ms idle; '
          '${protocolIdleSeconds * 1000} ms are required.',
        );
      }
      if (group.firstProcessStartedAtUtc.isBefore(group.idleFinishedAtUtc)) {
        blockers.add(
          'Run group ${group.groupIndex} started a process before its idle '
          'interval ended.',
        );
      }
      final recorded = processByGroup[group.groupIndex] ?? const [];
      final sorted = [
        ...recorded,
      ]..sort((left, right) => left.startedAtUtc.compareTo(right.startedAtUtc));
      final expectedOrder = plan.entries
          .where((entry) => entry.groupIndex == group.groupIndex)
          .map((entry) => entry.id)
          .toList(growable: false);
      final observedOrder = sorted
          .map((process) => process.planEntryId)
          .toList(growable: false);
      if (canonicalJson(observedOrder) != canonicalJson(expectedOrder)) {
        blockers.add(
          'Run group ${group.groupIndex} process chronology does not match '
          'the recorded Latin-square peer/size order.',
        );
      }
      for (var index = 1; index < sorted.length; index += 1) {
        if (sorted[index].startedAtUtc.isBefore(
          sorted[index - 1].finishedAtUtc,
        )) {
          blockers.add(
            'Run group ${group.groupIndex} has overlapping peer processes.',
          );
          break;
        }
      }
      if (sorted.isNotEmpty) {
        if (sorted.first.startedAtUtc != group.firstProcessStartedAtUtc ||
            sorted.last.finishedAtUtc != group.lastProcessFinishedAtUtc) {
          blockers.add(
            'Run group ${group.groupIndex} boundary timestamps do not match '
            'its process evidence.',
          );
        }
      }
    }
    final chronologicalGroups = [...groups]
      ..sort((left, right) => left.groupIndex.compareTo(right.groupIndex));
    for (var index = 1; index < chronologicalGroups.length; index += 1) {
      final previous = chronologicalGroups[index - 1];
      final current = chronologicalGroups[index];
      if (current.idleStartedAtUtc.isBefore(
        previous.lastProcessFinishedAtUtc,
      )) {
        blockers.add(
          'Run group ${current.groupIndex} idle interval overlaps the prior '
          'group instead of following it.',
        );
      }
    }
  }

  void _validateOuterProcess(
    PeerProcessEvidence evidence,
    PeerSuiteEntry entry,
    List<String> blockers,
  ) {
    if (evidence.exitCode != 0) {
      blockers.add('Profile process exited ${evidence.exitCode}.');
    }
    if (evidence.timedOut) blockers.add('Profile process timed out.');
    if (!evidence.finishedAtUtc.isAfter(evidence.startedAtUtc)) {
      blockers.add('Process timestamps are not strictly ordered.');
    }
    if (evidence.argv.isEmpty || evidence.cwd.isEmpty) {
      blockers.add('Exact argv and cwd were not retained.');
    }
    if (entry.peer == 'flutter_quill') {
      final expected = <String, String>{
        'COMPETITOR_SCENARIO': entry.workload,
        'COMPETITOR_TARGET_BYTES': '${entry.targetBytes}',
        'COMPETITOR_LOCATION': entry.location,
        'COMPETITOR_RUN_INDEX': '${entry.replicate}',
        'COMPETITOR_ORDER_INDEX': '${entry.orderSlot}',
        'COMPETITOR_PROCESS_RUN_ID': entry.id,
        'COMPETITOR_OUTPUT_PATH': evidence.resultPath,
      };
      for (final field in expected.entries) {
        if (evidence.environmentOverrides[field.key] != field.value) {
          blockers.add(
            'Quill environment override ${field.key} is missing or does not '
            'match the plan.',
          );
        }
      }
      if ((evidence.environmentOverrides['COMPETITOR_EXPORT_PATH'] ?? '')
          .isEmpty) {
        blockers.add('Quill export-path environment override is missing.');
      }
    }
    _validateHashedFile(
      evidence.stdoutPath,
      evidence.stdoutSha256,
      'stdout',
      blockers,
    );
    _validateHashedFile(
      evidence.stderrPath,
      evidence.stderrSha256,
      'stderr',
      blockers,
    );
  }

  Map<String, Object?>? _loadAndValidateResult(
    PeerProcessEvidence evidence,
    PeerSuiteEntry entry,
    List<String> blockers,
  ) {
    if (!_validateHashedFile(
      evidence.resultPath,
      evidence.resultSha256,
      'result receipt',
      blockers,
    )) {
      return null;
    }
    try {
      return _map(
        jsonDecode(File(evidence.resultPath).readAsStringSync()),
        'result receipt',
      );
    } catch (error) {
      blockers.add('Result receipt is not valid JSON: $error');
      return null;
    }
  }

  String? _validatePayload({
    required Map<String, Object?> payload,
    required PeerProcessEvidence evidence,
    required PeerSuiteEntry entry,
    required List<String> processCompletion,
    required List<String> processPerformance,
    required Set<String> exportPaths,
    required Set<String> rawEvidencePaths,
    required Set<String> superEditorProfileInstances,
    required Map<int, Set<String>> fixtureHashesBySize,
  }) {
    if (payload['peer'] != entry.peer) {
      processCompletion.add(
        'Receipt peer ${payload['peer']} does not match ${entry.peer}.',
      );
    }
    if (payload['claimEligible'] != false) {
      processCompletion.add(
        'Peer-local claimEligible must remain false; only the coordinator '
        'can assess the cohort.',
      );
    }
    if (payload['performanceClaimEligible'] == true) {
      processCompletion.add(
        'Peer-local performanceClaimEligible must remain false.',
      );
    }
    final config = _optionalMap(payload['config']);
    final workload = config?['scenario'] ?? config?['workload'];
    if (config == null ||
        config['protocolId'] != peerSuiteProtocolId ||
        workload != entry.workload ||
        config['targetBytes'] != entry.targetBytes ||
        config['location'] != entry.location) {
      processCompletion.add('Receipt configuration does not match the plan.');
    }
    if (entry.peer == 'flutter_quill') {
      if (config?['runIndex'] != entry.replicate ||
          config?['orderIndex'] != entry.orderSlot ||
          config?['processRunId'] != entry.id) {
        processCompletion.add(
          'Quill run index, order slot, or process run ID is not bound to '
          'the suite plan.',
        );
      }
      if (payload['completionEnvelopeEligible'] != true) {
        processCompletion.add(
          'Quill process completion envelope is ineligible.',
        );
      }
      if (config?['nonClaimRun'] != false ||
          config?['typingWarmups'] != 20 ||
          config?['typingSamples'] != 200 ||
          config?['typingCadenceHz'] != 10 ||
          config?['localWarmupPairs'] != 10 ||
          config?['localSamplePairs'] != 100 ||
          config?['pasteWarmups'] != 2 ||
          config?['pasteSamples'] != 20 ||
          config?['inputTimeoutSeconds'] != 60 ||
          config?['completionEnvelopeConfigurationEligible'] != true) {
        processCompletion.add(
          'Quill receipt does not retain the exact protocol counts and '
          '60-second liveness timeout.',
        );
      }
    } else if (entry.peer == 'super_editor') {
      if (payload['protocolConformant'] != true ||
          payload['profileMode'] != true ||
          payload['completion'] != 'complete') {
        processCompletion.add(
          'SuperEditor process did not complete a conformant profile run.',
        );
      }
      final driver = _optionalMap(payload['driver']);
      final invocation = _optionalMap(driver?['invocation']);
      final runControl = _optionalMap(driver?['runControl']);
      if (driver?['watchdogTimedOut'] != false ||
          invocation?['runId'] != entry.id ||
          runControl?['runGroupId'] != 'group-${entry.groupIndex}' ||
          '${runControl?['orderSlot']}' != '${entry.orderSlot}') {
        processCompletion.add(
          'SuperEditor driver evidence is not bound to the suite group and '
          'order slot.',
        );
      }
      final profileProcessId = driver?['processId'];
      final profileLaunch = driver?['processLaunchRequestedAtUtc'];
      if (profileProcessId is! int || profileLaunch is! String) {
        processCompletion.add(
          'SuperEditor fresh profile-process identity is missing.',
        );
      } else {
        _requireUnique(
          superEditorProfileInstances,
          '$profileProcessId@$profileLaunch',
          'SuperEditor profile-process instance',
          entry.id,
          processCompletion,
        );
      }
      final expectedCounts = switch (entry.workload) {
        'cold-open' => (warmups: 0, samples: 1, cadenceMillis: 0),
        'sustained-typing' => (warmups: 20, samples: 200, cadenceMillis: 100),
        'local-insert-delete' => (warmups: 10, samples: 100, cadenceMillis: 0),
        'paste-32kib' => (warmups: 2, samples: 20, cadenceMillis: 0),
        _ => throw StateError('Unknown workload ${entry.workload}'),
      };
      if (config?['warmupCount'] != expectedCounts.warmups ||
          config?['sampleCount'] != expectedCounts.samples ||
          config?['cadenceMillis'] != expectedCounts.cadenceMillis ||
          config?['timeoutMicros'] != 60000000 ||
          (entry.workload == 'paste-32kib' && config?['pasteBytes'] != 32768)) {
        processCompletion.add(
          'SuperEditor receipt does not retain the exact workload counts and '
          '60-second liveness timeout.',
        );
      }
    }

    final fixture = _optionalMap(payload['fixture']);
    final fixtureHash = fixture?['sha256'];
    final expectedFixtureHash = sha256Text(_fixtureFor(entry.targetBytes));
    if (fixture == null ||
        fixture['generatorId'] != 'flark-v4-deterministic-markdown-v1' ||
        fixture['shapeId'] != 'ordinary-prose' ||
        fixture['encoding'] != 'UTF-8' ||
        fixture['normalization'] != 'none' ||
        fixture['targetBytes'] != entry.targetBytes ||
        fixture['actualBytes'] != entry.targetBytes ||
        fixtureHash is! String ||
        !_sha256Pattern.hasMatch(fixtureHash) ||
        fixtureHash != expectedFixtureHash) {
      processCompletion.add(
        'Fixture does not prove the exact frozen byte/hash denominator.',
      );
    } else {
      fixtureHashesBySize
          .putIfAbsent(entry.targetBytes, () => <String>{})
          .add(fixtureHash);
    }

    if (entry.peer == 'flutter_quill') {
      _validateQuill(
        payload,
        evidence,
        entry,
        processCompletion,
        processPerformance,
        exportPaths,
      );
    } else {
      _validateSuperEditor(
        payload,
        entry,
        processCompletion,
        processPerformance,
        exportPaths,
        rawEvidencePaths,
      );
    }
    if (entry.workload != 'paste-32kib') return null;
    return _validatePasteStateContract(
      payload: payload,
      entry: entry,
      completion: processCompletion,
    );
  }

  String? _validatePasteStateContract({
    required Map<String, Object?> payload,
    required PeerSuiteEntry entry,
    required List<String> completion,
  }) {
    final blockerCount = completion.length;
    final contract = _optionalMap(payload['pasteStateContract']);
    final fixture = _fixtureFor(entry.targetBytes);
    final paste = _fixtureFor(32768);
    final offset = switch (entry.location) {
      'start' => 0,
      'middle' => fixture.length ~/ 2,
      'end' => fixture.length,
      _ => throw StateError('Unknown location ${entry.location}'),
    };
    final pasted =
        '${fixture.substring(0, offset)}$paste'
        '${fixture.substring(offset)}';
    final baseDenominator = _stateDenominator(fixture);
    final pastedDenominator = _stateDenominator(pasted);

    if (contract == null ||
        contract['schemaVersion'] != 1 ||
        contract['mode'] != 'reset-after-each-paste' ||
        contract['pasteViaPlatformInput'] != true ||
        contract['resetViaPlatformBackspace'] != true ||
        contract['selectionForReset'] !=
            'programmatic-exact-pasted-source-range' ||
        contract['warmupTransitions'] != 2 ||
        contract['measuredTransitions'] != 20 ||
        canonicalJson(contract['baseState']) !=
            canonicalJson(baseDenominator) ||
        canonicalJson(contract['singlePasteState']) !=
            canonicalJson(pastedDenominator) ||
        canonicalJson(contract['expectedFinalState']) !=
            canonicalJson(baseDenominator)) {
      completion.add(
        'Paste receipt does not declare the exact reset-after-each-paste '
        'base/single-paste/final byte-hash denominators.',
      );
      return null;
    }

    final transitions = _optionalList(contract['transitions']);
    if (transitions == null || transitions.length != 22) {
      completion.add(
        'Paste receipt must retain exactly 2 warmup and 20 measured state '
        'transitions.',
      );
      return null;
    }
    final canonicalTransitions = <Object?>[];
    final quillPasteOrdering = <_InputOrdering>[];
    final quillResetOrdering = <_InputOrdering>[];
    for (var index = 0; index < transitions.length; index += 1) {
      final transition = _optionalMap(transitions[index]);
      final expectedMeasured = index >= 2;
      final pasteInput = _optionalMap(transition?['pasteInput']);
      final pre = _optionalMap(transition?['preState']);
      final post = _optionalMap(transition?['postState']);
      final reset = _optionalMap(transition?['resetState']);
      final resetInput = _optionalMap(transition?['resetInput']);
      final quillResetEvidence = _optionalMap(resetInput?['evidence']);
      final expectedPasteSequence = index * 2;
      final expectedResetSequence = expectedPasteSequence + 1;
      final quillPaste = entry.peer == 'flutter_quill'
          ? _quillInputOrdering(
              link: pasteInput,
              expectedSequence: expectedPasteSequence,
              expectedTransition: index,
              expectedRole: 'paste-workload',
              expectedAction: 'paste-32kib',
              expectedMeasured: expectedMeasured,
            )
          : null;
      final quillReset = entry.peer == 'flutter_quill'
          ? _quillInputOrdering(
              link: resetInput,
              expectedSequence: expectedResetSequence,
              expectedTransition: index,
              expectedRole: 'paste-reset',
              expectedAction: 'paste-cleanup-delete',
              expectedMeasured: false,
            )
          : null;
      final preValid = _validateCanonicalStateProof(
        proof: pre,
        expectedCanonical: fixture,
        peer: entry.peer,
      );
      final postValid = _validateCanonicalStateProof(
        proof: post,
        expectedCanonical: pasted,
        peer: entry.peer,
      );
      final resetValid = _validateCanonicalStateProof(
        proof: reset,
        expectedCanonical: fixture,
        peer: entry.peer,
      );
      if (transition == null ||
          transition['transitionIndex'] != index ||
          transition['measured'] != expectedMeasured ||
          pasteInput?['evidenceSequence'] != expectedPasteSequence ||
          resetInput?['evidenceSequence'] != expectedResetSequence ||
          !preValid ||
          !postValid ||
          !resetValid ||
          resetInput?['operation'] !=
              'platform-backspace-over-exact-pasted-range' ||
          resetInput?['measured'] != false ||
          resetInput?['accepted'] != true ||
          resetInput?['rastered'] != true ||
          resetInput?['platformInputDispatched'] != true ||
          resetInput?['selectionStart'] != offset ||
          resetInput?['selectionEnd'] != offset + paste.length ||
          (entry.peer == 'flutter_quill' &&
              (quillPaste == null ||
                  quillReset == null ||
                  !_validateQuillResetEvidence(quillResetEvidence, index)))) {
        completion.add(
          'Paste transition $index does not prove base -> one paste -> exact '
          'platform-backspace reset without accumulation.',
        );
        continue;
      }
      if (quillPaste != null && quillReset != null) {
        quillPasteOrdering.add(quillPaste);
        quillResetOrdering.add(quillReset);
      }
      canonicalTransitions.add({
        'transitionIndex': index,
        'measured': expectedMeasured,
        'preState': _canonicalProofShape(pre!),
        'postState': _canonicalProofShape(post!),
        'resetState': _canonicalProofShape(reset!),
      });
    }
    if (entry.peer == 'flutter_quill') {
      _validatePasteResetOrdering(
        paste: quillPasteOrdering,
        reset: quillResetOrdering,
        peerLabel: 'Quill',
        completion: completion,
      );
    }
    if (completion.length != blockerCount) return null;

    final scenario = _optionalMap(payload['scenarioResult']);
    if (entry.peer == 'flutter_quill' &&
        canonicalJson(scenario?['pasteStateContract']) !=
            canonicalJson(contract)) {
      completion.add(
        'Quill top-level and scenario paste-state contracts differ.',
      );
      return null;
    }
    return canonicalJson({
      'baseState': baseDenominator,
      'singlePasteState': pastedDenominator,
      'expectedFinalState': baseDenominator,
      'transitions': canonicalTransitions,
    });
  }

  bool _validateCanonicalStateProof({
    required Map<String, Object?>? proof,
    required String expectedCanonical,
    required String peer,
  }) {
    if (proof == null) return false;
    final canonicalBytes = utf8.encode(expectedCanonical).length;
    final canonicalHash = sha256Text(expectedCanonical);
    if (proof['canonicalUtf8Bytes'] != canonicalBytes ||
        proof['canonicalSha256'] != canonicalHash ||
        proof['matchesExpectedCanonical'] != true) {
      return false;
    }
    final classification = proof['classification'];
    if (classification == 'exact') {
      return proof['rawUtf8Bytes'] == canonicalBytes &&
          proof['rawSha256'] == canonicalHash;
    }
    if (peer == 'flutter_quill' &&
        classification == 'peer-appended-terminal-newline') {
      return proof['rawUtf8Bytes'] == canonicalBytes + 1 &&
          proof['rawSha256'] == sha256Text('$expectedCanonical\n');
    }
    return false;
  }

  bool _validateQuillResetEvidence(
    Map<String, Object?>? evidence,
    int transitionIndex,
  ) {
    final accepted = evidence?['acceptedTraceMicros'];
    final frame = _optionalMap(evidence?['frame']);
    final buildStart = frame?['buildStartMicros'];
    final rasterFinish = frame?['rasterFinishMicros'];
    final callback = frame?['frameTimingCallbackTraceMicros'];
    return evidence?['action'] == 'paste-cleanup-delete' &&
        evidence?['sampleIndex'] == transitionIndex &&
        evidence?['measured'] == false &&
        evidence?['nativeInput'] is Map &&
        _optionalMap(evidence?['frameCorrelation'])?['proven'] == true &&
        accepted is int &&
        buildStart is int &&
        rasterFinish is int &&
        callback is int &&
        buildStart > accepted &&
        rasterFinish >= buildStart &&
        callback >= rasterFinish;
  }

  _InputOrdering? _quillInputOrdering({
    required Map<String, Object?>? link,
    required int expectedSequence,
    required int expectedTransition,
    required String expectedRole,
    required String expectedAction,
    required bool expectedMeasured,
  }) {
    final evidence = _optionalMap(link?['evidence']);
    final frame = _optionalMap(evidence?['frame']);
    final ordering = _InputOrdering.tryCreate(
      sequence: evidence?['inputSequence'],
      transitionIndex: evidence?['stateTransitionIndex'],
      request: evidence?['actionStartTraceMicros'],
      ingress: evidence?['nativeIngressTraceMicros'],
      accepted: evidence?['acceptedTraceMicros'],
      buildStart: frame?['buildStartMicros'],
      rasterFinish: frame?['rasterFinishMicros'],
      callback: frame?['frameTimingCallbackTraceMicros'],
    );
    if (link?['evidenceSequence'] != expectedSequence ||
        evidence?['inputSequence'] != expectedSequence ||
        evidence?['stateTransitionIndex'] != expectedTransition ||
        evidence?['evidenceRole'] != expectedRole ||
        evidence?['action'] != expectedAction ||
        evidence?['measured'] != expectedMeasured ||
        evidence?['nativeInput'] is! Map ||
        _optionalMap(evidence?['frameCorrelation'])?['proven'] != true ||
        ordering == null ||
        !ordering.isInternallyOrdered) {
      return null;
    }
    return ordering;
  }

  void _validatePasteResetOrdering({
    required List<_InputOrdering> paste,
    required List<_InputOrdering> reset,
    required String peerLabel,
    required List<String> completion,
  }) {
    if (paste.length != 22 || reset.length != 22) {
      completion.add(
        '$peerLabel paste timeline is missing warmup or measured input '
        'ordering evidence.',
      );
      return;
    }
    final sequences = <int>{};
    for (var index = 0; index < 22; index += 1) {
      final pasteInput = paste[index];
      final resetInput = reset[index];
      if (!sequences.add(pasteInput.sequence) ||
          !sequences.add(resetInput.sequence) ||
          pasteInput.transitionIndex != index ||
          resetInput.transitionIndex != index ||
          pasteInput.callback >= resetInput.request ||
          pasteInput.callback >= resetInput.ingress) {
        completion.add(
          '$peerLabel paste transition $index is not fully accepted and '
          'rastered before its reset begins.',
        );
      }
      if (index + 1 < 22) {
        final nextPaste = paste[index + 1];
        if (resetInput.callback >= nextPaste.request ||
            resetInput.callback >= nextPaste.ingress) {
          completion.add(
            '$peerLabel paste transition $index reset does not complete '
            'before transition ${index + 1} begins.',
          );
        }
      }
    }
  }

  void _validateSuperEditorPasteOrdering({
    required Map<String, Object?> raw,
    required Map<String, Object?> payload,
    required Map<int, Map<String, Object?>> frames,
    required List<String> completion,
  }) {
    if (canonicalJson(raw['pasteStateContract']) !=
        canonicalJson(payload['pasteStateContract'])) {
      completion.add(
        'SuperEditor result and hashed timeline paste-state contracts differ.',
      );
    }
    final pasteInputs = _optionalList(raw['inputs']);
    final resetInputs = _optionalList(raw['resetInputs']);
    final transitions = _optionalList(
      _optionalMap(payload['pasteStateContract'])?['transitions'],
    );
    if (pasteInputs == null || pasteInputs.length != 22) {
      completion.add(
        'SuperEditor hashed timeline is missing warmup or measured paste '
        'input evidence; exactly 22 are required.',
      );
      return;
    }
    if (resetInputs == null || resetInputs.length != 22) {
      completion.add(
        'SuperEditor hashed timeline must retain all 22 platform paste-reset '
        'inputs.',
      );
      return;
    }
    if (transitions == null || transitions.length != 22) return;

    final pasteOrdering = <_InputOrdering>[];
    final resetOrdering = <_InputOrdering>[];
    final sequences = <int>{};
    for (var index = 0; index < 22; index += 1) {
      final paste = _optionalMap(pasteInputs[index]);
      final reset = _optionalMap(resetInputs[index]);
      final transition = _optionalMap(transitions[index]);
      final pasteLink = _optionalMap(transition?['pasteInput']);
      final resetLink = _optionalMap(transition?['resetInput']);
      final pasteNative = _optionalMap(paste?['nativeEvent']);
      final resetNative = _optionalMap(reset?['nativeEvent']);
      final pasteFrame = frames[paste?['frameNumber']];
      final resetFrame = frames[reset?['frameNumber']];
      final pasteOrder = _superEditorInputOrdering(paste, pasteFrame);
      final resetOrder = _superEditorInputOrdering(reset, resetFrame);
      final expectedPasteSequence = index * 2;
      final expectedResetSequence = expectedPasteSequence + 1;
      final expectedMeasured = index >= 2;
      if (paste == null ||
          reset == null ||
          transition == null ||
          paste['sequence'] != expectedPasteSequence ||
          reset['sequence'] != expectedResetSequence ||
          !sequences.add(expectedPasteSequence) ||
          !sequences.add(expectedResetSequence) ||
          transition['transitionIndex'] != index ||
          paste['stateTransitionIndex'] != index ||
          reset['stateTransitionIndex'] != index ||
          pasteLink?['evidenceSequence'] != expectedPasteSequence ||
          resetLink?['evidenceSequence'] != expectedResetSequence ||
          paste['operation'] != 'paste' ||
          paste['evidenceRole'] != 'paste-workload' ||
          paste['measured'] != expectedMeasured ||
          paste['failure'] != null ||
          pasteNative?['platformRouteInvoked'] != true ||
          reset['operation'] != 'backspace' ||
          reset['evidenceRole'] != 'paste-reset' ||
          reset['measured'] != false ||
          reset['pair'] != index ||
          reset['failure'] != null ||
          resetNative?['eventPath'] is! String ||
          pasteOrder == null ||
          resetOrder == null ||
          !pasteOrder.isInternallyOrdered ||
          !resetOrder.isInternallyOrdered ||
          paste['rasterFinishTimelineMicros'] != pasteOrder.rasterFinish ||
          reset['rasterFinishTimelineMicros'] != resetOrder.rasterFinish) {
        completion.add(
          'SuperEditor paste transition $index lacks exact linked '
          'request/ingress/accept/raster/callback evidence.',
        );
        continue;
      }
      pasteOrdering.add(pasteOrder);
      resetOrdering.add(resetOrder);
    }
    _validatePasteResetOrdering(
      paste: pasteOrdering,
      reset: resetOrdering,
      peerLabel: 'SuperEditor',
      completion: completion,
    );
  }

  _InputOrdering? _superEditorInputOrdering(
    Map<String, Object?>? input,
    Map<String, Object?>? frame,
  ) => _InputOrdering.tryCreate(
    sequence: input?['sequence'],
    transitionIndex: input?['stateTransitionIndex'],
    request: input?['requestedTimelineMicros'],
    ingress: input?['platformIngressTimelineMicros'],
    accepted: input?['acceptedTimelineMicros'],
    buildStart: frame?['buildStartTimelineMicros'],
    rasterFinish: frame?['rasterFinishTimelineMicros'],
    callback: frame?['callbackTimelineMicros'],
  );

  Map<String, Object?> _canonicalProofShape(Map<String, Object?> proof) =>
      <String, Object?>{
        'canonicalUtf8Bytes': proof['canonicalUtf8Bytes'],
        'canonicalSha256': proof['canonicalSha256'],
      };

  Map<String, Object?> _stateDenominator(String source) => <String, Object?>{
    'utf8Bytes': utf8.encode(source).length,
    'sha256': sha256Text(source),
  };

  void _validateQuill(
    Map<String, Object?> payload,
    PeerProcessEvidence evidence,
    PeerSuiteEntry entry,
    List<String> completion,
    List<String> performance,
    Set<String> exportPaths,
  ) {
    final export = _optionalMap(payload['finalExportArtifact']);
    final fixture = _optionalMap(payload['fixture']);
    final initialFidelity = _optionalMap(payload['initialFidelity']);
    final fidelity = _optionalMap(payload['finalFidelity']);
    final expectedFinal = _expectedFinal(entry, 'flutter_quill');
    final expectedFinalHash = sha256Text(expectedFinal);
    final expectedFinalBytes = utf8.encode(expectedFinal).length;
    if (export?['written'] != true ||
        export?['path'] !=
            evidence.environmentOverrides['COMPETITOR_EXPORT_PATH'] ||
        !_validateExport(
          path: export?['path'],
          declaredSha256: export?['sha256'],
          declaredBytes: export?['utf8Bytes'],
          exportPaths: exportPaths,
          blockers: completion,
        )) {
      completion.add('Quill final export denominator is invalid.');
    }
    final exact = fidelity?['exact'] == true;
    final classifiedNewline =
        fidelity?['classification'] == 'peer-appended-terminal-newline' &&
        fidelity?['lengthDeltaUtf16'] == 1;
    final expectedReceiptMatches =
        fidelity?['expectedSha256'] == expectedFinalHash &&
        fidelity?['expectedUtf8Bytes'] == expectedFinalBytes;
    final actualMatchesExpected = exact
        ? (export?['sha256'] == expectedFinalHash &&
              export?['utf8Bytes'] == expectedFinalBytes)
        : (export?['sha256'] == sha256Text('$expectedFinal\n') &&
              export?['utf8Bytes'] == expectedFinalBytes + 1);
    if ((!exact && !classifiedNewline) ||
        !expectedReceiptMatches ||
        !actualMatchesExpected ||
        fidelity?['actualSha256'] != export?['sha256'] ||
        fidelity?['actualUtf8Bytes'] != export?['utf8Bytes']) {
      completion.add(
        'Quill final fidelity is neither exact nor the declared terminal-'
        'newline normalization bound to the retained export.',
      );
    }
    final initialExact = initialFidelity?['exact'] == true;
    final initialClassifiedNewline =
        initialFidelity?['classification'] ==
            'peer-appended-terminal-newline' &&
        initialFidelity?['lengthDeltaUtf16'] == 1;
    if ((!initialExact && !initialClassifiedNewline) ||
        initialFidelity?['expectedSha256'] != fixture?['sha256'] ||
        initialFidelity?['expectedUtf8Bytes'] != entry.targetBytes) {
      completion.add(
        'Quill initial import fidelity is not bound to the exact fixture.',
      );
    }

    final scenario = _optionalMap(payload['scenarioResult']);
    final samples = scenario == null
        ? const <Object?>[]
        : _optionalList(scenario['rawSamples']) ?? const <Object?>[];
    final sampleIdentities = <String>{};
    for (var index = 0; index < samples.length; index += 1) {
      final sample = _optionalMap(samples[index]);
      if (sample == null || sample['measured'] == false) continue;
      final identity = '${sample['action']}:${sample['sampleIndex']}';
      if (sample['action'] is! String ||
          sample['sampleIndex'] is! int ||
          !sampleIdentities.add(identity)) {
        completion.add(
          'Quill measured sample $index has a missing or duplicate event '
          'identity.',
        );
      }
      final accepted = sample['acceptedTraceMicros'];
      final frame = _optionalMap(sample['frame']);
      final buildStart = frame?['buildStartMicros'];
      final rasterFinish = frame?['rasterFinishMicros'];
      final callback = frame?['frameTimingCallbackTraceMicros'];
      if (sample['frameCorrelation'] is! Map ||
          (_optionalMap(sample['frameCorrelation'])?['proven'] != true) ||
          accepted is! int ||
          buildStart is! int ||
          rasterFinish is! int ||
          callback is! int ||
          frame?['buildDurationMicros'] is! int ||
          frame?['rasterDurationMicros'] is! int ||
          frame?['totalSpanMicros'] is! int ||
          buildStart <= accepted ||
          rasterFinish < buildStart ||
          callback < rasterFinish) {
        completion.add(
          'Quill measured sample $index lacks strict post-accept containing-'
          'frame proof.',
        );
      }
      if (entry.workload == 'paste-32kib' &&
          sample['stateTransitionIndex'] != index + 2) {
        completion.add(
          'Quill measured paste sample $index is not bound to its reset '
          'state transition.',
        );
      }
    }
    final measuredSamples = samples
        .map(_optionalMap)
        .whereType<Map<String, Object?>>()
        .where((sample) => sample['measured'] != false)
        .length;
    final expectedMeasured = _expectedMeasuredSamples(entry.workload);
    if (measuredSamples != expectedMeasured) {
      completion.add(
        'Quill retained $measuredSamples measured samples; '
        '$expectedMeasured are required.',
      );
    }
    if (entry.workload != 'cold-open' &&
        scenario?['maxInputBacklogUntilRaster'] is! int) {
      completion.add('Quill input backlog evidence is missing.');
    }
    final distributions = _optionalMap(scenario?['distributions']);
    if (entry.workload != 'cold-open') {
      _validateDistributionFamily(distributions, expectedMeasured, performance);
    } else {
      final cold = _optionalMap(payload['coldOpen']);
      final verification = _optionalMap(cold?['interactiveVerification']);
      final frame = _optionalMap(cold?['frame']);
      if (cold?['processStartToInteractiveRasterFinishMicros'] is! int ||
          cold?['documentLoadStartToRasterFinishMicros'] is! int ||
          verification?['focusNodeHasFocus'] != true ||
          verification?['editorStateMounted'] != true ||
          verification?['sourcePrefixMatchesFixture'] != true ||
          verification?['viewportLogicalWidth'] != 600.0 ||
          verification?['viewportLogicalHeight'] != 600.0 ||
          frame?['buildStartMicros'] is! int ||
          frame?['rasterFinishMicros'] is! int) {
        completion.add(
          'Quill cold-open receipt does not prove the exact interactive '
          '600x600 viewport reached raster.',
        );
      }
    }
    final memory = _optionalMap(payload['memory']);
    final finalMemory = _optionalMap(memory?['afterWorkload']);
    if (finalMemory?['peakResidentBytes'] is! int ||
        finalMemory?['residentBytes'] is! int) {
      performance.add('Peak and retained RSS are missing.');
    }
    performance.add(
      'The longest synchronous span is not captured by the Quill runner.',
    );
  }

  void _validateSuperEditor(
    Map<String, Object?> payload,
    PeerSuiteEntry entry,
    List<String> completion,
    List<String> performance,
    Set<String> exportPaths,
    Set<String> rawEvidencePaths,
  ) {
    final fixture = _optionalMap(payload['fixture']);
    final fidelity = _optionalMap(payload['fidelity']);
    final artifacts = _optionalMap(payload['artifacts']);
    final export = _optionalMap(artifacts?['finalExport']);
    final expectedFinal = _expectedFinal(entry, 'super_editor');
    final expectedFinalHash = sha256Text(expectedFinal);
    final expectedFinalBytes = utf8.encode(expectedFinal).length;
    if (fidelity?['pass'] != true ||
        fidelity?['initialSourceSha256'] != fixture?['sha256'] ||
        fidelity?['expectedFinalSourceSha256'] != expectedFinalHash ||
        fidelity?['exportedFinalSourceBytes'] != expectedFinalBytes ||
        fidelity?['expectedFinalSourceSha256'] !=
            fidelity?['exportedFinalSourceSha256'] ||
        fidelity?['exportedFinalSourceSha256'] != export?['sha256'] ||
        !_validateExport(
          path: export?['path'],
          declaredSha256: export?['sha256'],
          declaredBytes: fidelity?['exportedFinalSourceBytes'],
          exportPaths: exportPaths,
          blockers: completion,
        )) {
      completion.add(
        'SuperEditor final byte/hash denominator or exact fidelity is invalid.',
      );
    }

    final timeline = _optionalMap(artifacts?['rawTimeline']);
    final timelinePath = timeline?['path'];
    final timelineHash = timeline?['sha256'];
    if (timelinePath is! String ||
        timelineHash is! String ||
        !rawEvidencePaths.add(File(timelinePath).absolute.path) ||
        !_validateHashedFile(
          timelinePath,
          timelineHash,
          'SuperEditor raw timeline',
          completion,
        )) {
      completion.add('SuperEditor raw timeline is missing or unhashed.');
    } else {
      try {
        final raw = _map(
          jsonDecode(File(timelinePath).readAsStringSync()),
          'SuperEditor raw timeline',
        );
        final frames = <int, Map<String, Object?>>{};
        for (final value in _optionalList(raw['frames']) ?? const []) {
          final frame = _optionalMap(value);
          final number = frame?['frameNumber'];
          if (frame != null && number is int) {
            if (frames.containsKey(number)) {
              completion.add(
                'SuperEditor raw timeline repeats frame number $number.',
              );
            }
            frames[number] = frame;
          }
        }
        if (entry.workload == 'paste-32kib') {
          _validateSuperEditorPasteOrdering(
            raw: raw,
            payload: payload,
            frames: frames,
            completion: completion,
          );
        }
        var measuredCount = 0;
        final measuredSequences = <int>{};
        for (final value in _optionalList(raw['inputs']) ?? const []) {
          final sample = _optionalMap(value);
          if (sample == null || sample['measured'] != true) continue;
          measuredCount += 1;
          final sequence = sample['sequence'];
          if (sequence is! int || !measuredSequences.add(sequence)) {
            completion.add(
              'SuperEditor raw timeline has a missing or duplicate measured '
              'input sequence.',
            );
          }
          final accepted = sample['acceptedTimelineMicros'];
          final frame = frames[sample['frameNumber']];
          final buildStart = frame?['buildStartTimelineMicros'];
          final rasterFinish = frame?['rasterFinishTimelineMicros'];
          if (sample['failure'] != null ||
              accepted is! int ||
              buildStart is! int ||
              rasterFinish is! int ||
              buildStart <= accepted ||
              rasterFinish < buildStart) {
            completion.add(
              'SuperEditor measured input ${sample['sequence']} lacks strict '
              'post-accept containing-frame proof.',
            );
          }
          if (entry.workload == 'paste-32kib' &&
              sample['stateTransitionIndex'] != measuredCount + 1) {
            completion.add(
              'SuperEditor measured paste input ${sample['sequence']} is not '
              'bound to its reset state transition.',
            );
          }
        }
        final expectedMeasured = _expectedMeasuredSamples(entry.workload);
        if (measuredCount != expectedMeasured) {
          completion.add(
            'SuperEditor retained $measuredCount measured inputs; '
            '$expectedMeasured are required.',
          );
        }
      } catch (error) {
        completion.add('SuperEditor raw timeline is invalid JSON: $error');
      }
    }

    final measurements = _optionalMap(payload['measurements']);
    final expectedMeasured = _expectedMeasuredSamples(entry.workload);
    if (measurements == null ||
        measurements['measuredSampleCount'] != expectedMeasured ||
        measurements['maxInputBacklog'] is! int) {
      completion.add(
        'SuperEditor measured-sample denominator or backlog evidence is '
        'missing.',
      );
    }
    if (measurements?['peakSampledRssBytes'] is! int ||
        measurements?['retainedRssBytes'] is! int) {
      performance.add('Peak and retained RSS are missing.');
    }
    for (final field in const [
      'buildMicros',
      'rasterMicros',
      'totalSpanMicros',
    ]) {
      final distribution = _optionalMap(measurements?[field]);
      if (distribution == null ||
          distribution['count'] is! int ||
          distribution['p50'] is! int ||
          distribution['p90'] is! int ||
          distribution['p99'] is! int ||
          distribution['max'] is! int) {
        performance.add('$field distribution is missing.');
      }
    }
    if (measurements?['missedMeasuredFrames'] is! int ||
        measurements?['frameBudgetMicros'] is! int) {
      performance.add('Missed-frame or frame-budget evidence is missing.');
    }
    if (entry.workload != 'cold-open') {
      _validateDistributionFamily(measurements, expectedMeasured, performance);
    } else {
      final cold = _optionalMap(payload['coldOpen']);
      final endpoint = _optionalMap(cold?['endpointEvidence']);
      final frame = _optionalMap(cold?['interactiveFrame']);
      if (cold?['documentLoadToInteractiveRasterMicros'] is! int ||
          endpoint?['focus'] != true ||
          endpoint?['imeConnected'] != true ||
          endpoint?['expectedLeadingTextInRenderedModel'] != true ||
          endpoint?['rasterTimingReceived'] != true ||
          endpoint?['viewportLogicalWidth'] != 600.0 ||
          endpoint?['viewportLogicalHeight'] != 600.0 ||
          frame?['buildStartTimelineMicros'] is! int ||
          frame?['rasterFinishTimelineMicros'] is! int) {
        completion.add(
          'SuperEditor cold-open receipt does not prove the exact interactive '
          '600x600 viewport reached raster.',
        );
      }
    }
    final longest = _optionalMap(measurements?['longestSynchronousSpan']);
    if (longest?['supported'] != true) {
      performance.add(
        'The longest synchronous span is not captured by the SuperEditor '
        'runner.',
      );
    }
  }

  void _validateDistributionFamily(
    Map<String, Object?>? value,
    int expectedCount,
    List<String> blockers,
  ) {
    if (value == null) {
      blockers.add('Input-to-paint distributions are missing.');
      return;
    }
    Map<String, Object?>? distribution;
    for (final key in const [
      'acceptedInputToRasterFinishMicros',
      'inputToRasterMicros',
    ]) {
      distribution ??= _optionalMap(value[key]);
    }
    if (distribution == null ||
        distribution['count'] != expectedCount ||
        distribution['p50'] is! int ||
        distribution['p90'] is! int ||
        distribution['p99'] is! int ||
        distribution['max'] is! int) {
      blockers.add('p50/p90/p99/max input-to-paint distribution is missing.');
    }
  }

  bool _validateExport({
    required Object? path,
    required Object? declaredSha256,
    required Object? declaredBytes,
    required Set<String> exportPaths,
    required List<String> blockers,
  }) {
    if (path is! String || declaredSha256 is! String || declaredBytes is! int) {
      return false;
    }
    final absolute = File(path).absolute.path;
    if (!exportPaths.add(absolute)) {
      blockers.add('Final export path is reused by multiple processes.');
      return false;
    }
    final file = File(absolute);
    if (!file.existsSync()) return false;
    return file.lengthSync() == declaredBytes &&
        sha256File(file) == declaredSha256;
  }

  bool _validateHashedFile(
    String path,
    String? expectedSha256,
    String label,
    List<String> blockers,
  ) {
    final file = File(path);
    if (expectedSha256 == null ||
        !_sha256Pattern.hasMatch(expectedSha256) ||
        !file.existsSync()) {
      blockers.add('$label is absent or has no valid SHA-256.');
      return false;
    }
    if (sha256File(file) != expectedSha256) {
      blockers.add('$label SHA-256 does not match retained bytes.');
      return false;
    }
    return true;
  }

  int? _largestCompletedTier({
    required String peer,
    required PeerSuitePlan plan,
    required Set<String> completedEntryIds,
  }) {
    int? result;
    for (final size in protocolSizes) {
      final required = plan.entries.where(
        (entry) => entry.peer == peer && entry.targetBytes == size,
      );
      if (required.isNotEmpty &&
          required.every((entry) => completedEntryIds.contains(entry.id))) {
        result = size;
      } else {
        break;
      }
    }
    return result;
  }
}

final class _PeerCase {
  const _PeerCase({
    required this.workload,
    required this.targetBytes,
    required this.location,
    required this.replicate,
  });

  final String workload;
  final int targetBytes;
  final String location;
  final int replicate;
}

final class _InputOrdering {
  const _InputOrdering({
    required this.sequence,
    required this.transitionIndex,
    required this.request,
    required this.ingress,
    required this.accepted,
    required this.buildStart,
    required this.rasterFinish,
    required this.callback,
  });

  static _InputOrdering? tryCreate({
    required Object? sequence,
    required Object? transitionIndex,
    required Object? request,
    required Object? ingress,
    required Object? accepted,
    required Object? buildStart,
    required Object? rasterFinish,
    required Object? callback,
  }) {
    if (sequence is! int ||
        transitionIndex is! int ||
        request is! int ||
        ingress is! int ||
        accepted is! int ||
        buildStart is! int ||
        rasterFinish is! int ||
        callback is! int) {
      return null;
    }
    return _InputOrdering(
      sequence: sequence,
      transitionIndex: transitionIndex,
      request: request,
      ingress: ingress,
      accepted: accepted,
      buildStart: buildStart,
      rasterFinish: rasterFinish,
      callback: callback,
    );
  }

  final int sequence;
  final int transitionIndex;
  final int request;
  final int ingress;
  final int accepted;
  final int buildStart;
  final int rasterFinish;
  final int callback;

  bool get isInternallyOrdered =>
      request <= ingress &&
      ingress <= accepted &&
      accepted < buildStart &&
      buildStart <= rasterFinish &&
      rasterFinish <= callback;
}

final _sha256Pattern = RegExp(r'^[0-9a-f]{64}$');

int _expectedMeasuredSamples(String workload) => switch (workload) {
  'cold-open' => 0,
  'sustained-typing' => 200,
  'local-insert-delete' => 200,
  'paste-32kib' => 20,
  _ => throw StateError('Unknown workload $workload'),
};

String sha256File(File file) =>
    sha256.convert(file.readAsBytesSync()).toString();

String sha256Text(String value) =>
    sha256.convert(utf8.encode(value)).toString();

String canonicalJson(Object? value) => jsonEncode(_canonicalize(value));

Object? _canonicalize(Object? value) {
  if (value is Map) {
    final keys = value.keys.map((key) => '$key').toList()..sort();
    return <String, Object?>{
      for (final key in keys) key: _canonicalize(value[key]),
    };
  }
  if (value is List) return value.map(_canonicalize).toList();
  return value;
}

void _requireUnique<T>(
  Set<T> values,
  T value,
  String label,
  String entryId,
  List<String> blockers,
) {
  if (!values.add(value)) blockers.add('Duplicate $label for $entryId.');
}

List<String> _dedupe(List<String> values) =>
    values.toSet().toList(growable: false);

Map<String, Object?> _map(Object? value, String label) {
  if (value is! Map) throw FormatException('$label must be an object');
  return value.map((key, value) => MapEntry('$key', value));
}

Map<String, Object?>? _optionalMap(Object? value) {
  if (value is! Map) return null;
  return value.map((key, value) => MapEntry('$key', value));
}

List<Object?> _list(Map<String, Object?> value, String key) {
  final result = value[key];
  if (result is! List) throw FormatException('$key must be a list');
  return result.cast<Object?>();
}

List<Object?>? _optionalList(Object? value) =>
    value is List ? value.cast<Object?>() : null;

String _string(Map<String, Object?> value, String key) {
  final result = value[key];
  if (result is! String || result.isEmpty) {
    throw FormatException('$key must be a non-empty string');
  }
  return result;
}

String? _nullableString(Object? value) => value is String ? value : null;

int _int(Map<String, Object?> value, String key) {
  final result = value[key];
  if (result is! int) throw FormatException('$key must be an integer');
  return result;
}

bool _bool(Map<String, Object?> value, String key) {
  final result = value[key];
  if (result is! bool) throw FormatException('$key must be a boolean');
  return result;
}
