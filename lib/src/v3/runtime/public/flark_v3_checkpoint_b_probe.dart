import 'flark_v3_checkpoint_b_probe_stub.dart'
    if (dart.library.io) 'flark_v3_checkpoint_b_probe_native.dart'
    if (dart.library.js_interop) 'flark_v3_checkpoint_b_probe_web.dart'
    as platform;
import 'flark_v3_runtime_assets.dart';

/// Runs the fixed Feedback Checkpoint B evidence battery off the caller
/// context and returns its bounded JSON receipt.
///
/// This unstable adapter diagnostic validates incremental SourceFacts
/// identity reuse and lifecycle behavior. It is not part of Flark's document
/// protocol or stable v3 runtime API.
Future<String> runFlarkV3CheckpointBProbeJson({
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) => platform.runCheckpointBProbe(
  nativeLibraryPath: nativeLibraryPath,
  webAssets: webAssets,
);
