import 'dart:io';

String sovereignFixturePath(String relativePath) {
  return _resolveExistingPath(<String>[
    'test/fixtures/$relativePath',
  ]);
}

String sovereignNativeBridgeLibraryPathForPlatform() {
  final candidates = switch (Platform.operatingSystem) {
    'macos' => <String>[
        'native/comrak_bridge/target/release/libsovereign_comrak_bridge.dylib',
      ],
    'linux' => <String>[
        'native/comrak_bridge/target/release/libsovereign_comrak_bridge.so',
      ],
    'windows' => <String>[
        'native/comrak_bridge/target/release/sovereign_comrak_bridge.dll',
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
