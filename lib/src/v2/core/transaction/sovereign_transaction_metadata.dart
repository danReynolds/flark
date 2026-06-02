import 'sovereign_source_range.dart';

enum FlarkTransactionIntent {
  input,
  command,
  paste,
  selection,
  undo,
  redo,
  programmatic,
  unknown,
}

final class FlarkTransactionMetadata {
  const FlarkTransactionMetadata({
    this.intent = FlarkTransactionIntent.unknown,
    this.userEvent,
    this.undoGroupId,
    this.parseInvalidationRange,
    this.projectionInvalidationRange,
    this.addToHistory = true,
  });

  final FlarkTransactionIntent intent;
  final String? userEvent;
  final int? undoGroupId;
  final FlarkSourceRange? parseInvalidationRange;
  final FlarkSourceRange? projectionInvalidationRange;
  final bool addToHistory;

  FlarkTransactionMetadata copyWith({
    FlarkTransactionIntent? intent,
    String? userEvent,
    int? undoGroupId,
    FlarkSourceRange? parseInvalidationRange,
    FlarkSourceRange? projectionInvalidationRange,
    bool? addToHistory,
  }) {
    return FlarkTransactionMetadata(
      intent: intent ?? this.intent,
      userEvent: userEvent ?? this.userEvent,
      undoGroupId: undoGroupId ?? this.undoGroupId,
      parseInvalidationRange:
          parseInvalidationRange ?? this.parseInvalidationRange,
      projectionInvalidationRange:
          projectionInvalidationRange ?? this.projectionInvalidationRange,
      addToHistory: addToHistory ?? this.addToHistory,
    );
  }
}
