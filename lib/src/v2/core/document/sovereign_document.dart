import 'sovereign_text_buffer.dart';

final class SovereignDocument {
  SovereignDocument._({
    required this.buffer,
    required this.revision,
  });

  factory SovereignDocument.fromMarkdown(String markdown) {
    return SovereignDocument._(
      buffer: SovereignTextBuffer(markdown),
      revision: 0,
    );
  }

  final SovereignTextBuffer buffer;
  final int revision;

  String get markdown => buffer.text;

  int get length => buffer.length;

  SovereignDocument replaceRange(
    int start,
    int end,
    String replacement,
  ) {
    return SovereignDocument._(
      buffer: buffer.replaceRange(start, end, replacement),
      revision: revision + 1,
    );
  }

  SovereignDocument copyWith({
    SovereignTextBuffer? buffer,
    int? revision,
  }) {
    return SovereignDocument._(
      buffer: buffer ?? this.buffer,
      revision: revision ?? this.revision,
    );
  }
}
