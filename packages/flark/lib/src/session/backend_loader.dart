import '../parse/backend.dart';
import 'backend_loader_stub.dart'
    if (dart.library.ffi) 'backend_loader_native.dart'
    if (dart.library.js_interop) 'backend_loader_web.dart'
    as platform;

/// A parser lent to one holder, which calls [dispose] once it stops parsing.
final class FlarkBackendLease {
  FlarkBackendLease(this.backend, this.dispose);
  final FlarkParseBackend backend;
  final void Function() dispose;
}

/// Lease the platform's parser. On the Dart VM each lease owns a native
/// parser of its own. On the web every lease shares one Wasm parser, disposed
/// with the last lease; a lease's backend stops parsing once its lease is
/// disposed, and disposing that backend releases only its lease.
Future<FlarkBackendLease> loadFlarkBackend() => platform.load();
