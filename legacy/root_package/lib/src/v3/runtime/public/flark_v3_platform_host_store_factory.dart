import '../../host/host.dart';
import 'flark_v3_platform_host_store_factory_stub.dart'
    if (dart.library.io) 'flark_v3_platform_host_store_factory_native.dart'
    if (dart.library.js_interop) 'flark_v3_platform_host_store_factory_web.dart'
    as platform;
import 'flark_v3_runtime_assets.dart';

Future<FlarkV3HostStore> createFlarkV3DefaultPlatformHostStore({
  required FlarkV3DocumentSessionId documentSession,
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) => platform.createDefaultPlatformHostStore(
  documentSession: documentSession,
  nativeLibraryPath: nativeLibraryPath,
  webAssets: webAssets,
);
