import '../../host/host.dart';
import 'flark_v3_runtime_assets.dart';

Future<FlarkV3HostStore> createDefaultPlatformHostStore({
  required FlarkV3DocumentSessionId documentSession,
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) => Future<FlarkV3HostStore>.error(
  UnsupportedError(
    'The Flark v3 WebAssembly main-context host store is not implemented '
    'yet.',
  ),
);
