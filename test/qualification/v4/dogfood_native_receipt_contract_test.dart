import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import '../../../scripts/dogfood_bundle_manifest.dart';
import '../../../scripts/dogfood_native_receipt.dart';

void main() {
  test(
    'native receipt binds clean commit, bundle, binaries, and canary',
    () async {
      final root = await Directory.systemTemp.createTemp(
        'flark-native-receipt-',
      );
      addTearDown(() => root.delete(recursive: true));
      await _git(root, const ['init']);
      await _git(root, const ['config', 'user.email', 'test@example.com']);
      await _git(root, const ['config', 'user.name', 'Receipt Test']);
      final tracked = File('${root.path}/tracked.txt');
      await tracked.writeAsString('tracked\n');
      await _git(root, const ['add', 'tracked.txt']);
      await _git(root, const ['commit', '-m', 'fixture']);

      final app = Directory('${root.path}/Flark Dogfood.app');
      final main = File('${app.path}/Contents/MacOS/Flark Dogfood');
      final abi = File(
        '${app.path}/Contents/Frameworks/flark_abi.framework/flark_abi',
      );
      await main.parent.create(recursive: true);
      await abi.parent.create(recursive: true);
      await main.writeAsString('main');
      await abi.writeAsString('abi');
      final manifest = File('${root.path}/manifest.json');
      final bundleManifest = await buildDogfoodBundleManifest(app);
      await manifest.writeAsString(jsonEncode(bundleManifest.toJson()));
      final machine = File('${root.path}/machine.jsonl');
      await machine.writeAsString(
        '${jsonEncode({
          'type': 'testStart',
          'test': {'id': 1, 'name': 'required canary'},
        })}\n'
        '${jsonEncode({'type': 'testDone', 'testID': 1, 'result': 'success', 'skipped': false})}\n'
        '${jsonEncode({'type': 'done', 'success': true})}\n',
      );
      await _git(root, const ['add', '.']);
      await _git(root, const ['commit', '-m', 'artifacts']);

      final receipt = await buildDogfoodNativeReceipt(
        repository: root,
        appBundle: app,
        bundleManifest: manifest,
        mainExecutable: main,
        embeddedAbi: abi,
        machineLog: machine,
        expectedTestName: 'required canary',
      );
      expect(receipt['schema'], 'dogfood_native_receipt_v1');
      expect(receipt['worktreeClean'], isTrue);
      expect(receipt['candidateCommit'], hasLength(40));
      expect(receipt['candidateTree'], hasLength(40));
      expect(
        (receipt['appBundle']! as Map)['manifestSha256'],
        bundleManifest.sha256,
      );
      expect((receipt['mainExecutable']! as Map)['bytes'], 4);
      expect((receipt['embeddedAbi']! as Map)['bytes'], 3);
      expect((receipt['nativeCanary']! as Map)['skipped'], isFalse);
    },
  );

  test('native receipt rejects dirty worktrees and skipped canaries', () async {
    final root = await Directory.systemTemp.createTemp('flark-native-reject-');
    addTearDown(() => root.delete(recursive: true));
    await _git(root, const ['init']);
    await _git(root, const ['config', 'user.email', 'test@example.com']);
    await _git(root, const ['config', 'user.name', 'Receipt Test']);
    final app = Directory('${root.path}/App.app');
    final main = File('${app.path}/Contents/MacOS/App');
    final abi = File('${app.path}/Contents/Frameworks/abi.framework/abi');
    main.parent.createSync(recursive: true);
    abi.parent.createSync(recursive: true);
    main.writeAsStringSync('main');
    abi.writeAsStringSync('abi');
    final manifest = File('${root.path}/manifest.json');
    final bundleManifest = await buildDogfoodBundleManifest(app);
    manifest.writeAsStringSync(jsonEncode(bundleManifest.toJson()));
    final machine = File('${root.path}/machine.jsonl')
      ..writeAsStringSync(
        '${jsonEncode({
          'type': 'testStart',
          'test': {'id': 1, 'name': 'required canary'},
        })}\n'
        '${jsonEncode({'type': 'testDone', 'testID': 1, 'result': 'success', 'skipped': true})}\n'
        '${jsonEncode({'type': 'done', 'success': true})}\n',
      );
    await _git(root, const ['add', '.']);
    await _git(root, const ['commit', '-m', 'fixture']);

    await expectLater(
      buildDogfoodNativeReceipt(
        repository: root,
        appBundle: app,
        bundleManifest: manifest,
        mainExecutable: main,
        embeddedAbi: abi,
        machineLog: machine,
        expectedTestName: 'required canary',
      ),
      throwsA(isA<StateError>()),
    );

    await machine.writeAsString(
      '${jsonEncode({
        'type': 'testStart',
        'test': {'id': 1, 'name': 'required canary'},
      })}\n'
      '${jsonEncode({'type': 'testDone', 'testID': 1, 'result': 'success', 'skipped': false})}\n'
      '${jsonEncode({'type': 'done', 'success': true})}\n',
    );
    await expectLater(
      buildDogfoodNativeReceipt(
        repository: root,
        appBundle: app,
        bundleManifest: manifest,
        mainExecutable: main,
        embeddedAbi: abi,
        machineLog: machine,
        expectedTestName: 'required canary',
      ),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          contains('clean worktree'),
        ),
      ),
    );
  });

  test('native receipt rejects a forged bundle manifest', () async {
    final root = await Directory.systemTemp.createTemp('flark-native-forged-');
    addTearDown(() => root.delete(recursive: true));
    await _git(root, const ['init']);
    await _git(root, const ['config', 'user.email', 'test@example.com']);
    await _git(root, const ['config', 'user.name', 'Receipt Test']);
    final app = Directory('${root.path}/App.app');
    final main = File('${app.path}/Contents/MacOS/App');
    final abi = File('${app.path}/Contents/Frameworks/abi.framework/abi');
    main.parent.createSync(recursive: true);
    abi.parent.createSync(recursive: true);
    main.writeAsStringSync('main');
    abi.writeAsStringSync('abi');
    final manifest = File('${root.path}/manifest.json');
    final bundleManifest = await buildDogfoodBundleManifest(app);
    final forged = bundleManifest.toJson()..['sha256'] = 'bundle-digest';
    manifest.writeAsStringSync(jsonEncode(forged));
    final machine = File('${root.path}/machine.jsonl')
      ..writeAsStringSync(
        '${jsonEncode({
          'type': 'testStart',
          'test': {'id': 1, 'name': 'required canary'},
        })}\n'
        '${jsonEncode({'type': 'testDone', 'testID': 1, 'result': 'success', 'skipped': false})}\n'
        '${jsonEncode({'type': 'done', 'success': true})}\n',
      );
    await _git(root, const ['add', '.']);
    await _git(root, const ['commit', '-m', 'fixture']);

    await expectLater(
      buildDogfoodNativeReceipt(
        repository: root,
        appBundle: app,
        bundleManifest: manifest,
        mainExecutable: main,
        embeddedAbi: abi,
        machineLog: machine,
        expectedTestName: 'required canary',
      ),
      throwsStateError,
    );
  });
}

Future<void> _git(Directory directory, List<String> arguments) async {
  final result = await Process.run(
    'git',
    arguments,
    workingDirectory: directory.path,
  );
  expect(result.exitCode, 0, reason: result.stderr as String);
}
