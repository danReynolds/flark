import '../document/flark_document.dart';
import '../state/flark_editor_state.dart';
import '../transaction/flark_transaction.dart';
import '../transaction/flark_transaction_metadata.dart';

final class FlarkHistoryEntry {
  FlarkHistoryEntry({
    required List<FlarkTransaction> redoTransactions,
    required List<FlarkTransaction> undoTransactions,
    this.undoGroupId,
  }) : redoTransactions = List<FlarkTransaction>.unmodifiable(redoTransactions),
       undoTransactions = List<FlarkTransaction>.unmodifiable(undoTransactions);

  final List<FlarkTransaction> redoTransactions;
  final List<FlarkTransaction> undoTransactions;
  final int? undoGroupId;

  FlarkHistoryEntry append(
    FlarkTransaction transaction,
    FlarkTransaction inverse,
  ) {
    return FlarkHistoryEntry(
      redoTransactions: [...redoTransactions, transaction],
      undoTransactions: [inverse, ...undoTransactions],
      undoGroupId: undoGroupId,
    );
  }
}

final class FlarkHistoryResult {
  const FlarkHistoryResult({
    required this.state,
    required this.history,
    this.appliedTransactions = const <FlarkTransaction>[],
  });

  final FlarkEditorState state;
  final FlarkHistoryStack history;

  /// The transactions applied to produce [state], in application order.
  ///
  /// Empty when nothing was applied (e.g. undo on an empty stack). Consumers
  /// such as the Flutter controller map projections and render plans through
  /// these instead of discarding their predicted state.
  final List<FlarkTransaction> appliedTransactions;
}

final class FlarkHistoryStack {
  const FlarkHistoryStack({
    this.undoEntries = const <FlarkHistoryEntry>[],
    this.redoEntries = const <FlarkHistoryEntry>[],
    this.maxEntries = defaultMaxEntries,
  });

  /// Default bound on retained undo entries.
  ///
  /// Each entry holds its transactions and their inverses (including
  /// replaced text), so an unbounded stack grows with every edit for the
  /// lifetime of a document. One thousand logical edit groups is far more
  /// undo depth than editors conventionally offer while keeping long
  /// sessions bounded.
  static const int defaultMaxEntries = 1000;

  final List<FlarkHistoryEntry> undoEntries;
  final List<FlarkHistoryEntry> redoEntries;

  /// Maximum retained undo entries; the oldest entries are dropped first.
  final int maxEntries;

  bool get canUndo => undoEntries.isNotEmpty;

  bool get canRedo => redoEntries.isNotEmpty;

  /// Records [transaction] against [documentBefore].
  ///
  /// Callers that already applied the transaction (the runtime hot path) pass
  /// the resulting document as [documentAfter] so it is not recomputed here.
  /// [FlarkTransaction.applyToDocument] returns the identical document
  /// instance for no-op transactions, so identity alone detects them.
  FlarkHistoryStack record({
    required FlarkTransaction transaction,
    required FlarkDocument documentBefore,
    FlarkDocument? documentAfter,
  }) {
    if (!transaction.metadata.addToHistory || !transaction.changesDocument) {
      return this;
    }

    final effectiveDocumentAfter =
        documentAfter ?? transaction.applyToDocument(documentBefore);
    assert(
      documentAfter == null ||
          documentAfter.markdown ==
              transaction.applyToDocument(documentBefore).markdown,
      'documentAfter must be the result of applying transaction to '
      'documentBefore; the no-op check below relies on applyToDocument '
      'returning the identical instance for no-op transactions.',
    );
    if (identical(effectiveDocumentAfter, documentBefore)) {
      return this;
    }

    final inverse = transaction.invert(documentBefore);
    final groupId = transaction.metadata.undoGroupId;
    final nextUndoEntries = [...undoEntries];
    final lastEntry = nextUndoEntries.isEmpty ? null : nextUndoEntries.last;

    // An explicit group id (IME composition) merges by id; plain typing with no
    // group id coalesces consecutive single characters so one undo removes a
    // word rather than a letter.
    final coalesce =
        lastEntry != null &&
        (groupId != null
            ? lastEntry.undoGroupId == groupId
            : _coalescesTyping(lastEntry, transaction));

    if (coalesce) {
      nextUndoEntries[nextUndoEntries.length - 1] = lastEntry.append(
        transaction,
        inverse,
      );
    } else {
      nextUndoEntries.add(
        FlarkHistoryEntry(
          redoTransactions: [transaction],
          undoTransactions: [inverse],
          undoGroupId: groupId,
        ),
      );
    }
    if (maxEntries > 0 && nextUndoEntries.length > maxEntries) {
      nextUndoEntries.removeRange(0, nextUndoEntries.length - maxEntries);
    }

    return FlarkHistoryStack(
      undoEntries: nextUndoEntries,
      redoEntries: const <FlarkHistoryEntry>[],
      maxEntries: maxEntries,
    );
  }

