import 'package:flutter/services.dart';

import '../core/core.dart';

final class FlarkTextDeltaAdapter {
  const FlarkTextDeltaAdapter();

  FlarkTransaction? transactionFromDelta(
    TextEditingDelta delta, {
    required String currentMarkdown,
  }) {
    if (delta.oldText != currentMarkdown) return null;

    final operation = switch (delta) {
      TextEditingDeltaInsertion() => _insertion(delta),
      TextEditingDeltaDeletion() => _deletion(delta),
      TextEditingDeltaReplacement() => _replacement(delta),
      TextEditingDeltaNonTextUpdate() => null,
      _ => null,
    };

    if (operation == null && delta is! TextEditingDeltaNonTextUpdate) {
      return null;
    }

    final operations = operation == null
        ? const <FlarkSourceOperation>[]
        : <FlarkSourceOperation>[operation];
    final selectionAfter = _selection(delta.selection);
    return FlarkTransaction(
      operations: operations,
      selectionAfter: selectionAfter,
      metadata: FlarkTransactionMetadata(
        intent: operation == null
            ? FlarkTransactionIntent.selection
            : FlarkTransactionIntent.input,
        userEvent: operation == null ? 'input.selection' : 'input.delta',
        addToHistory: operation != null,
        parseInvalidationRange: operation?.replacedRange,
        projectionInvalidationRange: operation?.replacedRange,
      ),
    );
  }

  FlarkSourceOperation? _insertion(TextEditingDeltaInsertion delta) {
    if (delta.insertionOffset < 0 ||
        delta.insertionOffset > delta.oldText.length) {
      return null;
    }
    return FlarkSourceOperation.insert(
      delta.insertionOffset,
      delta.textInserted,
    );
  }

  FlarkSourceOperation? _deletion(TextEditingDeltaDeletion delta) {
    if (!_validRange(delta.deletedRange, delta.oldText.length)) return null;
    return FlarkSourceOperation.delete(
      delta.deletedRange.start,
      delta.deletedRange.end,
    );
  }

  FlarkSourceOperation? _replacement(TextEditingDeltaReplacement delta) {
    if (!_validRange(delta.replacedRange, delta.oldText.length)) return null;
    return FlarkSourceOperation.replace(
      replacedRange: FlarkSourceRange(
        delta.replacedRange.start,
        delta.replacedRange.end,
      ),
      replacementText: delta.replacementText,
    );
  }

  FlarkSelection? _selection(TextSelection selection) {
    if (!selection.isValid) return null;
    return FlarkSelection(
      baseOffset: selection.baseOffset,
      extentOffset: selection.extentOffset,
    );
  }

  bool _validRange(TextRange range, int textLength) {
    return range.isValid &&
        range.start >= 0 &&
        range.end <= textLength &&
        range.start <= range.end;
  }
}
