import '../../core/core.dart';

sealed class FlarkMarkdownInputResult {
  const FlarkMarkdownInputResult();
}

final class FlarkMarkdownSourceEdit extends FlarkMarkdownInputResult {
  const FlarkMarkdownSourceEdit({
    required this.range,
    required this.replacementText,
    required this.selectionAfter,
  });

  final FlarkSourceRange range;
  final String replacementText;
  final FlarkSelection selectionAfter;
}

final class FlarkMarkdownSelectionMove extends FlarkMarkdownInputResult {
  const FlarkMarkdownSelectionMove({required this.selectionAfter});

  final FlarkSelection selectionAfter;
}
