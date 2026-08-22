import 'dart:io';

import 'package:flark_flutter/flark_flutter_advanced.dart';

import 'flark_test_paths.dart';

void installNativeTestBackendOverride() {
  final libraryPath = flarkNativeBridgeLibraryPathForPlatform();
  if (libraryPath.isEmpty || !File(libraryPath).existsSync()) return;
  final backend = FlarkNativeComrakParseBackend.withNativeBridge(
    overrideLibraryPath: libraryPath,
  );
  debugRequiredDefaultBackendResolver = () => backend;
}

FlarkNativeComrakParseBackend flarkTestNativeBackend() {
  final libraryPath = flarkNativeBridgeLibraryPathForPlatform();
  return FlarkNativeComrakParseBackend.withNativeBridge(
    overrideLibraryPath: libraryPath,
  );
}
