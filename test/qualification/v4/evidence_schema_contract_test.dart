// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

import '../../../benchmark/peer_suite/lib/peer_suite.dart';

const _workloadsPath = 'benchmark/v4/workloads_v1.json';
const _schemaPath = 'benchmark/v4/result_v1.schema.json';
const _examplePath = 'benchmark/v4/result_v1.example.json';

void main() {
  final workloadsFile = File(_workloadsPath);
  final schemaFile = File(_schemaPath);
  final exampleFile = File(_examplePath);
  final workloads = _jsonObject(workloadsFile);
  final schema = _jsonObject(schemaFile);
  final example = _jsonObject(exampleFile);
  final schemaValidator = _JsonSchemaValidator(schema);
  late final resolutionGraph = _validResolutionGraph(workloads);
  late final warmedClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'warmed-local-insert',
  );
  late final typingClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'sustained-typing',
  );
  late final coldClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'cold-open',
  );
  late final referenceClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'reference-retarget',
  );
  late final pasteClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'paste-32kib',
  );
  late final deletionClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'sustained-deletion',
  );
  late final appendClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'streaming-append',
  );
  late final undoRedoClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'undo-redo',
  );
  late final fenceClaim = _validClaimEvidence(
    example,
    workloads,
    operationId: 'fence-close-reopen',
  );

  group('workload matrix', () {
    test('freezes the complete size and shape denominator', () {
      expect(workloads['schemaVersion'], 1);
      expect(workloads['matrixId'], 'flark-v4-performance-v1');
      expect(workloads['status'], 'm0-frozen');

      final sizes = {
        for (final value in _list(workloads, 'sizeTiers'))
          _string(_asObject(value, 'size tier'), 'id'): _asObject(
            value,
            'size tier',
          ),
      };
      expect(
        sizes.keys,
        containsAll(<String>{
          '1kib',
          '25kib',
          '100kib',
          '1mib',
          '2mib',
          '5mib',
          '10mib',
          'competitor-boundary',
          'competitor-next-tier',
          'engine-4x-envelope',
        }),
      );
      expect(
        <String, int>{
          for (final id in const <String>[
            '1kib',
            '25kib',
            '100kib',
            '1mib',
            '2mib',
            '5mib',
            '10mib',
          ])
            id: _integer(sizes[id]!, 'bytes'),
        },
        const <String, int>{
          '1kib': 1024,
          '25kib': 25600,
          '100kib': 102400,
          '1mib': 1048576,
          '2mib': 2097152,
          '5mib': 5242880,
          '10mib': 10485760,
        },
      );
      expect(
        _string(
          _object(sizes['engine-4x-envelope']!, 'derivation'),
          'baseSizeTierId',
        ),
        'competitor-boundary',
      );
      expect(
        _integer(_object(sizes['engine-4x-envelope']!, 'derivation'), 'factor'),
        4,
      );
      final boundaryDerivation = _object(
        sizes['competitor-boundary']!,
        'derivation',
      );
      final nextDerivation = _object(
        sizes['competitor-next-tier']!,
        'derivation',
      );
      final fourXDerivation = _object(
        sizes['engine-4x-envelope']!,
        'derivation',
      );
      for (final derivation in <Map<String, Object?>>[
        boundaryDerivation,
        nextDerivation,
        fourXDerivation,
      ]) {
        expect(
          derivation['resolutionReceiptPath'],
          'benchmark/v4/competitor_resolution_v1.json',
        );
        expect(derivation['requiredSuiteId'], 'm0-mac-two-peer-suite-v1');
        expect(
          derivation['requiredProtocolId'],
          'm0-mac-competitor-profile-v1',
        );
      }
      expect(_list(nextDerivation, 'meaningfulTierBytes'), const <int>[
        1048576,
        5242880,
        10485760,
        20971520,
      ]);

      final shapes = _ids(workloads, 'shapeRecipes');
      expect(shapes, const <String>{
        'ordinary-prose',
        'delimiter-dense',
        'giant-paragraph',
        'giant-physical-line',
        'many-tiny-blocks',
        'nested-containers',
        'gfm-tables-task-lists',
        'many-references',
        'open-fence-to-eof',
      });

      final operations = _ids(workloads, 'operationRecipes');
      expect(operations, const <String>{
        'cold-open',
        'warmed-local-insert',
        'sustained-typing',
        'sustained-deletion',
        'streaming-append',
        'undo-redo',
        'paste-32kib',
        'reference-retarget',
        'fence-close-reopen',
      });

      final expectedSampling = <String, List<Object>>{
        'cold-open': const <Object>['fresh-process-open', 0, 1, 30, 0, 30],
        'warmed-local-insert': const <Object>['edit', 20, 200, 3, 0, 600],
        'sustained-typing': const <Object>['edit', 20, 600, 3, 60, 1800],
        'sustained-deletion': const <Object>['edit', 20, 200, 3, 60, 600],
        'streaming-append': const <Object>['append', 4, 128, 3, 0, 384],
        'undo-redo': const <Object>['undo-redo-cycle', 5, 100, 3, 0, 300],
        'paste-32kib': const <Object>['paste', 2, 30, 3, 0, 90],
        'reference-retarget': const <Object>['edit', 10, 100, 3, 0, 300],
        'fence-close-reopen': const <Object>[
          'close-reopen-cycle',
          5,
          100,
          3,
          0,
          300,
        ],
      };
      for (final value in _list(workloads, 'operationRecipes')) {
        final operation = _asObject(value, 'operation recipe');
        final sampling = _object(operation, 'sampling');
        final expected = expectedSampling[_string(operation, 'id')]!;
        expect(<Object?>[
          sampling['iterationUnit'],
          sampling['warmupIterationsPerRun'],
          sampling['sampleIterationsPerRun'],
          sampling['runCount'],
          sampling['cadenceHz'],
          sampling['totalSampleCount'],
        ], expected);
        expect(
          _integer(sampling, 'sampleIterationsPerRun') *
              _integer(sampling, 'runCount'),
          _integer(sampling, 'totalSampleCount'),
        );
      }

      for (final value in _list(workloads, 'shapeRecipes')) {
        final shape = _asObject(value, 'shape recipe');
        final recipe = _object(shape, 'recipe');
        for (final key in const <String>['prefix', 'cycle', 'record']) {
          final literal = recipe[key];
          if (literal is String) {
            expect(
              literal.codeUnits.every((unit) => unit <= 0x7f),
              isTrue,
              reason:
                  '${shape['id']}.$key must remain ASCII for exact truncation',
            );
          }
        }
      }

      final render = _object(workloads, 'renderContract');
      expect(render['viewportLogicalWidth'], 600);
      expect(render['viewportLogicalHeight'], 600);
      expect(render['devicePixelRatio'], 2);
      expect(render['textScaleFactor'], 1);
      expect(render['fontAssetId'], 'FlarkBenchmarkMono-v1');
      expect(render['minimumVisibleCharacters'], 512);

      final rawContract = _object(workloads, 'rawEvidenceContract');
      expect(rawContract['artifactKind'], 'flark-v4-raw-evidence-v1');
      expect(
        _strings(rawContract, 'requiredSampleFields'),
        containsAll(<String>{
          'runId',
          'sampleId',
          'processId',
          'frameId',
          'scheduledMicros',
          'acceptedMicros',
          'sourcePaintMicros',
          'caretPaintMicros',
          'selectionPaintMicros',
        }),
      );
      final peerContract = _object(workloads, 'competitorResolutionContract');
      expect(peerContract['platform'], 'macos');
      expect(peerContract['expectedProcessCount'], 234);
      expect(peerContract['expectedRunGroupCount'], 3);
      expect(
        peerContract['canonicalPlanSha256'],
        '3daf93557b1ac671b4c9a2aaa743276d8d629758999398073bd7da6b2b370d8c',
      );
      expect(_strings(peerContract, 'requiredPeers'), const <String>[
        'flutter_quill',
        'super_editor',
      ]);

      final operationsById = <String, Map<String, Object?>>{
        for (final value in _list(workloads, 'operationRecipes'))
          _string(_asObject(value, 'operation'), 'id'): _asObject(
            value,
            'operation',
          ),
      };
      expect(
        _strings(operationsById['reference-retarget']!, 'destinationCycle'),
        const <String>[
          'https://changed-a.invalid/',
          'https://changed-b.invalid/',
        ],
      );
      expect(
        _list(
          operationsById['paste-32kib']!,
          'steps',
        ).map((step) => _asObject(step, 'step')['action']),
        contains('reset-to-exact-pre-sample-source-and-verify-hash'),
      );
      final expectedStages = <String, List<String>>{
        'cold-open': <String>['before', 'interactive'],
        'warmed-local-insert': <String>['before', 'inserted'],
        'sustained-typing': <String>['before', 'inserted'],
        'sustained-deletion': <String>['before', 'deleted'],
        'streaming-append': <String>['before', 'appended'],
        'undo-redo': <String>['before', 'inserted', 'undone', 'redone'],
        'paste-32kib': <String>['before', 'pasted', 'reset'],
        'reference-retarget': <String>['before', 'retargeted'],
        'fence-close-reopen': <String>['before', 'closed', 'reopened'],
      };
      for (final entry in expectedStages.entries) {
        final stateContract = _object(
          operationsById[entry.key]!,
          'stateContract',
        );
        expect(_strings(stateContract, 'stages'), entry.value);
        expect(stateContract['paintedStage'], isNotEmpty);
        expect(stateContract['finalStage'], isNotEmpty);
      }
      final applicability = _object(workloads, 'metricApplicability');
      final targets = _object(applicability, 'targetSpecific');
      expect(_object(targets, 'engine')['measurementSurface'], 'engine-only');
      expect(
        _object(targets, 'engine')['flutterLatencyAndFrameMetricsRequired'],
        isFalse,
      );

      final expanded = _expandedWorkloadIds(workloads);
      expect(expanded, hasLength(greaterThan(100)));
      expect(
        expanded,
        contains('flark-v4.product.1kib.ordinary-prose.warmed-local-insert'),
      );
      expect(
        expanded,
        contains(
          'flark-v4.engine.engine-4x-envelope.giant-physical-line.cold-open',
        ),
      );
    });

    test('freezes Mac and provisional physical-mobile thresholds', () {
      final profiles = {
        for (final value in _list(workloads, 'thresholdProfiles'))
          _string(_asObject(value, 'threshold profile'), 'id'): _asObject(
            value,
            'threshold profile',
          ),
      };
      final mac = profiles['tier-a-mac-m0-v1']!;
      final macGates = _object(mac, 'gates');
      expect(mac['status'], 'frozen');
      expect(macGates['sourceVisibilityMaxFrames'], 1);
      expect(macGates['caretVisibilityMaxFrames'], 1);
      expect(macGates['selectionVisibilityMaxFrames'], 1);
      expect(macGates['inputBacklogMaxFrames'], 1);
      expect(macGates['engineForegroundP99Micros'], 4000);
      expect(macGates['flutterFrameWorkP99Micros'], 8000);
      expect(macGates['editorAttributedFrameMaxExclusiveMicros'], 16000);
      expect(macGates['synchronousSpanMaxExclusiveMicros'], 16000);
      expect(macGates['editorAttributedDroppedFramesMax'], 0);
      expect(macGates['editorAttributedMissedFrameRateMax'], 0);
      expect(macGates['coldExactViewportPaintMaxExclusiveMicros'], 200000);
      expect(
        macGates['visibleProjectionCertificationMaxExclusiveMicros'],
        500000,
      );
      expect(macGates['liveHandlesAfterCloseMax'], 0);

      final mobile = profiles['tier-b-mobile-provisional-m0-v1']!;
      final requirements = _object(mobile, 'claimRequirements');
      final mobileGates = _object(mobile, 'gates');
      expect(mobile['status'], 'provisional-frozen');
      expect(requirements['namedPhysicalDevice'], isTrue);
      expect(requirements['simulatorForbidden'], isTrue);
      expect(mobileGates['sourceVisibilityMaxFrames'], 1);
      expect(mobileGates['caretVisibilityMaxFrames'], 1);
      expect(mobileGates['selectionVisibilityMaxFrames'], 1);
      expect(mobileGates['engineForegroundP99Micros'], 4000);
      expect(mobileGates['flutterFrameWorkP99Micros'], 8000);
      expect(mobileGates['editorAttributedMissedFrameRateMax'], 0);
      expect(mobileGates['minimumBackgroundForegroundCycles'], 20);
      expect(mobileGates['minimumSustainedRunSeconds'], 1800);
      expect(mobileGates['thermalThrottleEventsMax'], 0);
    });
  });

  group('result schema fixtures', () {
    test(
      'the checked-in synthetic example is structurally and semantically valid',
      () {
        expect(
          sha256.convert(workloadsFile.readAsBytesSync()).toString(),
          _object(example, 'contract')['workloadMatrixSha256'],
        );
        expect(
          sha256.convert(schemaFile.readAsBytesSync()).toString(),
          _object(example, 'contract')['resultSchemaSha256'],
        );

        expect(schemaValidator.validate(example), isEmpty);
        expect(_validateReceiptSemantics(example, workloads), isEmpty);
        expect(example['receiptKind'], 'schema_example');
        expect(example['claimEligible'], isFalse);
      },
    );

    test('schema requires provenance and every evidence family', () {
      expect(
        _stringSet(schema, 'required'),
        containsAll(<String>{
          'measurementSurface',
          'contract',
          'provenance',
          'thresholds',
          'metrics',
          'evaluation',
        }),
      );
      final definitions = _object(schema, r'$defs');
      expect(
        _stringSet(_object(definitions, 'provenance'), 'required'),
        containsAll(<String>{
          'commitSha',
          'commands',
          'fixture',
          'hardware',
          'runtime',
          'toolchain',
          'build',
          'sampling',
        }),
      );
      expect(
        _stringSet(_object(definitions, 'metrics'), 'required'),
        const <String>{
          'latency',
          'foreground',
          'frames',
          'ffi',
          'convergence',
          'memory',
          'lifecycle',
        },
      );
      expect(
        _stringSet(_object(definitions, 'distribution'), 'required'),
        const <String>{'sampleCount', 'p50', 'p90', 'p99', 'max'},
      );
      final provenanceDefinition = _object(definitions, 'provenance');
      final provenanceProperties = _object(provenanceDefinition, 'properties');
      expect(
        _stringSet(provenanceDefinition, 'required'),
        contains('renderSurface'),
      );
      expect(
        _stringSet(
          _asObject(provenanceProperties['sampling'], 'sampling schema'),
          'required',
        ),
        containsAll(<String>{
          'operationId',
          'iterationUnit',
          'warmupIterationsPerRun',
          'sampleIterationsPerRun',
          'runCount',
          'cadenceHz',
          'totalSampleCount',
          'visibleCharacterCount',
        }),
      );
      final fixtureSchema = _asObject(
        provenanceProperties['fixture'],
        'fixture schema',
      );
      expect(_stringSet(fixtureSchema, 'required'), contains('sizeResolution'));
      expect(
        _stringSet(_object(definitions, 'artifact'), 'required'),
        containsAll(<String>{'path', 'byteLength', 'sha256'}),
      );
      final buildSchema = _asObject(provenanceProperties['build'], 'build');
      expect(
        _stringSet(buildSchema, 'required'),
        containsAll(<String>{
          'artifactPath',
          'artifactBytes',
          'artifactSha256',
        }),
      );
      final latencySchema = _asObject(
        _object(_object(definitions, 'metrics'), 'properties')['latency'],
        'latency schema',
      );
      expect(
        _stringSet(latencySchema, 'required'),
        containsAll(<String>{
          'sourceVisibilityFrames',
          'caretVisibilityFrames',
          'selectionVisibilityFrames',
          'sourceVisibilityMicros',
          'caretVisibilityMicros',
          'selectionVisibilityMicros',
        }),
      );
      expect(
        _stringSet(_object(definitions, 'rawSample'), 'required'),
        containsAll(<String>{
          'runId',
          'sampleId',
          'sampleIndex',
          'processId',
          'frameId',
          'measurementStartVsyncOrdinal',
          'measurementEndVsyncOrdinal',
          'scheduledMicros',
          'acceptedMicros',
          'sourcePaintMicros',
          'caretPaintMicros',
          'selectionPaintMicros',
          'operationProof',
        }),
      );
      expect(
        _stringSet(_object(definitions, 'rawEvidence'), 'required'),
        containsAll(<String>{
          'measurementSurface',
          'contract',
          'warmups',
          'frames',
          'renderEvidence',
          'memorySamples',
          'lifecycle',
        }),
      );
      expect(
        _stringSet(_object(definitions, 'rawFrame'), 'required'),
        containsAll(<String>{
          'runId',
          'vsyncOrdinal',
          'sampleId',
          'editorAttributed',
          'workUnitIds',
          'pumpIds',
        }),
      );
      expect(
        _stringSet(_object(definitions, 'rawMemorySample'), 'required'),
        containsAll(<String>{
          'memorySampleId',
          'processId',
          'timestampMicros',
          'phase',
          'residentBytes',
        }),
      );
      expect(
        _stringSet(
          _object(definitions, 'competitorResolutionReceipt'),
          'required',
        ),
        containsAll(<String>{
          'plan',
          'runGroups',
          'processes',
          'completionEnvelopeEligible',
          'completionEnvelopeBlockers',
          'performanceClaimEligible',
          'performanceClaimBlockers',
          'claimEligible',
          'completedTierByPeer',
          'processesValidated',
        }),
      );
    });

    test('rejects an intentionally invalid fixture without a source hash', () {
      final invalid = _deepCopy(example);
      _object(_object(invalid, 'provenance'), 'fixture').remove('sha256');

      expect(
        schemaValidator.validate(invalid),
        contains(r'$.provenance.fixture.sha256 is required'),
      );
    });

    test('rejects an intentionally invalid distribution without p99', () {
      final invalid = _deepCopy(example);
      _distribution(invalid, 'foreground', 'rustEngineMicros').remove('p99');

      expect(
        schemaValidator.validate(invalid),
        contains(r'$.metrics.foreground.rustEngineMicros.p99 is required'),
      );
    });

    test('rejects a syntactically valid PASS that exceeds a hard gate', () {
      final invalid = _deepCopy(example);
      _distribution(invalid, 'foreground', 'rustEngineMicros')['p99'] = 5000;
      _object(
        _object(invalid, 'metrics'),
        'lifecycle',
      )['liveHandlesAfterClose'] = 1;

      expect(schemaValidator.validate(invalid), isEmpty);
      expect(
        _validateReceiptSemantics(invalid, workloads),
        containsAll(<String>{
          'PASS exceeds engineForegroundP99Micros',
          'PASS exceeds liveHandlesAfterCloseMax',
        }),
      );
    });

    test('rejects a typed fault or simulator as a mobile PASS', () {
      final invalid = _deepCopy(example);
      invalid['thresholdProfileId'] = 'tier-b-mobile-provisional-m0-v1';
      invalid['tier'] = 'B_MOBILE';
      invalid['platform'] = 'android';
      invalid['claimEligible'] = true;
      final runtime = _object(_object(invalid, 'provenance'), 'runtime');
      runtime['physicalDevice'] = false;
      runtime['simulator'] = true;
      final convergence = _object(_object(invalid, 'metrics'), 'convergence');
      convergence['terminalState'] = 'typed_fault';
      convergence['terminalReason'] = 'parser-stalled';

      expect(schemaValidator.validate(invalid), isEmpty);
      expect(
        _validateReceiptSemantics(invalid, workloads),
        containsAll(<String>{
          'Tier B PASS requires a named physical device',
          'Tier B PASS cannot use a simulator',
          'PASS requires convergence terminalState complete',
        }),
      );
    });

    test('rejects receipt-owned threshold inflation', () {
      final invalid = _deepCopy(example);
      _object(invalid, 'thresholds')['engineForegroundP99Micros'] = 999999;

      expect(schemaValidator.validate(invalid), isEmpty);
      expect(
        _validateReceiptSemantics(invalid, workloads),
        contains('resolved thresholds differ from frozen threshold profile'),
      );
    });

    test('binds Tier A to macOS and Tier B to Android or iOS', () {
      final invalidMac = _deepCopy(example);
      invalidMac['platform'] = 'android';
      expect(schemaValidator.validate(invalidMac), isEmpty);
      expect(
        _validateReceiptSemantics(invalidMac, workloads),
        contains('Tier A profile requires platform macos'),
      );

      final invalidMobile = _deepCopy(example);
      invalidMobile['thresholdProfileId'] = 'tier-b-mobile-provisional-m0-v1';
      invalidMobile['tier'] = 'B_MOBILE';
      invalidMobile['platform'] = 'macos';
      expect(schemaValidator.validate(invalidMobile), isEmpty);
      expect(
        _validateReceiptSemantics(invalidMobile, workloads),
        contains('Tier B profile requires platform android or ios'),
      );
    });

    test('rejects reduced or mismatched operation sampling denominators', () {
      final reduced = _deepCopy(example);
      final sampling = _object(_object(reduced, 'provenance'), 'sampling');
      sampling['sampleIterationsPerRun'] = 199;
      sampling['totalSampleCount'] = 597;
      _setDistributionSampleCounts(
        _object(reduced, 'metrics'),
        sampleCount: 597,
      );

      expect(schemaValidator.validate(reduced), isEmpty);
      expect(
        _validateReceiptSemantics(reduced, workloads),
        contains('receipt sampling differs from frozen operation sampling'),
      );

      final mismatchedDistribution = _deepCopy(example);
      _distribution(
        mismatchedDistribution,
        'latency',
        'sourceVisibilityFrames',
      )['sampleCount'] = 599;
      expect(schemaValidator.validate(mismatchedDistribution), isEmpty);
      expect(
        _validateReceiptSemantics(mismatchedDistribution, workloads).join('\n'),
        contains('sampleCount 599 differs from frozen total 600'),
      );
    });

    test('rejects a PASS with delayed caret or selection visibility', () {
      for (final metric in const <String>[
        'caretVisibilityFrames',
        'selectionVisibilityFrames',
      ]) {
        final delayed = _deepCopy(example);
        final distribution = _distribution(delayed, 'latency', metric);
        distribution['max'] = 2;

        expect(schemaValidator.validate(delayed), isEmpty);
        final expectedGate = switch (metric) {
          'caretVisibilityFrames' => 'caretVisibilityMaxFrames',
          _ => 'selectionVisibilityMaxFrames',
        };
        expect(
          _validateReceiptSemantics(delayed, workloads),
          contains('PASS exceeds $expectedGate'),
        );
      }
    });

    test('binds fixed tiers to their exact declared UTF-8 byte counts', () {
      final arbitrary = _deepCopy(example);
      final fixture = _object(_object(arbitrary, 'provenance'), 'fixture');
      fixture['targetBytes'] = 1025;
      fixture['actualBytes'] = 1025;
      _object(fixture, 'sizeResolution')['resolvedBytes'] = 1025;

      expect(schemaValidator.validate(arbitrary), isEmpty);
      expect(
        _validateReceiptSemantics(arbitrary, workloads),
        contains('fixed size tier 1kib must resolve to exactly 1024 bytes'),
      );
    });

    test('validates boundary, next-tier, and four-times derivations', () {
      final graph = resolutionGraph;
      final resolutionBytes = graph.receiptBytes;
      final receiptFiles = <String, List<int>>{
        _resolutionReceiptPath: resolutionBytes,
      };
      final cases = <String, int>{
        'competitor-boundary': 10485760,
        'competitor-next-tier': 20971520,
        'engine-4x-envelope': 41943040,
      };
      for (final entry in cases.entries) {
        final derived = _derivedReceipt(
          example,
          sizeTierId: entry.key,
          resolvedBytes: entry.value,
          resolutionReceiptBytes: resolutionBytes,
          workloads: workloads,
        );
        expect(schemaValidator.validate(derived), isEmpty);
        expect(
          _validateReceiptSemantics(
            derived,
            workloads,
            resolutionReceiptBytes: receiptFiles,
            artifactBytes: graph.artifacts,
            peerSuiteValidator: graph.validator,
          ),
          isEmpty,
          reason: entry.key,
        );

        final arbitrary = _deepCopy(derived);
        final fixture = _object(_object(arbitrary, 'provenance'), 'fixture');
        fixture['targetBytes'] = entry.value + 1;
        fixture['actualBytes'] = entry.value + 1;
        _object(fixture, 'sizeResolution')['resolvedBytes'] = entry.value + 1;
        expect(
          _validateReceiptSemantics(
            arbitrary,
            workloads,
            resolutionReceiptBytes: receiptFiles,
            artifactBytes: graph.artifacts,
            peerSuiteValidator: graph.validator,
          ).join('\n'),
          contains('must derive to exactly ${entry.value} bytes'),
          reason: entry.key,
        );
      }
    });

    test('rejects missing or wrong derived-size receipt authority', () {
      final graph = resolutionGraph;
      final resolutionBytes = graph.receiptBytes;
      final derived = _derivedReceipt(
        example,
        sizeTierId: 'competitor-boundary',
        resolvedBytes: 10485760,
        resolutionReceiptBytes: resolutionBytes,
        workloads: workloads,
      );

      final missing = _deepCopy(derived);
      _object(
        _object(_object(missing, 'provenance'), 'fixture'),
        'sizeResolution',
      ).remove('receiptSha256');
      expect(
        schemaValidator.validate(missing),
        contains(
          r'$.provenance.fixture.sizeResolution.receiptSha256 is required',
        ),
      );

      final wrongPath = _deepCopy(derived);
      _object(
        _object(_object(wrongPath, 'provenance'), 'fixture'),
        'sizeResolution',
      )['receiptPath'] = 'benchmark/v4/not-the-frozen-resolution.json';
      expect(
        _validateReceiptSemantics(
          wrongPath,
          workloads,
          resolutionReceiptBytes: <String, List<int>>{
            _resolutionReceiptPath: resolutionBytes,
          },
          artifactBytes: graph.artifacts,
          peerSuiteValidator: graph.validator,
        ),
        contains('derived size receipt path differs from frozen authority'),
      );

      final wrongHash = _deepCopy(derived);
      _object(
            _object(_object(wrongHash, 'provenance'), 'fixture'),
            'sizeResolution',
          )['receiptSha256'] =
          'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff';
      expect(
        _validateReceiptSemantics(
          wrongHash,
          workloads,
          resolutionReceiptBytes: <String, List<int>>{
            _resolutionReceiptPath: resolutionBytes,
          },
          artifactBytes: graph.artifacts,
          peerSuiteValidator: graph.validator,
        ),
        contains(
          'derived size receipt SHA-256 does not match checked-in bytes',
        ),
      );

      final wrongId = _deepCopy(derived);
      _object(
        _object(_object(wrongId, 'provenance'), 'fixture'),
        'sizeResolution',
      )['receiptId'] = 'some-other-receipt';
      expect(
        _validateReceiptSemantics(
          wrongId,
          workloads,
          resolutionReceiptBytes: <String, List<int>>{
            _resolutionReceiptPath: resolutionBytes,
          },
          artifactBytes: graph.artifacts,
          peerSuiteValidator: graph.validator,
        ),
        contains('derived size receipt ID does not match checked-in receipt'),
      );
    });

    test('replays a complete claim from real hash-verified raw artifacts', () {
      expect(schemaValidator.validate(warmedClaim.receipt), isEmpty);
      expect(
        _validateReceiptSemantics(
          warmedClaim.receipt,
          workloads,
          artifactBytes: warmedClaim.artifacts,
        ),
        isEmpty,
      );

      final wrongRecipe = _deepCopy(warmedClaim.receipt);
      _object(_object(wrongRecipe, 'provenance'), 'fixture')['recipeId'] =
          'delimiter-dense';
      expect(
        _validateReceiptSemantics(
          wrongRecipe,
          workloads,
          artifactBytes: warmedClaim.artifacts,
        ),
        contains('fixture recipeId must equal the workload shape'),
      );

      final selfAttestedAggregate = _deepCopy(warmedClaim.receipt);
      _distribution(
        selfAttestedAggregate,
        'foreground',
        'rustEngineMicros',
      )['p99'] = 999;
      expect(
        _validateReceiptSemantics(
          selfAttestedAggregate,
          workloads,
          artifactBytes: warmedClaim.artifacts,
        ),
        contains('receipt metrics differ from replayed raw evidence'),
      );

      final missingRaw = <String, List<int>>{...warmedClaim.artifacts}
        ..remove(warmedClaim.rawPath);
      expect(
        _validateReceiptSemantics(
          warmedClaim.receipt,
          workloads,
          artifactBytes: missingRaw,
        ).join('\n'),
        contains('raw artifact ${warmedClaim.rawPath} does not exist'),
      );
    });

    test('derives visibility from raw timestamps and coherent refresh data', () {
      final delayed = _mutateRawEvidence(warmedClaim, (raw) {
        final sample = _objectList(raw, 'samples').first;
        final accepted = _integer(sample, 'acceptedMicros');
        final delayedPaint = accepted + 16001;
        sample['sourcePaintMicros'] = delayedPaint;
        sample['caretPaintMicros'] = delayedPaint;
        sample['selectionPaintMicros'] = delayedPaint;
        final frameId = _string(sample, 'frameId');
        final frame = _objectList(
          raw,
          'frames',
        ).firstWhere((value) => value['frameId'] == frameId);
        frame['rasterFinishMicros'] = delayedPaint;
      });
      expect(
        _validateReceiptSemantics(
          delayed.receipt,
          workloads,
          artifactBytes: delayed.artifacts,
        ),
        contains(
          'raw source/caret/selection visibility exceeds the interaction limit',
        ),
      );

      final incoherent = _deepCopy(warmedClaim.receipt);
      _object(
        _object(incoherent, 'provenance'),
        'runtime',
      )['displayFramePeriodMicros'] = 10000;
      expect(
        _validateReceiptSemantics(
          incoherent,
          workloads,
          artifactBytes: warmedClaim.artifacts,
        ),
        contains('display refresh and frame period provenance disagree'),
      );

      final fastDisplay = _mutateRawEvidence(warmedClaim, (raw) {
        final sample = _objectList(raw, 'samples').first;
        final accepted = _integer(sample, 'acceptedMicros');
        final delayedPaint = accepted + 9000;
        sample['sourcePaintMicros'] = delayedPaint;
        sample['caretPaintMicros'] = delayedPaint;
        sample['selectionPaintMicros'] = delayedPaint;
        final frame = _objectList(
          raw,
          'frames',
        ).firstWhere((value) => value['frameId'] == sample['frameId']);
        frame['rasterFinishMicros'] = delayedPaint;
      });
      final fastReceipt = fastDisplay.receipt;
      final runtime = _object(_object(fastReceipt, 'provenance'), 'runtime');
      runtime['displayRefreshHz'] = 120;
      runtime['displayFramePeriodMicros'] = 8333.333333333334;
      _object(
        fastReceipt,
        'thresholds',
      )['uncertifiedVisibleCharacterFramesMax'] = 48000;
      expect(
        _validateReceiptSemantics(
          fastReceipt,
          workloads,
          artifactBytes: fastDisplay.artifacts,
        ),
        contains(
          'raw source/caret/selection visibility exceeds the interaction limit',
        ),
      );
    });

    test(
      'recomputes frame, synchronous, distribution, and lifecycle scalars',
      () {
        final scalarLie = _deepCopy(warmedClaim.receipt);
        _object(
          _object(scalarLie, 'metrics'),
          'foreground',
        )['longestSynchronousSpanMicros'] = 0;
        final frames = _object(_object(scalarLie, 'metrics'), 'frames');
        frames['missedFrames'] = 1;
        frames['editorAttributedDroppedFrames'] = 1;
        frames['editorAttributedMissedFrameRate'] = 1 / 600;
        _object(
          _object(scalarLie, 'metrics'),
          'lifecycle',
        )['openEditCloseCycles'] = 99;
        expect(
          _validateReceiptSemantics(
            scalarLie,
            workloads,
            artifactBytes: warmedClaim.artifacts,
          ),
          contains('receipt metrics differ from replayed raw evidence'),
        );

        final rawLongSpan = _mutateRawEvidence(warmedClaim, (raw) {
          final sample = _objectList(raw, 'samples').first;
          final span = _objectList(sample, 'synchronousSpans').first;
          span['finishMicros'] = _integer(span, 'startMicros') + 17000;
        });
        expect(
          _validateReceiptSemantics(
            rawLongSpan.receipt,
            workloads,
            artifactBytes: rawLongSpan.artifacts,
          ),
          contains('receipt metrics differ from replayed raw evidence'),
        );

        final rawMiss = _mutateRawEvidence(warmedClaim, (raw) {
          final frame = _objectList(raw, 'frames').first;
          frame['rasterFinishMicros'] =
              _integer(frame, 'vsyncStartMicros') + 17000;
          final sample = _objectList(raw, 'samples').first;
          sample['sourcePaintMicros'] = frame['rasterFinishMicros'];
          sample['caretPaintMicros'] = frame['rasterFinishMicros'];
          sample['selectionPaintMicros'] = frame['rasterFinishMicros'];
        });
        expect(
          _validateReceiptSemantics(
            rawMiss.receipt,
            workloads,
            artifactBytes: rawMiss.artifacts,
          ).join('\n'),
          contains('receipt metrics differ from replayed raw evidence'),
        );
      },
    );

    test(
      'enforces raw IDs, cadence, required timestamps, and cold processes',
      () {
        final badCadence = _mutateRawEvidence(typingClaim, (raw) {
          final samples = _objectList(raw, 'samples');
          samples.firstWhere(
            (sample) => sample['sampleIndex'] == 1,
          )['scheduledMicros'] = _integer(
                samples.firstWhere((sample) => sample['sampleIndex'] == 1),
                'scheduledMicros',
              ) +
              100;
        });
        expect(
          _validateReceiptSemantics(
            badCadence.receipt,
            workloads,
            artifactBytes: badCadence.artifacts,
          ),
          contains('raw scheduled timestamps violate frozen cadence'),
        );

        final reusedColdProcess = _mutateRawEvidence(coldClaim, (raw) {
          final samples = _objectList(raw, 'samples');
          samples[1]['processId'] = samples[0]['processId'];
        });
        expect(
          _validateReceiptSemantics(
            reusedColdProcess.receipt,
            workloads,
            artifactBytes: reusedColdProcess.artifacts,
          ).join('\n'),
          contains('every cold-open sample must use a distinct process'),
        );

        final missingTimestamp = _mutateRawEvidence(warmedClaim, (raw) {
          _objectList(raw, 'samples').first.remove('caretPaintMicros');
        });
        expect(
          _validateReceiptSemantics(
            missingTimestamp.receipt,
            workloads,
            artifactBytes: missingTimestamp.artifacts,
          ).join('\n'),
          contains('raw replay artifact violates rawEvidence schema'),
        );
      },
    );

    test('replays all peer-resolution authorities and artifact edges', () {
      final graph = resolutionGraph;
      final baseAuthority = _asObject(
        jsonDecode(utf8.decode(graph.receiptBytes)),
        'resolution authority',
      );

      List<String> validateMutation(
        void Function(
          Map<String, Object?> authority,
          Map<String, List<int>> artifacts,
        )
        mutation,
      ) {
        final authority = _deepCopy(baseAuthority);
        final artifacts = <String, List<int>>{...graph.artifacts};
        mutation(authority, artifacts);
        _syncResolutionArtifacts(graph, artifacts);
        final bytes = utf8.encode(jsonEncode(authority));
        final derived = _derivedReceipt(
          example,
          sizeTierId: 'competitor-boundary',
          resolvedBytes: 10485760,
          resolutionReceiptBytes: bytes,
          workloads: workloads,
        );
        return _validateReceiptSemantics(
          derived,
          workloads,
          resolutionReceiptBytes: <String, List<int>>{
            _resolutionReceiptPath: bytes,
          },
          artifactBytes: artifacts,
          peerSuiteValidator: graph.validator,
        );
      }

      expect(
        validateMutation((authority, _) {
          _object(authority, 'plan')['canonicalSha256'] =
              'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff';
        }).join('\n'),
        contains('competitor canonical plan hash/identity is not frozen'),
      );
      expect(
        validateMutation((authority, _) {
          final processes = _list(authority, 'processes');
          processes.removeLast();
          processes.add(_deepCopy(_asObject(processes.first, 'process')));
        }).join('\n'),
        contains('competitor resolution completion authority is ineligible'),
      );
      expect(
        validateMutation((authority, _) {
          _list(authority, 'completionEnvelopeBlockers').add('hidden failure');
        }),
        contains(
          'competitor resolution summary differs from PeerSuiteAssessment',
        ),
      );
      expect(
        validateMutation((authority, _) {
          _object(authority, 'completedTierByPeer')['flutter_quill'] = 5242880;
        }),
        contains(
          'competitor resolution summary differs from PeerSuiteAssessment',
        ),
      );
      expect(
        validateMutation((authority, artifacts) {
          final process = _asObject(
            _list(authority, 'processes').first,
            'process',
          );
          artifacts[_string(process, 'resultPath')] = utf8.encode('tampered');
        }).join('\n'),
        contains('result receipt SHA-256 does not match retained bytes'),
      );
      expect(
        validateMutation((authority, artifacts) {
          for (final group in _objectList(authority, 'runGroups')) {
            group['idleStartedAtUtc'] = group['idleFinishedAtUtc'];
          }
          for (final process in _objectList(authority, 'processes')) {
            final path = _string(process, 'resultPath');
            final payload = _asObject(
              jsonDecode(utf8.decode(artifacts[path]!)),
              'shallow peer result',
            );
            if (payload['peer'] == 'flutter_quill') {
              payload.remove('scenarioResult');
              payload.remove('pasteStateContract');
            } else {
              payload.remove('driver');
              payload.remove('measurements');
              payload.remove('pasteStateContract');
            }
            final bytes = utf8.encode(jsonEncode(payload));
            artifacts[path] = bytes;
            process['resultSha256'] = sha256.convert(bytes).toString();
          }
        }).join('\n'),
        allOf(
          contains('competitor resolution completion authority is ineligible'),
          contains('observed only 0 ms idle'),
        ),
        reason:
            'a 234-file shallow graph cannot substitute for peer-suite '
            'idle/input/paste semantics',
      );
    });

    test('default peer replay cannot select test-only fixture authority', () {
      final graph = resolutionGraph;
      final derived = _derivedReceipt(
        example,
        sizeTierId: 'competitor-boundary',
        resolvedBytes: 10485760,
        resolutionReceiptBytes: graph.receiptBytes,
        workloads: workloads,
      );
      expect(
        _validateReceiptSemantics(
          derived,
          workloads,
          resolutionReceiptBytes: <String, List<int>>{
            _resolutionReceiptPath: graph.receiptBytes,
          },
          artifactBytes: graph.artifacts,
        ).join('\n'),
        contains('competitor resolution completion authority is ineligible'),
      );
    });

    test('replays every frozen operation and run-local source chain', () {
      final claims = <String, _ClaimEvidenceFixture>{
        'warmed-local-insert': warmedClaim,
        'sustained-typing': typingClaim,
        'sustained-deletion': deletionClaim,
        'streaming-append': appendClaim,
        'undo-redo': undoRedoClaim,
        'paste-32kib': pasteClaim,
        'reference-retarget': referenceClaim,
        'fence-close-reopen': fenceClaim,
      };
      for (final entry in claims.entries) {
        expect(
          _validateReceiptSemantics(
            entry.value.receipt,
            workloads,
            artifactBytes: entry.value.artifacts,
          ),
          isEmpty,
          reason: entry.key,
        );
        final noOp = _mutateRawEvidence(entry.value, (raw) {
          final proof = _object(
            _objectList(raw, 'samples').first,
            'operationProof',
          );
          final stages = _objectList(proof, 'stages');
          stages.last['sourceRevision'] = stages.first['sourceRevision'];
          stages.last['sourceSha256'] = stages.first['sourceSha256'];
        });
        expect(
          _validateReceiptSemantics(
            noOp.receipt,
            workloads,
            artifactBytes: noOp.artifacts,
          ).join('\n'),
          contains('raw operation proof differs from exact'),
          reason: entry.key,
        );
      }

      final brokenChain = _mutateRawEvidence(warmedClaim, (raw) {
        final second = _objectList(raw, 'samples').firstWhere(
          (sample) => sample['runId'] == 'run-0' && sample['sampleIndex'] == 1,
        );
        _objectList(
          _object(second, 'operationProof'),
          'stages',
        ).first['sourceSha256'] = _hashText(
          'not-the-prior-final-state',
        );
      });
      expect(
        _validateReceiptSemantics(
          brokenChain.receipt,
          workloads,
          artifactBytes: brokenChain.artifacts,
        ).join('\n'),
        contains('raw operation proof differs from exact'),
      );

      final shortPaste = _mutateRawEvidence(pasteClaim, (raw) {
        _object(
          _objectList(raw, 'samples').first,
          'operationProof',
        )['insertedText'] = frozenOrdinaryProseExact(
          32767,
        );
      });
      expect(
        _validateReceiptSemantics(
          shortPaste.receipt,
          workloads,
          artifactBytes: shortPaste.artifacts,
        ).join('\n'),
        contains('raw operation proof differs from exact paste-32kib'),
      );
    });

    test('requires complete frame streams and recomputes hidden frames', () {
      final omitted = _mutateRawEvidence(warmedClaim, (raw) {
        final retained = _list(raw, 'frames').toList();
        retained.removeWhere((value) {
          final frame = _asObject(value, 'frame');
          return frame['runId'] == 'run-0' && frame['vsyncOrdinal'] == 1;
        });
        raw['frames'] = retained;
      });
      expect(
        _validateReceiptSemantics(
          omitted.receipt,
          workloads,
          artifactBytes: omitted.artifacts,
        ).join('\n'),
        anyOf(
          contains('raw full frame stream has a missing vsync ordinal'),
          contains('raw measurement interval omits a frame ordinal'),
        ),
      );

      final hiddenMiss = _mutateRawEvidence(warmedClaim, (raw) {
        final frame = _objectList(
          raw,
          'frames',
        ).firstWhere((candidate) => candidate['editorAttributed'] == false);
        frame['rasterFinishMicros'] =
            _integer(frame, 'vsyncStartMicros') + 20000;
      });
      expect(
        _validateReceiptSemantics(
          hiddenMiss.receipt,
          workloads,
          artifactBytes: hiddenMiss.artifacts,
        ),
        contains('receipt metrics differ from replayed raw evidence'),
      );
    });

    test('derives visible content, glyphs, projection, and raster from raw', () {
      final copiedCount = _deepCopy(warmedClaim.receipt);
      _object(
        _object(copiedCount, 'provenance'),
        'sampling',
      )['visibleCharacterCount'] = 900;
      expect(
        _validateReceiptSemantics(
          copiedCount,
          workloads,
          artifactBytes: warmedClaim.artifacts,
        ),
        contains('visibleCharacterCount is not derived from raw render proof'),
      );

      for (final field in const <String>[
        'visibleTextSha256',
        'glyphRunSha256',
        'projectionSha256',
        'rasterSha256',
      ]) {
        final forged = _mutateRawEvidence(warmedClaim, (raw) {
          _objectList(raw, 'renderEvidence').first[field] =
              'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff';
        });
        expect(
          _validateReceiptSemantics(
            forged.receipt,
            workloads,
            artifactBytes: forged.artifacts,
          ).join('\n'),
          contains('raw render evidence does not prove visible raster state'),
          reason: field,
        );
      }
    });

    test('binds warmup, memory, and lifecycle rows to process time', () {
      final missingWarmup = _mutateRawEvidence(warmedClaim, (raw) {
        final retained = _list(raw, 'warmups').toList()..removeAt(0);
        raw['warmups'] = retained;
      });
      expect(
        _validateReceiptSemantics(
          missingWarmup.receipt,
          workloads,
          artifactBytes: missingWarmup.artifacts,
        ).join('\n'),
        contains('raw warmup denominator differs from frozen sampling'),
      );

      final alienMemory = _mutateRawEvidence(warmedClaim, (raw) {
        _objectList(raw, 'memorySamples').first['processId'] = 'not-a-process';
      });
      expect(
        _validateReceiptSemantics(
          alienMemory.receipt,
          workloads,
          artifactBytes: alienMemory.artifacts,
        ).join('\n'),
        contains('raw memory sample is outside its retained process interval'),
      );

      final reorderedMemory = _mutateRawEvidence(warmedClaim, (raw) {
        final rows = _objectList(
          raw,
          'memorySamples',
        ).where((row) => row['processId'] == 'process-0').toList();
        rows[1]['timestampMicros'] = _integer(rows[0], 'timestampMicros') - 1;
      });
      expect(
        _validateReceiptSemantics(
          reorderedMemory.receipt,
          workloads,
          artifactBytes: reorderedMemory.artifacts,
        ).join('\n'),
        contains('raw memory phases must be baseline/peak/close/post-close'),
      );

      final alienLifecycle = _mutateRawEvidence(warmedClaim, (raw) {
        _objectList(
          _object(raw, 'lifecycle'),
          'finalLiveStateSamples',
        ).first['processId'] = 'not-a-process';
      });
      expect(
        _validateReceiptSemantics(
          alienLifecycle.receipt,
          workloads,
          artifactBytes: alienLifecycle.artifacts,
        ).join('\n'),
        contains('raw lifecycle point event is outside its process interval'),
      );
    });

    test('binds receipt and raw replay to the frozen contract hashes', () {
      final wrongReceipt = _deepCopy(warmedClaim.receipt);
      _object(wrongReceipt, 'contract')['workloadMatrixSha256'] =
          'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff';
      expect(
        _validateReceiptSemantics(
          wrongReceipt,
          workloads,
          artifactBytes: warmedClaim.artifacts,
        ),
        contains(
          'receipt workload/schema contract hashes are not authoritative',
        ),
      );

      final wrongRaw = _mutateRawEvidence(warmedClaim, (raw) {
        _object(raw, 'contract')['resultSchemaSha256'] =
            'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff';
      });
      expect(
        _validateReceiptSemantics(
          wrongRaw.receipt,
          workloads,
          artifactBytes: wrongRaw.artifacts,
        ),
        contains('raw evidence contract hashes differ from the result receipt'),
      );
    });

    test(
      'enforces reference alternation and paste reset on every raw sample',
      () {
        final staleReference = _mutateRawEvidence(referenceClaim, (raw) {
          final sample = _objectList(raw, 'samples').first;
          sample['sourceRevisionAfter'] = sample['sourceRevisionBefore'];
          sample['sourceSha256After'] = sample['sourceSha256Before'];
          sample['distantProjectionSha256After'] =
              sample['distantProjectionSha256Before'];
          sample['referenceDestinationAfter'] =
              sample['referenceDestinationBefore'];
        });
        expect(
          _validateReceiptSemantics(
            staleReference.receipt,
            workloads,
            artifactBytes: staleReference.artifacts,
          ).join('\n'),
          contains('raw sample pre/post state does not match operation replay'),
        );

        final accumulatingPaste = _mutateRawEvidence(pasteClaim, (raw) {
          _objectList(raw, 'samples').first['postIterationSourceSha256'] =
              _hashText('not-reset');
        });
        expect(
          _validateReceiptSemantics(
            accumulatingPaste.receipt,
            workloads,
            artifactBytes: accumulatingPaste.artifacts,
          ),
          contains('raw sample pre/post state does not match operation replay'),
        );
      },
    );

    test(
      'freezes render denominator, applicability, and peer platform scope',
      () {
        final wrongRender = _deepCopy(warmedClaim.receipt);
        final provenance = _object(wrongRender, 'provenance');
        _object(provenance, 'renderSurface')['devicePixelRatio'] = 3;
        _object(provenance, 'sampling')['visibleCharacterCount'] = 511;
        expect(
          _validateReceiptSemantics(
            wrongRender,
            workloads,
            artifactBytes: warmedClaim.artifacts,
          ),
          containsAll(<String>{
            'render surface differs from frozen devicePixelRatio',
            'visible character count is below the frozen minimum',
          }),
        );

        final wrongApplicability = _deepCopy(example);
        _object(
          _object(wrongApplicability, 'metrics'),
          'latency',
        )['coldExactViewportPaintMicros'] = <String, Object?>{
          'sampleCount': 600,
          'p50': 1,
          'p90': 1,
          'p99': 1,
          'max': 1,
        };
        expect(
          _validateReceiptSemantics(wrongApplicability, workloads),
          contains(
            'coldExactViewportPaintMicros must be null outside cold-open',
          ),
        );

        final graph = resolutionGraph;
        final engineWithFlutterMetrics = _derivedReceipt(
          example,
          sizeTierId: 'engine-4x-envelope',
          resolvedBytes: 41943040,
          resolutionReceiptBytes: graph.receiptBytes,
          workloads: workloads,
        );
        final engineMetrics = _object(engineWithFlutterMetrics, 'metrics');
        final productMetrics = _object(example, 'metrics');
        engineMetrics['latency'] = _deepCopy(<String, Object?>{
          'value': _object(productMetrics, 'latency'),
        })['value'];
        engineMetrics['frames'] = _deepCopy(<String, Object?>{
          'value': _object(productMetrics, 'frames'),
        })['value'];
        expect(
          _validateReceiptSemantics(
            engineWithFlutterMetrics,
            workloads,
            resolutionReceiptBytes: <String, List<int>>{
              _resolutionReceiptPath: graph.receiptBytes,
            },
            artifactBytes: graph.artifacts,
            peerSuiteValidator: graph.validator,
          ),
          contains(
            'engine-only receipt cannot assert Flutter latency/frame metrics',
          ),
        );

        final mobileDerived = _derivedReceipt(
          example,
          sizeTierId: 'competitor-boundary',
          resolvedBytes: 10485760,
          resolutionReceiptBytes: graph.receiptBytes,
          workloads: workloads,
        );
        mobileDerived['thresholdProfileId'] = 'tier-b-mobile-provisional-m0-v1';
        mobileDerived['tier'] = 'B_MOBILE';
        mobileDerived['platform'] = 'android';
        _object(_object(mobileDerived, 'provenance'), 'runtime')['osName'] =
            'android';
        expect(
          _validateReceiptSemantics(
            mobileDerived,
            workloads,
            resolutionReceiptBytes: <String, List<int>>{
              _resolutionReceiptPath: graph.receiptBytes,
            },
            artifactBytes: graph.artifacts,
            peerSuiteValidator: graph.validator,
          ),
          contains('Mac competitor receipt cannot resolve a android size tier'),
        );
      },
    );
  });
}

