import 'dart:ffi';
import 'dart:typed_data';

import 'backend.dart';
import 'native.dart' as native;

final class FfiCodeBackend implements CodeBackend {
  bool _disposed = false;

  @override
  int get version => native.codeVersion();

  @override
  Uint8List analyze(Uint8List source, int language) {
    return _exchange(source, language, 0);
  }

  @override
  Uint8List edit(Uint8List request, int language) =>
      _exchange(request, language, 1);

  @override
  Uint8List detect(Uint8List source) => _exchange(source, 0, 2);

  Uint8List _exchange(Uint8List source, int language, int operation) {
    if (_disposed) throw StateError('Code backend used after dispose.');
    final input = native.codeAlloc(source.length);
    final cell = native.codeAlloc(16);
    final output = cell.cast<Pointer<Uint8>>();
    final length = (cell + 8).cast<Uint32>();
    try {
      input.asTypedList(source.length).setAll(0, source);
      final status = ([
        native.codeAnalyze,
        native.codeEdit,
        native.codeDetect,
      ][operation])(input, source.length, language, output, length);
      if (status != 0) throw CodeException('Native analysis failed ($status).');
      try {
        return Uint8List.fromList(output.value.asTypedList(length.value));
      } finally {
        native.codeFree(output.value, length.value);
      }
    } finally {
      native.codeFree(input, source.length);
      native.codeFree(cell, 16);
    }
  }

  @override
  void dispose() => _disposed = true;
}

CodeBackend createCodeBackend() => FfiCodeBackend();
