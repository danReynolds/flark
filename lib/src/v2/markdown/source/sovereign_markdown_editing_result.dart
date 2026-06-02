import '../../core/core.dart';

sealed class SovereignMarkdownInputResult {
  const SovereignMarkdownInputResult();
}

final class SovereignMarkdownSourceEdit extends SovereignMarkdownInputResult {
  const SovereignMarkdownSourceEdit({
    required this.range,
    required this.replacementText,
    required this.selectionAfter,
  });

  final SovereignSourceRange range;
  final String replacementText;
  final SovereignSelection selectionAfter;
}

final class SovereignMarkdownSelectionMove
    extends SovereignMarkdownInputResult {
  const SovereignMarkdownSelectionMove({
    required this.selectionAfter,
  });

  final SovereignSelection selectionAfter;
}