Set<String> _expandedWorkloadIds(Map<String, Object?> workloads) {
  final sizeIds = _ids(workloads, 'sizeTiers');
  final shapeIds = _ids(workloads, 'shapeRecipes');
  final operationIds = _ids(workloads, 'operationRecipes');
  final result = <String>{};

  for (final matrixValue in _list(workloads, 'matrices')) {
    final matrix = _asObject(matrixValue, 'matrix');
    final target = _string(matrix, 'target');
    final matrixSizes = _strings(matrix, 'sizeTierIds');
    expect(sizeIds, containsAll(matrixSizes));
    for (final templateValue in _list(matrix, 'workloadTemplates')) {
      final template = _asObject(templateValue, 'workload template');
      final operation = _string(template, 'operationId');
      expect(operationIds, contains(operation));
      final templateShapes = _strings(template, 'shapeIds');
      expect(shapeIds, containsAll(templateShapes));
      for (final size in matrixSizes) {
        for (final shape in templateShapes) {
          result.add('flark-v4.$target.$size.$shape.$operation');
        }
      }
    }
  }
  return result;
}

List<String> _validateReceiptSemantics(
  Map<String, Object?> receipt,
  Map<String, Object?> workloads, {
  Map<String, List<int>> resolutionReceiptBytes = const <String, List<int>>{},
  Map<String, List<int>> artifactBytes = const <String, List<int>>{},
  PeerSuiteValidator peerSuiteValidator = const PeerSuiteValidator(),
}) {
  final errors = <String>[];
  final contract = _object(receipt, 'contract');
  if (contract['workloadMatrixPath'] != _workloadsPath ||
      contract['workloadMatrixSha256'] !=
          sha256.convert(File(_workloadsPath).readAsBytesSync()).toString() ||
      contract['resultSchemaPath'] != _schemaPath ||
      contract['resultSchemaSha256'] !=
          sha256.convert(File(_schemaPath).readAsBytesSync()).toString()) {
    errors.add('receipt workload/schema contract hashes are not authoritative');
  }
  final workloadId = _string(receipt, 'workloadId');
  if (!_expandedWorkloadIds(workloads).contains(workloadId)) {
    errors.add('workloadId is not declared by the matrix');
  }

  final provenance = _object(receipt, 'provenance');
  final fixture = _object(provenance, 'fixture');
  if (fixture['targetBytes'] != fixture['actualBytes']) {
    errors.add('fixture actualBytes differs from targetBytes');
  }
  _validateFixtureSize(
    fixture: fixture,
    workloads: workloads,
    receiptBytesByPath: resolutionReceiptBytes,
    artifactBytesByPath: artifactBytes,
    peerSuiteValidator: peerSuiteValidator,
    platform: _string(receipt, 'platform'),
    errors: errors,
  );

  final workloadParts = workloadId.split('.');
  final target = workloadParts.length == 5 ? workloadParts[1] : '';
  final operationId = workloadParts.length == 5 ? workloadParts[4] : '';
  final expectedMeasurementSurface = target == 'engine'
      ? 'engine-only'
      : 'flutter-product';
  if (receipt['measurementSurface'] != expectedMeasurementSurface) {
    errors.add('measurement surface disagrees with workload target');
  }
  if (workloadParts.length == 5 && fixture['sizeTierId'] != workloadParts[2]) {
    errors.add('fixture sizeTierId disagrees with workloadId');
  }
  if (workloadParts.length == 5) {
    _validateFixtureRecipe(
      shapeId: workloadParts[3],
      fixture: fixture,
      workloads: workloads,
      errors: errors,
    );
  }
  final operation = _list(workloads, 'operationRecipes')
      .map((value) => _asObject(value, 'operation recipe'))
      .where((value) => value['id'] == operationId)
      .firstOrNull;
  var expectedSampleCount = 0;
  if (operation == null) {
    errors.add('operation is not declared by the matrix');
  } else {
    final expectedSampling = _object(operation, 'sampling');
    final actualSampling = _object(provenance, 'sampling');
    expectedSampleCount = _integer(expectedSampling, 'totalSampleCount');
    const frozenKeys = <String>[
      'iterationUnit',
      'warmupIterationsPerRun',
      'sampleIterationsPerRun',
      'runCount',
      'cadenceHz',
      'totalSampleCount',
    ];
    if (actualSampling['operationId'] != operationId ||
        frozenKeys.any(
          (key) => !_deepJsonEquals(actualSampling[key], expectedSampling[key]),
        )) {
      errors.add('receipt sampling differs from frozen operation sampling');
    }
    if (_integer(actualSampling, 'sampleIterationsPerRun') *
            _integer(actualSampling, 'runCount') !=
        _integer(actualSampling, 'totalSampleCount')) {
      errors.add('receipt totalSampleCount is not samples per run times runs');
    }
  }

  final profileId = _string(receipt, 'thresholdProfileId');
  final tier = _string(receipt, 'tier');
  final platform = _string(receipt, 'platform');
  final profiles = {
    for (final value in _list(workloads, 'thresholdProfiles'))
      _string(_asObject(value, 'threshold profile'), 'id'): _asObject(
        value,
        'threshold profile',
      ),
  };
  final profile = profiles[profileId];
  if (profile == null) {
    errors.add('thresholdProfileId is not declared by the matrix');
  } else {
    if (profile['tier'] != tier) {
      errors.add('threshold profile and tier disagree');
    }
    final expectedThresholds = _resolveThresholds(
      profileId: profileId,
      profile: profile,
      provenance: provenance,
    );
    if (!_deepJsonEquals(_object(receipt, 'thresholds'), expectedThresholds)) {
      errors.add('resolved thresholds differ from frozen threshold profile');
    }
  }
  if (profileId == 'tier-a-mac-m0-v1' && tier != 'A_MAC') {
    errors.add('Tier A profile requires tier A_MAC');
  }
  if (profileId == 'tier-b-mobile-provisional-m0-v1' && tier != 'B_MOBILE') {
    errors.add('Tier B profile requires tier B_MOBILE');
  }
  if (tier == 'A_MAC' && platform != 'macos') {
    errors.add('Tier A profile requires platform macos');
  }
  if (tier == 'B_MOBILE' && platform != 'android' && platform != 'ios') {
    errors.add('Tier B profile requires platform android or ios');
  }

  final runtime = _object(provenance, 'runtime');
  if (platform == 'macos' && runtime['osName'] != 'macOS') {
    errors.add('macos platform requires macOS runtime provenance');
  }
  if ((platform == 'android' || platform == 'ios') &&
      (runtime['osName'] as String).toLowerCase() != platform) {
    errors.add('$platform platform disagrees with runtime OS provenance');
  }
  final expectedFramePeriod = 1000000 / _number(runtime, 'displayRefreshHz');
  final periodTolerance = _number(
    _object(workloads, 'rawEvidenceContract'),
    'displayPeriodToleranceMicros',
  );
  if ((_number(runtime, 'displayFramePeriodMicros') - expectedFramePeriod)
          .abs() >
      periodTolerance) {
    errors.add('display refresh and frame period provenance disagree');
  }
  _validateRenderSurface(
    provenance,
    workloads,
    errors,
    productSurface: target == 'product',
  );

  final metricsForDistributionChecks = _object(receipt, 'metrics');
  final frameMetricsValue = metricsForDistributionChecks['frames'];
  _visitDistributions(
    metricsForDistributionChecks,
    r'$.metrics',
    errors,
    expectedSampleCount: expectedSampleCount,
    expectedFrameCount: frameMetricsValue is Map<String, Object?>
        ? _integer(frameMetricsValue, 'totalFrames')
        : 0,
  );
  final claimEligible = _boolean(receipt, 'claimEligible');
  if (claimEligible) {
    _validateClaimEvidence(
      receipt: receipt,
      workloads: workloads,
      artifactBytesByPath: artifactBytes,
      errors: errors,
    );
  }

  final resultStatus = _string(receipt, 'resultStatus');
  final evaluation = _object(receipt, 'evaluation');
  final evaluationPassed = _boolean(evaluation, 'passed');
  if ((resultStatus == 'PASS') != evaluationPassed) {
    errors.add('resultStatus and evaluation.passed disagree');
  }
  if (resultStatus != 'PASS') {
    return errors;
  }
  for (final checkValue in _list(evaluation, 'checks')) {
    if (!_boolean(_asObject(checkValue, 'evaluation check'), 'passed')) {
      errors.add('PASS contains a failed evaluation check');
      break;
    }
  }

  final thresholds = _object(receipt, 'thresholds');
  final metrics = _object(receipt, 'metrics');
  final foreground = _object(metrics, 'foreground');
  final latencyValue = metrics['latency'];
  final framesValue = metrics['frames'];
  final convergence = _object(metrics, 'convergence');
  final memory = _object(metrics, 'memory');
  final lifecycle = _object(metrics, 'lifecycle');

  final productSurface = target == 'product';
  Map<String, Object?>? latency;
  Map<String, Object?>? frames;
  if (productSurface) {
    if (latencyValue is! Map<String, Object?> ||
        framesValue is! Map<String, Object?>) {
      errors.add('Flutter product receipt requires latency and frame metrics');
    } else {
      latency = latencyValue;
      frames = framesValue;
      _atMost(
        errors,
        'sourceVisibilityMaxFrames',
        _number(_object(latency, 'sourceVisibilityFrames'), 'max'),
        _number(thresholds, 'sourceVisibilityMaxFrames'),
      );
      _atMost(
        errors,
        'caretVisibilityMaxFrames',
        _number(_object(latency, 'caretVisibilityFrames'), 'max'),
        _number(thresholds, 'caretVisibilityMaxFrames'),
      );
      _atMost(
        errors,
        'selectionVisibilityMaxFrames',
        _number(_object(latency, 'selectionVisibilityFrames'), 'max'),
        _number(thresholds, 'selectionVisibilityMaxFrames'),
      );
      _validateLatestAcceptedStateVisibility(latency, errors);
      _atMost(
        errors,
        'inputBacklogMaxFrames',
        _number(_object(latency, 'inputBacklogFrames'), 'max'),
        _number(thresholds, 'inputBacklogMaxFrames'),
      );
    }
  } else {
    if (latencyValue != null || framesValue != null) {
      errors.add(
        'engine-only receipt cannot assert Flutter latency/frame metrics',
      );
    }
    for (final field in const <String>[
      'flutterBuildMicros',
      'flutterLayoutMicros',
      'flutterPaintMicros',
      'flutterRasterMicros',
    ]) {
      if (foreground[field] != null) {
        errors.add('engine-only receipt cannot assert $field');
      }
    }
  }
  _atMost(
    errors,
    'engineForegroundP99Micros',
    _number(_object(foreground, 'rustEngineMicros'), 'p99'),
    _number(thresholds, 'engineForegroundP99Micros'),
  );
  _below(
    errors,
    'synchronousSpanMaxExclusiveMicros',
    _number(foreground, 'longestSynchronousSpanMicros'),
    _number(thresholds, 'synchronousSpanMaxExclusiveMicros'),
  );
  if (frames != null && latency != null) {
    _atMost(
      errors,
      'flutterFrameWorkP99Micros',
      _number(_object(frames, 'editorWorkMicros'), 'p99'),
      _number(thresholds, 'flutterFrameWorkP99Micros'),
    );
    _below(
      errors,
      'editorAttributedFrameMaxExclusiveMicros',
      _number(frames, 'longestEditorAttributedFrameMicros'),
      _number(thresholds, 'editorAttributedFrameMaxExclusiveMicros'),
    );
    _atMost(
      errors,
      'editorAttributedDroppedFramesMax',
      _number(frames, 'editorAttributedDroppedFrames'),
      _number(thresholds, 'editorAttributedDroppedFramesMax'),
    );
    _atMost(
      errors,
      'editorAttributedMissedFrameRateMax',
      _number(frames, 'editorAttributedMissedFrameRate'),
      _number(thresholds, 'editorAttributedMissedFrameRateMax'),
    );
    final coldPaint = latency['coldExactViewportPaintMicros'];
    if (operationId == 'cold-open') {
      if (coldPaint is! Map<String, Object?>) {
        errors.add('cold-open requires coldExactViewportPaintMicros');
      } else {
        _below(
          errors,
          'coldExactViewportPaintMaxExclusiveMicros',
          _number(coldPaint, 'max'),
          _number(thresholds, 'coldExactViewportPaintMaxExclusiveMicros'),
        );
      }
    } else if (coldPaint != null) {
      errors.add('coldExactViewportPaintMicros must be null outside cold-open');
    }
    _below(
      errors,
      'visibleProjectionCertificationMaxExclusiveMicros',
      _number(_object(latency, 'visibleProjectionCertificationMicros'), 'max'),
      _number(thresholds, 'visibleProjectionCertificationMaxExclusiveMicros'),
    );
  }
  _below(
    errors,
    'convergenceWallMaxExclusiveMicros',
    _number(_object(convergence, 'wallTimeMicros'), 'max'),
    _number(thresholds, 'convergenceWallMaxExclusiveMicros'),
  );
  if (productSurface) {
    _atMost(
      errors,
      'uncertifiedVisibleCharacterFramesMax',
      _number(_object(convergence, 'uncertifiedVisibleCharacterFrames'), 'max'),
      _number(thresholds, 'uncertifiedVisibleCharacterFramesMax'),
    );
  }
  if (convergence['terminalState'] != 'complete') {
    errors.add('PASS requires convergence terminalState complete');
  }
  if (!_boolean(convergence, 'progressTokenAdvanced')) {
    errors.add('PASS requires an advancing progress token');
  }

  _atMost(
    errors,
    'peakRssOverBaselineMaxBytes',
    _number(memory, 'peakRssBytes') - _number(memory, 'baselineRssBytes'),
    _number(thresholds, 'peakRssOverBaselineMaxBytes'),
  );
  _atMost(
    errors,
    'retainedRssOverBaselineAfterCloseMaxBytes',
    _number(memory, 'retainedRssBytesAfterClose') -
        _number(memory, 'baselineRssBytes'),
    _number(thresholds, 'retainedRssOverBaselineAfterCloseMaxBytes'),
  );
  for (final name in const <String>[
    'liveDocumentsAfterClose',
    'liveTransactionsAfterClose',
    'liveContinuationsAfterClose',
    'liveHandlesAfterClose',
  ]) {
    _atMost(
      errors,
      '${name}Max',
      _number(lifecycle, name),
      _number(thresholds, '${name}Max'),
    );
  }
  _atLeast(
    errors,
    'minimumOpenEditCloseCycles',
    _number(lifecycle, 'openEditCloseCycles'),
    _number(thresholds, 'minimumOpenEditCloseCycles'),
  );
  _atLeast(
    errors,
    'minimumProcessReopenCount',
    _number(lifecycle, 'processReopenCount'),
    _number(thresholds, 'minimumProcessReopenCount'),
  );

  if (tier == 'B_MOBILE') {
    if (!_boolean(runtime, 'physicalDevice')) {
      errors.add('Tier B PASS requires a named physical device');
    }
    if (_boolean(runtime, 'simulator')) {
      errors.add('Tier B PASS cannot use a simulator');
    }
    _atLeast(
      errors,
      'minimumBackgroundForegroundCycles',
      _number(lifecycle, 'backgroundForegroundCycles'),
      _number(thresholds, 'minimumBackgroundForegroundCycles'),
    );
    _atLeast(
      errors,
      'minimumSustainedRunSeconds',
      _number(lifecycle, 'sustainedRunSeconds'),
      _number(thresholds, 'minimumSustainedRunSeconds'),
    );
    _atMost(
      errors,
      'thermalThrottleEventsMax',
      _number(lifecycle, 'thermalThrottleEvents'),
      _number(thresholds, 'thermalThrottleEventsMax'),
    );
  }

  if (claimEligible) {
    if (receipt['receiptKind'] != 'measurement') {
      errors.add('claimEligible requires receiptKind measurement');
    }
    if (_boolean(provenance, 'dirty')) {
      errors.add('claimEligible requires a clean commit');
    }
    final target = _string(receipt, 'workloadId').split('.')[1];
    if (target == 'product' &&
        _object(provenance, 'build')['mode'] != 'profile') {
      errors.add('claimEligible product result requires profile mode');
    }
  }

  return errors;
}