  /// Whether [transaction] continues the typing run in [lastEntry], so it
  /// should merge into that undo entry instead of starting a new one.
  ///
  /// Merges contiguous single-character insertions; breaks at the start of a
  /// new word (whitespace → non-whitespace) so undo removes one word at a time,
  /// and at a caret jump or any non-typing edit.
  static bool _coalescesTyping(
    FlarkHistoryEntry lastEntry,
    FlarkTransaction transaction,
  ) {
    if (lastEntry.undoGroupId != null) return false;
    final typed = _typedChar(transaction);
    if (typed == null) return false;
    final previous = _typedChar(lastEntry.redoTransactions.last);
    if (previous == null) return false;
    if (typed.offset != previous.offset + 1) return false;
    if (_isWhitespace(previous.char) && !_isWhitespace(typed.char)) return false;
    return true;
  }

  /// The insertion offset and character of a single-character typed insertion
  /// eligible for coalescing, or null for anything else (multi-char inserts,
  /// replacements, deletions, commands, pastes, or IME-grouped input).
  static ({int offset, String char})? _typedChar(FlarkTransaction transaction) {
    final metadata = transaction.metadata;
    if (metadata.intent != FlarkTransactionIntent.input) return null;
    if (metadata.undoGroupId != null) return null;
    if (transaction.operations.length != 1) return null;
    final operation = transaction.operations.single;
    if (!operation.replacedRange.isCollapsed ||
        operation.replacementText.length != 1) {
      return null;
    }
    return (offset: operation.replacedRange.start, char: operation.replacementText);
  }

  static bool _isWhitespace(String char) {
    return char == ' ' || char == '\t' || char == '\n' || char == '\r';
  }

  FlarkHistoryResult undo(FlarkEditorState state) {
    if (undoEntries.isEmpty) {
      return FlarkHistoryResult(state: state, history: this);
    }

    final entry = undoEntries.last;
    var nextState = state;
    for (final transaction in entry.undoTransactions) {
      nextState = nextState.applyTransaction(transaction);
    }

    return FlarkHistoryResult(
      state: nextState,
      history: FlarkHistoryStack(
        undoEntries: undoEntries.sublist(0, undoEntries.length - 1),
        redoEntries: [...redoEntries, entry],
        maxEntries: maxEntries,
      ),
      appliedTransactions: entry.undoTransactions,
    );
  }

  FlarkHistoryResult redo(FlarkEditorState state) {
    if (redoEntries.isEmpty) {
      return FlarkHistoryResult(state: state, history: this);
    }

    final entry = redoEntries.last;
    var nextState = state;
    for (final transaction in entry.redoTransactions) {
      nextState = nextState.applyTransaction(transaction);
    }

    return FlarkHistoryResult(
      state: nextState,
      history: FlarkHistoryStack(
        undoEntries: [...undoEntries, entry],
        redoEntries: redoEntries.sublist(0, redoEntries.length - 1),
        maxEntries: maxEntries,
      ),
      appliedTransactions: entry.redoTransactions,
    );
  }
}
