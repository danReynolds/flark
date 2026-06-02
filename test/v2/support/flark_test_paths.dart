import 'dart:io';

String flarkFixturePath(String relativePath) {
  return _resolveExistingPath(<String>['test/fixtures/$relativePath']);
}

String flarkNativeBridgeLibraryPathForPlatform() {
  final candidates = switch (Platform.operatingSystem) {
    'macos' => <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
    ],
    'linux' => <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.so',
    ],
    'windows' => <String>[
      'native/comrak_bridge/target/release/flark_comrak_bridge.dll',
    ],
    _ => const <String>[],
  };
  return _resolveExistingPath(candidates);
}

String _resolveExistingPath(List<String> candidates) {
  for (final candidate in candidates) {
    if (File(candidate).existsSync()) return candidate;
  }
  return candidates.isEmpty ? '' : candidates.first;
}
