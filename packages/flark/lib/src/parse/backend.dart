import 'render_model.dart';

/// Parses Markdown source into a [RenderModel]. Every implementation is
/// synchronous once created; creation may be asynchronous on the web.
abstract interface class FlarkParseBackend {
  /// The render-model schema version the backend writes.
  int get schemaVersion;

  /// Parse [source] and return its render model. Invalid host text, a
  /// fail-closed extraction deviation, or a contained native fault surfaces as
  /// a [FlarkParseException].
  RenderModel parse(String source);
}

class FlarkParseException implements Exception {
  const FlarkParseException(this.code, this.message);

  /// Native return codes, shared by both transports.
  factory FlarkParseException.fromCode(int code) =>
      FlarkParseException(code, switch (code) {
        1 => 'null argument',
        2 => 'invalid UTF-8',
        faultCode => 'contained native fault',
        extractionDeviationCode => 'render-model extraction deviation',
        _ => 'unknown parse error $code',
      });

  static const int faultCode = 3;
  static const int extractionDeviationCode = 4;
  static const int loadFailedCode = -1;
  static const int schemaMismatchCode = -2;
  static const int invalidHostTextCode = -3;

  final int code;
  final String message;
  @override
  String toString() => 'FlarkParseException($code): $message';
}

/// Reject malformed UTF-16 before Dart's UTF-8 encoder can replace it with
/// U+FFFD and silently change canonical source.
void validateFlarkSourceText(String source) {
  for (var i = 0; i < source.length; i++) {
    final unit = source.codeUnitAt(i);
    if (unit >= 0xD800 && unit <= 0xDBFF) {
      if (i + 1 < source.length) {
        final next = source.codeUnitAt(i + 1);
        if (next >= 0xDC00 && next <= 0xDFFF) {
          i++;
          continue;
        }
      }
      throw FlarkParseException(
        FlarkParseException.invalidHostTextCode,
        'unpaired UTF-16 surrogate at offset $i',
      );
    }
    if (unit >= 0xDC00 && unit <= 0xDFFF) {
      throw FlarkParseException(
        FlarkParseException.invalidHostTextCode,
        'unpaired UTF-16 surrogate at offset $i',
      );
    }
  }
}
