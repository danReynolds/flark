import 'flark_text_buffer.dart';

final class FlarkDocument {
  FlarkDocument._({required this.buffer, required this.revision});

  factory FlarkDocument.fromMarkdown(String markdown) {
    return FlarkDocument._(
      buffer: FlarkTextBuffer(normalizeLineEndings(markdown)),
      revision: 0,
    );
  }

  /// CRLF/CR sources normalize to LF.
  ///
  /// The buffer's line math, the markdown commands, and the fence scanner
  /// all treat `\n` as the line boundary; letting `\r` through gives every
  /// line-based edit a trailing carriage return to mis-handle. Applied at
  /// document ingest, and by any caller that replaces whole-document text
  /// from external input (e.g. form resets). Offsets in a normalized
  /// document differ from the caller's original string, which is the
  /// documented contract: the document's markdown is the truth.
  static String normalizeLineEndings(String markdown) {
    if (!markdown.contains('\r')) return markdown;
    return markdown.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
  }

  final FlarkTextBuffer buffer;
  final int revision;

  String get markdown => buffer.text;

  int get length => buffer.length;

  FlarkDocument replaceRange(int start, int end, String replacement) {
    return FlarkDocument._(
      buffer: buffer.replaceRange(start, end, replacement),
      revision: revision + 1,
    );
  }

  FlarkDocument copyWith({FlarkTextBuffer? buffer, int? revision}) {
    return FlarkDocument._(
      buffer: buffer ?? this.buffer,
      revision: revision ?? this.revision,
    );
  }
}
