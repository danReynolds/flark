import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:test/test.dart';

/// `hook/prebuilt.json` is what the published package downloads its parser
/// from, and pub.dev keeps every version, so an incomplete pin must never
/// ship.
///
/// During development the manifest is the empty placeholder and the newest
/// changelog entry is "Unreleased". A changelog entry naming the version marks
/// the release commit (rk refuses to release flark without one), and that
/// commit must pin a published library for every target the hook downloads,
/// built from the parser it releases with.
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
  final named = RegExp(
    '^#+\\s*\\[?v?${RegExp.escape(version)}(?![0-9A-Za-z.+-])',
    multiLine: true,
  ).hasMatch(File('CHANGELOG.md').readAsStringSync());

  String library(String triple) => triple.contains('apple')
      ? 'libflark_parse.dylib'
      : triple.contains('windows')
      ? 'flark_parse.dll'
      : 'libflark_parse.so';

  test('is the placeholder or a complete pin', () {
    expect(
      manifest.keys.toSet().difference({
        'release',
        'commit',
        'source',
        'files',
      }),
      isEmpty,
    );
    if (release == null) {
      expect(files, isEmpty);
      expect(manifest['commit'], isNull);
      expect(manifest['source'], isNull);
      return;
    }
    expect(release, matches(RegExp(r'^flark_parse-v\d+\.\d+\.\d+')));
    expect(manifest['commit'], matches(RegExp(r'^[0-9a-f]{40}$')));
    expect(manifest['source'], matches(RegExp(r'^[0-9a-f]{64}$')));
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

  test(
    'the source digest is the one the manifest writer records',
    () {
      final written = Process.runSync('python3', [
        'native/flark_parse/tool/write_prebuilt_manifest.py',
        '--source-digest',
        'HEAD',
      ], workingDirectory: '../..');
      expect(written.exitCode, 0, reason: '${written.stderr}');
      expect((written.stdout as String).trim(), _sourceAtHead());
    },
    skip: _has('python3') ? false : 'needs python3',
  );

  test('a release commit pins the parser it is built from', () {
    if (!named || release == null) return;
    expect(
      manifest['source'],
      _sourceAtHead(),
      reason:
          'the parse crate or rust-toolchain.toml changed since $release was '
          'pinned, so this release would download libraries built from other '
          'source. Release the parser again and pin that release '
          '(RELEASING.md), or keep the entry as "Unreleased".',
    );
  });
}

/// What write_prebuilt_manifest.py records as `source`: the SHA-256 of every
/// file of the parse crate but its release tooling, and rust-toolchain.toml,
/// as Git records them, here at HEAD. A shallow checkout has HEAD's tree but
/// not the pinned commit, which is why the manifest carries the digest.
String _sourceAtHead() {
  final listing = Process.runSync('git', [
    'ls-tree',
    '-r',
    '-z',
    'HEAD',
    '--',
    'native/flark_parse',
    'rust-toolchain.toml',
  ], workingDirectory: '../..');
  if (listing.exitCode != 0) fail('git ls-tree failed: ${listing.stderr}');
  final entries = <(String, String)>[];
  for (final record in (listing.stdout as String).split(
    String.fromCharCode(0),
  )) {
    if (record.isEmpty) continue;
    final tab = record.indexOf('\t');
    final path = record.substring(tab + 1);
    if (path.startsWith('native/flark_parse/tool/')) continue;
    entries.add((path, record.substring(0, tab).split(' ')[2]));
  }
  entries.sort((a, b) => a.$1.compareTo(b.$1));
  return sha256
      .convert(
        utf8.encode(
          [for (final (path, object) in entries) '$object $path\n'].join(),
        ),
      )
      .toString();
}

bool _has(String executable) {
  try {
    return Process.runSync(executable, ['--version']).exitCode == 0;
  } on ProcessException {
    return false;
  }
}
