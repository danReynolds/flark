import 'sovereign_source_range.dart';

enum SovereignTransactionIntent {
  input,
  command,
  paste,
  selection,
  undo,
  redo,
  programmatic,
  unknown,
}

final class SovereignTransactionMetadata {
  const SovereignTransactionMetadata({
    this.intent = SovereignTransactionIntent.unknown,
    this.userEvent,
    this.undoGroupId,
    this.parseInvalidationRange,
    this.projectionInvalidationRange,
    this.addToHistory = true,
  });

  final SovereignTransactionIntent intent;
  final String? userEvent;
  final int? undoGroupId;
  final SovereignSourceRange? parseInvalidationRange;
  final SovereignSourceRange? projectionInvalidationRange;
  final bool addToHistory;

  SovereignTransactionMetadata copyWith({
    SovereignTransactionIntent? intent,
    String? userEvent,
    int? undoGroupId,
    SovereignSourceRange? parseInvalidationRange,
    SovereignSourceRange? projectionInvalidationRange,
    bool? addToHistory,
  }) {
    return SovereignTransactionMetadata(
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