void _validateFixtureRecipe({
  required String shapeId,
  required Map<String, Object?> fixture,
  required Map<String, Object?> workloads,
  required List<String> errors,
}) {
  if (fixture['recipeId'] != shapeId) {
    errors.add('fixture recipeId must equal the workload shape');
    return;
  }
  final recipes = <String, Map<String, Object?>>{
    for (final value in _list(workloads, 'shapeRecipes'))
      _string(_asObject(value, 'shape recipe'), 'id'): _object(
        _asObject(value, 'shape recipe'),
        'recipe',
      ),
  };
  final recipe = recipes[shapeId];
  if (recipe == null) {
    errors.add('workload shape has no deterministic fixture recipe');
    return;
  }
  final generated = _generateFixtureBytes(
    recipe,
    _integer(fixture, 'targetBytes'),
  );
  final generatedHash = sha256.convert(generated).toString();
  if (generated.length != fixture['actualBytes'] ||
      generatedHash != fixture['sha256']) {
    errors.add('fixture bytes/hash differ from deterministic regeneration');
  }
}

Uint8List _generateFixtureBytes(Map<String, Object?> recipe, int targetBytes) {
  final result = Uint8List(targetBytes);
  var offset = 0;

  void appendTruncated(List<int> bytes) {
    if (offset >= targetBytes || bytes.isEmpty) return;
    final remaining = targetBytes - offset;
    final take = bytes.length < remaining ? bytes.length : remaining;
    result.setRange(offset, offset + take, bytes);
    offset += take;
  }

  appendTruncated(utf8.encode((recipe['prefix'] as String?) ?? ''));
  switch (_string(recipe, 'algorithm')) {
    case 'repeat_ascii_exact':
      final cycle = utf8.encode(_string(recipe, 'cycle'));
      if (cycle.isEmpty && offset < targetBytes) {
        throw FormatException('repeat_ascii_exact cycle cannot be empty');
      }
      final bytesRemaining = targetBytes - offset;
      final chunkLength = bytesRemaining < 1048576 ? bytesRemaining : 1048576;
      final chunk = Uint8List(chunkLength);
      for (var chunkOffset = 0; chunkOffset < chunk.length;) {
        final chunkRemaining = chunk.length - chunkOffset;
        final take = cycle.length < chunkRemaining
            ? cycle.length
            : chunkRemaining;
        chunk.setRange(chunkOffset, chunkOffset + take, cycle);
        chunkOffset += take;
      }
      while (offset < targetBytes) {
        appendTruncated(chunk);
      }
      break;
    case 'indexed_ascii_exact':
      final record = _string(recipe, 'record');
      final width = _integer(recipe, 'indexWidth');
      for (var index = 0; offset < targetBytes; index += 1) {
        appendTruncated(
          utf8.encode(
            record.replaceAll('{index}', index.toString().padLeft(width, '0')),
          ),
        );
      }
      break;
    default:
      throw FormatException('unknown fixture generator algorithm');
  }
  return result;
}

