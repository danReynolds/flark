import 'package:flutter/services.dart';

import '../core/core.dart';

final class SovereignTextDeltaAdapter {
  const SovereignTextDeltaAdapter();

  SovereignTransaction? transactionFromDelta(
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
        ? const <SovereignSourceOperation>[]
        : <SovereignSourceOperation>[operation];
    final selectionAfter = _selection(delta.selection);
    return SovereignTransaction(
      operations: operations,
      selectionAfter: selectionAfter,
      metadata: SovereignTransactionMetadata(
        intent: operation == null
            ? SovereignTransactionIntent.selection
            : SovereignTransactionIntent.input,
        userEvent: operation == null ? 'input.selection' : 'input.delta',
        addToHistory: operation != null,
        parseInvalidationRange: operation?.replacedRange,
        projectionInvalidationRange: operation?.replacedRange,
      ),
    );
  }

  SovereignSourceOperation? _insertion(TextEditingDeltaInsertion delta) {
    if (delta.insertionOffset < 0 ||
        delta.insertionOffset > delta.oldText.length) {
      return null;
    }
    return SovereignSourceOperation.insert(
      delta.insertionOffset,
      delta.textInserted,
    );
  }

  SovereignSourceOperation? _deletion(TextEditingDeltaDeletion delta) {
    if (!_validRange(delta.deletedRange, delta.oldText.length)) return null;
    return SovereignSourceOperation.delete(
      delta.deletedRange.start,
      delta.deletedRange.end,
    );
  }

  SovereignSourceOperation? _replacement(TextEditingDeltaReplacement delta) {
    if (!_validRange(delta.replacedRange, delta.oldText.length)) return null;
    return SovereignSourceOperation.replace(
      replacedRange: SovereignSourceRange(
        delta.replacedRange.start,
        delta.replacedRange.end,
      ),
      replacementText: delta.replacementText,
    );
  }

  SovereignSelection? _selection(TextSelection selection) {
    if (!selection.isValid) return null;
    return SovereignSelection(
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
