import 'dart:convert';
import 'dart:io';

import 'package:test/test.dart';

/// `hook/prebuilt.json` is what the published package downloads its parser
/// from, and pub.dev keeps every version, so an incomplete pin must never
/// ship.
///
/// During development the manifest is the empty placeholder and the newest
/// changelog entry is "Unreleased". A changelog entry naming the version marks
/// the release commit (rk refuses to release flark without one), and that
/// commit must pin a published library for every target the hook downloads.
void main() {
  final manifest =
      jsonDecode(File('hook/prebuilt.json').readAsStringSync())
          as Map<String, Object?>;
  final version = RegExp(
    r'^version: (\S+)$',
    multiLine: true,
  ).firstMatch(File('pubspec.yaml').readAsStringSync())!.group(1)!;
  // The release script's target list is the one list of downloadable targets.
  final targets = RegExp(r'^ALL="([^"]+)"$', multiLine: true)
      .firstMatch(
        File(
          '../../native/flark_parse/tool/build_release_libraries.sh',
        ).readAsStringSync(),
      )!
      .group(1)!
      .split(' ');
  final release = manifest['release'] as String?;
  final files = manifest['files'] as Map<String, Object?>;

  String library(String triple) => triple.contains('apple')
      ? 'libflark_parse.dylib'
      : triple.contains('windows')
      ? 'flark_parse.dll'
      : 'libflark_parse.so';

  test('is the placeholder or a complete pin', () {
    expect(
      manifest.keys.toSet().difference({'release', 'commit', 'files'}),
      isEmpty,
    );
    if (release == null) {
      expect(files, isEmpty);
      expect(manifest['commit'], isNull);
      return;
    }
    expect(release, matches(RegExp(r'^flark_parse-v\d+\.\d+\.\d+')));
    expect(manifest['commit'], matches(RegExp(r'^[0-9a-f]{40}$')));
    expect(files.keys.toSet(), targets.toSet());
    for (final MapEntry(key: triple, value: entry) in files.entries) {
      final pin = entry as Map<String, Object?>;
      expect(
        pin['url'],
        'https://github.com/danReynolds/flark/releases/download/'
        '$release/$triple-${library(triple)}',
      );
      expect(pin['sha256'], matches(RegExp(r'^[0-9a-f]{64}$')));
    }
  });

  test('a changelog entry for the version needs a pin', () {
    final named = RegExp(
      '^#+\\s*\\[?v?${RegExp.escape(version)}(?![0-9A-Za-z.+-])',
      multiLine: true,
    ).hasMatch(File('CHANGELOG.md').readAsStringSync());
    if (named) {
      expect(
        release,
        isNotNull,
        reason:
            'CHANGELOG.md names $version, which makes this a release commit, '
            'but hook/prebuilt.json pins no parser libraries. Publish them '
            'and pin them (RELEASING.md), or keep the entry as "Unreleased".',
      );
    }
  });
}