void _validateRenderSurface(
  Map<String, Object?> provenance,
  Map<String, Object?> workloads,
  List<String> errors, {
  required bool productSurface,
}) {
  final expected = _object(workloads, 'renderContract');
  final actualValue = provenance['renderSurface'];
  final sampling = _object(provenance, 'sampling');
  if (!productSurface) {
    if (actualValue != null || sampling['visibleCharacterCount'] != 0) {
      errors.add('engine-only receipt cannot declare a Flutter render surface');
    }
    return;
  }
  if (actualValue is! Map<String, Object?>) {
    errors.add('Flutter product receipt requires the frozen render surface');
    return;
  }
  final actual = actualValue;
  for (final entry in expected.entries) {
    if (entry.key == 'id') continue;
    if (!_deepJsonEquals(actual[entry.key], entry.value)) {
      errors.add('render surface differs from frozen ${entry.key}');
    }
  }
  if (_integer(sampling, 'visibleCharacterCount') <
      _integer(expected, 'minimumVisibleCharacters')) {
    errors.add('visible character count is below the frozen minimum');
  }
}

void _validateFixtureSize({
  required Map<String, Object?> fixture,
  required Map<String, Object?> workloads,
  required Map<String, List<int>> receiptBytesByPath,
  required Map<String, List<int>> artifactBytesByPath,
  required PeerSuiteValidator peerSuiteValidator,
  required String platform,
  required List<String> errors,
}) {
  final sizeTierId = _string(fixture, 'sizeTierId');
  final tiers = <String, Map<String, Object?>>{
    for (final value in _list(workloads, 'sizeTiers'))
      _string(_asObject(value, 'size tier'), 'id'): _asObject(
        value,
        'size tier',
      ),
  };
  final tier = tiers[sizeTierId];
  if (tier == null) {
    errors.add('fixture sizeTierId is not declared by the matrix');
    return;
  }

  final targetBytes = _integer(fixture, 'targetBytes');
  final actualBytes = _integer(fixture, 'actualBytes');
  final resolution = _object(fixture, 'sizeResolution');
  final resolvedBytes = _integer(resolution, 'resolvedBytes');
  if (resolvedBytes != targetBytes) {
    errors.add('sizeResolution.resolvedBytes differs from targetBytes');
  }

  final fixedBytes = tier['bytes'];
  if (fixedBytes is int) {
    if (resolution['kind'] != 'fixed' ||
        targetBytes != fixedBytes ||
        actualBytes != fixedBytes ||
        resolvedBytes != fixedBytes) {
      errors.add(
        'fixed size tier $sizeTierId must resolve to exactly $fixedBytes bytes',
      );
    }
    if (resolution['receiptPath'] != null ||
        resolution['receiptSha256'] != null ||
        resolution['receiptId'] != null) {
      errors.add('fixed size tier cannot cite a competitor resolution receipt');
    }
    return;
  }

  if (resolution['kind'] != 'competitor-receipt') {
    errors.add('derived size tier requires kind competitor-receipt');
    return;
  }
  final derivation = _object(tier, 'derivation');
  if (platform != derivation['resolutionPlatform']) {
    errors.add('Mac competitor receipt cannot resolve a $platform size tier');
    return;
  }
  final expectedPath = _string(derivation, 'resolutionReceiptPath');
  final actualPath = resolution['receiptPath'];
  if (actualPath != expectedPath) {
    errors.add('derived size receipt path differs from frozen authority');
    return;
  }
  final receiptPath = actualPath! as String;
  var receiptBytes = receiptBytesByPath[receiptPath];
  if (receiptBytes == null) {
    final receiptFile = File(receiptPath);
    if (!receiptFile.existsSync()) {
      errors.add('derived size resolution receipt is not checked in');
      return;
    }
    final tracked = Process.runSync('git', <String>[
      'ls-files',
      '--error-unmatch',
      '--',
      receiptPath,
    ]);
    if (tracked.exitCode != 0) {
      errors.add('derived size resolution receipt is not checked in');
      return;
    }
    receiptBytes = receiptFile.readAsBytesSync();
  }
  final actualSha256 = sha256.convert(receiptBytes).toString();
  if (resolution['receiptSha256'] != actualSha256) {
    errors.add('derived size receipt SHA-256 does not match checked-in bytes');
  }

  Map<String, Object?> authority;
  try {
    authority = _asObject(
      jsonDecode(utf8.decode(receiptBytes)),
      'competitor resolution receipt',
    );
  } on Object {
    errors.add('derived size resolution receipt is not valid UTF-8 JSON');
    return;
  }
  if (resolution['receiptId'] != authority['receiptId']) {
    errors.add('derived size receipt ID does not match checked-in receipt');
  }
  if (authority['suiteId'] != derivation['requiredSuiteId']) {
    errors.add('derived size receipt suite ID differs from frozen authority');
  }
  if (authority['protocolId'] != derivation['requiredProtocolId']) {
    errors.add(
      'derived size receipt protocol ID differs from frozen authority',
    );
  }
  if (authority['mayResolveCompetitorDerivedSizeTiers'] != true) {
    errors.add('derived size receipt is not eligible to resolve size tiers');
  }
  _validateCompetitorResolutionReceipt(
    authority: authority,
    workloads: workloads,
    artifactBytesByPath: artifactBytesByPath,
    peerSuiteValidator: peerSuiteValidator,
    errors: errors,
  );

  final boundaryBytes = authority['cohortCompletedTierBytes'];
  if (boundaryBytes is! int || boundaryBytes <= 0) {
    errors.add('derived size receipt lacks a positive cohort boundary');
    return;
  }
  final nextTier = tiers['competitor-next-tier'];
  if (nextTier == null) {
    errors.add('matrix lacks competitor-next-tier');
    return;
  }
  final nextDerivation = _object(nextTier, 'derivation');
  final meaningfulTiers = _list(
    nextDerivation,
    'meaningfulTierBytes',
  ).cast<int>();
  final boundaryIndex = meaningfulTiers.indexOf(boundaryBytes);
  if (boundaryIndex < 0 || boundaryIndex + 1 >= meaningfulTiers.length) {
    errors.add('competitor boundary has no frozen next meaningful tier');
    return;
  }
  final computedNextBytes = meaningfulTiers[boundaryIndex + 1];
  if (authority['nextCompetitorTierBytes'] != computedNextBytes) {
    errors.add('resolution receipt next tier violates frozen derivation');
  }

  final expectedBytes = switch (_string(derivation, 'kind')) {
    'largest-comparable-passing-envelope' => boundaryBytes,
    'next-meaningful-tier-after' => computedNextBytes,
    'multiply' => boundaryBytes * _integer(derivation, 'factor'),
    final kind => throw StateError('unknown size derivation $kind'),
  };
  if (targetBytes != expectedBytes ||
      actualBytes != expectedBytes ||
      resolvedBytes != expectedBytes) {
    errors.add(
      'derived size tier $sizeTierId must derive to exactly '
      '$expectedBytes bytes',
    );
  }
}

void _validateLatestAcceptedStateVisibility(
  Map<String, Object?> latency,
  List<String> errors,
) {
  final inputToPaint = _object(latency, 'inputToPaintMicros');
  final visibility = <Map<String, Object?>>[
    _object(latency, 'sourceVisibilityMicros'),
    _object(latency, 'caretVisibilityMicros'),
    _object(latency, 'selectionVisibilityMicros'),
  ];
  for (final statistic in const <String>['p50', 'p90', 'p99', 'max']) {
    final latest = visibility
        .map((distribution) => _number(distribution, statistic))
        .reduce((left, right) => left > right ? left : right);
    if (_number(inputToPaint, statistic) < latest) {
      errors.add(
        'inputToPaintMicros.$statistic is earlier than the latest '
        'source/caret/selection visibility',
      );
    }
  }
}

void _validateCompetitorResolutionReceipt({
  required Map<String, Object?> authority,
  required Map<String, Object?> workloads,
  required Map<String, List<int>> artifactBytesByPath,
  required PeerSuiteValidator peerSuiteValidator,
  required List<String> errors,
}) {
  final shapeErrors = _JsonSchemaValidator(
    _jsonObject(File(_schemaPath)),
  ).validateDefinition('competitorResolutionReceipt', authority);
  if (shapeErrors.isNotEmpty) {
    errors.add('competitor resolution receipt violates its frozen schema');
    errors.addAll(shapeErrors.map((value) => 'competitor receipt: $value'));
    return;
  }
  final contract = _object(workloads, 'competitorResolutionContract');
  if (authority['suiteId'] != contract['suiteId'] ||
      authority['protocolId'] != contract['protocolId'] ||
      authority['mode'] != 'full-profile-protocol') {
    errors.add('competitor resolution identity differs from frozen authority');
  }

  final planAuthority = _object(authority, 'plan');
  final planBytes = _loadPeerArtifact(
    path: _string(planAuthority, 'path'),
    expectedSha256: _string(planAuthority, 'sha256'),
    artifactBytesByPath: artifactBytesByPath,
    label: 'competitor canonical plan',
    errors: errors,
  );
  if (planBytes == null) return;

  try {
    final plan = PeerSuitePlan.fromJson(
      _asObject(jsonDecode(utf8.decode(planBytes)), 'competitor plan'),
    );
    final expectedProcesses = _integer(contract, 'expectedProcessCount');
    final expectedGroups = _integer(contract, 'expectedRunGroupCount');
    if (plan.sha256 != planAuthority['canonicalSha256'] ||
        plan.sha256 != contract['canonicalPlanSha256'] ||
        planAuthority['processCount'] != expectedProcesses ||
        plan.entries.length != expectedProcesses) {
      errors.add('competitor canonical plan hash/identity is not frozen');
    }
    final processes = _list(
      authority,
      'processes',
    ).map(PeerProcessEvidence.fromJson).toList(growable: false);
    final runGroups = _list(
      authority,
      'runGroups',
    ).map(RunGroupEvidence.fromJson).toList(growable: false);
    if (processes.length != expectedProcesses ||
        runGroups.length != expectedGroups) {
      errors.add(
        'competitor resolution process/run-group denominator is wrong',
      );
    }

    final assessment = peerSuiteValidator.validate(
      plan: plan,
      processes: processes,
      runGroups: runGroups,
      exclusiveMachineAttested: authority['exclusiveMachineAttested'] == true,
      dryRun: false,
    );
    final recomputed = assessment.toJson();
    const exactFields = <String>[
      'completionEnvelopeEligible',
      'completionEnvelopeBlockers',
      'mayResolveCompetitorDerivedSizeTiers',
      'performanceClaimEligible',
      'performanceClaimBlockers',
      'claimEligible',
      'completedTierByPeer',
      'cohortCompletedTierBytes',
      'nextCompetitorTierBytes',
      'processesValidated',
    ];
    if (exactFields.any(
      (field) => !_deepJsonEquals(authority[field], recomputed[field]),
    )) {
      errors.add(
        'competitor resolution summary differs from PeerSuiteAssessment',
      );
    }
    if (!assessment.completionEnvelopeEligible ||
        assessment.completionEnvelopeBlockers.isNotEmpty ||
        assessment.cohortCompletedTierBytes == null) {
      errors.add(
        'competitor resolution completion authority is ineligible: '
        '${assessment.completionEnvelopeBlockers.join(' | ')}',
      );
    }
  } on Object catch (error) {
    errors.add('competitor peer-suite semantic replay failed: $error');
  }
}

List<int>? _loadPeerArtifact({
  required String path,
  required String expectedSha256,
  required Map<String, List<int>> artifactBytesByPath,
  required String label,
  required List<String> errors,
}) {
  var bytes = artifactBytesByPath[path];
  if (bytes == null) {
    final file = File(path);
    if (!file.existsSync()) {
      errors.add('$label does not exist');
      return null;
    }
    bytes = file.readAsBytesSync();
  }
  if (sha256.convert(bytes).toString() != expectedSha256) {
    errors.add('$label SHA-256 does not match receipt');
  }
  return bytes;
}

String _canonicalJson(Object? value) => jsonEncode(_canonicalize(value));

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

void _validateClaimEvidence({
  required Map<String, Object?> receipt,
  required Map<String, Object?> workloads,
  required Map<String, List<int>> artifactBytesByPath,
  required List<String> errors,
}) {
  if (receipt['receiptKind'] != 'measurement') return;
  final provenance = _object(receipt, 'provenance');
  final build = _object(provenance, 'build');
  _loadVerifiedArtifact(
    path: _string(build, 'artifactPath'),
    expectedBytes: _integer(build, 'artifactBytes'),
    expectedSha256: _string(build, 'artifactSha256'),
    artifactBytesByPath: artifactBytesByPath,
    label: 'profile build artifact',
    errors: errors,
  );

  final sampling = _object(provenance, 'sampling');
  final rawArtifacts = _list(
    sampling,
    'rawArtifacts',
  ).map((value) => _asObject(value, 'raw artifact')).toList(growable: false);
  final paths = <String>{};
  Map<String, Object?>? replayArtifact;
  List<int>? replayBytes;
  for (final artifact in rawArtifacts) {
    final path = _string(artifact, 'path');
    if (!paths.add(path)) errors.add('claim raw artifact paths must be unique');
    final bytes = _loadVerifiedArtifact(
      path: path,
      expectedBytes: _integer(artifact, 'byteLength'),
      expectedSha256: _string(artifact, 'sha256'),
      artifactBytesByPath: artifactBytesByPath,
      label: 'raw artifact $path',
      errors: errors,
    );
    if (artifact['kind'] == 'flark-v4-raw-evidence-v1') {
      if (replayArtifact != null) {
        errors.add('claim must cite exactly one raw replay artifact');
      } else {
        replayArtifact = artifact;
        replayBytes = bytes;
      }
    }
  }
  if (replayArtifact == null || replayBytes == null) {
    errors.add('claim must cite exactly one raw replay artifact');
    return;
  }

  Map<String, Object?> raw;
  try {
    raw = _asObject(
      jsonDecode(utf8.decode(replayBytes)),
      'raw replay evidence',
    );
  } on Object {
    errors.add('raw replay artifact is not valid UTF-8 JSON');
    return;
  }
  final rawSchemaErrors = _JsonSchemaValidator(
    _jsonObject(File(_schemaPath)),
  ).validateDefinition('rawEvidence', raw);
  if (rawSchemaErrors.isNotEmpty) {
    errors.add('raw replay artifact violates rawEvidence schema');
    errors.addAll(rawSchemaErrors.map((value) => 'raw evidence: $value'));
    return;
  }
  final fixture = _object(provenance, 'fixture');
  if (raw['workloadId'] != receipt['workloadId'] ||
      !_deepJsonEquals(_object(raw, 'fixture'), <String, Object?>{
        'recipeId': fixture['recipeId'],
        'targetBytes': fixture['targetBytes'],
        'actualBytes': fixture['actualBytes'],
        'sha256': fixture['sha256'],
      })) {
    errors.add('raw replay identity/fixture differs from the result receipt');
  }

  try {
    final replay = _replayRawEvidence(
      raw: raw,
      receipt: receipt,
      workloads: workloads,
      errors: errors,
    );
    if (replay == null) return;
    if (receipt['durationMicros'] != replay.durationMicros) {
      errors.add('durationMicros differs from replayed process timestamps');
    }
    if (!_deepJsonEquals(_object(receipt, 'metrics'), replay.metrics)) {
      errors.add('receipt metrics differ from replayed raw evidence');
    }
  } on Object catch (error) {
    errors.add('raw replay could not be evaluated: $error');
  }
}

List<int>? _loadVerifiedArtifact({
  required String path,
  required int expectedBytes,
  required String expectedSha256,
  required Map<String, List<int>> artifactBytesByPath,
  required String label,
  required List<String> errors,
}) {
  var bytes = artifactBytesByPath[path];
  if (bytes == null) {
    final file = File(path);
    if (!file.existsSync()) {
      errors.add('$label does not exist');
      return null;
    }
    bytes = file.readAsBytesSync();
  }
  if (bytes.length != expectedBytes) {
    errors.add('$label byte length does not match receipt');
  }
  if (sha256.convert(bytes).toString() != expectedSha256) {
    errors.add('$label SHA-256 does not match receipt');
  }
  return bytes;
}

final class _ReplayResult {
  const _ReplayResult({required this.durationMicros, required this.metrics});

  final int durationMicros;
  final Map<String, Object?> metrics;
}

