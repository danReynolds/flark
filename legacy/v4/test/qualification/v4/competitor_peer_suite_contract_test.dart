// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import '../../../benchmark/peer_suite/lib/peer_suite.dart';

const _smallFixtures = <int, String>{
  1048576: 'small-fixture-1mib',
  5242880: 'small-fixture-5mib',
  10485760: 'small-fixture-10mib',
  32768: 'small-paste-fixture',
};

void main() {
  group('two-peer protocol plan', () {
    final plan = PeerSuitePlan.protocol();

    test('freezes the 234-process interleaved denominator', () {
      expect(plan.entries, hasLength(234));
      expect(
        plan.sha256,
        '3daf93557b1ac671b4c9a2aaa743276d8d629758999398073bd7da6b2b370d8c',
      );
      expect(
        {
          for (var group = 0; group < 3; group += 1)
            group: plan.entries
                .where((entry) => entry.groupIndex == group)
                .length,
        },
        const {0: 78, 1: 78, 2: 78},
      );
      expect(
        {
          for (final workload in protocolWorkloads)
            workload: plan.entries
                .where((entry) => entry.workload == workload)
                .length,
        },
        const {
          'cold-open': 180,
          'sustained-typing': 18,
          'local-insert-delete': 18,
          'paste-32kib': 18,
        },
      );
      for (final peer in protocolPeers) {
        for (final size in protocolSizes) {
          expect(
            plan.entries.where(
              (entry) => entry.peer == peer && entry.targetBytes == size,
            ),
            hasLength(39),
          );
        }
      }
    });

    test('pairs both peers at every exact case and rotates peer-first', () {
      for (var index = 0; index < plan.entries.length; index += 2) {
        final first = plan.entries[index];
        final second = plan.entries[index + 1];
        expect(first.caseKey, second.caseKey);
        expect({first.peer, second.peer}, protocolPeers.toSet());
        if (index >= 2 &&
            first.groupIndex == plan.entries[index - 2].groupIndex) {
          expect(first.peer, isNot(plan.entries[index - 2].peer));
        }
      }
      for (final workload in const ['local-insert-delete', 'paste-32kib']) {
        for (var group = 0; group < 3; group += 1) {
          expect(
            plan.entries
                .where(
                  (entry) =>
                      entry.groupIndex == group && entry.workload == workload,
                )
                .map((entry) => entry.location)
                .toSet(),
            {protocolLocations[group]},
          );
        }
      }
    });

    test('mechanically advances 1 to 5 to 10 to 20 MiB', () {
      expect(nextCompetitorTierBytes(1048576), 5242880);
      expect(nextCompetitorTierBytes(5242880), 10485760);
      expect(nextCompetitorTierBytes(10485760), 20971520);
      expect(nextCompetitorTierBytes(null), isNull);
      expect(nextCompetitorTierBytes(20971520), isNull);
    });

    test('pins the real frozen generator hashes at all production tiers', () {
      const expected = <int, String>{
        1048576:
            '63a62298f4c3d4b6f3227e712db5b7a0ee2d05e9d6b457e7e7a88d875e58db84',
        5242880:
            'e6d203e607bb92c8869f704795d43e7fe4cb12022713aea7f8e0058faf1bb95a',
        10485760:
            '0e20aa5141c3c6d8fd616eb551545f8da27d2e6e0532d2c87fdb92b127081c29',
      };
      for (final entry in expected.entries) {
        final fixture = frozenOrdinaryProseExact(entry.key);
        expect(utf8.encode(fixture), hasLength(entry.key));
        expect(sha256Text(fixture), entry.value);
      }
    });

    test('both peers finish every paste process at the unchanged fixture', () {
      final entries = plan.entries.where(
        (entry) => entry.workload == 'paste-32kib',
      );
      for (final entry in entries) {
        expect(
          frozenExpectedFinalSource(entry, peer: entry.peer),
          frozenOrdinaryProseExact(entry.targetBytes),
        );
      }
    });

    test('dry run validates structure but remains explicitly non-claim', () {
      final result = const PeerSuiteValidator().validate(
        plan: plan,
        processes: const [],
        runGroups: const [],
        exclusiveMachineAttested: false,
        dryRun: true,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(result.performanceClaimEligible, isFalse);
      expect(result.completionEnvelopeBlockers.join('\n'), contains('Dry-run'));
      expect(result.performanceClaimBlockers.join('\n'), contains('Dry-run'));
      expect(result.toJson()['claimEligible'], isFalse);
    });
  });

  group('aggregate validator', () {
    late Directory temporary;

    setUp(() {
      temporary = Directory.systemTemp.createTempSync('flark-peer-suite-test-');
    });

    tearDown(() {
      temporary.deleteSync(recursive: true);
    });

    test(
      'completion can resolve 10 MiB while longest sync blocks performance',
      () {
        final evidence = _buildCompleteEvidence(temporary);
        final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
          plan: evidence.plan,
          processes: evidence.processes,
          runGroups: evidence.groups,
          exclusiveMachineAttested: true,
          dryRun: false,
        );

        expect(result.completionEnvelopeEligible, isTrue);
        expect(result.completionEnvelopeBlockers, isEmpty);
        expect(result.performanceClaimEligible, isFalse);
        expect(
          result.performanceClaimBlockers.join('\n'),
          contains('synchronous span'),
        );
        expect(
          result.performanceClaimBlockers.join('\n'),
          contains('Cross-process cold-open'),
        );
        expect(
          result.performanceClaimBlockers.join('\n'),
          contains('buildMicros distribution'),
        );
        expect(
          result.performanceClaimBlockers.join('\n'),
          contains('Missed-frame'),
        );
        expect(result.completedTierByPeer, const {
          'flutter_quill': 10485760,
          'super_editor': 10485760,
        });
        expect(result.cohortCompletedTierBytes, 10485760);
        expect(result.nextCompetitorTierBytes, 20971520);
        expect(result.toJson()['mayResolveCompetitorDerivedSizeTiers'], isTrue);
        expect(result.toJson()['claimEligible'], isFalse);
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test(
      'rejects a raster whose frame did not begin after acceptance',
      () {
        final evidence = _buildCompleteEvidence(temporary);
        final recordIndex = evidence.processes.indexWhere((process) {
          final entry = evidence.plan.entries.firstWhere(
            (candidate) => candidate.id == process.planEntryId,
          );
          return entry.peer == 'super_editor' &&
              entry.workload == 'sustained-typing';
        });
        final record = evidence.processes[recordIndex];
        final resultFile = File(record.resultPath);
        final payload = (jsonDecode(resultFile.readAsStringSync()) as Map)
            .cast<String, Object?>();
        final artifacts = (payload['artifacts'] as Map).cast<String, Object?>();
        final timelineArtifact = (artifacts['rawTimeline'] as Map)
            .cast<String, Object?>();
        final timelineFile = File(timelineArtifact['path']! as String);
        final timeline = (jsonDecode(timelineFile.readAsStringSync()) as Map)
            .cast<String, Object?>();
        final inputs = (timeline['inputs'] as List).cast<Map>();
        final frames = (timeline['frames'] as List).cast<Map>();
        final accepted = inputs.first['acceptedTimelineMicros']! as int;
        frames.first['buildStartTimelineMicros'] = accepted;
        timelineFile.writeAsStringSync(jsonEncode(timeline));
        timelineArtifact['sha256'] = sha256File(timelineFile);
        resultFile.writeAsStringSync(jsonEncode(payload));
        final replacementJson = record.toJson()
          ..['resultSha256'] = sha256File(resultFile);
        evidence.processes[recordIndex] = PeerProcessEvidence.fromJson(
          replacementJson,
        );

        final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
          plan: evidence.plan,
          processes: evidence.processes,
          runGroups: evidence.groups,
          exclusiveMachineAttested: true,
          dryRun: false,
        );
        expect(result.completionEnvelopeEligible, isFalse);
        expect(
          result.completionEnvelopeBlockers.join('\n'),
          contains('strict post-accept containing-frame proof'),
        );
      },
      timeout: const Timeout(Duration(minutes: 1)),
    );

    test('rejects a consistently shaped but wrong frozen fixture hash', () {
      final evidence = _buildCompleteEvidence(temporary);
      final index = evidence.processes.indexWhere((process) {
        final entry = evidence.plan.entries.firstWhere(
          (candidate) => candidate.id == process.planEntryId,
        );
        return entry.peer == 'flutter_quill';
      });
      _rewriteResult(evidence, index, (payload) {
        final fixture = (payload['fixture'] as Map).cast<String, Object?>();
        fixture['sha256'] = ''.padRight(64, '0');
      });

      final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
        plan: evidence.plan,
        processes: evidence.processes,
        runGroups: evidence.groups,
        exclusiveMachineAttested: true,
        dryRun: false,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('exact frozen byte/hash denominator'),
      );
    });

    test('rejects an unrelated Quill export labeled exact', () {
      final evidence = _buildCompleteEvidence(temporary);
      final index = evidence.processes.indexWhere((process) {
        final entry = evidence.plan.entries.firstWhere(
          (candidate) => candidate.id == process.planEntryId,
        );
        return entry.peer == 'flutter_quill' && entry.workload == 'cold-open';
      });
      final record = evidence.processes[index];
      final payload =
          (jsonDecode(File(record.resultPath).readAsStringSync()) as Map)
              .cast<String, Object?>();
      final export = (payload['finalExportArtifact'] as Map)
          .cast<String, Object?>();
      final exportFile = File(export['path']! as String);
      exportFile.deleteSync();
      exportFile.writeAsStringSync('unrelated-export');
      final wrongHash = sha256File(exportFile);
      export
        ..['sha256'] = wrongHash
        ..['utf8Bytes'] = exportFile.lengthSync();
      final fidelity = (payload['finalFidelity'] as Map)
          .cast<String, Object?>();
      fidelity
        ..['exact'] = true
        ..['classification'] = 'exact'
        ..['expectedSha256'] = wrongHash
        ..['actualSha256'] = wrongHash
        ..['expectedUtf8Bytes'] = exportFile.lengthSync()
        ..['actualUtf8Bytes'] = exportFile.lengthSync();
      _writeReplacementResult(evidence, index, payload);

      final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
        plan: evidence.plan,
        processes: evidence.processes,
        runGroups: evidence.groups,
        exclusiveMachineAttested: true,
        dryRun: false,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('neither exact nor the declared terminal-newline'),
      );
    });

    test('rejects reused raw timelines and duplicate measured sequences', () {
      final evidence = _buildCompleteEvidence(temporary);
      final coldIndexes = <int>[];
      for (var index = 0; index < evidence.processes.length; index += 1) {
        final entry = evidence.plan.entries.firstWhere(
          (candidate) => candidate.id == evidence.processes[index].planEntryId,
        );
        if (entry.peer == 'super_editor' && entry.workload == 'cold-open') {
          coldIndexes.add(index);
          if (coldIndexes.length == 2) break;
        }
      }
      final firstPayload =
          (jsonDecode(
                    File(
                      evidence.processes[coldIndexes.first].resultPath,
                    ).readAsStringSync(),
                  )
                  as Map)
              .cast<String, Object?>();
      final firstTimeline =
          (((firstPayload['artifacts'] as Map)
                      .cast<String, Object?>()['rawTimeline']
                  as Map)
              .cast<String, Object?>());
      _rewriteResult(evidence, coldIndexes.last, (payload) {
        final artifacts = (payload['artifacts'] as Map).cast<String, Object?>();
        artifacts['rawTimeline'] = Map<String, Object?>.from(firstTimeline);
      });

      final sequenceIndex = evidence.processes.indexWhere((process) {
        final entry = evidence.plan.entries.firstWhere(
          (candidate) => candidate.id == process.planEntryId,
        );
        return entry.peer == 'super_editor' &&
            entry.workload == 'local-insert-delete';
      });
      final sequenceRecord = evidence.processes[sequenceIndex];
      final sequencePayload =
          (jsonDecode(File(sequenceRecord.resultPath).readAsStringSync())
                  as Map)
              .cast<String, Object?>();
      final artifacts = (sequencePayload['artifacts'] as Map)
          .cast<String, Object?>();
      final timelineArtifact = (artifacts['rawTimeline'] as Map)
          .cast<String, Object?>();
      final timelineFile = File(timelineArtifact['path']! as String);
      final timeline = (jsonDecode(timelineFile.readAsStringSync()) as Map)
          .cast<String, Object?>();
      final inputs = (timeline['inputs'] as List).cast<Map>();
      inputs[1]['sequence'] = inputs.first['sequence'];
      timelineFile.writeAsStringSync(jsonEncode(timeline));
      timelineArtifact['sha256'] = sha256File(timelineFile);
      _writeReplacementResult(evidence, sequenceIndex, sequencePayload);

      final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
        plan: evidence.plan,
        processes: evidence.processes,
        runGroups: evidence.groups,
        exclusiveMachineAttested: true,
        dryRun: false,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('raw timeline is missing or unhashed'),
      );
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('duplicate measured input sequence'),
      );
    });

    test('rejects any accumulating or unreset paired paste transition', () {
      final evidence = _buildCompleteEvidence(temporary);
      final index = evidence.processes.indexWhere((process) {
        final entry = evidence.plan.entries.firstWhere(
          (candidate) => candidate.id == process.planEntryId,
        );
        return entry.peer == 'flutter_quill' && entry.workload == 'paste-32kib';
      });
      _rewriteResult(evidence, index, (payload) {
        final contract = (payload['pasteStateContract'] as Map)
            .cast<String, Object?>();
        final transitions = (contract['transitions'] as List).cast<Map>();
        final pre = (transitions[3]['preState'] as Map).cast<String, Object?>();
        pre['canonicalUtf8Bytes'] = (pre['canonicalUtf8Bytes']! as int) + 1;
      });

      final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
        plan: evidence.plan,
        processes: evidence.processes,
        runGroups: evidence.groups,
        exclusiveMachineAttested: true,
        dryRun: false,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        anyOf(contains('without accumulation'), contains('contracts differ')),
      );
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('paired paste-state evidence is incomplete'),
      );
    });

    test('rejects an all-pastes-then-resets timeline', () {
      final evidence = _buildCompleteEvidence(temporary);
      _rewriteFirstSuperEditorPasteTimeline(evidence, (timeline) {
        final resets = (timeline['resetInputs'] as List).cast<Map>();
        final frames = (timeline['frames'] as List).cast<Map>();
        for (var index = 0; index < resets.length; index += 1) {
          final request = 10000 + index * 100;
          final reset = resets[index];
          reset
            ..['requestedTimelineMicros'] = request
            ..['platformIngressTimelineMicros'] = request + 1
            ..['acceptedTimelineMicros'] = request + 2
            ..['rasterFinishTimelineMicros'] = request + 4;
          final frame = frames.firstWhere(
            (candidate) => candidate['frameNumber'] == reset['frameNumber'],
          );
          frame
            ..['buildStartTimelineMicros'] = request + 3
            ..['rasterFinishTimelineMicros'] = request + 4
            ..['callbackTimelineMicros'] = request + 5;
        }
      });

      final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
        plan: evidence.plan,
        processes: evidence.processes,
        runGroups: evidence.groups,
        exclusiveMachineAttested: true,
        dryRun: false,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('reset does not complete before transition'),
      );
    });

    test('rejects a reset that begins before its paste raster callback', () {
      final evidence = _buildCompleteEvidence(temporary);
      _rewriteFirstQuillPasteContracts(evidence, (contract) {
        final transition = (contract['transitions'] as List).cast<Map>().first;
        final reset = (transition['resetInput'] as Map).cast<String, Object?>();
        final resetEvidence = (reset['evidence'] as Map)
            .cast<String, Object?>();
        final frame = (resetEvidence['frame'] as Map).cast<String, Object?>();
        const request = 1003;
        resetEvidence
          ..['actionStartTraceMicros'] = request
          ..['nativeIngressTraceMicros'] = request + 1
          ..['acceptedTraceMicros'] = request + 2;
        frame
          ..['buildStartMicros'] = request + 3
          ..['rasterFinishMicros'] = request + 4
          ..['frameTimingCallbackTraceMicros'] = request + 5;
      });

      final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
        plan: evidence.plan,
        processes: evidence.processes,
        runGroups: evidence.groups,
        exclusiveMachineAttested: true,
        dryRun: false,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('not fully accepted and rastered before its reset begins'),
      );
    });

    test('rejects missing warmup paste evidence', () {
      final evidence = _buildCompleteEvidence(temporary);
      _rewriteFirstQuillPasteContracts(evidence, (contract) {
        final transition = (contract['transitions'] as List).cast<Map>().first;
        transition.remove('pasteInput');
      });

      final result = PeerSuiteValidator.testOnly(_smallFixtures).validate(
        plan: evidence.plan,
        processes: evidence.processes,
        runGroups: evidence.groups,
        exclusiveMachineAttested: true,
        dryRun: false,
      );
      expect(result.completionEnvelopeEligible, isFalse);
      expect(
        result.completionEnvelopeBlockers.join('\n'),
        contains('missing warmup or measured input ordering evidence'),
      );
    });
  });
}

