import 'dart:convert';
import 'dart:isolate';

import '../native/flark_v3_native_endpoint_bindings.dart';
import '../native/flark_v3_native_library_locator.dart';
import 'flark_v3_runtime_assets.dart';

/// Runs the private Feedback Checkpoint B evidence probe off the caller isolate.
///
/// [webAssets] keeps this diagnostic entry point compatible with
/// platform-selecting call sites. Native execution intentionally ignores it.
Future<String> runCheckpointBProbe({
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) => Isolate.run(() {
  final library = openFlarkV3NativeLibrary(
    overrideLibraryPath: nativeLibraryPath,
  );
  final bytes = FlarkV3NativeCheckpointBProbeBindings.load(library).run();
  return utf8.decode(bytes);
});