_ReplayResult? _replayRawEvidence({
  required Map<String, Object?> raw,
  required Map<String, Object?> receipt,
  required Map<String, Object?> workloads,
  required List<String> errors,
}) {
  final provenance = _object(receipt, 'provenance');
  final sampling = _object(provenance, 'sampling');
  final operationId = _string(sampling, 'operationId');
  final workloadParts = _string(receipt, 'workloadId').split('.');
  final target = workloadParts.length == 5 ? workloadParts[1] : '';
  if (target != 'product' || raw['measurementSurface'] != 'flutter-product') {
    errors.add('product raw replay requires the flutter-product surface');
    return null;
  }
  final receiptContract = _object(receipt, 'contract');
  final rawContract = _object(raw, 'contract');
  if (rawContract['workloadMatrixSha256'] !=
          receiptContract['workloadMatrixSha256'] ||
      rawContract['resultSchemaSha256'] !=
          receiptContract['resultSchemaSha256']) {
    errors.add('raw evidence contract hashes differ from the result receipt');
  }

  final expectedRuns = _integer(sampling, 'runCount');
  final expectedWarmupsPerRun = _integer(sampling, 'warmupIterationsPerRun');
  final expectedSamplesPerRun = _integer(sampling, 'sampleIterationsPerRun');
  final expectedTotal = _integer(sampling, 'totalSampleCount');
  final runtime = _object(provenance, 'runtime');
  final framePeriod = _number(runtime, 'displayFramePeriodMicros');
  final rawContractDefinition = _object(workloads, 'rawEvidenceContract');
  final interactionLimit = framePeriod < 16000 ? framePeriod : 16000;
  final cadenceTolerance = _number(
    rawContractDefinition,
    'cadenceToleranceMicros',
  );

  final processes = _objectList(raw, 'processes');
  final processById = <String, Map<String, Object?>>{};
  final processByRun = <String, Map<String, Object?>>{};
  for (final process in processes) {
    final processId = _string(process, 'processId');
    final runId = _string(process, 'runId');
    if (processById.putIfAbsent(processId, () => process) != process ||
        processByRun.putIfAbsent(runId, () => process) != process) {
      errors.add('raw processes must have unique process and run IDs');
    }
    if (_integer(process, 'finishedMicros') <=
        _integer(process, 'startedMicros')) {
      errors.add('raw process timestamps are not strictly ordered');
    }
  }
  if (processes.length != expectedRuns || processByRun.length != expectedRuns) {
    errors.add('raw process denominator differs from frozen runCount');
  }

  bool insideProcess(
    String processId,
    int timestamp, {
    bool inclusiveFinish = true,
  }) {
    final process = processById[processId];
    if (process == null) return false;
    final start = _integer(process, 'startedMicros');
    final finish = _integer(process, 'finishedMicros');
    return timestamp >= start &&
        (inclusiveFinish ? timestamp <= finish : timestamp < finish);
  }

  final frames = _objectList(raw, 'frames');
  final frameById = <String, Map<String, Object?>>{};
  final frameByRunOrdinal = <String, Map<int, Map<String, Object?>>>{};
  for (final frame in frames) {
    final frameId = _string(frame, 'frameId');
    final runId = _string(frame, 'runId');
    final processId = _string(frame, 'processId');
    final ordinal = _integer(frame, 'vsyncOrdinal');
    if (frameById.putIfAbsent(frameId, () => frame) != frame) {
      errors.add('raw frame IDs must be unique');
    }
    final byOrdinal = frameByRunOrdinal.putIfAbsent(
      runId,
      () => <int, Map<String, Object?>>{},
    );
    if (byOrdinal.putIfAbsent(ordinal, () => frame) != frame) {
      errors.add('raw frame vsync ordinals must be unique within each run');
    }
    final process = processByRun[runId];
    if (process == null || process['processId'] != processId) {
      errors.add('raw frame process/run identity is not authoritative');
    }
    final vsync = _integer(frame, 'vsyncStartMicros');
    final buildStart = _integer(frame, 'buildStartMicros');
    final buildFinish = _integer(frame, 'buildFinishMicros');
    final rasterStart = _integer(frame, 'rasterStartMicros');
    final rasterFinish = _integer(frame, 'rasterFinishMicros');
    if (!(vsync <= buildStart &&
        buildStart <= buildFinish &&
        buildFinish <= rasterStart &&
        rasterStart <= rasterFinish)) {
      errors.add('raw frame phase timestamps are not ordered');
    }
    if (!insideProcess(processId, vsync) ||
        !insideProcess(processId, rasterFinish)) {
      errors.add('raw frame timestamps fall outside the retained process');
    }
  }
  for (final entry in frameByRunOrdinal.entries) {
    final ordered = entry.value.keys.toList()..sort();
    if (ordered.isEmpty) continue;
    final firstOrdinal = ordered.first;
    final firstVsync = _integer(entry.value[firstOrdinal]!, 'vsyncStartMicros');
    for (var index = 0; index < ordered.length; index += 1) {
      if (ordered[index] != firstOrdinal + index) {
        errors.add('raw full frame stream has a missing vsync ordinal');
        break;
      }
      final expected =
          firstVsync + ((ordered[index] - firstOrdinal) * framePeriod).round();
      final actual = _integer(entry.value[ordered[index]]!, 'vsyncStartMicros');
      if ((actual - expected).abs() > cadenceTolerance) {
        errors.add('raw full frame stream violates display cadence');
        break;
      }
    }
  }
  final samples = _objectList(raw, 'samples');
  final warmups = _objectList(raw, 'warmups');
  if (samples.length != expectedTotal) {
    errors.add('raw sample denominator differs from frozen total');
  }
  if (warmups.length != expectedRuns * expectedWarmupsPerRun) {
    errors.add('raw warmup denominator differs from frozen sampling');
  }
  final samplesByRun = <String, List<Map<String, Object?>>>{};
  final warmupsByRun = <String, List<Map<String, Object?>>>{};
  for (final sample in samples) {
    samplesByRun
        .putIfAbsent(_string(sample, 'runId'), () => <Map<String, Object?>>[])
        .add(sample);
  }
  for (final warmup in warmups) {
    warmupsByRun
        .putIfAbsent(_string(warmup, 'runId'), () => <Map<String, Object?>>[])
        .add(warmup);
  }
  if (samplesByRun.length != expectedRuns ||
      (expectedWarmupsPerRun > 0 && warmupsByRun.length != expectedRuns)) {
    errors.add('raw run coverage differs from the frozen denominator');
  }

  final renderRows = _objectList(raw, 'renderEvidence');
  final renderBySample = <String, Map<String, Object?>>{};
  for (final row in renderRows) {
    final sampleId = _string(row, 'sampleId');
    if (renderBySample.putIfAbsent(sampleId, () => row) != row) {
      errors.add('raw render evidence must be unique per sample');
    }
  }
  if (renderRows.length != expectedTotal) {
    errors.add('raw render evidence denominator differs from measured samples');
  }

  final shapeId = workloadParts.length == 5 ? workloadParts[3] : '';
  final shape = _list(workloads, 'shapeRecipes')
      .map((value) => _asObject(value, 'shape recipe'))
      .firstWhere((value) => value['id'] == shapeId);
  final fixtureBytes = _generateFixtureBytes(
    _object(shape, 'recipe'),
    _integer(_object(provenance, 'fixture'), 'actualBytes'),
  );
  final fixtureSource = utf8.decode(fixtureBytes);
  final operation = _operationById(workloads, operationId);
  final sampleIds = <String>{};
  final warmupIds = <String>{};
  final provingFrameIds = <String>{};
  final frameCoverageCount = <String, int>{};
  final visibleCounts = <int>[];

  final sourceFrames = <num>[];
  final caretFrames = <num>[];
  final selectionFrames = <num>[];
  final backlogFrames = <num>[];
  final sourceMicros = <num>[];
  final caretMicros = <num>[];
  final selectionMicros = <num>[];
  final inputToPaintMicros = <num>[];
  final coldPaintMicros = <num>[];
  final certificationMicros = <num>[];
  final totalForegroundMicros = <num>[];
  final rustMicros = <num>[];
  final ffiMicros = <num>[];
  final dartMicros = <num>[];
  final layoutMicros = <num>[];
  final paintMicros = <num>[];
  final buildMicrosBySample = <num>[];
  final rasterMicrosBySample = <num>[];
  final synchronousSpans = <num>[];
  final workUnits = <num>[];
  final pumpCounts = <num>[];
  final convergenceMicros = <num>[];
  final uncertifiedCharacterFrames = <num>[];
  final terminalStates = <String>{};
  final terminalReasons = <Object?>{};
  var allProgressTokensAdvanced = true;

  for (final runEntry in processByRun.entries) {
    final runId = runEntry.key;
    final processId = _string(runEntry.value, 'processId');
    final runWarmups = warmupsByRun[runId] ?? <Map<String, Object?>>[];
    final runSamples = samplesByRun[runId] ?? <Map<String, Object?>>[];
    runWarmups.sort(
      (left, right) => _integer(
        left,
        'warmupIndex',
      ).compareTo(_integer(right, 'warmupIndex')),
    );
    runSamples.sort(
      (left, right) => _integer(
        left,
        'sampleIndex',
      ).compareTo(_integer(right, 'sampleIndex')),
    );
    if (runWarmups.length != expectedWarmupsPerRun ||
        !_integerSequence(
          runWarmups.map((value) => _integer(value, 'warmupIndex')),
          expectedWarmupsPerRun,
        )) {
      errors.add('raw per-run warmup denominator/index sequence is invalid');
    }
    if (runSamples.length != expectedSamplesPerRun ||
        !_integerSequence(
          runSamples.map((value) => _integer(value, 'sampleIndex')),
          expectedSamplesPerRun,
        )) {
      errors.add('raw per-run sample denominator/index sequence is invalid');
    }

    var state = _initialOperationState(operationId, fixtureSource);
    var lastIterationFinished = _integer(runEntry.value, 'startedMicros');
    for (var index = 0; index < runWarmups.length; index += 1) {
      final warmup = runWarmups[index];
      final warmupId = _string(warmup, 'warmupId');
      final started = _integer(warmup, 'startedMicros');
      final finished = _integer(warmup, 'finishedMicros');
      if (!warmupIds.add(warmupId) ||
          warmup['processId'] != processId ||
          started < lastIterationFinished ||
          finished <= started ||
          !insideProcess(processId, started) ||
          !insideProcess(processId, finished)) {
        errors.add('raw warmup identity/timestamps are invalid');
      }
      final outcome = _expectedOperationOutcome(
        operationId: operationId,
        operation: operation,
        before: state,
        operationOrdinal: index,
      );
      if (!_deepJsonEquals(warmup['operationProof'], outcome.proof)) {
        errors.add('raw warmup operation proof differs from frozen replay');
      }
      state = outcome.finalState;
      lastIterationFinished = finished;
    }

    final cadenceHz = _number(sampling, 'cadenceHz');
    final firstScheduled = runSamples.isEmpty
        ? 0
        : _integer(runSamples.first, 'scheduledMicros');
    for (var index = 0; index < runSamples.length; index += 1) {
      final sample = runSamples[index];
      final sampleId = _string(sample, 'sampleId');
      if (!sampleIds.add(sampleId)) errors.add('raw sample IDs must be unique');
      if (sample['processId'] != processId) {
        errors.add('raw sample process/run identity is not authoritative');
      }
      final expectedOutcome = _expectedOperationOutcome(
        operationId: operationId,
        operation: operation,
        before: state,
        operationOrdinal: expectedWarmupsPerRun + index,
      );
      if (!_deepJsonEquals(sample['operationProof'], expectedOutcome.proof)) {
        errors.add(
          'raw operation proof differs from exact $operationId state replay',
        );
      }
      final beforeProjection = _renderProjectionHash(
        expectedOutcome.beforeState,
        0,
        _visibleEnd(expectedOutcome.beforeState.source),
      );
      final paintedProjection = _renderProjectionHash(
        expectedOutcome.paintedState,
        0,
        _visibleEnd(expectedOutcome.paintedState.source),
      );
      if (sample['sourceRevisionBefore'] !=
              expectedOutcome.beforeState.revision ||
          sample['sourceRevisionAfter'] !=
              expectedOutcome.paintedState.revision ||
          sample['sourceSha256Before'] !=
              _hashText(expectedOutcome.beforeState.source) ||
          sample['sourceSha256After'] !=
              _hashText(expectedOutcome.paintedState.source) ||
          sample['postIterationSourceSha256'] !=
              _hashText(expectedOutcome.finalState.source) ||
          sample['distantProjectionSha256Before'] != beforeProjection ||
          sample['distantProjectionSha256After'] != paintedProjection ||
          sample['referenceDestinationBefore'] !=
              expectedOutcome.referenceDestinationBefore ||
          sample['referenceDestinationAfter'] !=
              expectedOutcome.referenceDestinationAfter) {
        errors.add('raw sample pre/post state does not match operation replay');
      }
      state = expectedOutcome.finalState;

      final scheduled = _integer(sample, 'scheduledMicros');
      final accepted = _integer(sample, 'acceptedMicros');
      final sourcePaint = _integer(sample, 'sourcePaintMicros');
      final caretPaint = _integer(sample, 'caretPaintMicros');
      final selectionPaint = _integer(sample, 'selectionPaintMicros');
      final latestPaint = <int>[
        sourcePaint,
        caretPaint,
        selectionPaint,
      ].reduce((left, right) => left > right ? left : right);
      if (!(scheduled <= accepted &&
              accepted <= sourcePaint &&
              accepted <= caretPaint &&
              accepted <= selectionPaint) ||
          !insideProcess(processId, scheduled) ||
          !insideProcess(processId, latestPaint)) {
        errors.add('raw input and paint timestamps are not ordered');
      }
      if (cadenceHz > 0) {
        final expected = firstScheduled + (index * 1000000 / cadenceHz).round();
        if ((scheduled - expected).abs() > cadenceTolerance) {
          errors.add('raw scheduled timestamps violate frozen cadence');
        }
      }
      final sourceLatency = sourcePaint - accepted;
      final caretLatency = caretPaint - accepted;
      final selectionLatency = selectionPaint - accepted;
      final latestLatency = latestPaint - accepted;
      if (sourceLatency > interactionLimit ||
          caretLatency > interactionLimit ||
          selectionLatency > interactionLimit ||
          latestLatency > interactionLimit) {
        errors.add(
          'raw source/caret/selection visibility exceeds the interaction limit',
        );
      }

      final startOrdinal = _integer(sample, 'measurementStartVsyncOrdinal');
      final endOrdinal = _integer(sample, 'measurementEndVsyncOrdinal');
      if (endOrdinal < startOrdinal) {
        errors.add('raw measurement frame interval is reversed');
      }
      final runFrames = frameByRunOrdinal[runId] ?? const {};
      for (var ordinal = startOrdinal; ordinal <= endOrdinal; ordinal += 1) {
        final intervalFrame = runFrames[ordinal];
        if (intervalFrame == null) {
          errors.add('raw measurement interval omits a frame ordinal');
          continue;
        }
        final id = _string(intervalFrame, 'frameId');
        frameCoverageCount[id] = (frameCoverageCount[id] ?? 0) + 1;
      }
      final provingFrame = frameById[_string(sample, 'frameId')];
      if (provingFrame == null ||
          provingFrame['runId'] != runId ||
          provingFrame['processId'] != processId ||
          provingFrame['sampleId'] != sampleId ||
          provingFrame['editorAttributed'] != true ||
          _integer(provingFrame, 'vsyncOrdinal') < startOrdinal ||
          _integer(provingFrame, 'vsyncOrdinal') > endOrdinal ||
          _integer(provingFrame, 'buildStartMicros') <= accepted ||
          _integer(provingFrame, 'rasterFinishMicros') != latestPaint ||
          !_deepJsonEquals(
            provingFrame['workUnitIds'],
            sample['workUnitIds'],
          ) ||
          !_deepJsonEquals(provingFrame['pumpIds'], sample['pumpIds'])) {
        errors.add('raw proving frame is not bound to the accepted operation');
      } else if (!provingFrameIds.add(_string(sample, 'frameId'))) {
        errors.add('each raw sample must cite a distinct proving frame');
      }

      final render = renderBySample[sampleId];
      if (render == null ||
          !_validateRawRenderEvidence(
            render: render,
            sample: sample,
            paintedState: expectedOutcome.paintedState,
            provingFrame: provingFrame,
            provenance: provenance,
            workloads: workloads,
            errors: errors,
          )) {
        errors.add('raw render evidence does not prove visible raster state');
      } else {
        visibleCounts.add(
          _integer(render, 'visibleEndUtf8') -
              _integer(render, 'visibleStartUtf8'),
        );
      }

      int duration(String start, String finish) {
        final startValue = _integer(sample, start);
        final finishValue = _integer(sample, finish);
        if (finishValue < startValue) {
          errors.add('raw phase $start/$finish is not ordered');
        }
        return finishValue - startValue;
      }

      sourceFrames.add(_framesFor(sourceLatency, framePeriod));
      caretFrames.add(_framesFor(caretLatency, framePeriod));
      selectionFrames.add(_framesFor(selectionLatency, framePeriod));
      backlogFrames.add(_framesFor(accepted - scheduled, framePeriod));
      sourceMicros.add(sourceLatency);
      caretMicros.add(caretLatency);
      selectionMicros.add(selectionLatency);
      inputToPaintMicros.add(latestLatency);
      if (operationId == 'cold-open') {
        coldPaintMicros.add(latestPaint - scheduled);
      }
      totalForegroundMicros.add(
        duration('foregroundStartMicros', 'foregroundFinishMicros'),
      );
      rustMicros.add(duration('rustStartMicros', 'rustFinishMicros'));
      ffiMicros.add(duration('ffiStartMicros', 'ffiFinishMicros'));
      dartMicros.add(duration('dartStartMicros', 'dartFinishMicros'));
      layoutMicros.add(duration('layoutStartMicros', 'layoutFinishMicros'));
      paintMicros.add(duration('paintStartMicros', 'paintFinishMicros'));
      if (provingFrame != null) {
        buildMicrosBySample.add(
          _integer(provingFrame, 'buildFinishMicros') -
              _integer(provingFrame, 'buildStartMicros'),
        );
        rasterMicrosBySample.add(
          _integer(provingFrame, 'rasterFinishMicros') -
              _integer(provingFrame, 'rasterStartMicros'),
        );
      }
      for (final span in _objectList(sample, 'synchronousSpans')) {
        final spanDuration =
            _integer(span, 'finishMicros') - _integer(span, 'startMicros');
        if (spanDuration < 0) errors.add('raw synchronous span is not ordered');
        synchronousSpans.add(spanDuration);
      }
      workUnits.add(_list(sample, 'workUnitIds').length);
      pumpCounts.add(_list(sample, 'pumpIds').length);
      final convergence =
          _integer(sample, 'convergenceFinishedMicros') - accepted;
      if (convergence < 0) errors.add('raw convergence precedes acceptance');
      convergenceMicros.add(convergence);
      uncertifiedCharacterFrames.add(
        _list(
          sample,
          'uncertifiedVisibleCharacterFrameCounts',
        ).cast<int>().fold<int>(0, (sum, value) => sum + value),
      );
      certificationMicros.add(convergence);
      terminalStates.add(_string(sample, 'terminalState'));
      terminalReasons.add(sample['terminalReason']);
      allProgressTokensAdvanced &= _boolean(sample, 'progressTokenAdvanced');
    }
  }
  if (operationId == 'cold-open' &&
      samples.map((sample) => sample['processId']).toSet().length !=
          expectedTotal) {
    errors.add('every cold-open sample must use a distinct process');
  }
  if (frameCoverageCount.length != frames.length ||
      frameCoverageCount.values.any((count) => count != 1)) {
    errors.add(
      'raw full frame stream must contain every frame exactly once across '
      'measurement intervals',
    );
  }
  if (visibleCounts.length != expectedTotal) {
    errors.add('raw visible-character denominator is incomplete');
  } else {
    final derivedVisible = visibleCounts.reduce(
      (left, right) => left < right ? left : right,
    );
    if (sampling['visibleCharacterCount'] != derivedVisible ||
        derivedVisible <
            _integer(
              _object(workloads, 'renderContract'),
              'minimumVisibleCharacters',
            )) {
      errors.add('visibleCharacterCount is not derived from raw render proof');
    }
  }

  final frameBuildMicros = <num>[];
  final frameRasterMicros = <num>[];
  final frameEditorWorkMicros = <num>[];
  final callsPerFrame = <num>[];
  final returnedBytesPerFrame = <num>[];
  final attributedFrameSpans = <num>[];
  var totalCalls = 0;
  var totalReturnedBytes = 0;
  var missedFrames = 0;
  var attributedMissedFrames = 0;
  for (final frame in frames) {
    final build =
        _integer(frame, 'buildFinishMicros') -
        _integer(frame, 'buildStartMicros');
    final raster =
        _integer(frame, 'rasterFinishMicros') -
        _integer(frame, 'rasterStartMicros');
    final span =
        _integer(frame, 'rasterFinishMicros') -
        _integer(frame, 'vsyncStartMicros');
    final ffiCalls = _objectList(frame, 'ffiCalls');
    final returned = ffiCalls.fold<int>(
      0,
      (sum, call) => sum + _integer(call, 'returnedBytes'),
    );
    frameBuildMicros.add(build);
    frameRasterMicros.add(raster);
    frameEditorWorkMicros.add(build + raster);
    callsPerFrame.add(ffiCalls.length);
    returnedBytesPerFrame.add(returned);
    totalCalls += ffiCalls.length;
    totalReturnedBytes += returned;
    final missed = span >= framePeriod;
    if (missed) missedFrames += 1;
    if (frame['editorAttributed'] == true) {
      attributedFrameSpans.add(span);
      if (missed) attributedMissedFrames += 1;
    }
  }
  final attributedRate = attributedFrameSpans.isEmpty
      ? 0
      : attributedMissedFrames / attributedFrameSpans.length;

  final memoryReplay = _replayMemoryEvidence(
    _objectList(raw, 'memorySamples'),
    processById,
    errors,
  );
  final lifecycle = _replayLifecycleBound(
    _object(raw, 'lifecycle'),
    processById,
    memoryReplay.postCloseMicrosByProcess,
    errors,
  );
  final terminalState = terminalStates.length == 1
      ? terminalStates.single
      : 'typed_fault';
  if (terminalStates.length != 1 || terminalReasons.length != 1) {
    errors.add('raw convergence terminal records disagree');
  }
  final metrics = <String, Object?>{
    'latency': <String, Object?>{
      'sourceVisibilityFrames': _rawDistribution(sourceFrames),
      'caretVisibilityFrames': _rawDistribution(caretFrames),
      'selectionVisibilityFrames': _rawDistribution(selectionFrames),
      'inputBacklogFrames': _rawDistribution(backlogFrames),
      'sourceVisibilityMicros': _rawDistribution(sourceMicros),
      'caretVisibilityMicros': _rawDistribution(caretMicros),
      'selectionVisibilityMicros': _rawDistribution(selectionMicros),
      'inputToPaintMicros': _rawDistribution(inputToPaintMicros),
      'coldExactViewportPaintMicros': operationId == 'cold-open'
          ? _rawDistribution(coldPaintMicros)
          : null,
      'visibleProjectionCertificationMicros': _rawDistribution(
        certificationMicros,
      ),
    },
    'foreground': <String, Object?>{
      'totalMicros': _rawDistribution(totalForegroundMicros),
      'rustEngineMicros': _rawDistribution(rustMicros),
      'ffiMicros': _rawDistribution(ffiMicros),
      'dartMicros': _rawDistribution(dartMicros),
      'flutterBuildMicros': _rawDistribution(buildMicrosBySample),
      'flutterLayoutMicros': _rawDistribution(layoutMicros),
      'flutterPaintMicros': _rawDistribution(paintMicros),
      'flutterRasterMicros': _rawDistribution(rasterMicrosBySample),
      'longestSynchronousSpanMicros': synchronousSpans.reduce(
        (left, right) => left > right ? left : right,
      ),
    },
    'frames': <String, Object?>{
      'totalFrames': frames.length,
      'missedFrames': missedFrames,
      'editorAttributedDroppedFrames': attributedMissedFrames,
      'editorAttributedMissedFrameRate': attributedRate,
      'buildMicros': _rawDistribution(frameBuildMicros),
      'rasterMicros': _rawDistribution(frameRasterMicros),
      'editorWorkMicros': _rawDistribution(frameEditorWorkMicros),
      'longestEditorAttributedFrameMicros': attributedFrameSpans.reduce(
        (left, right) => left > right ? left : right,
      ),
    },
    'ffi': <String, Object?>{
      'totalCalls': totalCalls,
      'totalReturnedBytes': totalReturnedBytes,
      'callsPerFrame': _rawDistribution(callsPerFrame),
      'returnedBytesPerFrame': _rawDistribution(returnedBytesPerFrame),
    },
    'convergence': <String, Object?>{
      'workUnits': _rawDistribution(workUnits),
      'pumpCount': _rawDistribution(pumpCounts),
      'wallTimeMicros': _rawDistribution(convergenceMicros),
      'uncertifiedVisibleCharacterFrames': _rawDistribution(
        uncertifiedCharacterFrames,
      ),
      'terminalState': terminalState,
      'terminalReason': terminalReasons.length == 1
          ? terminalReasons.single
          : 'raw-terminal-disagreement',
      'progressTokenAdvanced': allProgressTokensAdvanced,
    },
    'memory': memoryReplay.metrics,
    'lifecycle': lifecycle,
  };

  final start = processes
      .map((process) => _integer(process, 'startedMicros'))
      .reduce((left, right) => left < right ? left : right);
  final finish = processes
      .map((process) => _integer(process, 'finishedMicros'))
      .reduce((left, right) => left > right ? left : right);
  return _ReplayResult(durationMicros: finish - start, metrics: metrics);
}

Map<String, Object?> _rawDistribution(List<num> unsorted) {
  if (unsorted.isEmpty) throw StateError('raw distribution cannot be empty');
  final values = [...unsorted]..sort();
  num percentile(num p) {
    final index = (p * values.length).ceil() - 1;
    return values[index < 0 ? 0 : index];
  }

  return <String, Object?>{
    'sampleCount': values.length,
    'p50': percentile(0.50),
    'p90': percentile(0.90),
    'p99': percentile(0.99),
    'max': values.last,
  };
}

int _framesFor(num micros, num framePeriodMicros) =>
    micros <= 0 ? 0 : (micros / framePeriodMicros).ceil();

final class _RawSourceState {
  const _RawSourceState({
    required this.source,
    required this.revision,
    required this.caretOffsetUtf8,
  });

  final String source;
  final int revision;
  final int caretOffsetUtf8;
}

final class _OperationOutcome {
  const _OperationOutcome({
    required this.beforeState,
    required this.paintedState,
    required this.finalState,
    required this.proof,
    required this.referenceDestinationBefore,
    required this.referenceDestinationAfter,
  });

  final _RawSourceState beforeState;
  final _RawSourceState paintedState;
  final _RawSourceState finalState;
  final Map<String, Object?> proof;
  final String? referenceDestinationBefore;
  final String? referenceDestinationAfter;
}

_RawSourceState _initialOperationState(String operationId, String source) =>
    _RawSourceState(
      source: source,
      revision: 0,
      caretOffsetUtf8:
          operationId == 'sustained-typing' ||
              operationId == 'sustained-deletion'
          ? utf8.encode(source).length
          : 0,
    );

