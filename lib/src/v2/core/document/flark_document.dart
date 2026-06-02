import 'flark_text_buffer.dart';

final class FlarkDocument {
  FlarkDocument._({required this.buffer, required this.revision});

  factory FlarkDocument.fromMarkdown(String markdown) {
    return FlarkDocument._(buffer: FlarkTextBuffer(markdown), revision: 0);
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
