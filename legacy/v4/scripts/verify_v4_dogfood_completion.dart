import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

import 'dogfood_bundle_manifest.dart';
import 'dogfood_gate_receipt.dart';
import 'verify_flutter_machine_test.dart';
import 'verify_v4_dogfood_receipt.dart';

const _requiredCiChecks = {'v4-integration-gate', 'macos-smoke'};
const _nativeCanaryTestName =
    'macOS routes the native editing canaries without faults or visual relay';
const _movingSurfaceSteps = {
  'type-product-tour-prose',
  'replace-undo-redo',
  'return-successor-backspace-merge',
  'toggle-task',
  'edit-table-and-tab',
  'scroll-long-paragraph',
  'resize-out-and-back',
  'cycle-focus',
  'close-cleanly',
};

typedef GitHubJobLookup =
    Future<Map<String, Object?>> Function(String repository, int jobId);

Future<Map<String, Object?>> buildDogfoodCompletionReceipt({
  required Directory repository,
  required File defaultGateReceiptFile,
  required File stressGateReceiptFile,
  required File actualPaintReceiptFile,
  required File nativeReceiptFile,
  required File performanceReceiptFile,
  required File candidateEvidenceFile,
  GitHubJobLookup? githubJobLookup,
}) async {
  final head = await _git(repository, const ['rev-parse', 'HEAD']);
  final tree = await _git(repository, const ['rev-parse', 'HEAD^{tree}']);
  final status = await _git(repository, const ['status', '--porcelain']);
  if (status.isNotEmpty) {
    throw StateError('D0 completion requires a clean worktree.');
  }
  final defaultGate = await _readObject(defaultGateReceiptFile);
  final stressGate = await _readObject(stressGateReceiptFile);
  final actualPaintGate = await _readObject(actualPaintReceiptFile);
  final native = await _readObject(nativeReceiptFile);
  final performance = await _readObject(performanceReceiptFile);
  final evidence = await _readObject(candidateEvidenceFile);
  for (final entry in <(String, Map<String, Object?>)>[
    ('default', defaultGate),
    ('stress', stressGate),
    ('actual-paint', actualPaintGate),
  ]) {
    _requireCandidate(entry.$2, head, tree);
    if (entry.$2['lane'] != entry.$1) {
      throw StateError('D0 gate receipt has the wrong lane: ${entry.$1}.');
    }
    await verifyDogfoodGateReceipt(entry.$2, repository: repository);
  }
  _requireCandidate(native, head, tree, nativeNames: true);
  _requireCandidate(performance, head, tree);
  _requireCandidate(evidence, head, tree);

  if (native['schema'] != 'dogfood_native_receipt_v1' ||
      native['worktreeClean'] != true) {
    throw StateError('Native receipt is not a clean D0 receipt.');
  }
  final canary = _object(native['nativeCanary'], 'nativeCanary');
  if (canary['name'] != _nativeCanaryTestName ||
      canary['result'] != 'success' ||
      canary['skipped'] != false ||
      canary['runnerSucceeded'] != true) {
    throw StateError(
      'Native canary did not execute successfully without skip.',
    );
  }
  final machineLog = _object(canary['machineLog'], 'nativeCanary.machineLog');
  await _verifyFileIdentity(machineLog);
  final machineReplay = verifyFlutterMachineTest(
    await File(machineLog['path']! as String).readAsLines(),
    expectedName: _nativeCanaryTestName,
  );
  if (machineReplay.result != canary['result'] ||
      machineReplay.skipped != canary['skipped'] ||
      machineReplay.runnerSucceeded != canary['runnerSucceeded']) {
    throw StateError('Native receipt does not replay from its machine log.');
  }
  final performanceValidation = await verifyDogfoodPerformanceReceipt(
    performance,
    repository: repository,
  );
  if (!performanceValidation.passed) {
    throw StateError(
      'Performance receipt failed replay: '
      '${performanceValidation.blockers.join('; ')}',
    );
  }
  final performanceAssessment = _object(
    performance['assessment'],
    'performance.assessment',
  );
  if (performanceAssessment['result'] != 'PASS') {
    throw StateError('Performance receipt does not declare PASS.');
  }
  final verifiedApp = await _requireSameApp(native, performance);
  final actualPaintAbi = _object(
    actualPaintGate['embeddedAbi'],
    'actualPaint.embeddedAbi',
  );
  final nativeAbi = _object(native['embeddedAbi'], 'native.embeddedAbi');
  if (actualPaintAbi['bytes'] != nativeAbi['bytes'] ||
      actualPaintAbi['sha256'] != nativeAbi['sha256']) {
    throw StateError('Actual-paint gate did not use the candidate app ABI.');
  }

  if (evidence['schema'] != 'dogfood_candidate_evidence_v1') {
    throw StateError('Candidate evidence has the wrong schema.');
  }
  final opening = _object(evidence['openingSession'], 'openingSession');
  if (opening['result'] != 'DISABLED' ||
      opening['reason'] != 'streamed preset disabled in D0 app') {
    throw StateError(
      'Opening-session status does not match the frozen D0 app.',
    );
  }
  final expectedRepository = await _githubRepository(repository);
  await _verifyCi(
    _object(evidence['ci'], 'ci'),
    head,
    expectedRepository,
    githubJobLookup ?? _fetchGitHubJob,
  );
  final reviews = _object(evidence['reviews'], 'reviews');
  final architecture = await _verifyReview(
    _object(reviews['architecture'], 'reviews.architecture'),
    head,
    tree,
  );
  final evidenceReview = await _verifyReview(
    _object(reviews['evidence'], 'reviews.evidence'),
    head,
    tree,
  );
  if (architecture == evidenceReview) {
    throw StateError(
      'Architecture and evidence reviews require distinct reviewers.',
    );
  }
  await _verifyMovingSurface(
    _object(evidence['movingSurface'], 'movingSurface'),
    native,
    head,
    tree,
  );
  final blockers = _object(evidence['blockers'], 'blockers');
  if (blockers['openB0'] != 0 || blockers['openB1'] != 0) {
    throw StateError('D0 completion requires zero open B0 and B1 issues.');
  }
  await _verifyFileIdentity(_object(blockers['b2Ledger'], 'blockers.b2Ledger'));
  final handoff = _object(evidence['handoff'], 'handoff');
  if (handoff['date'] is! String || (handoff['date']! as String).isEmpty) {
    throw StateError('Handoff date is missing.');
  }
  if (handoff['appBundlePath'] != verifiedApp.absolute.path) {
    throw StateError('Handoff app is not the canary-tested app bundle.');
  }

  return {
    'schema': 'dogfood_completion_v1',
    'candidate': {'commit': head, 'tree': tree, 'clean': true},
    'artifacts': {
      'appBundle': native['appBundle'],
      'mainExecutable': native['mainExecutable'],
      'embeddedAbi': native['embeddedAbi'],
      'defaultGateReceipt': await _fileIdentity(defaultGateReceiptFile),
      'stressGateReceipt': await _fileIdentity(stressGateReceiptFile),
      'actualPaintReceipt': await _fileIdentity(actualPaintReceiptFile),
      'nativeReceipt': await _fileIdentity(nativeReceiptFile),
      'performanceReceipt': await _fileIdentity(performanceReceiptFile),
      'candidateEvidence': await _fileIdentity(candidateEvidenceFile),
    },
    'gates': {
      'default': {'result': 'PASS', 'evidence': defaultGate},
      'openingSession': opening,
      'stress': {'result': 'PASS', 'evidence': stressGate},
      'actualPaint': {'result': 'PASS', 'evidence': actualPaintGate},
      'nativeCanary': {'result': 'PASS', 'skipped': 0},
      'performanceAndLifecycle': {
        'result': 'PASS',
        'metrics': performanceAssessment['metrics'],
      },
    },
    'ci': evidence['ci'],
    'reviews': evidence['reviews'],
    'movingSurface': evidence['movingSurface'],
    'blockers': evidence['blockers'],
    'handoff': handoff,
    'assessment': {'result': 'PASS', 'openB0': 0, 'openB1': 0},
  };
}