_OperationOutcome _expectedOperationOutcome({
  required String operationId,
  required Map<String, Object?> operation,
  required _RawSourceState before,
  required int operationOrdinal,
}) {
  final contract = _object(operation, 'stateContract');
  final states = <_RawSourceState>[before];
  var editOffset = 0;
  var inserted = '';
  var deleted = '';
  String? referenceBefore;
  String? referenceAfter;

  _RawSourceState mutate({
    required _RawSourceState current,
    required String source,
    required int caret,
  }) => _RawSourceState(
    source: source,
    revision: current.revision + 1,
    caretOffsetUtf8: caret,
  );

  switch (operationId) {
    case 'cold-open':
      states.add(before);
    case 'warmed-local-insert':
      editOffset = utf8.encode(before.source).length ~/ 2;
      inserted = _string(contract, 'insertedText');
      final source = _replaceAscii(before.source, editOffset, '', inserted);
      states.add(
        mutate(
          current: before,
          source: source,
          caret: editOffset + utf8.encode(inserted).length,
        ),
      );
    case 'sustained-typing':
      editOffset = before.caretOffsetUtf8;
      inserted = _string(contract, 'insertedText');
      final source = _replaceAscii(before.source, editOffset, '', inserted);
      states.add(
        mutate(
          current: before,
          source: source,
          caret: editOffset + utf8.encode(inserted).length,
        ),
      );
    case 'sustained-deletion':
      final deleteBytes = _integer(contract, 'deletedGraphemeCount');
      editOffset = before.caretOffsetUtf8 - deleteBytes;
      if (editOffset < 0) {
        throw StateError('deletion fixture exhausted');
      }
      deleted = before.source.substring(editOffset, before.caretOffsetUtf8);
      final source = _replaceAscii(before.source, editOffset, deleted, '');
      states.add(mutate(current: before, source: source, caret: editOffset));
    case 'streaming-append':
      editOffset = utf8.encode(before.source).length;
      inserted = frozenOrdinaryProseExact(_integer(contract, 'appendedBytes'));
      states.add(
        mutate(
          current: before,
          source: '$before.source$inserted',
          caret: before.caretOffsetUtf8,
        ),
      );
    case 'undo-redo':
      editOffset = utf8.encode(before.source).length ~/ 2;
      inserted = _string(contract, 'insertedText');
      final insertedSource = _replaceAscii(
        before.source,
        editOffset,
        '',
        inserted,
      );
      final insertedState = mutate(
        current: before,
        source: insertedSource,
        caret: editOffset + utf8.encode(inserted).length,
      );
      final undoneState = mutate(
        current: insertedState,
        source: before.source,
        caret: before.caretOffsetUtf8,
      );
      final redoneState = mutate(
        current: undoneState,
        source: insertedSource,
        caret: insertedState.caretOffsetUtf8,
      );
      states.addAll(<_RawSourceState>[insertedState, undoneState, redoneState]);
    case 'paste-32kib':
      final midpoint = utf8.encode(before.source).length ~/ 2;
      final paragraph = before.source.indexOf('\n\n', midpoint);
      editOffset = paragraph < 0 ? before.source.length : paragraph + 2;
      inserted = frozenOrdinaryProseExact(_integer(contract, 'pastedBytes'));
      final pasted = mutate(
        current: before,
        source: _replaceAscii(before.source, editOffset, '', inserted),
        caret: editOffset + utf8.encode(inserted).length,
      );
      final reset = mutate(
        current: pasted,
        source: before.source,
        caret: before.caretOffsetUtf8,
      );
      states.addAll(<_RawSourceState>[pasted, reset]);
    case 'reference-retarget':
      final match = RegExp(
        r'https://(?:example|changed-[ab])\.invalid/(?:[0-9]+)?',
      ).firstMatch(before.source);
      if (match == null) throw StateError('reference destination is missing');
      editOffset = match.start;
      deleted = match.group(0)!;
      final cycle = _strings(operation, 'destinationCycle');
      inserted = cycle[operationOrdinal % cycle.length];
      referenceBefore = deleted;
      referenceAfter = inserted;
      states.add(
        mutate(
          current: before,
          source: _replaceAscii(before.source, editOffset, deleted, inserted),
          caret: before.caretOffsetUtf8,
        ),
      );
    case 'fence-close-reopen':
      editOffset = utf8.encode(before.source).length;
      inserted = _string(contract, 'appendedText');
      deleted = inserted;
      final closed = mutate(
        current: before,
        source: '$before.source$inserted',
        caret: before.caretOffsetUtf8,
      );
      final reopened = mutate(
        current: closed,
        source: before.source,
        caret: before.caretOffsetUtf8,
      );
      states.addAll(<_RawSourceState>[closed, reopened]);
    default:
      throw StateError('unknown operation $operationId');
  }

  final stageNames = _strings(contract, 'stages');
  if (states.length != stageNames.length) {
    throw StateError('$operationId stage contract is inconsistent');
  }
  final stageByName = <String, _RawSourceState>{
    for (var index = 0; index < stageNames.length; index += 1)
      stageNames[index]: states[index],
  };
  final painted = stageByName[_string(contract, 'paintedStage')]!;
  final finalState = stageByName[_string(contract, 'finalStage')]!;
  final proof = <String, Object?>{
    'kind': contract['kind'],
    'operationOrdinal': operationOrdinal,
    'editOffsetUtf8': editOffset,
    'insertedText': inserted,
    'deletedText': deleted,
    'paintedStage': contract['paintedStage'],
    'finalStage': contract['finalStage'],
    'stages': <Object?>[
      for (var index = 0; index < states.length; index += 1)
        _operationStage(stageNames[index], states[index]),
    ],
  };
  return _OperationOutcome(
    beforeState: before,
    paintedState: painted,
    finalState: finalState,
    proof: proof,
    referenceDestinationBefore: referenceBefore,
    referenceDestinationAfter: referenceAfter,
  );
}

Map<String, Object?> _operationStage(String name, _RawSourceState state) =>
    <String, Object?>{
      'stage': name,
      'sourceRevision': state.revision,
      'sourceUtf8Bytes': utf8.encode(state.source).length,
      'sourceSha256': _hashText(state.source),
      'caretOffsetUtf8': state.caretOffsetUtf8,
    };

String _replaceAscii(
  String source,
  int offset,
  String deleted,
  String inserted,
) {
  if (offset < 0 ||
      offset + deleted.length > source.length ||
      source.substring(offset, offset + deleted.length) != deleted) {
    throw StateError('operation edit does not match current source');
  }
  return '${source.substring(0, offset)}$inserted'
      '${source.substring(offset + deleted.length)}';
}

int _visibleEnd(String source) {
  final bytes = utf8.encode(source).length;
  return bytes < 800 ? bytes : 800;
}

String _renderProjectionHash(
  _RawSourceState state,
  int visibleStart,
  int visibleEnd,
) {
  final bytes = utf8.encode(state.source);
  final visible = utf8.decode(bytes.sublist(visibleStart, visibleEnd));
  return _hashText(
    _canonicalJson(<String, Object?>{
      'sourceRevision': state.revision,
      'sourceSha256': _hashText(state.source),
      'visibleStartUtf8': visibleStart,
      'visibleEndUtf8': visibleEnd,
      'visibleTextSha256': _hashText(visible),
    }),
  );
}

String _glyphRunHash(Map<String, Object?> workloads, String visibleText) =>
    _hashText(
      _canonicalJson(<String, Object?>{
        'fontContract': _object(workloads, 'renderContract'),
        'visibleText': visibleText,
      }),
    );

String _rasterHash(
  Map<String, Object?> workloads,
  String glyphRunHash,
  String projectionHash,
) => _hashText(
  _canonicalJson(<String, Object?>{
    'viewportContract': _object(workloads, 'renderContract'),
    'glyphRunSha256': glyphRunHash,
    'projectionSha256': projectionHash,
  }),
);

bool _validateRawRenderEvidence({
  required Map<String, Object?> render,
  required Map<String, Object?> sample,
  required _RawSourceState paintedState,
  required Map<String, Object?>? provingFrame,
  required Map<String, Object?> provenance,
  required Map<String, Object?> workloads,
  required List<String> errors,
}) {
  final start = _integer(render, 'visibleStartUtf8');
  final end = _integer(render, 'visibleEndUtf8');
  final sourceBytes = utf8.encode(paintedState.source);
  if (start < 0 || end < start || end > sourceBytes.length) return false;
  final visibleText = utf8.decode(sourceBytes.sublist(start, end));
  final projection = _renderProjectionHash(paintedState, start, end);
  final glyph = _glyphRunHash(workloads, visibleText);
  final raster = _rasterHash(workloads, glyph, projection);
  final expectedSurface = Map<String, Object?>.from(
    _object(workloads, 'renderContract'),
  )..remove('id');
  final rasterFinish = provingFrame == null
      ? null
      : provingFrame['rasterFinishMicros'];
  return render['runId'] == sample['runId'] &&
      render['sampleId'] == sample['sampleId'] &&
      render['processId'] == sample['processId'] &&
      render['frameId'] == sample['frameId'] &&
      render['sourceRevision'] == paintedState.revision &&
      render['sourceSha256'] == _hashText(paintedState.source) &&
      render['visibleText'] == visibleText &&
      render['visibleTextSha256'] == _hashText(visibleText) &&
      render['projectionSha256'] == projection &&
      render['glyphCount'] == visibleText.runes.length &&
      render['glyphRunSha256'] == glyph &&
      render['rasterSha256'] == raster &&
      render['rasterFinishedMicros'] == rasterFinish &&
      render['rasterFinishedMicros'] == sample['sourcePaintMicros'] &&
      _deepJsonEquals(_object(provenance, 'renderSurface'), expectedSurface);
}

final class _MemoryReplay {
  const _MemoryReplay({
    required this.metrics,
    required this.postCloseMicrosByProcess,
  });

  final Map<String, Object?> metrics;
  final Map<String, int> postCloseMicrosByProcess;
}

_MemoryReplay _replayMemoryEvidence(
  List<Map<String, Object?>> rows,
  Map<String, Map<String, Object?>> processById,
  List<String> errors,
) {
  final ids = <String>{};
  final byProcess = <String, List<Map<String, Object?>>>{};
  for (final row in rows) {
    final id = _string(row, 'memorySampleId');
    final processId = _string(row, 'processId');
    if (!ids.add(id)) errors.add('raw memory sample IDs must be unique');
    final process = processById[processId];
    final timestamp = _integer(row, 'timestampMicros');
    if (process == null ||
        timestamp < _integer(process, 'startedMicros') ||
        timestamp > _integer(process, 'finishedMicros')) {
      errors.add('raw memory sample is outside its retained process interval');
    }
    byProcess.putIfAbsent(processId, () => <Map<String, Object?>>[]).add(row);
  }
  if (byProcess.length != processById.length) {
    errors.add('raw memory evidence does not cover every retained process');
  }
  final baselines = <int>[];
  final peaks = <int>[];
  final retained = <int>[];
  final variances = <int>[];
  var allocationCount = 0;
  var allocatedBytes = 0;
  final postClose = <String, int>{};
  for (final processId in processById.keys) {
    final processRows = byProcess[processId] ?? <Map<String, Object?>>[];
    processRows.sort(
      (left, right) => _integer(
        left,
        'timestampMicros',
      ).compareTo(_integer(right, 'timestampMicros')),
    );
    final phases = processRows.map((row) => row['phase']).toList();
    if (!_deepJsonEquals(phases, const <String>[
      'baseline',
      'peak',
      'close',
      'post-close',
    ])) {
      errors.add(
        'raw memory phases must be baseline/peak/close/post-close in order',
      );
      continue;
    }
    final timestamps = processRows
        .map((row) => _integer(row, 'timestampMicros'))
        .toList();
    if (!_strictlyIncreasing(timestamps)) {
      errors.add('raw memory phase timestamps are not strictly ordered');
    }
    final counts = processRows
        .map((row) => _integer(row, 'allocationCount'))
        .toList();
    final bytes = processRows
        .map((row) => _integer(row, 'allocatedBytes'))
        .toList();
    if (!_nonDecreasing(counts) || !_nonDecreasing(bytes)) {
      errors.add('raw memory allocation counters are not monotonic');
    }
    baselines.add(_integer(processRows[0], 'residentBytes'));
    peaks.add(_integer(processRows[1], 'residentBytes'));
    retained.add(_integer(processRows[3], 'residentBytes'));
    variances.addAll(
      processRows.map((row) => _integer(row, 'allocatorRssVarianceBytes')),
    );
    allocationCount += counts.reduce(
      (left, right) => left > right ? left : right,
    );
    allocatedBytes += bytes.reduce(
      (left, right) => left > right ? left : right,
    );
    postClose[processId] = timestamps[3];
  }
  int minimum(List<int> values) => values.isEmpty
      ? 0
      : values.reduce((left, right) => left < right ? left : right);
  int maximum(List<int> values) => values.isEmpty
      ? 0
      : values.reduce((left, right) => left > right ? left : right);
  return _MemoryReplay(
    metrics: <String, Object?>{
      'allocationCount': allocationCount,
      'allocatedBytes': allocatedBytes,
      'baselineRssBytes': minimum(baselines),
      'peakRssBytes': maximum(peaks),
      'retainedRssBytesAfterClose': maximum(retained),
      'allocatorRssVarianceBytes': maximum(variances),
    },
    postCloseMicrosByProcess: postClose,
  );
}

Map<String, Object?> _replayLifecycleBound(
  Map<String, Object?> raw,
  Map<String, Map<String, Object?>> processById,
  Map<String, int> postCloseMicrosByProcess,
  List<String> errors,
) {
  final openCycles = _objectList(raw, 'openEditCloseCycles');
  final reopens = _objectList(raw, 'processReopens');
  final backgroundCycles = _objectList(raw, 'backgroundForegroundCycles');
  final sustained = _objectList(raw, 'sustainedIntervals');
  final thermal = _objectList(raw, 'thermalSamples')
    ..sort(
      (left, right) => _integer(
        left,
        'timestampMicros',
      ).compareTo(_integer(right, 'timestampMicros')),
    );
  final finalStates = _objectList(raw, 'finalLiveStateSamples');

  bool timestampInside(String processId, int timestamp) {
    final process = processById[processId];
    return process != null &&
        timestamp >= _integer(process, 'startedMicros') &&
        timestamp <= _integer(process, 'finishedMicros');
  }

  void validateRecords(
    List<Map<String, Object?>> records,
    String idKey,
    List<String> timestampKeys,
    String label,
  ) {
    if (records.map((record) => record[idKey]).toSet().length !=
        records.length) {
      errors.add('raw lifecycle $label IDs must be unique');
    }
    final previousFinish = <String, int>{};
    for (final record in records) {
      final processId = _string(record, 'processId');
      final times = timestampKeys.map((key) => _integer(record, key)).toList();
      if (!_strictlyIncreasing(times)) {
        errors.add('raw lifecycle $label timestamps are not ordered');
      }
      if (times.any((timestamp) => !timestampInside(processId, timestamp))) {
        errors.add('raw lifecycle $label is outside its process interval');
      }
      final previous = previousFinish[processId];
      if (previous != null && times.first <= previous) {
        errors.add('raw lifecycle $label records overlap or regress');
      }
      previousFinish[processId] = times.last;
    }
  }

  validateRecords(openCycles, 'cycleId', const <String>[
    'openMicros',
    'editMicros',
    'closeMicros',
  ], 'open/edit/close');
  validateRecords(reopens, 'reopenId', const <String>[
    'closedMicros',
    'openedMicros',
  ], 'process reopen');
  validateRecords(backgroundCycles, 'cycleId', const <String>[
    'backgroundMicros',
    'foregroundMicros',
  ], 'background/foreground');
  validateRecords(sustained, 'intervalId', const <String>[
    'startMicros',
    'finishMicros',
  ], 'sustained interval');

  for (final collection in <List<Map<String, Object?>>>[
    thermal,
    _objectList(raw, 'thermalThrottleEvents'),
    _objectList(raw, 'memoryPressureEvents'),
    finalStates,
  ]) {
    for (final row in collection) {
      final processId = _string(row, 'processId');
      if (!timestampInside(processId, _integer(row, 'timestampMicros'))) {
        errors.add('raw lifecycle point event is outside its process interval');
      }
    }
  }
  final finalByProcess = <String, Map<String, Object?>>{};
  for (final state in finalStates) {
    final processId = _string(state, 'processId');
    if (finalByProcess.putIfAbsent(processId, () => state) != state) {
      errors.add('raw final live state must be unique per process');
    }
    if (_integer(state, 'timestampMicros') <
        (postCloseMicrosByProcess[processId] ?? 0)) {
      errors.add('raw final live state precedes post-close memory evidence');
    }
  }
  if (finalByProcess.length != processById.length) {
    errors.add('raw final live state does not cover every process');
  }

  final severity = <String, int>{
    'not-applicable': 0,
    'nominal': 0,
    'fair': 1,
    'serious': 2,
    'critical': 3,
  };
  final states = thermal.map((sample) => _string(sample, 'state')).toList();
  if (states.isEmpty || states.any((state) => !severity.containsKey(state))) {
    errors.add('raw lifecycle contains an unknown thermal state');
  }
  final safeStates = states.isEmpty ? <String>['not-applicable'] : states;
  final maxState = safeStates.reduce(
    (left, right) =>
        (severity[left] ?? -1) >= (severity[right] ?? -1) ? left : right,
  );
  int maxLive(String key) => finalStates.isEmpty
      ? 0
      : finalStates
            .map((sample) => _integer(sample, key))
            .reduce((left, right) => left > right ? left : right);
  final sustainedMicros = sustained.fold<int>(
    0,
    (sum, interval) =>
        sum +
        _integer(interval, 'finishMicros') -
        _integer(interval, 'startMicros'),
  );
  return <String, Object?>{
    'openEditCloseCycles': openCycles.length,
    'processReopenCount': reopens.length,
    'backgroundForegroundCycles': backgroundCycles.length,
    'sustainedRunSeconds': sustainedMicros ~/ 1000000,
    'thermalStartState': safeStates.first,
    'thermalMaxState': maxState,
    'thermalEndState': safeStates.last,
    'thermalThrottleEvents': _list(raw, 'thermalThrottleEvents').length,
    'memoryPressureEvents': _list(raw, 'memoryPressureEvents').length,
    'liveDocumentsAfterClose': maxLive('liveDocuments'),
    'liveTransactionsAfterClose': maxLive('liveTransactions'),
    'liveContinuationsAfterClose': maxLive('liveContinuations'),
    'liveHandlesAfterClose': maxLive('liveHandles'),
  };
}

bool _strictlyIncreasing(List<int> values) {
  for (var index = 1; index < values.length; index += 1) {
    if (values[index] <= values[index - 1]) return false;
  }
  return true;
}

bool _nonDecreasing(List<int> values) {
  for (var index = 1; index < values.length; index += 1) {
    if (values[index] < values[index - 1]) return false;
  }
  return true;
}

bool _integerSequence(Iterable<int> values, int length) {
  var expected = 0;
  for (final value in values) {
    if (value != expected) return false;
    expected += 1;
  }
  return expected == length;
}

Map<String, Object?> _operationById(
  Map<String, Object?> workloads,
  String operationId,
) => _list(workloads, 'operationRecipes')
    .map((value) => _asObject(value, 'operation recipe'))
    .firstWhere((operation) => operation['id'] == operationId);

Map<String, Object?> _resolveThresholds({
  required String profileId,
  required Map<String, Object?> profile,
  required Map<String, Object?> provenance,
}) {
  final gates = _object(profile, 'gates');
  final resolved = <String, Object?>{
    for (final entry in gates.entries)
      if (!entry.key.endsWith('Formula')) entry.key: entry.value,
  };
  final fixture = _object(provenance, 'fixture');
  final runtime = _object(provenance, 'runtime');
  final sampling = _object(provenance, 'sampling');
  final visibleCharacterCount = _integer(sampling, 'visibleCharacterCount');
  final displayFramePeriodMicros = _number(runtime, 'displayFramePeriodMicros');
  resolved['uncertifiedVisibleCharacterFramesMax'] =
      visibleCharacterCount * (500000 / displayFramePeriodMicros).ceil();

  final fixtureBytes = _integer(fixture, 'actualBytes');
  resolved['peakRssOverBaselineMaxBytes'] = switch (profileId) {
    'tier-a-mac-m0-v1' =>
      fixtureBytes * 8 > 67108864 ? fixtureBytes * 8 : 67108864,
    'tier-b-mobile-provisional-m0-v1' =>
      fixtureBytes * 6 > 50331648 ? fixtureBytes * 6 : 50331648,
    _ => throw StateError('unknown frozen threshold profile $profileId'),
  };

  for (final name in const <String>[
    'minimumBackgroundForegroundCycles',
    'minimumSustainedRunSeconds',
    'thermalThrottleEventsMax',
  ]) {
    resolved.putIfAbsent(name, () => 0);
  }
  return resolved;
}

bool _deepJsonEquals(Object? left, Object? right) {
  if (left is num && right is num) return left == right;
  if (left is List<Object?> && right is List<Object?>) {
    return left.length == right.length &&
        List<bool>.generate(
          left.length,
          (index) => _deepJsonEquals(left[index], right[index]),
        ).every((value) => value);
  }
  if (left is Map<String, Object?> && right is Map<String, Object?>) {
    return left.length == right.length &&
        left.keys.every(
          (key) =>
              right.containsKey(key) && _deepJsonEquals(left[key], right[key]),
        );
  }
  return left == right;
}

void _visitDistributions(
  Object? value,
  String path,
  List<String> errors, {
  required int expectedSampleCount,
  required int expectedFrameCount,
}) {
  if (value is List<Object?>) {
    for (var index = 0; index < value.length; index += 1) {
      _visitDistributions(
        value[index],
        '$path[$index]',
        errors,
        expectedSampleCount: expectedSampleCount,
        expectedFrameCount: expectedFrameCount,
      );
    }
    return;
  }
  if (value is! Map<String, Object?>) return;
  final distributionKeys = const <String>{
    'sampleCount',
    'p50',
    'p90',
    'p99',
    'max',
  };
  if (value.keys.toSet().containsAll(distributionKeys)) {
    final sampleCount = _integer(value, 'sampleCount');
    final expectedCount =
        path.startsWith(r'$.metrics.frames.') ||
            path.startsWith(r'$.metrics.ffi.')
        ? expectedFrameCount
        : expectedSampleCount;
    if (expectedCount > 0 && sampleCount != expectedCount) {
      errors.add(
        '$path sampleCount $sampleCount differs from frozen total '
        '$expectedCount',
      );
    }
    final p50 = _number(value, 'p50');
    final p90 = _number(value, 'p90');
    final p99 = _number(value, 'p99');
    final maximum = _number(value, 'max');
    if (!(p50 <= p90 && p90 <= p99 && p99 <= maximum)) {
      errors.add('$path percentiles are not monotonic');
    }
  }
  for (final entry in value.entries) {
    _visitDistributions(
      entry.value,
      '$path.${entry.key}',
      errors,
      expectedSampleCount: expectedSampleCount,
      expectedFrameCount: expectedFrameCount,
    );
  }
}

void _atMost(List<String> errors, String gate, num observed, num threshold) {
  if (observed > threshold) errors.add('PASS exceeds $gate');
}

void _below(List<String> errors, String gate, num observed, num threshold) {
  if (observed >= threshold) errors.add('PASS reaches or exceeds $gate');
}

void _atLeast(List<String> errors, String gate, num observed, num threshold) {
  if (observed < threshold) errors.add('PASS is below $gate');
}

final class _JsonSchemaValidator {
  _JsonSchemaValidator(this._root);

  final Map<String, Object?> _root;

  List<String> validate(Object? value) {
    final errors = <String>[];
    _visit(_root, value, r'$', errors);
    return errors;
  }

  List<String> validateDefinition(String name, Object? value) {
    final definitions = _object(_root, r'$defs');
    final definition = _asObject(definitions[name], name);
    final errors = <String>[];
    _visit(definition, value, r'$', errors);
    return errors;
  }

