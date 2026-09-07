import 'package:flark/flark.dart';
import 'package:flark/wasm.dart';
import 'package:flutter/services.dart';

Future<FlarkParseBackend> loadBackend() async {
  final bytes = await rootBundle.load(
    'packages/flark/lib/assets/wasm/flark_parse.wasm',
  );
  return WasmParseBackend.fromBytes(
    bytes.buffer.asUint8List(bytes.offsetInBytes, bytes.lengthInBytes),
  );
}
