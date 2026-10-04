import 'dart:io';

import 'package:flark/flark.dart';

/// The compiler and runtime: `vm`, `dart2js` or `dart2wasm`.
const String hostPlatform = 'vm';

/// The platform's parser: the native library through FFI.
Future<FlarkParseBackend> loadTestBackend() async => createParseBackend();

String? hostEnvironment(String name) => Platform.environment[name];

/// The text of the file at [path], relative to the package directory, or
/// null when there is none.
String? readHostFile(String path) {
  final file = File(path);
  return file.existsSync() ? file.readAsStringSync() : null;
}

void writeHostFile(String path, String contents) =>
    File(path).writeAsStringSync(contents);

void appendHostFile(String path, String contents) =>
    File(path).writeAsStringSync(contents, mode: FileMode.append);
