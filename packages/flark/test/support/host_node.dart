import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:flark/flark.dart';
import 'package:flark/wasm.dart';

/// The compiler and runtime: `vm`, `dart2js` or `dart2wasm`.
const String hostPlatform = bool.fromEnvironment('dart.tool.dart2wasm')
    ? 'dart2wasm'
    : 'dart2js';

/// The platform's parser: the Wasm module the package bundles, as a web host
/// loads it.
Future<FlarkParseBackend> loadTestBackend() => WasmParseBackend.bundled();

/// Node's `process`; a browser has none, and so no environment or files.
final _Process? _process = globalContext.has('process')
    ? globalContext.getProperty<_Process>('process'.toJS)
    : null;

String? hostEnvironment(String name) =>
    _process?.env.getProperty<JSString?>(name.toJS)?.toDart;

/// The text of the file at [path], relative to node's working directory (the
/// package directory under `dart test`), or null when there is none.
String? readHostFile(String path) {
  final fs = _fs;
  return fs != null && fs.existsSync(path)
      ? fs.readFileSync(path, 'utf8')
      : null;
}

void writeHostFile(String path, String contents) =>
    _files.writeFileSync(path, contents);

void appendHostFile(String path, String contents) =>
    _files.appendFileSync(path, contents);

extension type _Process._(JSObject _) implements JSObject {
  external JSObject get env;
  // Node 20.16 and 22.3 or later; an ES module, as dart2wasm's test runner
  // loads, has no `require`.
  external JSObject getBuiltinModule(String name);
}

final _Fs? _fs = switch (_process) {
  final process? => _Fs._(process.getBuiltinModule('fs')),
  null => null,
};

_Fs get _files =>
    _fs ?? (throw UnsupportedError('writing files needs node, not a browser'));

extension type _Fs._(JSObject _) implements JSObject {
  external bool existsSync(String path);
  external String readFileSync(String path, String encoding);
  external void writeFileSync(String path, String contents);
  external void appendFileSync(String path, String contents);
}
