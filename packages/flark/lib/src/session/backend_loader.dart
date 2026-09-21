import '../parse/backend.dart';
import 'backend_loader_stub.dart'
    if (dart.library.ffi) 'backend_loader_native.dart'
    if (dart.library.js_interop) 'backend_loader_web.dart'
    as platform;

/// An owned parser instance; compilation may be shared, mutable buffers are not.
final class FlarkBackendLease {
  FlarkBackendLease(this.backend, this.dispose);
  final FlarkParseBackend backend;
  final void Function() dispose;
}

Future<FlarkBackendLease> loadFlarkBackend() => platform.load();
