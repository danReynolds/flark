import 'flark_v3_platform_endpoint_handle.dart';
import 'flark_v3_runtime_assets.dart';

const bool defaultPlatformEndpointSupported = false;
const String defaultPlatformEndpointName = 'Web Worker + Wasm';
const String defaultPlatformEndpointUnavailableReason =
    'The Flark v3 Web Worker endpoint is not implemented yet.';

Future<FlarkV3PlatformEndpointHandle> startDefaultPlatformEndpoint({
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) => Future<FlarkV3PlatformEndpointHandle>.error(
  UnsupportedError(defaultPlatformEndpointUnavailableReason),
);