void _rewriteFirstSuperEditorPasteTimeline(
  _CompleteEvidence evidence,
  void Function(Map<String, Object?> timeline) mutation,
) {
  final index = evidence.processes.indexWhere((process) {
    final entry = evidence.plan.entries.firstWhere(
      (candidate) => candidate.id == process.planEntryId,
    );
    return entry.peer == 'super_editor' && entry.workload == 'paste-32kib';
  });
  final payload =
      (jsonDecode(File(evidence.processes[index].resultPath).readAsStringSync())
              as Map)
          .cast<String, Object?>();
  final artifacts = (payload['artifacts'] as Map).cast<String, Object?>();
  final timelineArtifact = (artifacts['rawTimeline'] as Map)
      .cast<String, Object?>();
  final timelineFile = File(timelineArtifact['path']! as String);
  final timeline = (jsonDecode(timelineFile.readAsStringSync()) as Map)
      .cast<String, Object?>();
  mutation(timeline);
  timelineFile.writeAsStringSync(jsonEncode(timeline));
  timelineArtifact['sha256'] = sha256File(timelineFile);
  _writeReplacementResult(evidence, index, payload);
}

void _rewriteFirstQuillPasteContracts(
  _CompleteEvidence evidence,
  void Function(Map<String, Object?> contract) mutation,
) {
  final index = evidence.processes.indexWhere((process) {
    final entry = evidence.plan.entries.firstWhere(
      (candidate) => candidate.id == process.planEntryId,
    );
    return entry.peer == 'flutter_quill' && entry.workload == 'paste-32kib';
  });
  _rewriteResult(evidence, index, (payload) {
    final topLevel = (payload['pasteStateContract'] as Map)
        .cast<String, Object?>();
    final scenario = (payload['scenarioResult'] as Map).cast<String, Object?>();
    final nested = (scenario['pasteStateContract'] as Map)
        .cast<String, Object?>();
    mutation(topLevel);
    mutation(nested);
  });
}

