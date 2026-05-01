import 'native_comrak_ffi.dart';
import 'native_comrak_bridge_factory_stub.dart'
    if (dart.library.ffi) 'native_comrak_bridge_factory_ffi.dart'
    as bridge_factory;

NativeComrakBridge createNativeComrakBridge({String? overrideLibraryPath}) {
  return bridge_factory.createNativeComrakBridge(
    overrideLibraryPath: overrideLibraryPath,
  );
}

NativeComrakBridgePreflightResult preflightNativeComrakBridge({
  String? overrideLibraryPath,
}) {
  return bridge_factory.preflightNativeComrakBridge(
    overrideLibraryPath: overrideLibraryPath,
  );
}
