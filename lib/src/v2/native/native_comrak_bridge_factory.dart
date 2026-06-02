import 'native_comrak_ffi.dart';
import 'native_comrak_bridge_factory_stub.dart'
    if (dart.library.js_interop) 'native_comrak_bridge_factory_web.dart'
    if (dart.library.ffi) 'native_comrak_bridge_factory_ffi.dart'
    as bridge_factory;

/// Creates the platform native comrak bridge.
///
/// When [overrideLibraryPath] is provided, the bridge attempts to load that
/// dynamic library instead of the platform default candidate paths.
NativeComrakBridge createNativeComrakBridge({String? overrideLibraryPath}) {
  return bridge_factory.createNativeComrakBridge(
    overrideLibraryPath: overrideLibraryPath,
  );
}

/// Checks whether the platform native comrak bridge can be loaded.
///
/// This returns a diagnostic result instead of throwing so apps can surface a
/// user- or developer-facing remediation path.
NativeComrakBridgePreflightResult preflightNativeComrakBridge({
  String? overrideLibraryPath,
}) {
  return bridge_factory.preflightNativeComrakBridge(
    overrideLibraryPath: overrideLibraryPath,
  );
}