void _rewriteResult(
  _CompleteEvidence evidence,
  int index,
  void Function(Map<String, Object?> payload) mutation,
) {
  final payload =
      (jsonDecode(File(evidence.processes[index].resultPath).readAsStringSync())
              as Map)
          .cast<String, Object?>();
  mutation(payload);
  _writeReplacementResult(evidence, index, payload);
}

void _writeReplacementResult(
  _CompleteEvidence evidence,
  int index,
  Map<String, Object?> payload,
) {
  final record = evidence.processes[index];
  final resultFile = File(record.resultPath)
    ..writeAsStringSync(jsonEncode(payload));
  final replacement = record.toJson()
    ..['resultSha256'] = sha256File(resultFile);
  evidence.processes[index] = PeerProcessEvidence.fromJson(replacement);
}

_CompleteEvidence _buildCompleteEvidence(Directory root) {
  final plan = PeerSuitePlan.protocol();
  final processes = <PeerProcessEvidence>[];
  final groups = <RunGroupEvidence>[];
  final sharedExports = <String, File>{};
  final base = DateTime.utc(2026, 8, 8, 12);

  for (var group = 0; group < 3; group += 1) {
    final groupBase = base.add(Duration(hours: group * 8));
    final idleFinished = groupBase.add(const Duration(minutes: 5));
    final entries = plan.entries
        .where((entry) => entry.groupIndex == group)
        .toList(growable: false);
    DateTime? firstStart;
    DateTime? lastFinish;
    for (var index = 0; index < entries.length; index += 1) {
      final entry = entries[index];
      final directory = Directory('${root.path}/${entry.id}')
        ..createSync(recursive: true);
      final expectedFinal = _smallExpectedFinal(entry, entry.peer);
      final expectedHash = sha256Text(expectedFinal);
      final sharedExport = sharedExports.putIfAbsent(expectedHash, () {
        final file = File('${root.path}/shared-$expectedHash.md');
        file.writeAsStringSync(expectedFinal);
        return file;
      });
      final export = File('${directory.path}/final-source.md');
      Link(export.path).createSync(sharedExport.path);
      final exportHash = sha256File(export);
      final timeline = File('${directory.path}/raw-timeline.json');
      final payload = entry.peer == 'flutter_quill'
          ? _quillPayload(entry, export, exportHash)
          : _superEditorPayload(entry, export, exportHash, timeline);
      final result = File('${directory.path}/result.json')
        ..writeAsStringSync(jsonEncode(payload));
      final stdoutFile = File('${directory.path}/stdout.log')
        ..writeAsStringSync('stdout:${entry.id}');
      final stderrFile = File('${directory.path}/stderr.log')
        ..writeAsStringSync('');
      final started = idleFinished.add(Duration(seconds: index * 2));
      final finished = started.add(const Duration(seconds: 1));
      firstStart ??= started;
      lastFinish = finished;
      processes.add(
        PeerProcessEvidence(
          evidenceId: '${entry.id}-evidence',
          planEntryId: entry.id,
          processId: 10000 + entry.orderSlot,
          startedAtUtc: started,
          finishedAtUtc: finished,
          exitCode: 0,
          timedOut: false,
          argv: ['/profile/app', entry.id],
          cwd: directory.path,
          environmentOverrides: entry.peer == 'flutter_quill'
              ? {
                  'COMPETITOR_SCENARIO': entry.workload,
                  'COMPETITOR_TARGET_BYTES': '${entry.targetBytes}',
                  'COMPETITOR_LOCATION': entry.location,
                  'COMPETITOR_RUN_INDEX': '${entry.replicate}',
                  'COMPETITOR_ORDER_INDEX': '${entry.orderSlot}',
                  'COMPETITOR_PROCESS_RUN_ID': entry.id,
                  'COMPETITOR_OUTPUT_PATH': result.path,
                  'COMPETITOR_EXPORT_PATH': export.path,
                }
              : const {},
          resultPath: result.path,
          resultSha256: sha256File(result),
          stdoutPath: stdoutFile.path,
          stdoutSha256: sha256File(stdoutFile),
          stderrPath: stderrFile.path,
          stderrSha256: sha256File(stderrFile),
        ),
      );
    }
    groups.add(
      RunGroupEvidence(
        groupIndex: group,
        idleStartedAtUtc: groupBase,
        idleFinishedAtUtc: idleFinished,
        firstProcessStartedAtUtc: firstStart!,
        lastProcessFinishedAtUtc: lastFinish!,
      ),
    );
  }
  return _CompleteEvidence(plan, processes, groups);
}

