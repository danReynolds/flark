import 'dart:ffi';
import 'dart:io';

/// Opens the packaged Flark native engine from one deterministic layout.
///
/// Both the parser isolate and the caller-isolate host store use this exact
/// resolver. Keeping one loader is important for relocated `dart build cli`
/// bundles: the two halves of a document runtime must never accidentally bind
/// different native artifacts because their working-directory searches
/// diverged.
DynamicLibrary openFlarkV3NativeLibrary({String? overrideLibraryPath}) {
  if (overrideLibraryPath != null && overrideLibraryPath.isNotEmpty) {
    return DynamicLibrary.open(overrideLibraryPath);
  }
  if (Platform.isIOS) {
    const frameworkBinary = 'flark_comrak_bridge.framework/flark_comrak_bridge';
    final bundledPath = File(
      Platform.resolvedExecutable,
    ).parent.uri.resolve('Frameworks/$frameworkBinary').toFilePath();
    for (final candidate in <String>['@rpath/$frameworkBinary', bundledPath]) {
      try {
        return DynamicLibrary.open(candidate);
      } on Object {
        // Continue through the complete deterministic candidate list.
      }
    }
    throw StateError(
      'Unable to load the bundled Flark native framework at $bundledPath.',
    );
  }

  final libraryName = switch (Platform.operatingSystem) {
    'android' || 'linux' => 'libflark_comrak_bridge.so',
    'macos' => 'libflark_comrak_bridge.dylib',
    'windows' => 'flark_comrak_bridge.dll',
    final platform => throw UnsupportedError(
      'Flark native engine does not support $platform.',
    ),
  };
  final candidates = flarkV3NativeLibraryCandidates(
    libraryName: libraryName,
    executable: File(Platform.resolvedExecutable),
    currentDirectory: Directory.current,
    includeMacFrameworks: Platform.isMacOS,
  );
  Object? lastError;
  for (final candidate in candidates) {
    if (candidate != libraryName && !File(candidate).existsSync()) continue;
    try {
      return DynamicLibrary.open(candidate);
    } catch (error) {
      lastError = error;
    }
  }
  throw StateError(
    'Unable to load $libraryName from ${candidates.join(', ')}: $lastError',
  );
}

/// Deterministic native bridge candidates for one Dart executable layout.
///
/// Kept separate from `DynamicLibrary.open` so package and relocated-bundle
/// layouts remain executable contract tests rather than environment accidents.
List<String> flarkV3NativeLibraryCandidates({
  required String libraryName,
  required File executable,
  required Directory currentDirectory,
  required bool includeMacFrameworks,
}) {
  final executableDirectory = executable.parent;
  final contentsDirectory = executableDirectory.parent;
  return [
    libraryName,
    currentDirectory.uri.resolve('.dart_tool/lib/$libraryName').toFilePath(),
    currentDirectory.uri
        .resolve('native/comrak_bridge/target/release/$libraryName')
        .toFilePath(),
    currentDirectory.uri
        .resolve('../native/comrak_bridge/target/release/$libraryName')
        .toFilePath(),
    executableDirectory.uri.resolve(libraryName).toFilePath(),
    // `dart build cli` emits `bundle/bin/<executable>` plus
    // `bundle/lib/<code asset>`. Resolve from the executable rather than the
    // caller's working directory so the whole bundle is relocatable.
    executableDirectory.parent.uri.resolve('lib/$libraryName').toFilePath(),
    if (includeMacFrameworks)
      contentsDirectory.uri
          .resolve(
            'Frameworks/flark_comrak_bridge.framework/'
            'Versions/A/flark_comrak_bridge',
          )
          .toFilePath(),
    if (includeMacFrameworks)
      contentsDirectory.uri
          .resolve(
            'Frameworks/flark_comrak_bridge.framework/'
            'flark_comrak_bridge',
          )
          .toFilePath(),
  ];
}
