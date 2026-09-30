import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

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
/// built from the parser it releases with. After a release, reopening
/// "Unreleased" bumps the pubspec to the next version; the pin stays until
/// the next release needs a new one.
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
  final changelog = File('CHANGELOG.md').readAsStringSync();
  final named = _mentions(changelog, version);

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

  test('a release commit pins the parser it is built from', () {
    if (!named) return;
    final problem = _stalePin(manifest, _sourceDigest('../..'));
    expect(problem, isNull, reason: problem);
  });

  test('an entry rk would release marks a release commit', () {
    expect(_mentions('# Changelog\n\n## 0.5.0\n\n- A.\n', '0.5.0'), isTrue);
    expect(
      _mentions('# Changelog\n\n## [0.5.0] - 2026-10-01\n', '0.5.0'),
      isTrue,
    );
    expect(
      _mentions('# Changelog\n\n## Unreleased\n\n## 0.5.0\n', '0.5.0'),
      isTrue,
      reason: 'rk releases a version whose entry is anywhere in the file',
    );
    expect(_mentions('# Changelog\n\n  ## (0.5.0)\n', '0.5.0'), isTrue);
    expect(_mentions('# Changelog\n\n## 0.5.0-dev.1\n', '0.5.0'), isFalse);
    expect(_mentions('# Changelog\n\n## v0.5.0\n', '0.5.0'), isFalse);
  });

  test('a pin is stale when the parser has changed since it', () {
    final pin = {'release': 'flark_parse-v0.1.0', 'source': 'a' * 64};
    expect(_stalePin(pin, 'a' * 64), isNull);
    expect(
      _stalePin(pin, 'b' * 64),
      startsWith(
        'the parse crate or rust-toolchain.toml changed since '
        'flark_parse-v0.1.0',
      ),
    );
    expect(
      _stalePin({'release': null}, 'b' * 64),
      isNull,
      reason: 'an unpinned release is the changelog test\'s refusal',
    );
  });

  group('the source digest', () {
    late Directory repository;
    void write(String path, String contents) => File('${repository.path}/$path')
      ..createSync(recursive: true)
      ..writeAsStringSync(contents);
    void commit() {
      for (final arguments in [
        ['add', '-A'],
        [
          '-c',
          'user.name=flark',
          '-c',
          'user.email=flark@example.test',
          '-c',
          'commit.gpgSign=false',
          'commit',
          '-qm',
          'change',
        ],
      ]) {
        final result = Process.runSync(
          'git',
          arguments,
          workingDirectory: repository.path,
        );
        expect(result.exitCode, 0, reason: '${result.stderr}');
      }
    }

    setUp(() {
      repository = Directory.systemTemp.createTempSync('flark-digest-');
      Process.runSync('git', ['init', '-q'], workingDirectory: repository.path);
      write('native/flark_parse/src/lib.rs', 'pub fn parse() {}\n');
      write('native/flark_parse/tool/build.sh', 'echo build\n');
      write('native/flark_parse_extra/src/lib.rs', 'pub fn other() {}\n');
      write('rust-toolchain.toml', '[toolchain]\nchannel = "1.98.0"\n');
      commit();
    });
    tearDown(() => repository.deleteSync(recursive: true));

    test('follows the crate and toolchain, and not the release tooling', () {
      final first = _sourceDigest(repository.path);

      write('native/flark_parse/tool/build.sh', 'echo build faster\n');
      write('native/flark_parse_extra/src/lib.rs', 'pub fn changed() {}\n');
      commit();
      expect(_sourceDigest(repository.path), first);

      write('native/flark_parse/src/lib.rs', 'pub fn parse() -> u8 { 0 }\n');
      commit();
      final second = _sourceDigest(repository.path);
      expect(second, isNot(first));

      write('rust-toolchain.toml', '[toolchain]\nchannel = "1.99.0"\n');
      commit();
      expect(_sourceDigest(repository.path), isNot(second));
    });

    test(
      'is the one the manifest writer records',
      () {
        final written = Process.runSync('python3', [
          'native/flark_parse/tool/write_prebuilt_manifest.py',
          '--source-digest',
          'HEAD',
        ], workingDirectory: '../..');
        expect(written.exitCode, 0, reason: '${written.stderr}');
        expect((written.stdout as String).trim(), _sourceDigest('../..'));
      },
      skip: _has('python3') ? false : 'needs python3',
    );
  });
}

/// Whether [changelog] has an entry for [version], which rk requires before
/// it releases flark: a heading that, stripped of its `#`s and any leading
/// brackets or quotes, begins with the version. The same reading as rk's own
/// `Changelog.mentions`.
bool _mentions(String changelog, String version) {
  for (final line in changelog.split('\n')) {
    final trimmed = line.trimLeft();
    if (!trimmed.startsWith('#')) continue;
    final heading = trimmed
        .replaceFirst(RegExp(r'^#+'), '')
        .trim()
        .replaceFirst(
          RegExp(
            r'^[\[\("'
            "'"
            r']+',
          ),
          '',
        );
    if (!heading.startsWith(version)) continue;
    if (heading.length == version.length ||
        !RegExp(r'[0-9A-Za-z.\-+]').hasMatch(heading[version.length])) {
      return true;
    }
  }
  return false;
}

/// Why [manifest]'s pin cannot ship with source whose digest is [source], or
/// null when it can.
String? _stalePin(Map<String, Object?> manifest, String source) {
  final release = manifest['release'];
  if (release == null || manifest['source'] == source) return null;
  return 'the parse crate or rust-toolchain.toml changed since $release was '
      'pinned, and CHANGELOG.md has an entry for this version, which rk would '
      'release with that pin. Release the parser again and pin that release '
      '(RELEASING.md). If this version is already out, bump flark\'s version '
      'for the next one.';
}

/// What write_prebuilt_manifest.py records as `source`: the SHA-256 of Git's
/// own record, mode, object and path, of every file of the parse crate but
/// its release tooling, and of rust-toolchain.toml, as the bytes
/// `git ls-tree -z` prints them for [commit] in [repository]. A shallow
/// checkout has HEAD's tree but not the pinned commit, which is why the
/// manifest carries the digest.
String _sourceDigest(String repository, [String commit = 'HEAD']) {
  final listing = Process.runSync(
    'git',
    [
      'ls-tree',
      '-r',
      '-z',
      commit,
      '--',
      'native/flark_parse',
      'rust-toolchain.toml',
    ],
    workingDirectory: repository,
    stdoutEncoding: null,
  );
  if (listing.exitCode != 0) fail('git ls-tree failed: ${listing.stderr}');
  final bytes = listing.stdout as List<int>;
  final kept = BytesBuilder(copy: false);
  var start = 0;
  for (var end = 0; end < bytes.length; end++) {
    if (bytes[end] != 0) continue;
    final record = bytes.sublist(start, end + 1);
    start = end + 1;
    final path = String.fromCharCodes(
      record.sublist(record.indexOf(9) + 1, record.length - 1),
    );
    if (!path.startsWith('native/flark_parse/tool/')) kept.add(record);
  }
  return sha256.convert(kept.takeBytes()).toString();
}

bool _has(String executable) {
  try {
    return Process.runSync(executable, ['--version']).exitCode == 0;
  } on ProcessException {
    return false;
  }
}
