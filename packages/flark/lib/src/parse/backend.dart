import 'render_model.dart';

/// Parses Markdown source into a [RenderModel]. Every implementation is
/// synchronous once created; creation may be asynchronous on the web.
abstract interface class FlarkParseBackend {
  /// The render-model schema version the backend writes.
  int get schemaVersion;

  /// Parse [source] and return its render model. Never throws for valid
  /// Dart strings; a contained native fault surfaces as [FlarkParseException].
  RenderModel parse(String source);
}

class FlarkParseException implements Exception {
  const FlarkParseException(this.code, this.message);
  final int code;
  final String message;
  @override
  String toString() => 'FlarkParseException($code): $message';
}
