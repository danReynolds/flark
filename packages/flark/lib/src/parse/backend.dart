import 'render_model.dart';

/// Parses Markdown source into a [RenderModel]. Every implementation is
/// synchronous once created; creation may be asynchronous on the web.
abstract interface class FlarkParseBackend {
  /// The render-model schema version the backend writes.
  int get schemaVersion;

  /// Parse [source] and return its render model. Never throws for a Dart
  /// string; a contained native fault surfaces as [FlarkParseException]
  /// with [FlarkParseException.faultCode].
  RenderModel parse(String source);
}

class FlarkParseException implements Exception {
  const FlarkParseException(this.code, this.message);

  /// Native return codes, shared by both transports.
  factory FlarkParseException.fromCode(int code) => FlarkParseException(code, switch (code) {
        1 => 'null argument',
        2 => 'invalid UTF-8',
        faultCode => 'contained native fault',
        _ => 'unknown parse error $code',
      });

  static const int faultCode = 3;
  static const int loadFailedCode = -1;
  static const int schemaMismatchCode = -2;

  final int code;
  final String message;
  @override
  String toString() => 'FlarkParseException($code): $message';
}
