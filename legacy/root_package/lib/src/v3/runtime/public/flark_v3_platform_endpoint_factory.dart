import 'flark_v3_platform_endpoint_factory_stub.dart'
    if (dart.library.io) 'flark_v3_platform_endpoint_factory_native.dart'
    if (dart.library.js_interop) 'flark_v3_platform_endpoint_factory_web.dart'
    as platform;
import 'flark_v3_platform_endpoint_handle.dart';
import 'flark_v3_runtime_assets.dart';

bool get flarkV3DefaultPlatformEndpointSupported =>
    platform.defaultPlatformEndpointSupported;

String get flarkV3DefaultPlatformEndpointName =>
    platform.defaultPlatformEndpointName;

String? get flarkV3DefaultPlatformEndpointUnavailableReason =>
    platform.defaultPlatformEndpointUnavailableReason;

Future<FlarkV3PlatformEndpointHandle> startFlarkV3DefaultPlatformEndpoint({
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) => platform.startDefaultPlatformEndpoint(
  nativeLibraryPath: nativeLibraryPath,
  webAssets: webAssets,
);
