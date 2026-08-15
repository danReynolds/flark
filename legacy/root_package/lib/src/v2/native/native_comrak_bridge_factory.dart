import 'native_comrak_ffi.dart';
import 'native_comrak_bridge_factory_stub.dart'
    if (dart.library.js_interop) 'native_comrak_bridge_factory_web.dart'
    if (dart.library.ffi) 'native_comrak_bridge_factory_ffi.dart'
    as bridge_factory;

/// Creates the platform native comrak bridge.
///
/// When [overrideLibraryPath] is provided, a native bridge attempts to load
/// that dynamic library instead of the platform defaults. On web,
/// [wasmSource] supplies an application-served URI, already-loaded bytes, or a
/// lazy platform byte loader. Each platform consumes its own option, so a
/// cross-platform caller may provide both [overrideLibraryPath] and
/// [wasmSource]. When no web source is supplied, Dart-package URI candidates
/// are tried.
NativeComrakBridge createNativeComrakBridge({
  String? overrideLibraryPath,
  NativeComrakWasmSource? wasmSource,
}) {
  return bridge_factory.createNativeComrakBridge(
    overrideLibraryPath: overrideLibraryPath,
    wasmSource: wasmSource,
  );
}

/// Checks whether the platform native comrak bridge can be loaded.
///
/// This returns a diagnostic result instead of throwing so apps can surface a
/// user- or developer-facing remediation path.
NativeComrakBridgePreflightResult preflightNativeComrakBridge({
  String? overrideLibraryPath,
  NativeComrakWasmSource? wasmSource,
}) {
  return bridge_factory.preflightNativeComrakBridge(
    overrideLibraryPath: overrideLibraryPath,
    wasmSource: wasmSource,
  );
}