void _requireCandidate(
  Map<String, Object?> value,
  String head,
  String tree, {
  bool nativeNames = false,
}) {
  final commit = nativeNames
      ? value['candidateCommit']
      : _object(value['candidate'], 'candidate')['commit'];
  final candidateTree = nativeNames
      ? value['candidateTree']
      : _object(value['candidate'], 'candidate')['tree'];
  if (commit != head || candidateTree != tree) {
    throw StateError('Evidence candidate does not match $head/$tree.');
  }
}

Future<Directory> _requireSameApp(
  Map<String, Object?> native,
  Map<String, Object?> performance,
) async {
  final nativeApp = _object(native['appBundle'], 'native.appBundle');
  final artifacts = _object(performance['artifacts'], 'performance.artifacts');
  final manifestIdentity = _object(
    artifacts['appBundleManifest'],
    'performance.artifacts.appBundleManifest',
  );
  await _verifyFileIdentity(manifestIdentity);
  final manifestFile = File(manifestIdentity['path']! as String);
  final manifest = await _readObject(manifestFile);
  final bundlePath = manifest['bundlePath'];
  if (bundlePath is! String || bundlePath.isEmpty) {
    throw StateError('Performance manifest has no app bundle path.');
  }
  final appBundle = Directory(bundlePath).absolute;
  final verifiedManifest = await verifyDogfoodBundleManifest(
    appBundle,
    manifestFile,
  );
  if (nativeApp['path'] != appBundle.path ||
      nativeApp['manifestPath'] != manifestFile.absolute.path ||
      nativeApp['manifestSha256'] != verifiedManifest.sha256) {
    throw StateError(
      'Native receipt is not bound to the performance app path.',
    );
  }
  if (nativeApp['manifestSha256'] != manifest['sha256']) {
    throw StateError(
      'Native and performance receipts use different app bundles.',
    );
  }
  for (final name in const ['mainExecutable', 'embeddedAbi']) {
    final left = _object(native[name], 'native.$name');
    final right = _object(artifacts[name], 'performance.$name');
    if (left['bytes'] != right['bytes'] || left['sha256'] != right['sha256']) {
      throw StateError('Native and performance receipts disagree on $name.');
    }
  }
  return appBundle;
}

