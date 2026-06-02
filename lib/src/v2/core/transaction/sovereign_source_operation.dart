import '../selection/sovereign_selection.dart';
import 'sovereign_source_range.dart';

final class SovereignSourceOperation {
  const SovereignSourceOperation.replace({
    required this.replacedRange,
    required this.replacementText,
  });

  factory SovereignSourceOperation.insert(int offset, String text) {
    return SovereignSourceOperation.replace(
      replacedRange: SovereignSourceRange(offset, offset),
      replacementText: text,
    );
  }

  factory SovereignSourceOperation.delete(int start, int end) {
    return SovereignSourceOperation.replace(
      replacedRange: SovereignSourceRange(start, end),
      replacementText: '',
    );
  }

  final SovereignSourceRange replacedRange;
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

  SovereignSourceOperation validate(int textLength) {
    replacedRange.validate(textLength);
    return this;
  }

  int mapOffset(
    int offset, {
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
  }) {
    final start = replacedRange.start;
    final end = replacedRange.end;

    if (offset < start) return offset;
    if (offset > end) return offset + delta;

    if (replacedRange.isCollapsed) {
      return switch (affinity) {
        SovereignMapAffinity.upstream => start,
        SovereignMapAffinity.downstream => start + insertedLength,
      };
    }

    return switch (affinity) {
      SovereignMapAffinity.upstream => start,
      SovereignMapAffinity.downstream => start + insertedLength,
    };
  }

  @override
  bool operator ==(Object other) {
    return other is SovereignSourceOperation &&
        other.replacedRange == replacedRange &&
        other.replacementText == replacementText;
  }

  @override
  int get hashCode => Object.hash(replacedRange, replacementText);

  @override
  String toString() {
    return 'SovereignSourceOperation.replace('
        'range: $replacedRange, replacementText: "$replacementText")';
  }
}
