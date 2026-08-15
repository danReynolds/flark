import '../../host/host.dart';
import '../web/flark_v3_web_host_store.dart';
import 'flark_v3_runtime_assets.dart';

Future<FlarkV3HostStore> createDefaultPlatformHostStore({
  required FlarkV3DocumentSessionId documentSession,
  String? nativeLibraryPath,
  FlarkV3WebRuntimeAssets? webAssets,
}) {
  final assets = webAssets ?? FlarkV3WebRuntimeAssets.packageDefaults();
  return FlarkV3WebHostStore.create(
    wasmUri: assets.wasmUri,
    documentSession: documentSession,
  );
}
