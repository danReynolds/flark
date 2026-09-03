/// Selects the parse transport for the platform: FFI on the Dart VM, Wasm
/// through js_interop on the web.
library;

export 'backend_stub.dart'
    if (dart.library.ffi) 'backend_ffi.dart'
    if (dart.library.js_interop) 'backend_web.dart';
