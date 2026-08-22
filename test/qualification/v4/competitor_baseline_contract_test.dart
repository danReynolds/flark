import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

const _baselinePath = 'benchmark/v4/competitor_baseline_v1.json';

void main() {
  final baselineFile = File(_baselinePath);
  final baseline = _map(jsonDecode(baselineFile.readAsStringSync()));
  final historical = _object(baseline, 'historicalSeed');
  final protocol = _object(baseline, 'macFirstProtocol');

  group('competitor historical seed', () {
    test('is explicit non-claim evidence with pinned source artifacts', () {
      expect(baseline['schemaVersion'], 1);
      expect(baseline['baselineId'], 'm0-leading-editor-cohort-v1');
      expect(baseline['claimEligible'], isFalse);
      expect(baseline['mayResolveCompetitorDerivedSizeTiers'], isFalse);
      final cohortScope = _object(baseline, 'cohortScope');
      expect(
        cohortScope['selectionStatus'],
        'selected-leading-relevant-embeddable-flutter-editor-sdk-cohort',
      );
      expect(cohortScope['leadingMarketClaim'], isFalse);
      expect(cohortScope['boundaryScope'], contains('not a global'));
      expect(
        _list(cohortScope, 'selectionRationale'),
        hasLength(greaterThanOrEqualTo(3)),
      );
      final selection = _object(baseline, 'cohortSelectionSnapshot');
      expect(selection['retrievedAtUtc'], '2026-08-09T02:06:28Z');
      expect(
        selection['criterionId'],
        'top-two-eligible-native-flutter-editor-sdks-by-pub-like-count-v1',
      );
      final candidates = _list(selection, 'candidates').map(_map).toList();
      expect(candidates.map((candidate) => candidate['id']).toSet(), {
        'flutter_quill',
        'super_editor',
        'appflowy_editor',
        'fleather',
      });
      expect(
        candidates.map((candidate) => candidate['scoreEndpoint']),
        everyElement(startsWith('https://pub.dev/api/packages/')),
      );
      final ranked =
          candidates
              .where((candidate) => candidate['eligible'] == true)
              .toList()
            ..sort(
              (left, right) => (right['likeCount']! as int).compareTo(
                left['likeCount']! as int,
              ),
            );
      final selectedByCriterion = ranked
          .take(2)
          .map((candidate) => candidate['id'])
          .toList();
      expect(selectedByCriterion, _list(selection, 'selectedIds'));
      expect(selectedByCriterion, _list(cohortScope, 'included'));
      for (final candidate in candidates) {
        expect(
          candidate['selected'],
          selectedByCriterion.contains(candidate['id']),
          reason: '${candidate['id']} selection drifted from the frozen rule',
        );
      }
      expect(selection['leadingClaimScope'], contains('no global'));
      expect(historical['evidenceClass'], 'historical-debug-test-vm-seed');
      expect(historical['claimEligible'], isFalse);

      final environment = _object(historical, 'environment');
      expect(environment['buildMode'], 'debug-test-vm');
      expect(environment['flutter'], '3.41.9');
      expect(environment['dart'], '3.11.5');
      expect(environment['hardware'], isNull);
      expect(environment['repositoryCommit'], isNull);
      expect(environment['trackedDependencyLockfiles'], isFalse);
      expect(_list(environment, 'rawSampleArtifacts'), isEmpty);
      expect(
        _list(historical, 'limitations'),
        hasLength(greaterThanOrEqualTo(8)),
      );

      final artifacts = _list(baseline, 'sourceArtifacts').map(_map).toList();
      expect(artifacts, hasLength(10));
      expect(
        baseline['sourceArtifactAuthority'],
        'git-object-at-inventory-commit',
      );
      final inventoryCommit = baseline['inventoryCommit']! as String;
      for (final artifact in artifacts) {
        final path = artifact['path']! as String;
        final object = Process.runSync(
          'git',
          ['show', '$inventoryCommit:$path'],
          stdoutEncoding: null,
          stderrEncoding: null,
        );
        expect(
          object.exitCode,
          0,
          reason: 'missing historical object $inventoryCommit:$path',
        );
        expect(
          sha256.convert(object.stdout! as List<int>).toString(),
          artifact['sha256'],
          reason: '$path does not match its pinned historical Git object',
        );
      }
    });

    test('preserves the exact debug block-count values', () {
      final actual = <String, List<int>>{};
      for (final competitorValue in _list(historical, 'competitors')) {
        final competitor = _map(competitorValue);
        final competitorId = competitor['id']! as String;
        for (final resultValue in _list(competitor, 'blockEditPumpResults')) {
          final result = _map(resultValue);
          expect(result['warmupCount'], 5);
          expect(result['sampleCount'], 40);
          actual['$competitorId:${result['blockCount']}'] = [
            result['medianMicros']! as int,
            result['p95Micros']! as int,
          ];
        }
      }

      expect(actual, const <String, List<int>>{
        'flutter_quill:10': [6820, 9270],
        'flutter_quill:20': [8590, 30480],
        'flutter_quill:40': [9780, 28780],
        'flutter_quill:80': [8530, 16560],
        'super_editor:10': [7200, 12310],
        'super_editor:20': [8460, 15080],
        'super_editor:40': [7350, 16730],
        'super_editor:80': [7830, 12060],
      });
    });

    test('preserves large-document values without relabeling decimal chars', () {
      final actual = <String, List<int>>{};
      for (final competitorValue in _list(historical, 'competitors')) {
        final competitor = _map(competitorValue);
        final competitorId = competitor['id']! as String;
        for (final resultValue in _list(competitor, 'largeDocumentResults')) {
          final result = _map(resultValue);
          expect(result['actualCharacters'], isNull);
          expect(result['exactByteTier'], isFalse);
          expect(result['targetCharacters'], anyOf(100000, 1000000));
          actual['$competitorId:${result['sizeLabel']}:${result['operation']}'] =
              [
                result['warmupCount']! as int,
                result['sampleCount']! as int,
                result['medianMicros']! as int,
                result['p95Micros']! as int,
              ];
        }
      }

      expect(actual, const <String, List<int>>{
        'flutter_quill:100KB:model-build': [3, 12, 41840, 58900],
        'flutter_quill:100KB:edit-apply': [8, 40, 5720, 15230],
        'flutter_quill:100KB:edit-pump': [5, 20, 30230, 81440],
        'flutter_quill:1MB:model-build': [1, 5, 3770000, 5070000],
        'flutter_quill:1MB:edit-apply': [4, 20, 1110000, 2280000],
        'flutter_quill:1MB:edit-pump': [3, 10, 211380, 450180],
        'super_editor:100KB:model-build': [3, 12, 1580, 7050],
        'super_editor:100KB:edit-apply': [8, 40, 143, 356],
        'super_editor:100KB:edit-pump': [5, 20, 26690, 44820],
        'super_editor:1MB:model-build': [1, 5, 4410, 11510],
        'super_editor:1MB:edit-apply': [4, 20, 1160, 4070],
        'super_editor:1MB:edit-pump': [3, 10, 128630, 134130],
      });

      final competitors = {
        for (final value in _list(historical, 'competitors'))
          (_map(value)['id']! as String): _map(value),
      };
      expect(competitors.keys, {'flutter_quill', 'super_editor'});
      expect(
        _object(
          competitors['flutter_quill']!,
          'dependency',
        )['declaredConstraint'],
        '^11.5.0',
      );
      final superDependency = _object(
        competitors['super_editor']!,
        'dependency',
      );
      expect(
        superDependency['sourceRevision'],
        '22853bcc89def2b234017202a3f3cac36d3c088f',
      );
      expect(superDependency['locallyPatched'], isTrue);
      expect(superDependency['compatibilityPatchArtifact'], isNull);
    });
  });

  group('Mac-first competitor protocol', () {
    test(
      'pins profile runners, exact byte tiers, and workload denominator',
      () {
        expect(protocol['protocolId'], 'm0-mac-competitor-profile-v1');
        expect(protocol['status'], 'specified-not-yet-executed');
        expect(protocol['platform'], 'macos');
        expect(protocol['buildMode'], 'profile');
        expect(_list(protocol, 'cohort').toSet(), {
          'flutter_quill',
          'super_editor',
        });

        final runners = _list(protocol, 'requiredRunnerEntrypoints').map(_map);
        expect(runners.map((runner) => runner['path']).toSet(), {
          'benchmark/peer/lib/competitor_profile_harness.dart',
          'benchmark/peer_supereditor/lib/competitor_profile_harness.dart',
        });
        expect(
          runners.every((runner) => runner['existsAtInventory'] == false),
          isTrue,
        );

        expect(
          {
            for (final value in _list(protocol, 'sizeTiers'))
              (_map(value)['id']! as String): _map(value)['bytes'],
          },
          const <String, int>{
            '1mib': 1048576,
            '5mib': 5242880,
            '10mib': 10485760,
          },
        );

        final workloads = {
          for (final value in _list(protocol, 'workloads'))
            (_map(value)['id']! as String): _map(value),
        };
        expect(workloads.keys, {
          'cold-open',
          'sustained-typing',
          'local-insert-delete',
          'paste-32kib',
        });
        expect(workloads['cold-open']!['samplesPerCompetitorAndSize'], 30);
        expect(workloads['sustained-typing']!['cadenceHz'], 10);
        expect(workloads['sustained-typing']!['samplesPerRun'], 200);
        expect(workloads['local-insert-delete']!['locations'], [
          'start',
          'middle',
          'end',
        ]);
        expect(workloads['paste-32kib']!['payloadBytes'], 32768);
      },
    );

    test(
      'requires full provenance and keeps the 10 MiB target independent',
      () {
        final requiredFields = _list(
          protocol,
          'requiredRecordedFields',
        ).cast<String>().join('\n');
        for (final requiredTerm in const [
          'repository commit',
          'lockfile hash',
          'machine model',
          'display refresh rate',
          'actual bytes',
          'p50, p90, p99, and max',
          'retained RSS',
          'source-fidelity',
        ]) {
          expect(requiredFields, contains(requiredTerm));
        }

        final completion = _object(protocol, 'comparableCompletion');
        expect(completion['performanceThreshold'], isNull);
        expect(_list(completion, 'required'), hasLength(5));

        final policy = _object(protocol, 'targetPolicy');
        expect(policy['flarkFixedTenMiBTierBytes'], 10485760);
        expect(
          policy['flarkTenMiBTierIsIndependentOfCompetitorBehavior'],
          isTrue,
        );
        expect(policy['competitorFailureMayLowerFlarkTenMiBTier'], isFalse);
        expect(policy['competitorLatencyMayDefineFlarkThresholds'], isFalse);
        expect(
          policy['competitorSuccessMayCreateSeparateDerivedStretchTier'],
          isTrue,
        );
      },
    );
  });
}

Map<String, Object?> _object(Map<String, Object?> value, String key) =>
    _map(value[key]);

Map<String, Object?> _map(Object? value) =>
    (value as Map).cast<String, Object?>();

List<Object?> _list(Map<String, Object?> value, String key) =>
    (value[key] as List).cast<Object?>();
