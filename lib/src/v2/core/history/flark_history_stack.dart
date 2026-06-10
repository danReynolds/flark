import '../document/flark_document.dart';
import '../state/flark_editor_state.dart';
import '../transaction/flark_transaction.dart';

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

    if (groupId != null &&
        nextUndoEntries.isNotEmpty &&
        nextUndoEntries.last.undoGroupId == groupId) {
      nextUndoEntries[nextUndoEntries.length - 1] = nextUndoEntries.last.append(
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
