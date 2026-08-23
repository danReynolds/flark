// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

import '../../../scripts/dogfood_bundle_manifest.dart';
import '../../../scripts/dogfood_gate_receipt.dart';
import '../../../scripts/dogfood_native_receipt.dart';
import '../../../scripts/verify_v4_dogfood_completion.dart';
import '../../../scripts/verify_v4_dogfood_receipt.dart';
import 'dogfood_performance_receipt_contract_test.dart' as performance_fixture;

void main() {
  test('completion binds every D0 lane to one exact candidate app', () async {
    final fixture = await _CompletionFixture.create();
    addTearDown(fixture.dispose);

    final receipt = await fixture.build();
    expect(receipt['schema'], 'dogfood_completion_v1');
    expect((receipt['assessment']! as Map)['result'], 'PASS');
    expect((receipt['candidate']! as Map)['commit'], fixture.head);
    expect(((receipt['gates']! as Map)['nativeCanary']! as Map)['skipped'], 0);
  });

  test('completion rejects stale CI and changed review evidence', () async {
    final fixture = await _CompletionFixture.create();
    addTearDown(fixture.dispose);

    final stale = jsonDecode(await fixture.evidence.readAsString()) as Map;
    (stale['ci']! as Map)['headSha'] = List.filled(40, 'f').join();
    await fixture.evidence.writeAsString(jsonEncode(stale));
    await expectLater(fixture.build(), throwsStateError);

    await fixture.writeEvidence();
    await fixture.architectureReview.writeAsString('changed\n');
    await expectLater(fixture.build(), throwsStateError);
  });

  test(
    'completion replays gates, native machine output, CI, and app path',
    () async {
      final fixture = await _CompletionFixture.create();
      addTearDown(fixture.dispose);

      final defaultLog = await fixture.defaultLog.readAsString();
      await fixture.defaultLog.writeAsString('forged success marker\n');
      await expectLater(fixture.build(), throwsStateError);
      await fixture.defaultLog.writeAsString(defaultLog);

      final defaultReceipt = await fixture.defaultGateReceipt.readAsString();
      await fixture.defaultLog.writeAsString('${defaultLog}late output\n');
      final lateReceipt = jsonDecode(defaultReceipt) as Map;
      lateReceipt['log'] = await _identity(fixture.defaultLog);
      await fixture.defaultGateReceipt.writeAsString(jsonEncode(lateReceipt));
      await expectLater(fixture.build(), throwsStateError);

      final reordered = const LineSplitter().convert(defaultLog);
      final last = reordered.removeLast();
      final beforeLast = reordered.removeLast();
      await fixture.defaultLog.writeAsString(
        '${reordered.join('\n')}\n$last\n$beforeLast\n',
      );
      final reorderedReceipt = jsonDecode(defaultReceipt) as Map;
      reorderedReceipt['log'] = await _identity(fixture.defaultLog);
      await fixture.defaultGateReceipt.writeAsString(
        jsonEncode(reorderedReceipt),
      );
      await expectLater(fixture.build(), throwsStateError);

      await fixture.defaultLog.writeAsString(defaultLog);
      await fixture.defaultGateReceipt.writeAsString(defaultReceipt);

      final paintLog = await fixture.paintLog.readAsString();
      final paintReceipt = await fixture.actualPaintReceipt.readAsString();
      final mislabeledPaintLines = const LineSplitter().convert(paintLog);
      var relabeledPaintTests = 0;
      for (var index = 0; index < mislabeledPaintLines.length; index += 1) {
        Object? decoded;
        try {
          decoded = jsonDecode(mislabeledPaintLines[index]);
        } on FormatException {
          continue;
        }
        if (decoded is! Map || decoded['type'] != 'testStart') continue;
        final test = decoded['test'];
        if (test is! Map || test['root_url'] is! String) continue;
        test['root_url'] = 'file://${fixture.base.path}/unrelated_test.dart';
        mislabeledPaintLines[index] = jsonEncode(decoded);
        relabeledPaintTests += 1;
      }
      expect(relabeledPaintTests, greaterThan(0));
      await fixture.paintLog.writeAsString(
        '${mislabeledPaintLines.join('\n')}\n',
      );
      final mislabeledPaintReceipt = jsonDecode(paintReceipt) as Map;
      mislabeledPaintReceipt['log'] = await _identity(fixture.paintLog);
      await fixture.actualPaintReceipt.writeAsString(
        jsonEncode(mislabeledPaintReceipt),
      );
      await expectLater(fixture.build(), throwsStateError);
      await fixture.paintLog.writeAsString(paintLog);
      await fixture.actualPaintReceipt.writeAsString(paintReceipt);

      final machineLog = await fixture.machineLog.readAsString();
      await fixture.machineLog.writeAsString(
        '${jsonEncode({'type': 'done', 'success': true})}\n',
      );
      await expectLater(fixture.build(), throwsStateError);
      await fixture.machineLog.writeAsString(machineLog);

      await expectLater(
        fixture.build(
          githubJobLookup: (repository, jobId) async => {
            'id': jobId,
            'run_id': jobId == 11 ? 101 : 102,
            'name': jobId == 11 ? 'v4-integration-gate' : 'macos-smoke',
            'conclusion': 'failure',
            'head_sha': fixture.head,
            'html_url': jobId == 11
                ? 'https://github.com/example/flark/actions/runs/101/job/11'
                : 'https://github.com/example/flark/actions/runs/102/job/12',
          },
        ),
        throwsStateError,
      );

      await fixture.writeEvidence();
      final evidence = jsonDecode(await fixture.evidence.readAsString()) as Map;
      (evidence['handoff']! as Map)['appBundlePath'] =
          '${fixture.base.path}/other.app';
      await fixture.evidence.writeAsString(jsonEncode(evidence));
      await expectLater(fixture.build(), throwsStateError);
    },
  );

  test('gate execution and native test identity cannot be attested', () async {
    final fixture = await _CompletionFixture.create();
    addTearDown(fixture.dispose);

    final failingRepository = Directory('${fixture.base.path}/failing-repo')
      ..createSync();
    await _git(failingRepository, const ['init']);
    await _git(failingRepository, const [
      'config',
      'user.email',
      'test@example.com',
    ]);
    await _git(failingRepository, const [
      'config',
      'user.name',
      'Receipt Test',
    ]);
    File('${failingRepository.path}/scripts/verify_v4.sh')
      ..createSync(recursive: true)
      ..writeAsStringSync(
        '#!/usr/bin/env bash\n'
        "echo 'verify_v4: active rust + dart + flutter v4 suites executed and passed.'\n"
        "echo 'verify_v4: run scripts/verify_v4_certification_stress.sh for slow stress lanes.'\n"
        'exit 1\n',
      );
    await _git(failingRepository, const ['add', '.']);
    await _git(failingRepository, const ['commit', '-m', 'failing gate']);
    final failedLog = File('${fixture.base.path}/failed-default.log');
    await expectLater(
      runDogfoodGate(
        repository: failingRepository,
        lane: 'default',
        log: failedLog,
      ),
      throwsStateError,
    );

    final sanitized = dogfoodGateProcessEnvironment({
      'PATH': '/fake/bin',
      'HOME': '/safe/home',
      'FLARK_V4_FEATURES': 'opening-session',
      'FLARK_V4_PROFILE': 'release',
      'RUSTFLAGS': '-Cinstrument-coverage',
      'BASH_ENV': '/tmp/inject-success.sh',
    });
    expect(sanitized, {'PATH': '/fake/bin', 'HOME': '/safe/home'});

    final paintReceipt =
        jsonDecode(await fixture.actualPaintReceipt.readAsString()) as Map;
    final originalPaintReceipt = jsonEncode(paintReceipt);
    ((paintReceipt['environment']! as Map)['overrides']!
            as Map)['FLARK_V4_LIBRARY_PATH'] =
        '${fixture.base.path}/different-abi.dylib';
    await fixture.actualPaintReceipt.writeAsString(jsonEncode(paintReceipt));
    await expectLater(fixture.build(), throwsStateError);
    await fixture.actualPaintReceipt.writeAsString(originalPaintReceipt);

    final shimReceipt =
        jsonDecode(await fixture.actualPaintReceipt.readAsString()) as Map;
    final shimIdentity = ((shimReceipt['toolchain']! as List).single as Map);
    final shim = await _writeExecutable(
      File('${fixture.base.path}/untrusted-flutter'),
      '#!/bin/sh\nexit 0\n',
    );
    final shimFileIdentity = await _identity(shim);
    shimIdentity
      ..['path'] = shimFileIdentity['path']
      ..['bytes'] = shimFileIdentity['bytes']
      ..['sha256'] = shimFileIdentity['sha256'];
    (shimReceipt['command']! as List).first = shim.absolute.path;
    await fixture.actualPaintReceipt.writeAsString(jsonEncode(shimReceipt));
    await expectLater(fixture.build(), throwsStateError);
    await fixture.actualPaintReceipt.writeAsString(originalPaintReceipt);

    await fixture.machineLog.writeAsString(
      '${jsonEncode({
        'type': 'testStart',
        'test': {'id': 1, 'name': 'an unrelated successful Flutter test'},
      })}\n'
      '${jsonEncode({'type': 'testDone', 'testID': 1, 'result': 'success', 'skipped': false})}\n'
      '${jsonEncode({'type': 'done', 'success': true})}\n',
    );
    final native =
        jsonDecode(await fixture.nativeReceipt.readAsString()) as Map;
    (native['nativeCanary']! as Map)['name'] =
        'an unrelated successful Flutter test';
    (native['nativeCanary']! as Map)['machineLog'] = await _identity(
      fixture.machineLog,
    );
    await fixture.nativeReceipt.writeAsString(jsonEncode(native));
    await expectLater(fixture.build(), throwsStateError);
  });

  test('candidate evidence schema is strict and versioned', () {
    final schema =
        jsonDecode(
              File(
                'docs/testing/dogfood_candidate_evidence_v1.schema.json',
              ).readAsStringSync(),
            )
            as Map;
    expect(schema[r'$schema'], 'https://json-schema.org/draft/2020-12/schema');
    expect(schema['additionalProperties'], isFalse);
    expect((schema['properties']! as Map)['schema'], {
      'const': 'dogfood_candidate_evidence_v1',
    });
  });
}