Map<String, Object?> _quillPayload(
  PeerSuiteEntry entry,
  File export,
  String exportHash,
) {
  final pasteStateContract = entry.workload == 'paste-32kib'
      ? _pasteStateContract(entry, entry.peer)
      : null;
  final measured = pasteStateContract == null
      ? List<Object?>.generate(_expectedSamples(entry.workload), (index) {
          final accepted = 100 + index * 10;
          return {
            'action': entry.workload == 'local-insert-delete'
                ? (index.isEven ? 'insert-x' : 'delete-x')
                : 'type-character',
            'sampleIndex': entry.workload == 'local-insert-delete'
                ? index ~/ 2
                : index,
            'measured': true,
            'acceptedTraceMicros': accepted,
            'frameCorrelation': {'proven': true},
            'frame': {
              'buildStartMicros': accepted + 1,
              'rasterFinishMicros': accepted + 2,
              'frameTimingCallbackTraceMicros': accepted + 3,
              'buildDurationMicros': 1,
              'rasterDurationMicros': 1,
              'totalSpanMicros': 2,
            },
          };
        })
      : ((_pasteTransitions(pasteStateContract).skip(2).map((transition) {
          final link = (transition['pasteInput'] as Map)
              .cast<String, Object?>();
          return Map<String, Object?>.from(
            (link['evidence'] as Map).cast<String, Object?>(),
          );
        }).toList()));
  return {
    'schemaVersion': 1,
    'peer': entry.peer,
    'claimEligible': false,
    'performanceClaimEligible': false,
    'completionEnvelopeEligible': true,
    'config': {
      'protocolId': peerSuiteProtocolId,
      'scenario': entry.workload,
      'targetBytes': entry.targetBytes,
      'location': entry.location,
      'runIndex': entry.replicate,
      'orderIndex': entry.orderSlot,
      'processRunId': entry.id,
      'nonClaimRun': false,
      'typingWarmups': 20,
      'typingSamples': 200,
      'typingCadenceHz': 10,
      'localWarmupPairs': 10,
      'localSamplePairs': 100,
      'pasteWarmups': 2,
      'pasteSamples': 20,
      'inputTimeoutSeconds': 60,
      'completionEnvelopeConfigurationEligible': true,
    },
    'fixture': _fixture(entry.targetBytes),
    'initialFidelity': {
      'exact': true,
      'expectedSha256': _fixtureHash(entry.targetBytes),
      'expectedUtf8Bytes': entry.targetBytes,
    },
    'coldOpen': {
      'processStartToInteractiveRasterFinishMicros': 20,
      'documentLoadStartToRasterFinishMicros': 10,
      'interactiveVerification': {
        'focusNodeHasFocus': true,
        'editorStateMounted': true,
        'sourcePrefixMatchesFixture': true,
        'viewportLogicalWidth': 600.0,
        'viewportLogicalHeight': 600.0,
      },
      'frame': {'buildStartMicros': 1, 'rasterFinishMicros': 2},
    },
    'scenarioResult': {
      'rawSamples': measured,
      'pasteStateContract': ?pasteStateContract,
      if (entry.workload != 'cold-open') 'maxInputBacklogUntilRaster': 1,
      if (entry.workload != 'cold-open')
        'distributions': {
          'acceptedInputToRasterFinishMicros': _distribution(
            _expectedSamples(entry.workload),
          ),
        },
    },
    'pasteStateContract': ?pasteStateContract,
    'finalFidelity': {
      'exact': true,
      'expectedSha256': exportHash,
      'expectedUtf8Bytes': export.lengthSync(),
      'actualSha256': exportHash,
      'actualUtf8Bytes': export.lengthSync(),
    },
    'finalExportArtifact': {
      'written': true,
      'path': export.path,
      'sha256': exportHash,
      'utf8Bytes': export.lengthSync(),
    },
    'memory': {
      'afterWorkload': {'peakResidentBytes': 2, 'residentBytes': 1},
    },
  };
}