  void _visit(
    Map<String, Object?> schema,
    Object? value,
    String path,
    List<String> errors,
  ) {
    final reference = schema[r'$ref'];
    if (reference is String) {
      _visit(_resolve(reference), value, path, errors);
      return;
    }

    if (schema.containsKey('const') && value != schema['const']) {
      errors.add('$path must equal ${schema['const']}');
      return;
    }
    final enumValues = schema['enum'];
    if (enumValues is List<Object?> && !enumValues.contains(value)) {
      errors.add('$path is not an allowed enum value');
      return;
    }

    final allowedTypes = switch (schema['type']) {
      final String type => <String>[type],
      final List<Object?> types => types.cast<String>(),
      _ => const <String>[],
    };
    if (allowedTypes.isNotEmpty &&
        !allowedTypes.any((type) => _matchesType(type, value))) {
      errors.add('$path has the wrong type');
      return;
    }
    if (value == null) return;

    if (value is String) {
      final minLength = schema['minLength'];
      if (minLength is int && value.length < minLength) {
        errors.add('$path is shorter than $minLength');
      }
      final pattern = schema['pattern'];
      if (pattern is String && !RegExp(pattern).hasMatch(value)) {
        errors.add('$path does not match $pattern');
      }
    }
    if (value is num) {
      final minimum = schema['minimum'];
      if (minimum is num && value < minimum) {
        errors.add('$path is below minimum $minimum');
      }
      final exclusiveMinimum = schema['exclusiveMinimum'];
      if (exclusiveMinimum is num && value <= exclusiveMinimum) {
        errors.add('$path is not above $exclusiveMinimum');
      }
      final maximum = schema['maximum'];
      if (maximum is num && value > maximum) {
        errors.add('$path is above maximum $maximum');
      }
    }
    if (value is List<Object?>) {
      final minItems = schema['minItems'];
      if (minItems is int && value.length < minItems) {
        errors.add('$path has fewer than $minItems items');
      }
      final maxItems = schema['maxItems'];
      if (maxItems is int && value.length > maxItems) {
        errors.add('$path has more than $maxItems items');
      }
      final itemSchema = schema['items'];
      if (itemSchema is Map<String, Object?>) {
        for (var index = 0; index < value.length; index += 1) {
          _visit(itemSchema, value[index], '$path[$index]', errors);
        }
      }
    }
    if (value is Map<String, Object?>) {
      final required = schema['required'];
      if (required is List<Object?>) {
        for (final key in required.cast<String>()) {
          if (!value.containsKey(key)) errors.add('$path.$key is required');
        }
      }
      final properties = switch (schema['properties']) {
        final Map<String, Object?> map => map,
        _ => const <String, Object?>{},
      };
      for (final entry in value.entries) {
        final propertySchema = properties[entry.key];
        if (propertySchema is Map<String, Object?>) {
          _visit(propertySchema, entry.value, '$path.${entry.key}', errors);
          continue;
        }
        final additional = schema['additionalProperties'];
        if (additional == false) {
          errors.add('$path.${entry.key} is not allowed');
        } else if (additional is Map<String, Object?>) {
          _visit(additional, entry.value, '$path.${entry.key}', errors);
        }
      }
    }
  }

  Map<String, Object?> _resolve(String reference) {
    if (!reference.startsWith('#/')) {
      throw FormatException(
        'Only local JSON Schema references are supported: $reference',
      );
    }
    Object? current = _root;
    for (final encoded in reference.substring(2).split('/')) {
      final key = encoded.replaceAll('~1', '/').replaceAll('~0', '~');
      current = (current! as Map<String, Object?>)[key];
    }
    return current! as Map<String, Object?>;
  }

  bool _matchesType(String type, Object? value) => switch (type) {
    'null' => value == null,
    'object' => value is Map<String, Object?>,
    'array' => value is List<Object?>,
    'string' => value is String,
    'integer' => value is int,
    'number' => value is num,
    'boolean' => value is bool,
    _ => throw FormatException('Unsupported JSON Schema type: $type'),
  };
}

const _resolutionReceiptPath = 'benchmark/v4/competitor_resolution_v1.json';
const _resolutionReceiptId = 'peer-suite-test-resolution';

final class _ResolutionGraph {
  const _ResolutionGraph({
    required this.receiptBytes,
    required this.artifacts,
    required this.validator,
    required this.root,
  });

  final List<int> receiptBytes;
  final Map<String, List<int>> artifacts;
  final PeerSuiteValidator validator;
  final Directory root;
}

const _resolutionPeerFixtures = <int, String>{
  1048576: 'small-fixture-1mib',
  5242880: 'small-fixture-5mib',
  10485760: 'small-fixture-10mib',
  32768: 'small-paste-fixture',
};

_ResolutionGraph _validResolutionGraph(Map<String, Object?> workloads) {
  final store = _ResolutionArtifactStore(
    Directory.systemTemp.createTempSync('flark-v4-peer-resolution-'),
  );
  final evidence = _buildResolutionEvidence(store);
  final planFile = store.writeText(
    'plan.json',
    jsonEncode(evidence.plan.toJson()),
  );
  final validator = PeerSuiteValidator.testOnly(_resolutionPeerFixtures);
  final assessment = validator.validate(
    plan: evidence.plan,
    processes: evidence.processes,
    runGroups: evidence.groups,
    exclusiveMachineAttested: true,
    dryRun: false,
  );
  if (!assessment.completionEnvelopeEligible ||
      assessment.completionEnvelopeBlockers.isNotEmpty ||
      assessment.cohortCompletedTierBytes != 10485760) {
    throw StateError(
      'synthetic peer suite is not completion eligible: '
      '${assessment.completionEnvelopeBlockers}',
    );
  }
  final receipt = <String, Object?>{
    'schemaVersion': 1,
    'receiptId': _resolutionReceiptId,
    'suiteId': peerSuiteId,
    'protocolId': peerSuiteProtocolId,
    'mode': 'full-profile-protocol',
    'exclusiveMachineAttested': true,
    'plan': <String, Object?>{
      'path': planFile.path,
      'sha256': sha256File(planFile),
      'canonicalSha256': evidence.plan.sha256,
      'processCount': evidence.plan.entries.length,
    },
    'runGroups': evidence.groups.map((group) => group.toJson()).toList(),
    'processes': evidence.processes.map((process) => process.toJson()).toList(),
    ...assessment.toJson(),
  };
  return _ResolutionGraph(
    receiptBytes: utf8.encode(jsonEncode(receipt)),
    artifacts: store.artifacts,
    validator: validator,
    root: store.root,
  );
}

final class _ResolutionArtifactStore {
  _ResolutionArtifactStore(this.root);

  final Directory root;
  final Map<String, List<int>> artifacts = <String, List<int>>{};

  File writeText(String relativePath, String contents) {
    if (relativePath.startsWith('/') || relativePath.contains('..')) {
      throw ArgumentError.value(relativePath, 'relativePath');
    }
    final file = File('${root.path}/$relativePath');
    file.parent.createSync(recursive: true);
    final bytes = utf8.encode(contents);
    file.writeAsBytesSync(bytes, flush: true);
    artifacts[file.path] = bytes;
    return file;
  }
}

void _syncResolutionArtifacts(
  _ResolutionGraph graph,
  Map<String, List<int>> artifacts,
) {
  final rootPrefix = '${graph.root.absolute.path}${Platform.pathSeparator}';
  for (final entry in artifacts.entries) {
    final absolute = File(entry.key).absolute;
    if (!absolute.path.startsWith(rootPrefix)) {
      throw StateError('test peer artifact escaped its temporary root');
    }
    absolute.writeAsBytesSync(entry.value, flush: true);
  }
}

final class _ResolutionEvidence {
  const _ResolutionEvidence(this.plan, this.processes, this.groups);

  final PeerSuitePlan plan;
  final List<PeerProcessEvidence> processes;
  final List<RunGroupEvidence> groups;
}

_ResolutionEvidence _buildResolutionEvidence(_ResolutionArtifactStore store) {
  final plan = PeerSuitePlan.protocol();
  final processes = <PeerProcessEvidence>[];
  final groups = <RunGroupEvidence>[];
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
      final prefix = entry.id;
      final expectedFinal = _resolutionExpectedFinal(entry);
      final export = store.writeText('$prefix/final-source.md', expectedFinal);
      final exportHash = sha256File(export);
      final payload = entry.peer == 'flutter_quill'
          ? _resolutionQuillPayload(entry, export, exportHash)
          : _resolutionSuperEditorPayload(entry, export, exportHash, store);
      final result = store.writeText(
        '$prefix/result.json',
        jsonEncode(payload),
      );
      final stdout = store.writeText(
        '$prefix/stdout.log',
        'stdout:${entry.id}',
      );
      final stderr = store.writeText('$prefix/stderr.log', '');
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
          argv: <String>['/profile/app', entry.id],
          cwd: '${store.root.path}/$prefix',
          environmentOverrides: entry.peer == 'flutter_quill'
              ? <String, String>{
                  'COMPETITOR_SCENARIO': entry.workload,
                  'COMPETITOR_TARGET_BYTES': '${entry.targetBytes}',
                  'COMPETITOR_LOCATION': entry.location,
                  'COMPETITOR_RUN_INDEX': '${entry.replicate}',
                  'COMPETITOR_ORDER_INDEX': '${entry.orderSlot}',
                  'COMPETITOR_PROCESS_RUN_ID': entry.id,
                  'COMPETITOR_OUTPUT_PATH': result.path,
                  'COMPETITOR_EXPORT_PATH': export.path,
                }
              : const <String, String>{},
          resultPath: result.path,
          resultSha256: sha256File(result),
          stdoutPath: stdout.path,
          stdoutSha256: sha256File(stdout),
          stderrPath: stderr.path,
          stderrSha256: sha256File(stderr),
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
  return _ResolutionEvidence(plan, processes, groups);
}

Map<String, Object?> _resolutionQuillPayload(
  PeerSuiteEntry entry,
  File export,
  String exportHash,
) {
  final pasteContract = entry.workload == 'paste-32kib'
      ? _resolutionPasteContract(entry)
      : null;
  final measured = pasteContract == null
      ? List<Object?>.generate(_resolutionExpectedSamples(entry.workload), (
          index,
        ) {
          final accepted = 100 + index * 10;
          return <String, Object?>{
            'action': entry.workload == 'local-insert-delete'
                ? (index.isEven ? 'insert-x' : 'delete-x')
                : 'type-character',
            'sampleIndex': entry.workload == 'local-insert-delete'
                ? index ~/ 2
                : index,
            'measured': true,
            'acceptedTraceMicros': accepted,
            'frameCorrelation': <String, Object?>{'proven': true},
            'frame': <String, Object?>{
              'buildStartMicros': accepted + 1,
              'rasterFinishMicros': accepted + 2,
              'frameTimingCallbackTraceMicros': accepted + 3,
              'buildDurationMicros': 1,
              'rasterDurationMicros': 1,
              'totalSpanMicros': 2,
            },
          };
        })
      : _resolutionPasteTransitions(pasteContract)
            .skip(2)
            .map(
              (transition) => Map<String, Object?>.from(
                _object(_object(transition, 'pasteInput'), 'evidence'),
              ),
            )
            .toList();
  return <String, Object?>{
    'schemaVersion': 1,
    'peer': entry.peer,
    'claimEligible': false,
    'performanceClaimEligible': false,
    'completionEnvelopeEligible': true,
    'config': <String, Object?>{
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
    'fixture': _resolutionFixture(entry.targetBytes),
    'initialFidelity': <String, Object?>{
      'exact': true,
      'expectedSha256': _resolutionFixtureHash(entry.targetBytes),
      'expectedUtf8Bytes': entry.targetBytes,
    },
    'coldOpen': <String, Object?>{
      'processStartToInteractiveRasterFinishMicros': 20,
      'documentLoadStartToRasterFinishMicros': 10,
      'interactiveVerification': <String, Object?>{
        'focusNodeHasFocus': true,
        'editorStateMounted': true,
        'sourcePrefixMatchesFixture': true,
        'viewportLogicalWidth': 600.0,
        'viewportLogicalHeight': 600.0,
      },
      'frame': <String, Object?>{
        'buildStartMicros': 1,
        'rasterFinishMicros': 2,
      },
    },
    'scenarioResult': <String, Object?>{
      'rawSamples': measured,
      'pasteStateContract': ?pasteContract,
      if (entry.workload != 'cold-open') 'maxInputBacklogUntilRaster': 1,
      if (entry.workload != 'cold-open')
        'distributions': <String, Object?>{
          'acceptedInputToRasterFinishMicros': _resolutionDistribution(
            _resolutionExpectedSamples(entry.workload),
          ),
        },
    },
    'pasteStateContract': ?pasteContract,
    'finalFidelity': <String, Object?>{
      'exact': true,
      'expectedSha256': exportHash,
      'expectedUtf8Bytes': export.lengthSync(),
      'actualSha256': exportHash,
      'actualUtf8Bytes': export.lengthSync(),
    },
    'finalExportArtifact': <String, Object?>{
      'written': true,
      'path': export.path,
      'sha256': exportHash,
      'utf8Bytes': export.lengthSync(),
    },
    'memory': <String, Object?>{
      'afterWorkload': <String, Object?>{
        'peakResidentBytes': 2,
        'residentBytes': 1,
      },
    },
  };
}

Map<String, Object?> _resolutionSuperEditorPayload(
  PeerSuiteEntry entry,
  File export,
  String exportHash,
  _ResolutionArtifactStore store,
) {
  final expectedSamples = _resolutionExpectedSamples(entry.workload);
  final pasteContract = entry.workload == 'paste-32kib'
      ? _resolutionPasteContract(entry)
      : null;
  final rawTimeline = pasteContract == null
      ? <String, Object?>{
          'frames': List<Object?>.generate(expectedSamples, (index) {
            final accepted = 100 + index * 10;
            return <String, Object?>{
              'frameNumber': index + 7,
              'buildStartTimelineMicros': accepted + 1,
              'rasterFinishTimelineMicros': accepted + 2,
            };
          }),
          'inputs': List<Object?>.generate(
            expectedSamples,
            (index) => <String, Object?>{
              'sequence': index,
              'measured': true,
              'acceptedTimelineMicros': 100 + index * 10,
              'frameNumber': index + 7,
              'failure': null,
            },
          ),
        }
      : _resolutionSuperEditorPasteTimeline(pasteContract);
  final timeline = store.writeText(
    '${entry.id}/raw-timeline.json',
    jsonEncode(<String, Object?>{
      ...rawTimeline,
      'pasteStateContract': ?pasteContract,
    }),
  );
  return <String, Object?>{
    'schemaVersion': 1,
    'peer': entry.peer,
    'claimEligible': false,
    'performanceClaimEligible': false,
    'profileMode': true,
    'protocolConformant': true,
    'completion': 'complete',
    'config': <String, Object?>{
      'protocolId': peerSuiteProtocolId,
      'workload': entry.workload,
      'targetBytes': entry.targetBytes,
      'location': entry.location,
      ..._resolutionSuperEditorCounts(entry.workload),
      'timeoutMicros': 60000000,
    },
    'fixture': _resolutionFixture(entry.targetBytes),
    'pasteStateContract': ?pasteContract,
    'fidelity': <String, Object?>{
      'pass': true,
      'initialSourceSha256': _resolutionFixtureHash(entry.targetBytes),
      'expectedFinalSourceSha256': exportHash,
      'exportedFinalSourceSha256': exportHash,
      'exportedFinalSourceBytes': export.lengthSync(),
    },
    'artifacts': <String, Object?>{
      'finalExport': <String, Object?>{
        'path': export.path,
        'sha256': exportHash,
      },
      'rawTimeline': <String, Object?>{
        'path': timeline.path,
        'sha256': sha256File(timeline),
      },
    },
    'measurements': <String, Object?>{
      'measuredSampleCount': expectedSamples,
      'maxInputBacklog': 1,
      'peakSampledRssBytes': 2,
      'retainedRssBytes': 1,
      if (entry.workload != 'cold-open')
        'inputToRasterMicros': _resolutionDistribution(expectedSamples),
      'longestSynchronousSpan': <String, Object?>{
        'supported': false,
        'reason': 'not captured',
      },
    },
    'coldOpen': <String, Object?>{
      'documentLoadToInteractiveRasterMicros': 10,
      'interactiveFrame': <String, Object?>{
        'buildStartTimelineMicros': 1,
        'rasterFinishTimelineMicros': 2,
      },
      'endpointEvidence': <String, Object?>{
        'focus': true,
        'imeConnected': true,
        'expectedLeadingTextInRenderedModel': true,
        'rasterTimingReceived': true,
        'viewportLogicalWidth': 600.0,
        'viewportLogicalHeight': 600.0,
      },
    },
    'driver': <String, Object?>{
      'watchdogTimedOut': false,
      'processId': 50000 + entry.orderSlot,
      'processLaunchRequestedAtUtc': DateTime.utc(
        2026,
        8,
        8,
      ).add(Duration(seconds: entry.orderSlot)).toIso8601String(),
      'invocation': <String, Object?>{'runId': entry.id},
      'runControl': <String, Object?>{
        'runGroupId': 'group-${entry.groupIndex}',
        'orderSlot': '${entry.orderSlot}',
      },
    },
  };
}

Map<String, Object?> _resolutionFixture(int bytes) => <String, Object?>{
  'generatorId': 'flark-v4-deterministic-markdown-v1',
  'shapeId': 'ordinary-prose',
  'encoding': 'UTF-8',
  'normalization': 'none',
  'targetBytes': bytes,
  'actualBytes': bytes,
  'sha256': _resolutionFixtureHash(bytes),
};

String _resolutionFixtureHash(int bytes) =>
    sha256Text(_resolutionPeerFixtures[bytes]!);

String _resolutionExpectedFinal(PeerSuiteEntry entry) {
  final fixture = _resolutionPeerFixtures[entry.targetBytes]!;
  final offset = switch (entry.location) {
    'start' => 0,
    'middle' => fixture.length ~/ 2,
    'end' => fixture.length,
    _ => throw StateError('unknown location ${entry.location}'),
  };
  if (entry.workload != 'sustained-typing') return fixture;
  final typed = List.generate(
    220,
    (index) => frozenTypingCycle[index % frozenTypingCycle.length],
  ).join();
  return '${fixture.substring(0, offset)}$typed${fixture.substring(offset)}';
}

Map<String, Object?> _resolutionPasteContract(PeerSuiteEntry entry) {
  final fixture = _resolutionPeerFixtures[entry.targetBytes]!;
  final paste = _resolutionPeerFixtures[32768]!;
  final offset = switch (entry.location) {
    'start' => 0,
    'middle' => fixture.length ~/ 2,
    'end' => fixture.length,
    _ => throw StateError('unknown location ${entry.location}'),
  };
  final pasted =
      '${fixture.substring(0, offset)}$paste${fixture.substring(offset)}';
  Map<String, Object?> denominator(String source) => <String, Object?>{
    'utf8Bytes': utf8.encode(source).length,
    'sha256': sha256Text(source),
  };
  Map<String, Object?> proof(String source) => <String, Object?>{
    'canonicalUtf8Bytes': utf8.encode(source).length,
    'canonicalSha256': sha256Text(source),
    'rawUtf8Bytes': utf8.encode(source).length,
    'rawSha256': sha256Text(source),
    'classification': 'exact',
    'matchesExpectedCanonical': true,
  };
  return <String, Object?>{
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
      return <String, Object?>{
        'transitionIndex': index,
        'measured': index >= 2,
        'pasteInput': <String, Object?>{
          'evidenceSequence': pasteSequence,
          if (entry.peer == 'flutter_quill')
            'evidence': _resolutionQuillInputEvidence(
              sequence: pasteSequence,
              transitionIndex: index,
              role: 'paste-workload',
              action: 'paste-32kib',
              measured: index >= 2,
              request: 1000 + index * 100,
            ),
        },
        'preState': proof(fixture),
        'postState': proof(pasted),
        'resetState': proof(fixture),
        'resetInput': <String, Object?>{
          'operation': 'platform-backspace-over-exact-pasted-range',
          'measured': false,
          'accepted': true,
          'rastered': true,
          'platformInputDispatched': true,
          'selectionStart': offset,
          'selectionEnd': offset + paste.length,
          'evidenceSequence': resetSequence,
          if (entry.peer == 'flutter_quill')
            'evidence': _resolutionQuillInputEvidence(
              sequence: resetSequence,
              transitionIndex: index,
              role: 'paste-reset',
              action: 'paste-cleanup-delete',
              measured: false,
              request: 1010 + index * 100,
            ),
        },
      };
    }),
  };
}

List<Map<String, Object?>> _resolutionPasteTransitions(
  Map<String, Object?> contract,
) => _objectList(contract, 'transitions');

Map<String, Object?> _resolutionQuillInputEvidence({
  required int sequence,
  required int transitionIndex,
  required String role,
  required String action,
  required bool measured,
  required int request,
}) => <String, Object?>{
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
  'nativeInput': <String, Object?>{'dispatchEpochMicros': request + 1},
  'frameCorrelation': <String, Object?>{'proven': true},
  'frame': <String, Object?>{
    'buildStartMicros': request + 3,
    'rasterFinishMicros': request + 4,
    'frameTimingCallbackTraceMicros': request + 5,
    'buildDurationMicros': 1,
    'rasterDurationMicros': 1,
    'totalSpanMicros': 2,
  },
};

Map<String, Object?> _resolutionSuperEditorPasteTimeline(
  Map<String, Object?> contract,
) {
  final frames = <Object?>[];
  final inputs = <Object?>[];
  final resetInputs = <Object?>[];
  final transitions = _resolutionPasteTransitions(contract);
  for (var index = 0; index < 22; index += 1) {
    final pasteSequence = _integer(
      _object(transitions[index], 'pasteInput'),
      'evidenceSequence',
    );
    final resetSequence = _integer(
      _object(transitions[index], 'resetInput'),
      'evidenceSequence',
    );
    final pasteRequest = 1000 + index * 100;
    final resetRequest = pasteRequest + 10;
    final pasteFrame = 100 + index * 2;
    final resetFrame = pasteFrame + 1;
    frames.addAll(<Object?>[
      <String, Object?>{
        'frameNumber': pasteFrame,
        'buildStartTimelineMicros': pasteRequest + 3,
        'rasterFinishTimelineMicros': pasteRequest + 4,
        'callbackTimelineMicros': pasteRequest + 5,
      },
      <String, Object?>{
        'frameNumber': resetFrame,
        'buildStartTimelineMicros': resetRequest + 3,
        'rasterFinishTimelineMicros': resetRequest + 4,
        'callbackTimelineMicros': resetRequest + 5,
      },
    ]);
    inputs.add(<String, Object?>{
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
      'frameNumber': pasteFrame,
      'failure': null,
      'nativeEvent': <String, Object?>{'platformRouteInvoked': true},
    });
    resetInputs.add(<String, Object?>{
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
      'frameNumber': resetFrame,
      'failure': null,
      'nativeEvent': <String, Object?>{
        'eventPath': 'NSApplication.postEvent-to-Flutter-macOS-text-input',
      },
    });
  }
  return <String, Object?>{
    'frames': frames,
    'inputs': inputs,
    'resetInputs': resetInputs,
  };
}

