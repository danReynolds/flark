import 'package:flark_tree_sitter/highlight_worker.dart';

Future<CodeHighlightWorker> createWorker() => CodeHighlightWorker.start(
  workerUri: Uri.parse('../lib/assets/highlight_worker.mjs'),
  wasmUri: Uri.parse('../lib/assets/wasm/flark_tree_sitter.wasm'),
);
