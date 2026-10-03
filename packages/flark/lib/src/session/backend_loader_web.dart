import '../parse/backend_web.dart';
import 'backend_loader.dart';
import 'shared_backend.dart';

/// Every reader and session on the page leases this one parser. Each Wasm
/// instance has its own linear memory, about 1.25 MiB before a document grows
/// it, so an instance per reader cost about 125 MiB for 100 mounted readers.
/// One serves them all: parsing is synchronous on the page's one thread, and
/// the parser rebuilds its instance after a trap.
final _parser = SharedBackend(WasmParseBackend.bundled);

Future<FlarkBackendLease> load() => _parser.lease();