int _resolutionExpectedSamples(String workload) => switch (workload) {
  'cold-open' => 0,
  'sustained-typing' => 200,
  'local-insert-delete' => 200,
  'paste-32kib' => 20,
  _ => throw StateError('unknown workload $workload'),
};

Map<String, Object?> _resolutionSuperEditorCounts(String workload) =>
    switch (workload) {
      'cold-open' => const <String, Object?>{
        'warmupCount': 0,
        'sampleCount': 1,
        'cadenceMillis': 0,
        'pasteBytes': 32768,
      },
      'sustained-typing' => const <String, Object?>{
        'warmupCount': 20,
        'sampleCount': 200,
        'cadenceMillis': 100,
        'pasteBytes': 32768,
      },
      'local-insert-delete' => const <String, Object?>{
        'warmupCount': 10,
        'sampleCount': 100,
        'cadenceMillis': 0,
        'pasteBytes': 32768,
      },
      'paste-32kib' => const <String, Object?>{
        'warmupCount': 2,
        'sampleCount': 20,
        'cadenceMillis': 0,
        'pasteBytes': 32768,
      },
      _ => throw StateError('unknown workload $workload'),
    };

Map<String, Object?> _resolutionDistribution(int count) => <String, Object?>{
  'count': count,
  'p50': 2,
  'p90': 2,
  'p99': 2,
  'max': 2,
};

Map<String, Object?> _derivedReceipt(
  Map<String, Object?> example, {
  required String sizeTierId,
  required int resolvedBytes,
  required List<int> resolutionReceiptBytes,
  required Map<String, Object?> workloads,
}) {
  final receipt = _deepCopy(example);
  final target = sizeTierId == 'engine-4x-envelope' ? 'engine' : 'product';
  receipt['workloadId'] =
      'flark-v4.$target.$sizeTierId.ordinary-prose.warmed-local-insert';
  final fixture = _object(_object(receipt, 'provenance'), 'fixture');
  fixture['sizeTierId'] = sizeTierId;
  fixture['targetBytes'] = resolvedBytes;
  fixture['actualBytes'] = resolvedBytes;
  final ordinaryRecipe = _object(
    _list(workloads, 'shapeRecipes')
        .map((value) => _asObject(value, 'shape recipe'))
        .firstWhere((shape) => shape['id'] == 'ordinary-prose'),
    'recipe',
  );
  fixture['sha256'] = sha256
      .convert(_generateFixtureBytes(ordinaryRecipe, resolvedBytes))
      .toString();
  fixture['sizeResolution'] = <String, Object?>{
    'kind': 'competitor-receipt',
    'resolvedBytes': resolvedBytes,
    'receiptPath': _resolutionReceiptPath,
    'receiptSha256': sha256.convert(resolutionReceiptBytes).toString(),
    'receiptId': _resolutionReceiptId,
  };
  _object(receipt, 'thresholds')['peakRssOverBaselineMaxBytes'] =
      resolvedBytes * 8 > 67108864 ? resolvedBytes * 8 : 67108864;
  if (target == 'engine') {
    receipt['measurementSurface'] = 'engine-only';
    final provenance = _object(receipt, 'provenance');
    provenance['renderSurface'] = null;
    _object(provenance, 'sampling')['visibleCharacterCount'] = 0;
    _object(receipt, 'thresholds')['uncertifiedVisibleCharacterFramesMax'] = 0;
    final metrics = _object(receipt, 'metrics');
    metrics['latency'] = null;
    metrics['frames'] = null;
    _object(metrics, 'convergence')['uncertifiedVisibleCharacterFrames'] = null;
    final foreground = _object(metrics, 'foreground');
    for (final field in const <String>[
      'flutterBuildMicros',
      'flutterLayoutMicros',
      'flutterPaintMicros',
      'flutterRasterMicros',
    ]) {
      foreground[field] = null;
    }
  }
  return receipt;
}

final class _ClaimEvidenceFixture {
  const _ClaimEvidenceFixture({
    required this.receipt,
    required this.raw,
    required this.rawPath,
    required this.artifacts,
  });

  final Map<String, Object?> receipt;
  final Map<String, Object?> raw;
  final String rawPath;
  final Map<String, List<int>> artifacts;
}

_ClaimEvidenceFixture _validClaimEvidence(
  Map<String, Object?> example,
  Map<String, Object?> workloads, {
  required String operationId,
}) {
  final receipt = _deepCopy(example);
  final shapeId = switch (operationId) {
    'reference-retarget' => 'many-references',
    'fence-close-reopen' => 'open-fence-to-eof',
    _ => 'ordinary-prose',
  };
  receipt['receiptKind'] = 'measurement';
  receipt['resultId'] = 'synthetic-claim-$operationId';
  receipt['workloadId'] = 'flark-v4.product.1kib.$shapeId.$operationId';
  receipt['claimEligible'] = true;
  final provenance = _object(receipt, 'provenance');
  provenance['repositoryUrl'] = 'https://github.com/example/flark';
  provenance['dirty'] = false;
  provenance['dirtyDiffSha256'] = null;
  final fixture = _object(provenance, 'fixture');
  fixture['recipeId'] = shapeId;
  final recipe = _object(
    _list(workloads, 'shapeRecipes')
        .map((value) => _asObject(value, 'shape recipe'))
        .firstWhere((shape) => shape['id'] == shapeId),
    'recipe',
  );
  final fixtureBytes = _generateFixtureBytes(
    recipe,
    _integer(fixture, 'targetBytes'),
  );
  final fixtureSource = utf8.decode(fixtureBytes);
  fixture['sha256'] = sha256.convert(fixtureBytes).toString();

  final operation = _operationById(workloads, operationId);
  final expectedSampling = _object(operation, 'sampling');
  final sampling = _object(provenance, 'sampling');
  sampling['operationId'] = operationId;
  for (final key in const <String>[
    'iterationUnit',
    'warmupIterationsPerRun',
    'sampleIterationsPerRun',
    'runCount',
    'cadenceHz',
    'totalSampleCount',
  ]) {
    sampling[key] = expectedSampling[key];
  }
  sampling['visibleCharacterCount'] = _visibleEnd(fixtureSource);

  final buildBytes = utf8.encode('synthetic-profile-build-$operationId');
  final buildPath = '/synthetic/flark/$operationId.profile-app';
  final build = _object(provenance, 'build');
  build['artifactPath'] = buildPath;
  build['artifactBytes'] = buildBytes.length;
  build['artifactSha256'] = sha256.convert(buildBytes).toString();

  final runCount = _integer(sampling, 'runCount');
  final warmupsPerRun = _integer(sampling, 'warmupIterationsPerRun');
  final samplesPerRun = _integer(sampling, 'sampleIterationsPerRun');
  final cadenceHz = _number(sampling, 'cadenceHz');
  final framePeriod = _number(
    _object(provenance, 'runtime'),
    'displayFramePeriodMicros',
  );
  final framesPerSample = cadenceHz > 0 ? 1 : 2;
  final processes = <Map<String, Object?>>[];
  final warmups = <Map<String, Object?>>[];
  final samples = <Map<String, Object?>>[];
  final frames = <Map<String, Object?>>[];
  final renderEvidence = <Map<String, Object?>>[];
  final memory = <Map<String, Object?>>[];
  final processTiming = <String, Map<String, int>>{};

  for (var run = 0; run < runCount; run += 1) {
    final runId = 'run-$run';
    final processId = 'process-$run';
    final runBase = (run + 1) * 1000000000;
    final processStart = runBase - 1000000;
    var state = _initialOperationState(operationId, fixtureSource);
    var previousFinish = processStart + 1000;
    for (var warmupIndex = 0; warmupIndex < warmupsPerRun; warmupIndex += 1) {
      final started = runBase - 500000 + warmupIndex * 1000;
      final finished = started + 500;
      final outcome = _expectedOperationOutcome(
        operationId: operationId,
        operation: operation,
        before: state,
        operationOrdinal: warmupIndex,
      );
      warmups.add(<String, Object?>{
        'runId': runId,
        'warmupId': '$runId-warmup-$warmupIndex',
        'warmupIndex': warmupIndex,
        'processId': processId,
        'startedMicros': started,
        'finishedMicros': finished,
        'operationProof': outcome.proof,
      });
      state = outcome.finalState;
      previousFinish = finished;
    }

    var lastEvidenceFinish = previousFinish;
    for (var sampleIndex = 0; sampleIndex < samplesPerRun; sampleIndex += 1) {
      final sampleId = '$runId-sample-$sampleIndex';
      final startOrdinal = sampleIndex * framesPerSample;
      final endOrdinal = startOrdinal + framesPerSample - 1;
      final scheduled = cadenceHz > 0
          ? runBase + (sampleIndex * 1000000 / cadenceHz).round()
          : runBase + (startOrdinal * framePeriod).round();
      final accepted = scheduled + 100;
      final rasterFinish = accepted + 5000;
      final outcome = _expectedOperationOutcome(
        operationId: operationId,
        operation: operation,
        before: state,
        operationOrdinal: warmupsPerRun + sampleIndex,
      );
      final visibleStart = 0;
      final visibleEnd = _visibleEnd(outcome.paintedState.source);
      final visibleBytes = utf8
          .encode(outcome.paintedState.source)
          .sublist(visibleStart, visibleEnd);
      final visibleText = utf8.decode(visibleBytes);
      final projection = _renderProjectionHash(
        outcome.paintedState,
        visibleStart,
        visibleEnd,
      );
      final beforeProjection = _renderProjectionHash(
        outcome.beforeState,
        visibleStart,
        visibleEnd > utf8.encode(outcome.beforeState.source).length
            ? utf8.encode(outcome.beforeState.source).length
            : visibleEnd,
      );
      final glyph = _glyphRunHash(workloads, visibleText);
      final raster = _rasterHash(workloads, glyph, projection);
      final provingFrameId = '$runId-frame-$startOrdinal';
      final workUnitIds = <String>['$sampleId-work'];
      final pumpIds = <String>['$sampleId-pump'];
      samples.add(<String, Object?>{
        'runId': runId,
        'sampleId': sampleId,
        'sampleIndex': sampleIndex,
        'processId': processId,
        'frameId': provingFrameId,
        'measurementStartVsyncOrdinal': startOrdinal,
        'measurementEndVsyncOrdinal': endOrdinal,
        'scheduledMicros': scheduled,
        'acceptedMicros': accepted,
        'sourcePaintMicros': rasterFinish,
        'caretPaintMicros': rasterFinish,
        'selectionPaintMicros': rasterFinish,
        'foregroundStartMicros': accepted,
        'foregroundFinishMicros': accepted + 3000,
        'rustStartMicros': accepted,
        'rustFinishMicros': accepted + 1000,
        'ffiStartMicros': accepted + 1000,
        'ffiFinishMicros': accepted + 1100,
        'dartStartMicros': accepted + 1100,
        'dartFinishMicros': accepted + 1500,
        'layoutStartMicros': accepted + 1500,
        'layoutFinishMicros': accepted + 2000,
        'paintStartMicros': accepted + 2000,
        'paintFinishMicros': accepted + 2500,
        'synchronousSpans': <Object?>[
          <String, Object?>{
            'spanId': '$sampleId-sync',
            'threadId': 'ui',
            'startMicros': accepted,
            'finishMicros': accepted + 2000,
          },
        ],
        'workUnitIds': workUnitIds,
        'pumpIds': pumpIds,
        'convergenceFinishedMicros': accepted + 100000,
        'uncertifiedVisibleCharacterFrameCounts': <int>[1],
        'terminalState': 'complete',
        'terminalReason': null,
        'progressTokenAdvanced': true,
        'sourceRevisionBefore': outcome.beforeState.revision,
        'sourceRevisionAfter': outcome.paintedState.revision,
        'sourceSha256Before': _hashText(outcome.beforeState.source),
        'sourceSha256After': _hashText(outcome.paintedState.source),
        'distantProjectionSha256Before': beforeProjection,
        'distantProjectionSha256After': projection,
        'referenceDestinationBefore': outcome.referenceDestinationBefore,
        'referenceDestinationAfter': outcome.referenceDestinationAfter,
        'postIterationSourceSha256': _hashText(outcome.finalState.source),
        'operationProof': outcome.proof,
      });
      renderEvidence.add(<String, Object?>{
        'renderEvidenceId': '$sampleId-render',
        'runId': runId,
        'sampleId': sampleId,
        'processId': processId,
        'frameId': provingFrameId,
        'sourceRevision': outcome.paintedState.revision,
        'sourceSha256': _hashText(outcome.paintedState.source),
        'visibleStartUtf8': visibleStart,
        'visibleEndUtf8': visibleEnd,
        'visibleText': visibleText,
        'visibleTextSha256': _hashText(visibleText),
        'projectionSha256': projection,
        'glyphCount': visibleText.runes.length,
        'glyphRunSha256': glyph,
        'rasterSha256': raster,
        'rasterFinishedMicros': rasterFinish,
      });
      for (var ordinal = startOrdinal; ordinal <= endOrdinal; ordinal += 1) {
        final attributed = ordinal == startOrdinal;
        final vsync = runBase + (ordinal * framePeriod).round();
        final frameId = '$runId-frame-$ordinal';
        final buildStart = attributed ? accepted + 1 : vsync + 1;
        final buildFinish = attributed ? accepted + 1001 : vsync + 101;
        final rasterStart = attributed ? accepted + 2000 : vsync + 200;
        final frameRasterFinish = attributed ? rasterFinish : vsync + 500;
        frames.add(<String, Object?>{
          'runId': runId,
          'frameId': frameId,
          'vsyncOrdinal': ordinal,
          'processId': processId,
          'sampleId': attributed ? sampleId : null,
          'editorAttributed': attributed,
          'vsyncStartMicros': vsync,
          'buildStartMicros': buildStart,
          'buildFinishMicros': buildFinish,
          'rasterStartMicros': rasterStart,
          'rasterFinishMicros': frameRasterFinish,
          'ffiCalls': attributed
              ? <Object?>[
                  <String, Object?>{
                    'callId': '$sampleId-ffi',
                    'returnedBytes': 64,
                  },
                ]
              : <Object?>[],
          'workUnitIds': attributed ? workUnitIds : <Object?>[],
          'pumpIds': attributed ? pumpIds : <Object?>[],
        });
      }
      state = outcome.finalState;
      lastEvidenceFinish = accepted + 120000;
    }

    final closeMicros = lastEvidenceFinish + 10000;
    final postCloseMicros = closeMicros + 10000;
    final processFinish = postCloseMicros + 100000;
    processTiming[processId] = <String, int>{
      'start': processStart,
      'close': closeMicros,
      'postClose': postCloseMicros,
      'finish': processFinish,
    };
    processes.add(<String, Object?>{
      'runId': runId,
      'processId': processId,
      'startedMicros': processStart,
      'finishedMicros': processFinish,
    });
    final memoryRows = <(String, int, int, int, int)>[
      ('baseline', processStart + 100, 104857600, 0, 0),
      ('peak', runBase + 1000, 157286400, 1000, 1048576),
      ('close', closeMicros, 120000000, 1000, 1048576),
      ('post-close', postCloseMicros, 115343360, 1000, 1048576),
    ];
    for (var index = 0; index < memoryRows.length; index += 1) {
      final row = memoryRows[index];
      memory.add(<String, Object?>{
        'memorySampleId': '$processId-memory-$index',
        'processId': processId,
        'timestampMicros': row.$2,
        'phase': row.$1,
        'residentBytes': row.$3,
        'allocationCount': row.$4,
        'allocatedBytes': row.$5,
        'allocatorRssVarianceBytes': 5242880,
      });
    }
  }

  final lifecycleProcess = _string(processes.first, 'processId');
  final lifecycleStart = _integer(processTiming[lifecycleProcess]!, 'start');
  final raw = <String, Object?>{
    'schemaVersion': 1,
    'evidenceId': 'synthetic-raw-$operationId',
    'measurementSurface': 'flutter-product',
    'contract': <String, Object?>{
      'workloadMatrixSha256': _object(
        receipt,
        'contract',
      )['workloadMatrixSha256'],
      'resultSchemaSha256': _object(receipt, 'contract')['resultSchemaSha256'],
    },
    'workloadId': receipt['workloadId'],
    'fixture': <String, Object?>{
      'recipeId': fixture['recipeId'],
      'targetBytes': fixture['targetBytes'],
      'actualBytes': fixture['actualBytes'],
      'sha256': fixture['sha256'],
    },
    'processes': processes,
    'warmups': warmups,
    'samples': samples,
    'frames': frames,
    'renderEvidence': renderEvidence,
    'memorySamples': memory,
    'lifecycle': <String, Object?>{
      'openEditCloseCycles': <Object?>[
        for (var index = 0; index < 100; index += 1)
          <String, Object?>{
            'cycleId': 'open-cycle-$index',
            'processId': lifecycleProcess,
            'openMicros': lifecycleStart + 1000 + index * 20,
            'editMicros': lifecycleStart + 1005 + index * 20,
            'closeMicros': lifecycleStart + 1010 + index * 20,
          },
      ],
      'processReopens': <Object?>[
        for (var index = 0; index < 10; index += 1)
          <String, Object?>{
            'reopenId': 'reopen-$index',
            'processId': lifecycleProcess,
            'closedMicros': lifecycleStart + 5000 + index * 20,
            'openedMicros': lifecycleStart + 5010 + index * 20,
          },
      ],
      'backgroundForegroundCycles': <Object?>[],
      'sustainedIntervals': <Object?>[],
      'thermalSamples': <Object?>[
        for (final process in processes)
          <String, Object?>{
            'processId': process['processId'],
            'timestampMicros': _integer(process, 'startedMicros') + 50,
            'state': 'not-applicable',
          },
      ],
      'thermalThrottleEvents': <Object?>[],
      'memoryPressureEvents': <Object?>[],
      'finalLiveStateSamples': <Object?>[
        for (final process in processes)
          <String, Object?>{
            'processId': process['processId'],
            'timestampMicros':
                _integer(
                  processTiming[_string(process, 'processId')]!,
                  'postClose',
                ) +
                10,
            'liveDocuments': 0,
            'liveTransactions': 0,
            'liveContinuations': 0,
            'liveHandles': 0,
          },
      ],
    },
  };
  final rawPath = '/synthetic/flark/$operationId.raw-evidence.json';
  final rawBytes = utf8.encode(jsonEncode(raw));
  sampling['rawArtifacts'] = <Object?>[
    <String, Object?>{
      'kind': 'flark-v4-raw-evidence-v1',
      'path': rawPath,
      'byteLength': rawBytes.length,
      'sha256': sha256.convert(rawBytes).toString(),
    },
  ];
  final artifacts = <String, List<int>>{
    buildPath: buildBytes,
    rawPath: rawBytes,
  };
  final replayErrors = <String>[];
  final replay = _replayRawEvidence(
    raw: raw,
    receipt: receipt,
    workloads: workloads,
    errors: replayErrors,
  );
  if (replay == null || replayErrors.isNotEmpty) {
    throw StateError('synthetic raw evidence is invalid: $replayErrors');
  }
  receipt['durationMicros'] = replay.durationMicros;
  receipt['metrics'] = replay.metrics;
  return _ClaimEvidenceFixture(
    receipt: receipt,
    raw: raw,
    rawPath: rawPath,
    artifacts: artifacts,
  );
}

_ClaimEvidenceFixture _mutateRawEvidence(
  _ClaimEvidenceFixture source,
  void Function(Map<String, Object?> raw) mutation,
) {
  final receipt = _deepCopy(source.receipt);
  final raw = _deepCopy(source.raw);
  mutation(raw);
  final bytes = utf8.encode(jsonEncode(raw));
  final sampling = _object(_object(receipt, 'provenance'), 'sampling');
  final artifact = _list(sampling, 'rawArtifacts')
      .map((value) => _asObject(value, 'raw artifact'))
      .firstWhere((value) => value['kind'] == 'flark-v4-raw-evidence-v1');
  artifact['byteLength'] = bytes.length;
  artifact['sha256'] = sha256.convert(bytes).toString();
  return _ClaimEvidenceFixture(
    receipt: receipt,
    raw: raw,
    rawPath: source.rawPath,
    artifacts: <String, List<int>>{...source.artifacts, source.rawPath: bytes},
  );
}

String _hashText(String value) => sha256.convert(utf8.encode(value)).toString();

void _setDistributionSampleCounts(Object? value, {required int sampleCount}) {
  if (value is List<Object?>) {
    for (final item in value) {
      _setDistributionSampleCounts(item, sampleCount: sampleCount);
    }
    return;
  }
  if (value is! Map<String, Object?>) return;
  const distributionKeys = <String>{'sampleCount', 'p50', 'p90', 'p99', 'max'};
  if (value.keys.toSet().containsAll(distributionKeys)) {
    value['sampleCount'] = sampleCount;
  }
  for (final nested in value.values) {
    _setDistributionSampleCounts(nested, sampleCount: sampleCount);
  }
}

Map<String, Object?> _deepCopy(Map<String, Object?> value) =>
    _asObject(jsonDecode(jsonEncode(value)), 'deep copy');

Map<String, Object?> _distribution(
  Map<String, Object?> receipt,
  String family,
  String name,
) => _object(_object(_object(receipt, 'metrics'), family), name);

Map<String, Object?> _jsonObject(File file) =>
    _asObject(jsonDecode(file.readAsStringSync()), file.path);

Map<String, Object?> _asObject(Object? value, String label) {
  if (value case final Map<String, Object?> object) return object;
  throw FormatException('$label is not a JSON object');
}

Map<String, Object?> _object(Map<String, Object?> object, String key) =>
    _asObject(object[key], key);

List<Object?> _list(Map<String, Object?> object, String key) {
  final value = object[key];
  if (value case final List<Object?> list) return list;
  throw FormatException('$key is not a JSON list');
}

List<Map<String, Object?>> _objectList(
  Map<String, Object?> object,
  String key,
) => _list(
  object,
  key,
).map((value) => _asObject(value, '$key entry')).toList(growable: false);

List<String> _strings(Map<String, Object?> object, String key) =>
    _list(object, key).cast<String>();

Set<String> _stringSet(Map<String, Object?> object, String key) =>
    _strings(object, key).toSet();

Set<String> _ids(Map<String, Object?> object, String key) => {
  for (final value in _list(object, key))
    _string(_asObject(value, '$key entry'), 'id'),
};

String _string(Map<String, Object?> object, String key) {
  final value = object[key];
  if (value case final String string) return string;
  throw FormatException('$key is not a string');
}

int _integer(Map<String, Object?> object, String key) {
  final value = object[key];
  if (value case final int integer) return integer;
  throw FormatException('$key is not an integer');
}

num _number(Map<String, Object?> object, String key) {
  final value = object[key];
  if (value case final num number) return number;
  throw FormatException('$key is not a number');
}

bool _boolean(Map<String, Object?> object, String key) {
  final value = object[key];
  if (value case final bool boolean) return boolean;
  throw FormatException('$key is not a boolean');
}