final class _CompletionFixture {
  _CompletionFixture({
    required this.base,
    required this.repository,
    required this.app,
    required this.mainExecutable,
    required this.abi,
    required this.manifest,
    required this.defaultLog,
    required this.stressLog,
    required this.paintLog,
    required this.defaultGateReceipt,
    required this.stressGateReceipt,
    required this.actualPaintReceipt,
    required this.machineLog,
    required this.nativeReceipt,
    required this.performanceReceipt,
    required this.evidence,
    required this.architectureReview,
    required this.evidenceReview,
    required this.capture,
    required this.commandLog,
    required this.b2Ledger,
    required this.head,
    required this.tree,
    required this.bundleDigest,
  });

  final Directory base;
  final Directory repository;
  final Directory app;
  final File mainExecutable;
  final File abi;
  final File manifest;
  final File defaultLog;
  final File stressLog;
  final File paintLog;
  final File defaultGateReceipt;
  final File stressGateReceipt;
  final File actualPaintReceipt;
  final File machineLog;
  final File nativeReceipt;
  final File performanceReceipt;
  final File evidence;
  final File architectureReview;
  final File evidenceReview;
  final File capture;
  final File commandLog;
  final File b2Ledger;
  final String head;
  final String tree;
  final String bundleDigest;

  static Future<_CompletionFixture> create() async {
    final base = await Directory.systemTemp.createTemp('flark-d0-completion-');
    final repository = Directory('${base.path}/repo')..createSync();
    await _git(repository, const ['init']);
    await _git(repository, const ['config', 'user.email', 'test@example.com']);
    await _git(repository, const ['config', 'user.name', 'Receipt Test']);
    await _git(repository, const [
      'remote',
      'add',
      'origin',
      'https://github.com/example/flark.git',
    ]);
    File('${repository.path}/tracked.txt').writeAsStringSync('tracked\n');
    File('${repository.path}/.gitignore').writeAsStringSync(
      '.dart_tool/\n.flutter-plugins*\nbuild/\npubspec.lock\n',
    );
    File('${repository.path}/scripts/verify_v4.sh')
      ..createSync(recursive: true)
      ..writeAsStringSync(
        '#!/usr/bin/env bash\n'
        "echo 'verify_v4: active rust + dart + flutter v4 suites executed and passed.'\n"
        "echo 'verify_v4: run scripts/verify_v4_certification_stress.sh for slow stress lanes.'\n"
        "echo 'nested qualification output may repeat the protocol'\n"
        "echo 'verify_v4: active rust + dart + flutter v4 suites executed and passed.'\n"
        "echo 'verify_v4: run scripts/verify_v4_certification_stress.sh for slow stress lanes.'\n",
      );
    File(
      '${repository.path}/scripts/verify_v4_certification_stress.sh',
    ).writeAsStringSync(
      '#!/usr/bin/env bash\n'
      "echo 'verify_v4_certification_stress: full payload-budget stress passed.'\n"
      "echo 'verify_v4_certification_stress: historical M0 receipt drift remains a separate audit.'\n",
    );
    for (final path in const [
      'packages/flark/test/north_star_paint_matrix_test.dart',
      'packages/flark/test/inline_dependency_island_paint_acceptance_test.dart',
    ]) {
      File('${repository.path}/$path')
        ..createSync(recursive: true)
        ..writeAsStringSync(
          "import 'package:flutter_test/flutter_test.dart';\n"
          "void main() { testWidgets('fixture actual paint', (tester) async {}); }\n",
        );
    }
    File('${repository.path}/packages/flark/pubspec.yaml').writeAsStringSync(
      'name: flark_dogfood_gate_fixture\n'
      'environment:\n'
      "  sdk: '>=3.9.0 <4.0.0'\n"
      'dev_dependencies:\n'
      '  flutter_test:\n'
      '    sdk: flutter\n',
    );
    await _git(repository, const ['add', '.']);
    await _git(repository, const ['commit', '-m', 'candidate']);
    final head = await _git(repository, const ['rev-parse', 'HEAD']);
    final tree = await _git(repository, const ['rev-parse', 'HEAD^{tree}']);

    final app = Directory('${base.path}/Flark Dogfood.app');
    final mainExecutable = File('${app.path}/Contents/MacOS/Flark Dogfood');
    final abi = File(
      '${app.path}/Contents/Frameworks/flark_abi.framework/flark_abi',
    );
    mainExecutable.parent.createSync(recursive: true);
    abi.parent.createSync(recursive: true);
    mainExecutable.writeAsStringSync('main');
    abi.writeAsStringSync('abi');
    final builtManifest = await buildDogfoodBundleManifest(app);
    final manifest = File('${base.path}/app_bundle_manifest.json')
      ..writeAsStringSync(jsonEncode(builtManifest.toJson()));

    final defaultLog = File('${base.path}/default.log');
    final stressLog = File('${base.path}/stress.log');
    final paintLog = File('${base.path}/paint.log');
    final defaultGateReceipt = File('${base.path}/default-gate.json')
      ..writeAsStringSync(
        jsonEncode(
          await runDogfoodGate(
            repository: repository,
            lane: 'default',
            log: defaultLog,
          ),
        ),
      );
    final stressGateReceipt = File('${base.path}/stress-gate.json')
      ..writeAsStringSync(
        jsonEncode(
          await runDogfoodGate(
            repository: repository,
            lane: 'stress',
            log: stressLog,
          ),
        ),
      );
    final actualPaintReceipt = File('${base.path}/actual-paint.json')
      ..writeAsStringSync(
        jsonEncode(
          await runDogfoodGate(
            repository: repository,
            lane: 'actual-paint',
            log: paintLog,
            embeddedAbi: abi,
          ),
        ),
      );
    final machine = File('${base.path}/machine.jsonl')
      ..writeAsStringSync(
        '${jsonEncode({
          'type': 'testStart',
          'test': {'id': 1, 'name': 'macOS routes the native editing canaries without faults or visual relay'},
        })}\n'
        '${jsonEncode({'type': 'testDone', 'testID': 1, 'result': 'success', 'skipped': false})}\n'
        '${jsonEncode({'type': 'done', 'success': true})}\n',
      );
    final native = await buildDogfoodNativeReceipt(
      repository: repository,
      appBundle: app,
      bundleManifest: manifest,
      mainExecutable: mainExecutable,
      embeddedAbi: abi,
      machineLog: machine,
      expectedTestName:
          'macOS routes the native editing canaries without faults or visual relay',
    );
    final nativeReceipt = File('${base.path}/native.json')
      ..writeAsStringSync(jsonEncode(native));

    final ledger = File('${base.path}/ledger.md')
      ..writeAsStringSync('ledger\n');
    final harness = File('${base.path}/harness.dart')
      ..writeAsStringSync('void main() {}\n');
    final raw = performance_fixture.validRawDogfoodPerformanceReceiptForTest();
    raw['candidate'] = {'commit': head, 'tree': tree, 'clean': true};
    (raw['configuration']! as Map)['ledger'] = await _identity(ledger);
    final mainIdentity = await _identity(mainExecutable);
    final abiIdentity = await _identity(abi);
    final host = (raw['host']! as Map).cast<String, Object?>();
    final binding = <String, Object?>{
      'candidateCommit': head,
      'candidateTree': tree,
      'bundleManifestSha256': builtManifest.sha256,
      'mainExecutable': mainIdentity,
      'embeddedAbi': abiIdentity,
      'measurementHost': {
        for (final name in const [
          'hostname',
          'operatingSystem',
          'architecture',
          'logicalCores',
          'physicalMemoryBytes',
        ])
          name: host[name],
      },
    };
    final fragments = <File>[];
    for (final rawCell in (raw['cells']! as List).cast<Map>()) {
      final cell = rawCell.cast<String, Object?>();
      for (final rawRun in (cell['runs']! as List).cast<Map>()) {
        final run = rawRun.cast<String, Object?>();
        final fragment = File(
          '${base.path}/fragments/${cell['id']}.run-${run['run']}.json',
        );
        await fragment.parent.create(recursive: true);
        await fragment.writeAsString(
          jsonEncode({
            'id': cell['id'],
            'sourceBytes': cell['sourceBytes'],
            'warmupsPerRun': cell['warmupsPerRun'],
            'samplesPerRun': cell['samplesPerRun'],
            'runCount': cell['runCount'],
            'cadenceHz': cell['cadenceHz'],
            'binding': binding,
            'fixture': cell['fixture'],
            'display': raw['display'],
            'run': run,
          }),
        );
        fragments.add(fragment);
      }
    }
    raw['artifacts'] = {
      'appBundleManifest': await _identity(manifest),
      'mainExecutable': mainIdentity,
      'embeddedAbi': abiIdentity,
      'profileHarness': await _identity(harness),
      'profileFragments': [
        for (final fragment in fragments) await _identity(fragment),
      ],
    };
    final performance = await sealDogfoodPerformanceReceipt(
      raw,
      repository: repository,
    );
    expect((performance['assessment']! as Map)['result'], 'PASS');
    final performanceReceipt = File('${base.path}/performance.json')
      ..writeAsStringSync(jsonEncode(performance));

    final architectureReview = File('${base.path}/architecture.md')
      ..writeAsStringSync('PASS\n');
    final evidenceReview = File('${base.path}/evidence.md')
      ..writeAsStringSync('PASS\n');
    final capture = File('${base.path}/capture.mov')
      ..writeAsStringSync('capture');
    final commandLog = File('${base.path}/moving.log')
      ..writeAsStringSync('commands\n');
    final b2Ledger = File('${base.path}/b2.md')..writeAsStringSync('B2\n');
    final evidence = File('${base.path}/candidate-evidence.json');
    final fixture = _CompletionFixture(
      base: base,
      repository: repository,
      app: app,
      mainExecutable: mainExecutable,
      abi: abi,
      manifest: manifest,
      defaultLog: defaultLog,
      stressLog: stressLog,
      paintLog: paintLog,
      defaultGateReceipt: defaultGateReceipt,
      stressGateReceipt: stressGateReceipt,
      actualPaintReceipt: actualPaintReceipt,
      machineLog: machine,
      nativeReceipt: nativeReceipt,
      performanceReceipt: performanceReceipt,
      evidence: evidence,
      architectureReview: architectureReview,
      evidenceReview: evidenceReview,
      capture: capture,
      commandLog: commandLog,
      b2Ledger: b2Ledger,
      head: head,
      tree: tree,
      bundleDigest: builtManifest.sha256,
    );
    await fixture.writeEvidence();
    return fixture;
  }

  Future<void> writeEvidence() async {
    await evidence.writeAsString(
      jsonEncode({
        'schema': 'dogfood_candidate_evidence_v1',
        'candidate': {'commit': head, 'tree': tree},
        'openingSession': {
          'result': 'DISABLED',
          'reason': 'streamed preset disabled in D0 app',
        },
        'ci': {
          'repository': 'example/flark',
          'headSha': head,
          'checks': [
            {
              'name': 'v4-integration-gate',
              'jobId': 11,
              'url': 'https://github.com/example/flark/actions/runs/101/job/11',
            },
            {
              'name': 'macos-smoke',
              'jobId': 12,
              'url': 'https://github.com/example/flark/actions/runs/102/job/12',
            },
          ],
        },
        'reviews': {
          'architecture': {
            'candidateCommit': head,
            'candidateTree': tree,
            'reviewer': 'architecture-reviewer',
            'result': 'PASS',
            'openB0': 0,
            'openB1': 0,
            'artifact': await _identity(architectureReview),
          },
          'evidence': {
            'candidateCommit': head,
            'candidateTree': tree,
            'reviewer': 'evidence-reviewer',
            'result': 'PASS',
            'openB0': 0,
            'openB1': 0,
            'artifact': await _identity(evidenceReview),
          },
        },
        'movingSurface': {
          'candidateCommit': head,
          'candidateTree': tree,
          'reviewer': 'moving-reviewer',
          'result': 'PASS',
          'appManifestSha256': bundleDigest,
          'completedSteps': const [
            'type-product-tour-prose',
            'replace-undo-redo',
            'return-successor-backspace-merge',
            'toggle-task',
            'edit-table-and-tab',
            'scroll-long-paragraph',
            'resize-out-and-back',
            'cycle-focus',
            'close-cleanly',
          ],
          'capture': await _identity(capture),
          'commandLog': await _identity(commandLog),
        },
        'blockers': {
          'openB0': 0,
          'openB1': 0,
          'b2Ledger': await _identity(b2Ledger),
        },
        'handoff': {'date': '2026-08-23', 'appBundlePath': app.absolute.path},
      }),
    );
  }

  Future<Map<String, Object?>> build({
    GitHubJobLookup? githubJobLookup,
  }) => buildDogfoodCompletionReceipt(
    repository: repository,
    defaultGateReceiptFile: defaultGateReceipt,
    stressGateReceiptFile: stressGateReceipt,
    actualPaintReceiptFile: actualPaintReceipt,
    nativeReceiptFile: nativeReceipt,
    performanceReceiptFile: performanceReceipt,
    candidateEvidenceFile: evidence,
    githubJobLookup:
        githubJobLookup ??
        (repository, jobId) async {
          final check = jobId == 11
              ? ('v4-integration-gate', 101)
              : ('macos-smoke', 102);
          return {
            'id': jobId,
            'run_id': check.$2,
            'name': check.$1,
            'conclusion': 'success',
            'head_sha': head,
            'html_url':
                'https://github.com/$repository/actions/runs/${check.$2}/job/$jobId',
          };
        },
  );

  Future<void> dispose() => base.delete(recursive: true);
}

Future<File> _writeExecutable(File file, String source) async {
  await file.writeAsString(source, flush: true);
  final result = await Process.run('chmod', ['+x', file.path]);
  expect(result.exitCode, 0, reason: result.stderr as String);
  return file;
}

Future<Map<String, Object>> _identity(File file) async => {
  'path': file.absolute.path,
  'bytes': await file.length(),
  'sha256': (await sha256.bind(file.openRead()).first).toString(),
};

Future<String> _git(Directory directory, List<String> arguments) async {
  final result = await Process.run(
    'git',
    arguments,
    workingDirectory: directory.path,
  );
  expect(result.exitCode, 0, reason: result.stderr as String);
  return (result.stdout as String).trim();
}
