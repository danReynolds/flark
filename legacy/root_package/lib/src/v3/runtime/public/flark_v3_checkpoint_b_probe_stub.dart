import 'flark_v3_runtime_assets.dart';

Future<String> runCheckpointBProbe({
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) => Future<String>.error(
  UnsupportedError(
    'Feedback Checkpoint B requires either dart:io native FFI or '
    'dart:js_interop Web Worker support.',
  ),
);
