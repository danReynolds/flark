import 'dart:io';

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter_test/flutter_test.dart';

Future<FlarkV3DocumentRuntime> openManagedRuntimeForTest(String markdown) {
  final relativeCandidates = switch (Platform.operatingSystem) {
    'macos' => const <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
      '../../native/comrak_bridge/target/release/libflark_comrak_bridge.dylib',
    ],
    'linux' => const <String>[
      'native/comrak_bridge/target/release/libflark_comrak_bridge.so',
      '../../native/comrak_bridge/target/release/libflark_comrak_bridge.so',
    ],
    'windows' => const <String>[
      'native/comrak_bridge/target/release/flark_comrak_bridge.dll',
      '../../native/comrak_bridge/target/release/flark_comrak_bridge.dll',
    ],
    _ => const <String>[],
  };
  for (final candidate in relativeCandidates) {
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
    nativeLibraryPath: relativeCandidates.firstOrNull,
  );
}

Future<T> runManagedRuntimeAsyncForTest<T>(
  WidgetTester tester,
  Future<T> Function() work,
) async {
  final result = await tester.runAsync(work);
  return result as T;
}
