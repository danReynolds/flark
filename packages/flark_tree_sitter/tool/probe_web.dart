import 'dart:js_interop';

import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/wasm.dart';

@JS('flarkTreeSitterBytes')
external JSUint8Array get _bytes;

Future<CodeAnalyzer> createAnalyzer() async =>
    CodeAnalyzer(backend: await WasmCodeBackend.fromBytes(_bytes.toDart));

Future<CodeBackend> createBackend() => WasmCodeBackend.fromBytes(_bytes.toDart);
