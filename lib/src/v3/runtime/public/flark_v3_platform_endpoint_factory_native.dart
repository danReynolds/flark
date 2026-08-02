import 'dart:io';

import '../native/flark_v3_native_isolate_byte_endpoint.dart';
import 'flark_v3_platform_endpoint_handle.dart';
import 'flark_v3_runtime_assets.dart';

bool get defaultPlatformEndpointSupported =>
    Platform.isMacOS ||
    Platform.isLinux ||
    Platform.isAndroid ||
    Platform.isIOS;

String get defaultPlatformEndpointName => defaultPlatformEndpointSupported
    ? 'native isolate + FFI'
    : 'unavailable native endpoint';

String? get defaultPlatformEndpointUnavailableReason =>
    defaultPlatformEndpointSupported
    ? null
    : 'The Flark v3 native endpoint does not support '
          '${Platform.operatingSystem}.';

Future<FlarkV3PlatformEndpointHandle> startDefaultPlatformEndpoint({
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) async {
  final unavailable = defaultPlatformEndpointUnavailableReason;
  if (unavailable != null) {
    throw UnsupportedError(unavailable);
  }
  final endpoint = await FlarkV3NativeIsolateByteEndpoint.start(
    overrideLibraryPath: nativeLibraryPath,
  );
  return FlarkV3PlatformEndpointHandle(endpoint: endpoint, done: endpoint.done);
}
