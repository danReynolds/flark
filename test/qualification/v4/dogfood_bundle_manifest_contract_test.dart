import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

import '../../../scripts/dogfood_bundle_manifest.dart';

void main() {
  test('manifest is sorted, content-bound, and symlink-aware', () async {
    final root = await Directory.systemTemp.createTemp(
      'flark-dogfood-manifest-',
    );
    addTearDown(() => root.delete(recursive: true));
    final nested = Directory('${root.path}/Contents/Frameworks');
    await nested.create(recursive: true);
    await File('${root.path}/Contents/main').writeAsString('main-v1');
    await File('${nested.path}/flark_abi').writeAsString('abi-v1');
    await Link(
      '${root.path}/flark_abi',
    ).create('Contents/Frameworks/flark_abi');

    final first = await buildDogfoodBundleManifest(root);
    expect(first.entries.map((entry) => entry.path), [
      'Contents/Frameworks/flark_abi',
      'Contents/main',
      'flark_abi',
    ]);
    expect(first.entries.last.type, 'link');
    expect(jsonEncode(first.toJson()), contains('dogfood_bundle_manifest_v1'));

    await File('${root.path}/Contents/main').writeAsString('main-v2');
    final second = await buildDogfoodBundleManifest(root);
    expect(second.sha256, isNot(first.sha256));
  });

  test('empty and missing bundles fail closed', () async {
    final empty = await Directory.systemTemp.createTemp('flark-dogfood-empty-');
    addTearDown(() => empty.delete(recursive: true));

    await expectLater(buildDogfoodBundleManifest(empty), throwsStateError);
    await expectLater(
      buildDogfoodBundleManifest(Directory('${empty.path}/missing')),
      throwsArgumentError,
    );
  });
}
