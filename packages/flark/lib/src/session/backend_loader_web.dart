import '../parse/backend_web.dart';
import 'backend_loader.dart';

Future<FlarkBackendLease> load() async {
  final backend = await WasmParseBackend.bundled();
  return FlarkBackendLease(backend, backend.dispose);
}
