import '../../host/host.dart';
import '../native/flark_v3_native_host_store.dart';
import '../native/flark_v3_native_library_locator.dart';
import 'flark_v3_runtime_assets.dart';

Future<FlarkV3HostStore> createDefaultPlatformHostStore({
  required FlarkV3DocumentSessionId documentSession,
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) async => FlarkV3NativeHostStore.create(
  library: openFlarkV3NativeLibrary(overrideLibraryPath: nativeLibraryPath),
  documentSession: documentSession,
);
