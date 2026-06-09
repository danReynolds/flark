import '../core/core.dart';

final class FlarkLiveBlockSourceEdit {
  const FlarkLiveBlockSourceEdit({
    required this.range,
    required this.replacementText,
    required this.editableRangeAfter,
    required this.selectionAfter,
  });

  final FlarkSourceRange range;
  final String replacementText;
  final FlarkSourceRange editableRangeAfter;
  final FlarkSelection selectionAfter;
}
