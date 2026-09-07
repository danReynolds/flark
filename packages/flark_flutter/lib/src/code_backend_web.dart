import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/wasm.dart';
import 'package:flutter/services.dart';

Future<CodeAnalyzer> loadCodeAnalyzer() async {
  final bytes = await rootBundle.load(
    'packages/flark_tree_sitter/lib/assets/wasm/flark_tree_sitter.wasm',
  );
  return CodeAnalyzer(
    backend: await WasmCodeBackend.fromBytes(
      bytes.buffer.asUint8List(bytes.offsetInBytes, bytes.lengthInBytes),
    ),
  );
}
