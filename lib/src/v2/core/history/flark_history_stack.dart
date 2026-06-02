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
  const FlarkHistoryResult({required this.state, required this.history});

  final FlarkEditorState state;
  final FlarkHistoryStack history;
}

final class FlarkHistoryStack {
  const FlarkHistoryStack({
    this.undoEntries = const <FlarkHistoryEntry>[],
    this.redoEntries = const <FlarkHistoryEntry>[],
  });

  final List<FlarkHistoryEntry> undoEntries;
  final List<FlarkHistoryEntry> redoEntries;

  bool get canUndo => undoEntries.isNotEmpty;

  bool get canRedo => redoEntries.isNotEmpty;

  FlarkHistoryStack record({
    required FlarkTransaction transaction,
    required FlarkDocument documentBefore,
  }) {
    if (!transaction.metadata.addToHistory || !transaction.changesDocument) {
      return this;
    }

    final documentAfter = transaction.applyToDocument(documentBefore);
    if (identical(documentAfter, documentBefore) ||
        documentAfter.markdown == documentBefore.markdown) {
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

    return FlarkHistoryStack(
      undoEntries: nextUndoEntries,
      redoEntries: const <FlarkHistoryEntry>[],
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
      ),
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
      ),
    );
  }
}
