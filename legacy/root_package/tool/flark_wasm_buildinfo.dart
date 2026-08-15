/// Shared logic for the WASM freshness guard.
///
/// The prebundled `lib/assets/wasm/flark_comrak_bridge.wasm` is a compiled
/// artifact of the Rust workspace in `native/comrak_bridge`. Nothing at build
/// time forces the two to stay in sync, so a Rust change not followed by a
/// WASM rebuild can silently give web and native consumers different parser
/// behavior.
///
/// To catch that deterministically, the WASM build records a manifest of the
/// source inputs that produced it (`*.wasm.buildinfo`). Both the generator and
/// packaging test call the functions here, so they agree on the input closure.
library;

import 'dart:io';

import 'package:crypto/crypto.dart';

const _excludedDirectoryNames = <String>{'.git', 'dist', 'target'};

/// Every production Cargo input whose contents determine the compiled bridge.
///
/// `native/comrak_bridge` is the closed production Cargo workspace boundary.
/// Every `Cargo.toml` and Rust source below it is included, so adding a
/// workspace member cannot silently escape the freshness receipt. The root
/// lockfile is included as the resolved dependency authority. Build output and
/// parser-research directories are pruned before traversal.
List<File> flarkWasmSourceInputs(Directory cargoWorkspaceDir) {
  final workspace = cargoWorkspaceDir.absolute;
  final workspaceManifest = File('${workspace.path}/Cargo.toml');
  final workspaceLockfile = File('${workspace.path}/Cargo.lock');
  if (!workspaceManifest.existsSync()) {
    throw StateError(
      'Cargo workspace manifest is missing: ${workspaceManifest.path}',
    );
  }
  if (!workspaceLockfile.existsSync()) {
    throw StateError(
      'Cargo workspace lockfile is missing: ${workspaceLockfile.path}',
    );
  }

  final files = <File>[];
  for (final file in _walkProductionFiles(workspace, workspace)) {
    final relative = _relativePath(workspace.path, file.absolute.path);
    if (relative == 'Cargo.lock' ||
        _fileName(relative) == 'Cargo.toml' ||
        relative.endsWith('.rs')) {
      files.add(file);
    }
  }
  files.sort(
    (left, right) => _relativePath(
      workspace.path,
      left.absolute.path,
    ).compareTo(_relativePath(workspace.path, right.absolute.path)),
  );
  return files;
}

/// A deterministic manifest of `<sha256>  <repository-relative-path>` lines.
///
/// Entries are path-sorted and use `/` separators, so the manifest is
/// independent of checkout location, host OS, and filesystem enumeration
/// order.
String flarkWasmBuildInfo(Directory cargoWorkspaceDir) {
  final workspace = cargoWorkspaceDir.absolute;
  final repositoryRoot = _repositoryRootFor(workspace);
  final entries = <({String path, String line})>[];
  for (final file in flarkWasmSourceInputs(workspace)) {
    final relative = _relativePath(repositoryRoot.path, file.absolute.path);
    final digest = sha256.convert(file.readAsBytesSync());
    entries.add((path: relative, line: '$digest  $relative'));
  }
  entries.sort((left, right) => left.path.compareTo(right.path));
  return '${entries.map((entry) => entry.line).join('\n')}\n';
}

/// Stable browser-cache key for one staged v3 Worker + Wasm asset set.
///
/// The Wasm bytes cover the actual executable, the buildinfo bytes bind that
/// executable to its freshness receipt, and the Worker bytes independently
/// invalidate changes to the JavaScript transport wrapper. Short digest
/// prefixes keep the URL compact; this is cache identity, not a trust proof.
String flarkV3WebAssetVersion({
  required File wasm,
  required File wasmBuildinfo,
  required File worker,
}) {
  String digestPrefix(File file) =>
      sha256.convert(file.readAsBytesSync()).toString().substring(0, 16);

  return [
    digestPrefix(wasm),
    digestPrefix(wasmBuildinfo),
    digestPrefix(worker),
  ].join('-');
}

Iterable<File> _walkProductionFiles(
  Directory directory,
  Directory cargoWorkspace,
) sync* {
  final entities = directory.listSync(followLinks: false)
    ..sort(
      (left, right) => _normalizedAbsolutePath(
        left.path,
      ).compareTo(_normalizedAbsolutePath(right.path)),
    );
  for (final entity in entities) {
    final relative = _relativePath(cargoWorkspace.path, entity.absolute.path);
    if (entity is Directory) {
      if (_excludedDirectory(relative)) continue;
      yield* _walkProductionFiles(entity, cargoWorkspace);
    } else if (entity is File && !_excludedDirectory(relative)) {
      yield entity;
    }
  }
}

bool _excludedDirectory(String relativePath) {
  final segments = relativePath.split('/');
  if (segments.any(_excludedDirectoryNames.contains)) return true;
  for (var index = 1; index < segments.length; index += 1) {
    if (segments[index - 1] == 'tool' && segments[index] == 'parser_research') {
      return true;
    }
  }
  return false;
}

Directory _repositoryRootFor(Directory cargoWorkspace) {
  var candidate = cargoWorkspace.absolute;
  while (true) {
    if (File('${candidate.path}/pubspec.yaml').existsSync()) return candidate;
    final parent = candidate.parent;
    if (_normalizedAbsolutePath(parent.path) ==
        _normalizedAbsolutePath(candidate.path)) {
      break;
    }
    candidate = parent;
  }
  throw StateError(
    'Could not find the package repository containing '
    '${cargoWorkspace.path}.',
  );
}

String _fileName(String path) => path.replaceAll('\\', '/').split('/').last;

String _normalizedAbsolutePath(String path) =>
    File(path).absolute.path.replaceAll('\\', '/');

String _relativePath(String base, String full) {
  final normalizedBase = _normalizedAbsolutePath(base);
  final normalizedFull = _normalizedAbsolutePath(full);
  if (normalizedBase == normalizedFull) return '';
  final prefix = normalizedBase.endsWith('/')
      ? normalizedBase
      : '$normalizedBase/';
  if (!normalizedFull.startsWith(prefix)) {
    throw StateError('$normalizedFull is outside $normalizedBase.');
  }
  return normalizedFull.substring(prefix.length);
}
