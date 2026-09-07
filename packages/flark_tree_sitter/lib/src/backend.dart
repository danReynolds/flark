import 'dart:typed_data';

/// Transport contract. The analyzer takes ownership and disposes this backend.
abstract interface class CodeBackend {
  int get version;
  Uint8List analyze(Uint8List source, int language);
  Uint8List detect(Uint8List source);
  Uint8List edit(Uint8List request, int language);
  void dispose();
}

final class CodeException implements Exception {
  const CodeException(this.message);
  final String message;
  @override
  String toString() => 'CodeException: $message';
}