Future<void> _verifyCi(
  Map<String, Object?> ci,
  String head,
  String expectedRepository,
  GitHubJobLookup lookup,
) async {
  if (ci['headSha'] != head || ci['repository'] != expectedRepository) {
    throw StateError('CI head_sha does not match the D0 candidate.');
  }
  final checks = (ci['checks'] as List<Object?>? ?? const [])
      .map((value) => _object(value, 'ci.checks[]'))
      .toList();
  final names = <String>{};
  for (final check in checks) {
    final name = check['name'];
    if (name is! String || !names.add(name)) {
      throw StateError('CI checks must have unique names.');
    }
    final url = check['url'];
    final jobId = check['jobId'];
    final match = url is String
        ? RegExp(
            '^https://github\\.com/${RegExp.escape(expectedRepository)}'
            r'/actions/runs/(\d+)/job/(\d+)$',
          ).firstMatch(url)
        : null;
    if (jobId is! int || match == null || match.group(2) != '$jobId') {
      throw StateError('CI check $name is not a canonical GitHub job URL.');
    }
    final job = await lookup(expectedRepository, jobId);
    if (job['id'] != jobId ||
        '${job['run_id']}' != match.group(1) ||
        job['name'] != name ||
        job['conclusion'] != 'success' ||
        job['head_sha'] != head ||
        job['html_url'] != url) {
      throw StateError('CI check $name did not replay as exact-head success.');
    }
  }
  if (!names.containsAll(_requiredCiChecks)) {
    throw StateError('Required exact-commit CI checks are missing.');
  }
}

Future<Map<String, Object?>> _fetchGitHubJob(
  String repository,
  int jobId,
) async {
  final result = await Process.run('gh', [
    'api',
    'repos/$repository/actions/jobs/$jobId',
  ]);
  if (result.exitCode != 0) {
    throw StateError(
      'GitHub job lookup failed: ${(result.stderr as String).trim()}',
    );
  }
  final value = jsonDecode(result.stdout as String);
  if (value is! Map) throw StateError('GitHub job response is not an object.');
  return value.cast<String, Object?>();
}

Future<String> _githubRepository(Directory repository) async {
  final remote = await _git(repository, const ['remote', 'get-url', 'origin']);
  final match = RegExp(
    r'^(?:git@github\.com:|https://github\.com/)([^/]+/[^/]+?)(?:\.git)?$',
  ).firstMatch(remote);
  if (match == null) {
    throw StateError('D0 completion requires a canonical GitHub origin.');
  }
  return match.group(1)!;
}

Future<String> _verifyReview(
  Map<String, Object?> review,
  String head,
  String tree,
) async {
  if (review['candidateCommit'] != head ||
      review['candidateTree'] != tree ||
      review['result'] != 'PASS' ||
      review['openB0'] != 0 ||
      review['openB1'] != 0) {
    throw StateError('Independent review is not a PASS on the candidate.');
  }
  final reviewer = review['reviewer'];
  if (reviewer is! String || reviewer.isEmpty) {
    throw StateError('Independent review has no reviewer identity.');
  }
  await _verifyFileIdentity(_object(review['artifact'], 'review.artifact'));
  return reviewer;
}

