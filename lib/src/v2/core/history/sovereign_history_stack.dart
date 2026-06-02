import '../document/sovereign_document.dart';
import '../state/sovereign_editor_state.dart';
import '../transaction/sovereign_transaction.dart';

final class SovereignHistoryEntry {
  SovereignHistoryEntry({
    required List<SovereignTransaction> redoTransactions,
    required List<SovereignTransaction> undoTransactions,
    this.undoGroupId,
  })  : redoTransactions = List<SovereignTransaction>.unmodifiable(
          redoTransactions,
        ),
        undoTransactions = List<SovereignTransaction>.unmodifiable(
          undoTransactions,
        );

  final List<SovereignTransaction> redoTransactions;
  final List<SovereignTransaction> undoTransactions;
  final int? undoGroupId;

  SovereignHistoryEntry append(
    SovereignTransaction transaction,
    SovereignTransaction inverse,
  ) {
    return SovereignHistoryEntry(
      redoTransactions: [...redoTransactions, transaction],
      undoTransactions: [inverse, ...undoTransactions],
      undoGroupId: undoGroupId,
    );
  }
}

final class SovereignHistoryResult {
  const SovereignHistoryResult({
    required this.state,
    required this.history,
  });

  final SovereignEditorState state;
  final SovereignHistoryStack history;
}

final class SovereignHistoryStack {
  const SovereignHistoryStack({
    this.undoEntries = const <SovereignHistoryEntry>[],
    this.redoEntries = const <SovereignHistoryEntry>[],
  });

  final List<SovereignHistoryEntry> undoEntries;
  final List<SovereignHistoryEntry> redoEntries;

  bool get canUndo => undoEntries.isNotEmpty;

  bool get canRedo => redoEntries.isNotEmpty;

  SovereignHistoryStack record({
    required SovereignTransaction transaction,
    required SovereignDocument documentBefore,
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
      nextUndoEntries[nextUndoEntries.length - 1] =
          nextUndoEntries.last.append(transaction, inverse);
    } else {
      nextUndoEntries.add(
        SovereignHistoryEntry(
          redoTransactions: [transaction],
          undoTransactions: [inverse],
          undoGroupId: groupId,
        ),
      );
    }

    return SovereignHistoryStack(undoEntries: nextUndoEntries);
  }

  SovereignHistoryResult undo(SovereignEditorState state) {
    if (undoEntries.isEmpty) {
      return SovereignHistoryResult(state: state, history: this);
    }

    final entry = undoEntries.last;
    var nextState = state;
    for (final transaction in entry.undoTransactions) {
      nextState = nextState.applyTransaction(transaction);
    }

    return SovereignHistoryResult(
      state: nextState,
      history: SovereignHistoryStack(
        undoEntries: undoEntries.sublist(0, undoEntries.length - 1),
        redoEntries: [...redoEntries, entry],
      ),
    );
  }

  SovereignHistoryResult redo(SovereignEditorState state) {
    if (redoEntries.isEmpty) {
      return SovereignHistoryResult(state: state, history: this);
    }

    final entry = redoEntries.last;
    var nextState = state;
    for (final transaction in entry.redoTransactions) {
      nextState = nextState.applyTransaction(transaction);
    }

    return SovereignHistoryResult(
      state: nextState,
      history: SovereignHistoryStack(
        undoEntries: [...undoEntries, entry],
        redoEntries: redoEntries.sublist(0, redoEntries.length - 1),
      ),
    );
  }
}
