// ignore_for_file: avoid_relative_lib_imports

import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

import '../../../scripts/dogfood_bundle_manifest.dart';
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
    File('${repository.path}/tracked.txt').writeAsStringSync('tracked\n');
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

    final candidateMarker = 'dogfood-candidate: commit=$head tree=$tree';
    final defaultLog = File('${base.path}/default.log')
      ..writeAsStringSync(
        'verify_v4: active rust + dart + flutter v4 suites executed and passed.\n'
        '$candidateMarker\n',
      );
    final stressLog = File('${base.path}/stress.log')
      ..writeAsStringSync(
        'verify_v4_certification_stress: full payload-budget stress passed.\n'
        '$candidateMarker\n',
      );
    final paintLog = File('${base.path}/paint.log')
      ..writeAsStringSync('All tests passed!\n$candidateMarker\n');
    final machine = File('${base.path}/machine.jsonl')
      ..writeAsStringSync(
        '${jsonEncode({
          'type': 'testStart',
          'test': {'id': 1, 'name': 'required canary'},
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
      expectedTestName: 'required canary',
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
    raw['artifacts'] = {
      'appBundleManifest': await _identity(manifest),
      'mainExecutable': await _identity(mainExecutable),
      'embeddedAbi': await _identity(abi),
      'profileHarness': await _identity(harness),
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
          'headSha': head,
          'checks': [
            {
              'name': 'v4-integration-gate',
              'result': 'SUCCESS',
              'url': 'https://github.com/example/actions/1',
            },
            {
              'name': 'macos-smoke',
              'result': 'SUCCESS',
              'url': 'https://github.com/example/actions/2',
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

  Future<Map<String, Object?>> build() => buildDogfoodCompletionReceipt(
    repository: repository,
    defaultGateLog: defaultLog,
    stressGateLog: stressLog,
    actualPaintLog: paintLog,
    nativeReceiptFile: nativeReceipt,
    performanceReceiptFile: performanceReceipt,
    candidateEvidenceFile: evidence,
  );

  Future<void> dispose() => base.delete(recursive: true);
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