Future<void> _verifyMovingSurface(
  Map<String, Object?> moving,
  Map<String, Object?> native,
  String head,
  String tree,
) async {
  final nativeApp = _object(native['appBundle'], 'native.appBundle');
  if (moving['candidateCommit'] != head ||
      moving['candidateTree'] != tree ||
      moving['result'] != 'PASS' ||
      moving['appManifestSha256'] != nativeApp['manifestSha256']) {
    throw StateError(
      'Moving-surface review is not bound to the candidate app.',
    );
  }
  final reviewer = moving['reviewer'];
  if (reviewer is! String || reviewer.isEmpty) {
    throw StateError('Moving-surface review has no reviewer identity.');
  }
  final completed = (moving['completedSteps'] as List<Object?>? ?? const [])
      .whereType<String>()
      .toSet();
  if (!completed.containsAll(_movingSurfaceSteps) ||
      !_movingSurfaceSteps.containsAll(completed)) {
    throw StateError('Moving-surface checklist is incomplete.');
  }
  await _verifyFileIdentity(_object(moving['capture'], 'moving.capture'));
  await _verifyFileIdentity(_object(moving['commandLog'], 'moving.commandLog'));
}

Future<void> _verifyFileIdentity(Map<String, Object?> identity) async {
  final path = identity['path'];
  if (path is! String || path.isEmpty) {
    throw StateError('Evidence artifact has no path.');
  }
  final actual = await _fileIdentity(File(path));
  if (actual['bytes'] != identity['bytes'] ||
      actual['sha256'] != identity['sha256']) {
    throw StateError('Evidence artifact identity changed: $path');
  }
}

Future<Map<String, Object>> _fileIdentity(File file) async {
  if (!await file.exists()) {
    throw StateError('Evidence artifact does not exist: ${file.absolute.path}');
  }
  return {
    'path': file.absolute.path,
    'bytes': await file.length(),
    'sha256': (await sha256.bind(file.openRead()).first).toString(),
  };
}

Future<Map<String, Object?>> _readObject(File file) async {
  if (!await file.exists()) {
    throw StateError('Required receipt does not exist: ${file.absolute.path}');
  }
  final value = jsonDecode(await file.readAsString());
  if (value is! Map) {
    throw StateError('Receipt is not a JSON object: ${file.absolute.path}');
  }
  return value.cast<String, Object?>();
}

Map<String, Object?> _object(Object? value, String path) {
  if (value is! Map) throw StateError('$path is not an object.');
  return value.cast<String, Object?>();
}

Future<String> _git(Directory repository, List<String> arguments) async {
  final result = await Process.run(
    'git',
    arguments,
    workingDirectory: repository.path,
  );
  if (result.exitCode != 0) {
    throw StateError(
      'git ${arguments.join(' ')} failed: ${(result.stderr as String).trim()}',
    );
  }
  return (result.stdout as String).trim();
}

Future<void> main(List<String> arguments) async {
  if (arguments.length != 8) {
    stderr.writeln(
      'usage: dart run scripts/verify_v4_dogfood_completion.dart '
      '<repository> <default-gate.json> <stress-gate.json> <actual-paint.json> '
      '<native-receipt.json> <performance-receipt.json> '
      '<candidate-evidence.json> <output.json>',
    );
    exitCode = 64;
    return;
  }
  try {
    final receipt = await buildDogfoodCompletionReceipt(
      repository: Directory(arguments[0]).absolute,
      defaultGateReceiptFile: File(arguments[1]).absolute,
      stressGateReceiptFile: File(arguments[2]).absolute,
      actualPaintReceiptFile: File(arguments[3]).absolute,
      nativeReceiptFile: File(arguments[4]).absolute,
      performanceReceiptFile: File(arguments[5]).absolute,
      candidateEvidenceFile: File(arguments[6]).absolute,
    );
    final output = File(arguments[7]).absolute;
    await output.parent.create(recursive: true);
    await output.writeAsString('${jsonEncode(receipt)}\n', flush: true);
    stdout.writeln(
      'verify-v4-dogfood-completion: PASS '
      'commit=${_object(receipt['candidate'], 'candidate')['commit']} '
      'receipt=${output.path}',
    );
  } on Object catch (error, stackTrace) {
    stderr.writeln('verify-v4-dogfood-completion: FAIL $error');
    stderr.writeln(stackTrace);
    exitCode = 1;
  }
}
