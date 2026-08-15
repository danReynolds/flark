import '../web/flark_v3_web_worker_byte_endpoint.dart';
import 'flark_v3_platform_endpoint_handle.dart';
import 'flark_v3_runtime_assets.dart';

const bool defaultPlatformEndpointSupported = true;
const String defaultPlatformEndpointName = 'Web Worker + Wasm';
const String? defaultPlatformEndpointUnavailableReason = null;

Future<FlarkV3PlatformEndpointHandle> startDefaultPlatformEndpoint({
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) async {
  if (nativeLibraryPath != null && nativeLibraryPath.isNotEmpty) {
    throw ArgumentError.value(
      nativeLibraryPath,
      'nativeLibraryPath',
      'is native-only; configure Web workerUri and wasmUri with webAssets',
    );
  }
  final assets = webAssets ?? FlarkV3WebRuntimeAssets.packageDefaults();
  final endpoint = await FlarkV3WebWorkerByteEndpoint.start(
    workerUri: assets.workerUri,
    wasmUri: assets.wasmUri,
  );
  return FlarkV3PlatformEndpointHandle(endpoint: endpoint, done: endpoint.done);
}
