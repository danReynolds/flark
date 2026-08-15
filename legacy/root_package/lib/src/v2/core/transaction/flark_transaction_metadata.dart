import 'flark_source_range.dart';

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
    this.authoredMarkerRanges,
    this.addToHistory = true,
  });

  final FlarkTransactionIntent intent;
  final String? userEvent;
  final int? undoGroupId;
  final FlarkSourceRange? parseInvalidationRange;
  final FlarkSourceRange? projectionInvalidationRange;

  /// Delimiter ranges this transaction itself authored, in post-transaction
  /// coordinates — a grammar claim: "these spans are markdown syntax, not
  /// content". The RFC 022 parser judge confirms each range parses as a
  /// hidden marker before the edit commits and rejects the command when the
  /// parser disagrees, so a declarer does not have to be correct by
  /// construction to be safe.
  final List<FlarkSourceRange>? authoredMarkerRanges;
  final bool addToHistory;

  FlarkTransactionMetadata copyWith({
    FlarkTransactionIntent? intent,
    String? userEvent,
    int? undoGroupId,
    FlarkSourceRange? parseInvalidationRange,
    FlarkSourceRange? projectionInvalidationRange,
    List<FlarkSourceRange>? authoredMarkerRanges,
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
      authoredMarkerRanges: authoredMarkerRanges ?? this.authoredMarkerRanges,
      addToHistory: addToHistory ?? this.addToHistory,
    );
  }
}
