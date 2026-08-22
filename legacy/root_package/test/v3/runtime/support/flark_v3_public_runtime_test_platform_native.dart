import 'dart:io';

import 'package:flark/flark_v3.dart';

Future<FlarkV3DocumentRuntime> openFlarkV3PublicRuntimeForTest(
  String markdown,
) {
  final candidates = switch (Platform.operatingSystem) {
    'macos' => const <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
    ],
    'linux' => const <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.so',
    ],
    'windows' => const <String>[
      'native/comrak_bridge/target/release/flark_comrak_bridge.dll',
    ],
    _ => const <String>[],
  };
  for (final candidate in candidates) {
    final library = File(candidate);
    if (library.existsSync()) {
      return FlarkV3DocumentRuntime.open(
        markdown,
        nativeLibraryPath: library.absolute.path,
      );
    }
  }
  return FlarkV3DocumentRuntime.open(
    markdown,
    nativeLibraryPath: candidates.isEmpty ? null : candidates.first,
  );
}