Map<String, Object?> _superEditorPayload(
  PeerSuiteEntry entry,
  File export,
  String exportHash,
  File timeline,
) {
  final expectedSamples = _expectedSamples(entry.workload);
  final pasteStateContract = entry.workload == 'paste-32kib'
      ? _pasteStateContract(entry, entry.peer)
      : null;
  final rawTimeline = pasteStateContract == null
      ? <String, Object?>{
          'frames': List<Object?>.generate(expectedSamples, (index) {
            final accepted = 100 + index * 10;
            return {
              'frameNumber': index + 7,
              'buildStartTimelineMicros': accepted + 1,
              'rasterFinishTimelineMicros': accepted + 2,
            };
          }),
          'inputs': List<Object?>.generate(expectedSamples, (index) {
            return {
              'sequence': index,
              'measured': true,
              'acceptedTimelineMicros': 100 + index * 10,
              'frameNumber': index + 7,
              'failure': null,
            };
          }),
        }
      : _syntheticSuperEditorPasteTimeline(pasteStateContract);
  timeline.writeAsStringSync(
    jsonEncode({...rawTimeline, 'pasteStateContract': ?pasteStateContract}),
  );
  return {
    'schemaVersion': 1,
    'peer': entry.peer,
    'claimEligible': false,
    'performanceClaimEligible': false,
    'profileMode': true,
    'protocolConformant': true,
    'completion': 'complete',
    'config': {
      'protocolId': peerSuiteProtocolId,
      'workload': entry.workload,
      'targetBytes': entry.targetBytes,
      'location': entry.location,
      ..._superEditorCounts(entry.workload),
      'timeoutMicros': 60000000,
    },
    'fixture': _fixture(entry.targetBytes),
    'pasteStateContract': ?pasteStateContract,
    'fidelity': {
      'pass': true,
      'initialSourceSha256': _fixtureHash(entry.targetBytes),
      'expectedFinalSourceSha256': exportHash,
      'exportedFinalSourceSha256': exportHash,
      'exportedFinalSourceBytes': export.lengthSync(),
    },
    'artifacts': {
      'finalExport': {'path': export.path, 'sha256': exportHash},
      'rawTimeline': {'path': timeline.path, 'sha256': sha256File(timeline)},
    },
    'measurements': {
      'measuredSampleCount': expectedSamples,
      'maxInputBacklog': 1,
      'peakSampledRssBytes': 2,
      'retainedRssBytes': 1,
      if (entry.workload != 'cold-open')
        'inputToRasterMicros': _distribution(expectedSamples),
      'longestSynchronousSpan': {'supported': false, 'reason': 'not captured'},
    },
    'coldOpen': {
      'documentLoadToInteractiveRasterMicros': 10,
      'interactiveFrame': {
        'buildStartTimelineMicros': 1,
        'rasterFinishTimelineMicros': 2,
      },
      'endpointEvidence': {
        'focus': true,
        'imeConnected': true,
        'expectedLeadingTextInRenderedModel': true,
        'rasterTimingReceived': true,
        'viewportLogicalWidth': 600.0,
        'viewportLogicalHeight': 600.0,
      },
    },
    'driver': {
      'watchdogTimedOut': false,
      'processId': 50000 + entry.orderSlot,
      'processLaunchRequestedAtUtc': DateTime.utc(
        2026,
        8,
        8,
      ).add(Duration(seconds: entry.orderSlot)).toIso8601String(),
      'invocation': {'runId': entry.id},
      'runControl': {
        'runGroupId': 'group-${entry.groupIndex}',
        'orderSlot': '${entry.orderSlot}',
      },
    },
  };
}

