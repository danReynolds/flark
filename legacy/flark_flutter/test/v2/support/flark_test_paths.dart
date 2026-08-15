import 'dart:io';

String flarkFixturePath(String relativePath) {
  return _resolveExistingPath(<String>[
    'test/fixtures/$relativePath',
    '../../test/fixtures/$relativePath',
  ]);
}

String flarkNativeBridgeLibraryPathForPlatform() {
  final relativeCandidates = switch (Platform.operatingSystem) {
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
  return _resolveExistingPath(<String>[
    ...relativeCandidates,
    for (final candidate in relativeCandidates) '../../$candidate',
  ]);
}

String _resolveExistingPath(List<String> candidates) {
  for (final candidate in candidates) {
    final file = File(candidate);
    if (file.existsSync()) return file.absolute.path;
  }
  return candidates.isEmpty ? '' : candidates.first;
}
