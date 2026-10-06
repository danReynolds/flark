/// What a test needs from the platform it runs on: a parser, environment
/// variables, and files to read and append to. On the Dart VM that is the FFI
/// parser and `dart:io`; under node (`dart test -p node`, with dart2js or
/// `-c dart2wasm`) it is the bundled Wasm parser and node's own `process` and
/// `fs`. Tests that import this instead of `dart:io` run on every platform.
library;

export 'host_node.dart' if (dart.library.io) 'host_vm.dart';
