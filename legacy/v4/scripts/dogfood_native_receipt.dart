import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

import 'dogfood_bundle_manifest.dart';
import 'verify_flutter_machine_test.dart';

Future<Map<String, Object?>> buildDogfoodNativeReceipt({
  required Directory repository,
  required Directory appBundle,
  required File bundleManifest,
  required File mainExecutable,
  required File embeddedAbi,
  required File machineLog,
  required String expectedTestName,
}) async {
  for (final file in [
    bundleManifest,
    mainExecutable,
    embeddedAbi,
    machineLog,
  ]) {
    if (!await file.exists()) {
      throw ArgumentError.value(file.path, 'file', 'does not exist');
    }
  }
  if (!await appBundle.exists()) {
    throw ArgumentError.value(appBundle.path, 'appBundle', 'does not exist');
  }

  final verifiedManifest = await verifyDogfoodBundleManifest(
    appBundle,
    bundleManifest,
  );
  final mainEntry = dogfoodBundleEntryForFile(
    verifiedManifest,
    appBundle,
    mainExecutable,
  );
  final abiEntry = dogfoodBundleEntryForFile(
    verifiedManifest,
    appBundle,
    embeddedAbi,
  );
  final testReceipt = verifyFlutterMachineTest(
    await machineLog.readAsLines(),
    expectedName: expectedTestName,
  );
  final head = await _git(repository, const ['rev-parse', 'HEAD']);
  final tree = await _git(repository, const ['rev-parse', 'HEAD^{tree}']);
  final status = await _git(repository, const ['status', '--porcelain']);
  if (status.isNotEmpty) {
    throw StateError('Dogfood native receipt requires a clean worktree.');
  }

  return {
    'schema': 'dogfood_native_receipt_v1',
    'candidateCommit': head,
    'candidateTree': tree,
    'worktreeClean': true,
    'appBundle': {
      'path': appBundle.absolute.path,
      'manifestPath': bundleManifest.absolute.path,
      'manifestSha256': verifiedManifest.sha256,
    },
    'mainExecutable': await _fileIdentity(
      mainExecutable,
      manifestEntry: mainEntry,
    ),
    'embeddedAbi': await _fileIdentity(embeddedAbi, manifestEntry: abiEntry),
    'nativeCanary': {
      'name': testReceipt.name,
      'result': testReceipt.result,
      'skipped': testReceipt.skipped,
      'runnerSucceeded': testReceipt.runnerSucceeded,
      'machineLog': await _fileIdentity(machineLog),
    },
  };
}

Future<Map<String, Object>> _fileIdentity(
  File file, {
  DogfoodBundleManifestEntry? manifestEntry,
}) async {
  final bytes = await file.length();
  final digest = (await sha256.bind(file.openRead()).first).toString();
  if (manifestEntry != null &&
      (manifestEntry.bytes != bytes || manifestEntry.sha256 != digest)) {
    throw StateError(
      'Artifact ${file.absolute.path} disagrees with the bundle manifest.',
    );
  }
  return {'path': file.absolute.path, 'bytes': bytes, 'sha256': digest};
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
      'usage: dart run scripts/dogfood_native_receipt.dart '
      '<repository> <app-bundle> <bundle-manifest.json> <main-executable> '
      '<embedded-abi> <machine-log.jsonl> <exact-test-name> <output.json>',
    );
    exitCode = 64;
    return;
  }
  try {
    final receipt = await buildDogfoodNativeReceipt(
      repository: Directory(arguments[0]),
      appBundle: Directory(arguments[1]),
      bundleManifest: File(arguments[2]),
      mainExecutable: File(arguments[3]),
      embeddedAbi: File(arguments[4]),
      machineLog: File(arguments[5]),
      expectedTestName: arguments[6],
    );
    final output = File(arguments[7]);
    await output.parent.create(recursive: true);
    await output.writeAsString('${jsonEncode(receipt)}\n');
    stdout.writeln(
      'dogfood-native-receipt: PASS '
      'commit=${receipt['candidateCommit']} '
      'bundle=${(receipt['appBundle']! as Map)['manifestSha256']}',
    );
  } on Object catch (error) {
    stderr.writeln('dogfood-native-receipt: FAIL $error');
    exitCode = 1;
  }
}