Map<String, Object?> _fixture(int bytes) => {
  'generatorId': 'flark-v4-deterministic-markdown-v1',
  'shapeId': 'ordinary-prose',
  'encoding': 'UTF-8',
  'normalization': 'none',
  'targetBytes': bytes,
  'actualBytes': bytes,
  'sha256': _fixtureHash(bytes),
};

String _fixtureHash(int bytes) => sha256Text(_smallFixtures[bytes]!);

String _smallExpectedFinal(PeerSuiteEntry entry, String peer) {
  if (!protocolPeers.contains(peer)) throw ArgumentError.value(peer, 'peer');
  final fixture = _smallFixtures[entry.targetBytes]!;
  final offset = switch (entry.location) {
    'start' => 0,
    'middle' => fixture.length ~/ 2,
    'end' => fixture.length,
    _ => throw StateError('unknown location ${entry.location}'),
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
  return fixture;
}

Map<String, Object?> _pasteStateContract(PeerSuiteEntry entry, String peer) {
  if (!protocolPeers.contains(peer)) throw ArgumentError.value(peer, 'peer');
  final fixture = _smallFixtures[entry.targetBytes]!;
  final paste = _smallFixtures[32768]!;
  final offset = switch (entry.location) {
    'start' => 0,
    'middle' => fixture.length ~/ 2,
    'end' => fixture.length,
    _ => throw StateError('unknown location ${entry.location}'),
  };
  final pasted =
      '${fixture.substring(0, offset)}$paste'
      '${fixture.substring(offset)}';
  Map<String, Object?> denominator(String source) => {
    'utf8Bytes': utf8.encode(source).length,
    'sha256': sha256Text(source),
  };
  Map<String, Object?> proof(String source) => {
    'canonicalUtf8Bytes': utf8.encode(source).length,
    'canonicalSha256': sha256Text(source),
    'rawUtf8Bytes': utf8.encode(source).length,
    'rawSha256': sha256Text(source),
    'classification': 'exact',
    'matchesExpectedCanonical': true,
  };
  return {
    'schemaVersion': 1,
    'mode': 'reset-after-each-paste',
    'pasteViaPlatformInput': true,
    'resetViaPlatformBackspace': true,
    'selectionForReset': 'programmatic-exact-pasted-source-range',
    'warmupTransitions': 2,
    'measuredTransitions': 20,
    'baseState': denominator(fixture),
    'singlePasteState': denominator(pasted),
    'expectedFinalState': denominator(fixture),
    'transitions': List<Object?>.generate(22, (index) {
      final pasteSequence = index * 2;
      final resetSequence = pasteSequence + 1;
      final pasteEvidence = _syntheticQuillInputEvidence(
        sequence: pasteSequence,
        transitionIndex: index,
        role: 'paste-workload',
        action: 'paste-32kib',
        measured: index >= 2,
        request: 1000 + index * 100,
      );
      final resetEvidence = _syntheticQuillInputEvidence(
        sequence: resetSequence,
        transitionIndex: index,
        role: 'paste-reset',
        action: 'paste-cleanup-delete',
        measured: false,
        request: 1010 + index * 100,
      );
      return {
        'transitionIndex': index,
        'measured': index >= 2,
        'pasteInput': {
          'evidenceSequence': pasteSequence,
          if (peer == 'flutter_quill') 'evidence': pasteEvidence,
        },
        'preState': proof(fixture),
        'postState': proof(pasted),
        'resetState': proof(fixture),
        'resetInput': {
          'operation': 'platform-backspace-over-exact-pasted-range',
          'measured': false,
          'accepted': true,
          'rastered': true,
          'platformInputDispatched': true,
          'selectionStart': offset,
          'selectionEnd': offset + paste.length,
          'evidenceSequence': resetSequence,
          if (peer == 'flutter_quill') 'evidence': resetEvidence,
        },
      };
    }),
  };
}

List<Map<String, Object?>> _pasteTransitions(Map<String, Object?> contract) =>
    (contract['transitions']! as List)
        .map((value) => (value as Map).cast<String, Object?>())
        .toList(growable: false);

Map<String, Object?> _syntheticQuillInputEvidence({
  required int sequence,
  required int transitionIndex,
  required String role,
  required String action,
  required bool measured,
  required int request,
}) => {
  'inputSequence': sequence,
  'stateTransitionIndex': transitionIndex,
  'evidenceRole': role,
  'action': action,
  'sampleIndex': role == 'paste-workload' && measured
      ? transitionIndex - 2
      : transitionIndex,
  'measured': measured,
  'actionStartTraceMicros': request,
  'nativeIngressTraceMicros': request + 1,
  'acceptedTraceMicros': request + 2,
  'nativeInput': {'dispatchEpochMicros': request + 1},
  'frameCorrelation': {'proven': true},
  'frame': {
    'buildStartMicros': request + 3,
    'rasterFinishMicros': request + 4,
    'frameTimingCallbackTraceMicros': request + 5,
    'buildDurationMicros': 1,
    'rasterDurationMicros': 1,
    'totalSpanMicros': 2,
  },
};

Map<String, Object?> _syntheticSuperEditorPasteTimeline(
  Map<String, Object?> contract,
) {
  final frames = <Object?>[];
  final inputs = <Object?>[];
  final resetInputs = <Object?>[];
  final transitions = _pasteTransitions(contract);
  for (var index = 0; index < 22; index += 1) {
    final transition = transitions[index];
    final pasteLink = (transition['pasteInput']! as Map)
        .cast<String, Object?>();
    final resetLink = (transition['resetInput']! as Map)
        .cast<String, Object?>();
    final pasteSequence = pasteLink['evidenceSequence']! as int;
    final resetSequence = resetLink['evidenceSequence']! as int;
    final pasteRequest = 1000 + index * 100;
    final resetRequest = pasteRequest + 10;
    final pasteFrameNumber = 100 + index * 2;
    final resetFrameNumber = pasteFrameNumber + 1;
    frames.addAll([
      {
        'frameNumber': pasteFrameNumber,
        'buildStartTimelineMicros': pasteRequest + 3,
        'rasterFinishTimelineMicros': pasteRequest + 4,
        'callbackTimelineMicros': pasteRequest + 5,
      },
      {
        'frameNumber': resetFrameNumber,
        'buildStartTimelineMicros': resetRequest + 3,
        'rasterFinishTimelineMicros': resetRequest + 4,
        'callbackTimelineMicros': resetRequest + 5,
      },
    ]);
    inputs.add({
      'sequence': pasteSequence,
      'operation': 'paste',
      'evidenceRole': 'paste-workload',
      'measured': index >= 2,
      'stateTransitionIndex': index,
      'payloadBytes': 32768,
      'requestedTimelineMicros': pasteRequest,
      'platformIngressTimelineMicros': pasteRequest + 1,
      'acceptedTimelineMicros': pasteRequest + 2,
      'rasterFinishTimelineMicros': pasteRequest + 4,
      'frameNumber': pasteFrameNumber,
      'failure': null,
      'nativeEvent': {'platformRouteInvoked': true},
    });
    resetInputs.add({
      'sequence': resetSequence,
      'operation': 'backspace',
      'evidenceRole': 'paste-reset',
      'measured': false,
      'stateTransitionIndex': index,
      'pair': index,
      'payloadBytes': 0,
      'requestedTimelineMicros': resetRequest,
      'platformIngressTimelineMicros': resetRequest + 1,
      'acceptedTimelineMicros': resetRequest + 2,
      'rasterFinishTimelineMicros': resetRequest + 4,
      'frameNumber': resetFrameNumber,
      'failure': null,
      'nativeEvent': {
        'eventPath': 'NSApplication.postEvent-to-Flutter-macOS-text-input',
      },
    });
  }
  return {'frames': frames, 'inputs': inputs, 'resetInputs': resetInputs};
}

int _expectedSamples(String workload) => switch (workload) {
  'cold-open' => 0,
  'sustained-typing' => 200,
  'local-insert-delete' => 200,
  'paste-32kib' => 20,
  _ => throw StateError('unknown workload $workload'),
};

Map<String, Object?> _superEditorCounts(String workload) => switch (workload) {
  'cold-open' => const {
    'warmupCount': 0,
    'sampleCount': 1,
    'cadenceMillis': 0,
    'pasteBytes': 32768,
  },
  'sustained-typing' => const {
    'warmupCount': 20,
    'sampleCount': 200,
    'cadenceMillis': 100,
    'pasteBytes': 32768,
  },
  'local-insert-delete' => const {
    'warmupCount': 10,
    'sampleCount': 100,
    'cadenceMillis': 0,
    'pasteBytes': 32768,
  },
  'paste-32kib' => const {
    'warmupCount': 2,
    'sampleCount': 20,
    'cadenceMillis': 0,
    'pasteBytes': 32768,
  },
  _ => throw StateError('unknown workload $workload'),
};

Map<String, Object?> _distribution(int count) => {
  'count': count,
  'p50': 2,
  'p90': 2,
  'p99': 2,
  'max': 2,
};

final class _CompleteEvidence {
  _CompleteEvidence(this.plan, this.processes, this.groups);

  final PeerSuitePlan plan;
  final List<PeerProcessEvidence> processes;
  final List<RunGroupEvidence> groups;
}
