import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';

final class DogfoodBundleManifestEntry {
  const DogfoodBundleManifestEntry({
    required this.path,
    required this.type,
    required this.bytes,
    required this.sha256,
  });

  final String path;
  final String type;
  final int bytes;
  final String sha256;

  Map<String, Object> toJson() => {
    'path': path,
    'type': type,
    'bytes': bytes,
    'sha256': sha256,
  };
}

final class DogfoodBundleManifest {
  DogfoodBundleManifest({
    required this.bundlePath,
    required List<DogfoodBundleManifestEntry> entries,
  }) : entries = List.unmodifiable(entries) {
    final canonical = entries
        .map(
          (entry) =>
              '${entry.type}\t${entry.path}\t${entry.bytes}\t${entry.sha256}\n',
        )
        .join();
    sha256 = _sha256Bytes(utf8.encode(canonical));
  }

  final String bundlePath;
  final List<DogfoodBundleManifestEntry> entries;
  late final String sha256;

  Map<String, Object> toJson() => {
    'schema': 'dogfood_bundle_manifest_v1',
    'bundlePath': bundlePath,
    'sha256': sha256,
    'entries': entries.map((entry) => entry.toJson()).toList(growable: false),
  };
}

Future<DogfoodBundleManifest> verifyDogfoodBundleManifest(
  Directory bundle,
  File manifestFile,
) async {
  if (!await manifestFile.exists()) {
    throw ArgumentError.value(
      manifestFile.absolute.path,
      'manifestFile',
      'does not exist',
    );
  }
  final decoded = jsonDecode(await manifestFile.readAsString());
  if (decoded is! Map<String, Object?> ||
      decoded.keys.toSet().difference(const {
        'schema',
        'bundlePath',
        'sha256',
        'entries',
      }).isNotEmpty ||
      decoded['schema'] != 'dogfood_bundle_manifest_v1') {
    throw const FormatException('Invalid dogfood bundle manifest.');
  }
  final recomputed = await buildDogfoodBundleManifest(bundle);
  final expected = recomputed.toJson();
  if (jsonEncode(decoded) != jsonEncode(expected)) {
    throw StateError(
      'Dogfood bundle manifest does not match ${bundle.absolute.path}.',
    );
  }
  return recomputed;
}

DogfoodBundleManifestEntry dogfoodBundleEntryForFile(
  DogfoodBundleManifest manifest,
  Directory bundle,
  File file,
) {
  final root = bundle.absolute.resolveSymbolicLinksSync();
  final absolute = file.absolute.resolveSymbolicLinksSync();
  if (!absolute.startsWith('$root${Platform.pathSeparator}')) {
    throw StateError('Artifact is outside the dogfood app bundle: $absolute');
  }
  final relative = absolute
      .substring(root.length + 1)
      .split(Platform.pathSeparator)
      .join('/');
  return manifest.entries.singleWhere(
    (entry) => entry.path == relative && entry.type == 'file',
    orElse: () => throw StateError(
      'Dogfood bundle manifest does not contain file $relative.',
    ),
  );
}

Future<DogfoodBundleManifest> buildDogfoodBundleManifest(
  Directory bundle,
) async {
  final root = bundle.absolute.path;
  if (!await bundle.exists()) {
    throw ArgumentError.value(root, 'bundle', 'does not exist');
  }
  final entities = await bundle
      .list(recursive: true, followLinks: false)
      .where((entity) => entity is File || entity is Link)
      .toList();
  final entries = <DogfoodBundleManifestEntry>[];
  for (final entity in entities) {
    final absolute = entity.absolute.path;
    if (!absolute.startsWith('$root${Platform.pathSeparator}')) {
      throw StateError('Bundle entity escaped its root: $absolute');
    }
    final relative = absolute
        .substring(root.length + 1)
        .split(Platform.pathSeparator)
        .join('/');
    if (entity is File) {
      final bytes = await entity.length();
      final digest = await sha256.bind(entity.openRead()).first;
      entries.add(
        DogfoodBundleManifestEntry(
          path: relative,
          type: 'file',
          bytes: bytes,
          sha256: digest.toString(),
        ),
      );
    } else if (entity is Link) {
      final target = await entity.target();
      final targetBytes = utf8.encode(target);
      entries.add(
        DogfoodBundleManifestEntry(
          path: relative,
          type: 'link',
          bytes: targetBytes.length,
          sha256: _sha256Bytes(targetBytes),
        ),
      );
    }
  }
  entries.sort((left, right) => left.path.compareTo(right.path));
  if (entries.isEmpty) {
    throw StateError('Bundle contains no files or links: $root');
  }
  return DogfoodBundleManifest(bundlePath: root, entries: entries);
}

String _sha256Bytes(List<int> bytes) => sha256.convert(bytes).toString();

Future<void> main(List<String> arguments) async {
  if (arguments.length != 2) {
    stderr.writeln(
      'usage: dart run scripts/dogfood_bundle_manifest.dart '
      '<app-bundle> <output.json>',
    );
    exitCode = 64;
    return;
  }
  try {
    final manifest = await buildDogfoodBundleManifest(Directory(arguments[0]));
    final output = File(arguments[1]);
    await output.parent.create(recursive: true);
    await output.writeAsString('${jsonEncode(manifest.toJson())}\n');
    stdout.writeln(
      'dogfood-bundle-manifest: PASS files=${manifest.entries.length} '
      'sha256=${manifest.sha256}',
    );
  } on Object catch (error) {
    stderr.writeln('dogfood-bundle-manifest: FAIL $error');
    exitCode = 1;
  }
}
