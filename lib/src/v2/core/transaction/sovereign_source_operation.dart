import '../selection/sovereign_selection.dart';
import 'sovereign_source_range.dart';

final class FlarkSourceOperation {
  const FlarkSourceOperation.replace({
    required this.replacedRange,
    required this.replacementText,
  });

  factory FlarkSourceOperation.insert(int offset, String text) {
    return FlarkSourceOperation.replace(
      replacedRange: FlarkSourceRange(offset, offset),
      replacementText: text,
    );
  }

  factory FlarkSourceOperation.delete(int start, int end) {
    return FlarkSourceOperation.replace(
      replacedRange: FlarkSourceRange(start, end),
      replacementText: '',
    );
  }

  final FlarkSourceRange replacedRange;
  final String replacementText;

  int get insertedLength => replacementText.length;

  int get deletedLength => replacedRange.length;

  int get delta => insertedLength - deletedLength;

  bool get isInsertion =>
      replacedRange.isCollapsed && replacementText.isNotEmpty;

  bool get isDeletion => !replacedRange.isCollapsed && replacementText.isEmpty;

  String apply(String text) {
    validate(text.length);
    return text.replaceRange(
      replacedRange.start,
      replacedRange.end,
      replacementText,
    );
  }

  FlarkSourceOperation validate(int textLength) {
    replacedRange.validate(textLength);
    return this;
  }

  int mapOffset(
    int offset, {
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
  }) {
    final start = replacedRange.start;
    final end = replacedRange.end;

    if (offset < start) return offset;
    if (offset > end) return offset + delta;

    if (replacedRange.isCollapsed) {
      return switch (affinity) {
        FlarkMapAffinity.upstream => start,
        FlarkMapAffinity.downstream => start + insertedLength,
      };
    }

    return switch (affinity) {
      FlarkMapAffinity.upstream => start,
      FlarkMapAffinity.downstream => start + insertedLength,
    };
  }

  @override
  bool operator ==(Object other) {
    return other is FlarkSourceOperation &&
        other.replacedRange == replacedRange &&
        other.replacementText == replacementText;
  }

  @override
  int get hashCode => Object.hash(replacedRange, replacementText);

  @override
  String toString() {
    return 'FlarkSourceOperation.replace('
        'range: $replacedRange, replacementText: "$replacementText")';
  }
}
