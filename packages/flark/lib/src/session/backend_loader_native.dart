import '../parse/backend_ffi.dart';
import 'backend_loader.dart';

Future<FlarkBackendLease> load() async {
  final backend = FfiParseBackend();
  return FlarkBackendLease(backend, backend.dispose);
}
