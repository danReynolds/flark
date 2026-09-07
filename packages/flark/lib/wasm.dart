/// Browser-only parser loading. Import this from a host's conditional web
/// implementation so static analysis can resolve the Wasm transport on any OS.
library;

export 'src/parse/backend_web.dart' show WasmParseBackend;
