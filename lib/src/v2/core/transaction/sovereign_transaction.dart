import '../document/sovereign_document.dart';
import '../selection/sovereign_selection.dart';
import 'sovereign_source_operation.dart';
import 'sovereign_source_range.dart';
import 'sovereign_transaction_metadata.dart';

final class SovereignTransaction {
  SovereignTransaction({
    required List<SovereignSourceOperation> operations,
    this.selectionBefore,
    this.selectionAfter,
    SovereignTransactionMetadata? metadata,
    String? userEvent,
    int? undoGroupId,
    bool addToHistory = true,
  }) : metadata =
           metadata ??
           SovereignTransactionMetadata(
             userEvent: userEvent,
             undoGroupId: undoGroupId,
             addToHistory: addToHistory,
           ),
       operations = List<SovereignSourceOperation>.unmodifiable(operations);

  factory SovereignTransaction.single(
    SovereignSourceOperation operation, {
    SovereignSelection? selectionBefore,
    SovereignSelection? selectionAfter,
    SovereignTransactionMetadata? metadata,
    String? userEvent,
    int? undoGroupId,
    bool addToHistory = true,
  }) {
    return SovereignTransaction(
      operations: [operation],
      selectionBefore: selectionBefore,
      selectionAfter: selectionAfter,
      metadata: metadata,
      userEvent: userEvent,
      undoGroupId: undoGroupId,
      addToHistory: addToHistory,
    );
  }

  final List<SovereignSourceOperation> operations;
  final SovereignSelection? selectionBefore;
  final SovereignSelection? selectionAfter;
  final SovereignTransactionMetadata metadata;

  String? get userEvent => metadata.userEvent;

  int? get undoGroupId => metadata.undoGroupId;

  bool get addToHistory => metadata.addToHistory;

  bool get changesDocument => operations.isNotEmpty;

  SovereignDocument applyToDocument(SovereignDocument document) {
    if (operations.isEmpty) return document;

    final text = document.markdown;
    final sorted = _validatedOperations(text.length);
    final nextText = _applyAtomic(text, sorted);
    if (nextText == text) return document;

    return document.copyWith(
      buffer: document.buffer.replaceRange(0, document.length, nextText),
      revision: document.revision + 1,
    );
  }

  SovereignSelection mapSelection(SovereignSelection selection) {
    final sorted = _sortedOperations();
    final mappedBase = _mapOffset(selection.baseOffset, sorted: sorted);
    final mappedExtent = _mapOffset(selection.extentOffset, sorted: sorted);
    return SovereignSelection(
      baseOffset: mappedBase,
      extentOffset: mappedExtent,
    );
  }

  int mapOffset(
    int offset, {
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
  }) {
    return _mapOffset(offset, affinity: affinity);
  }

  SovereignTransaction invert(SovereignDocument before) {
    final text = before.markdown;
    final sorted = _validatedOperations(text.length);
    var delta = 0;
    final inverseOperations = <SovereignSourceOperation>[];

    for (final operation in sorted) {
      final range = operation.replacedRange;
      final deletedText = text.substring(range.start, range.end);
      final mappedStart = range.start + delta;
      final mappedEnd = mappedStart + operation.insertedLength;
      inverseOperations.add(
        SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(mappedStart, mappedEnd),
          replacementText: deletedText,
        ),
      );
      delta += operation.delta;
    }

    return SovereignTransaction(
      operations: inverseOperations,
      selectionBefore: selectionAfter,
      selectionAfter: selectionBefore,
      metadata: SovereignTransactionMetadata(
        intent: SovereignTransactionIntent.undo,
        userEvent: metadata.userEvent == null
            ? null
            : 'undo:${metadata.userEvent}',
        undoGroupId: metadata.undoGroupId,
        addToHistory: false,
      ),
    );
  }

  List<SovereignSourceOperation> _validatedOperations(int textLength) {
    final sorted = _sortedOperations();

    var previousEnd = 0;
    for (final operation in sorted) {
      operation.validate(textLength);
      if (operation.replacedRange.start < previousEnd) {
        throw StateError(
          'Sovereign transactions cannot contain overlapping '
          'source operations.',
        );
      }
      previousEnd = operation.replacedRange.end;
    }
    return sorted;
  }

  List<SovereignSourceOperation> _sortedOperations() {
    final indexed = [
      for (var index = 0; index < operations.length; index += 1)
        _IndexedSourceOperation(index, operations[index]),
    ]..sort(_compareIndexedOperations);

    return [for (final indexedOperation in indexed) indexedOperation.operation];
  }

  static String _applyAtomic(
    String text,
    List<SovereignSourceOperation> operations,
  ) {
    final buffer = StringBuffer();
    var cursor = 0;

    for (final operation in operations) {
      final range = operation.replacedRange;
      buffer
        ..write(text.substring(cursor, range.start))
        ..write(operation.replacementText);
      cursor = range.end;
    }
    buffer.write(text.substring(cursor));
    return buffer.toString();
  }

  int _mapOffset(
    int offset, {
    SovereignMapAffinity affinity = SovereignMapAffinity.downstream,
    List<SovereignSourceOperation>? sorted,
  }) {
    var delta = 0;
    for (final operation in sorted ?? _sortedOperations()) {
      final range = operation.replacedRange;
      final start = range.start;
      final end = range.end;

      if (offset < start) break;
      if (offset > end) {
        delta += operation.delta;
        continue;
      }

      if (range.isCollapsed) {
        return switch (affinity) {
          SovereignMapAffinity.upstream => start + delta,
          SovereignMapAffinity.downstream =>
            start + delta + operation.insertedLength,
        };
      }

      return switch (affinity) {
        SovereignMapAffinity.upstream => start + delta,
        SovereignMapAffinity.downstream =>
          start + delta + operation.insertedLength,
      };
    }
    return offset + delta;
  }
}

final class _IndexedSourceOperation {
  const _IndexedSourceOperation(this.index, this.operation);

  final int index;
  final SovereignSourceOperation operation;
}

int _compareIndexedOperations(
  _IndexedSourceOperation a,
  _IndexedSourceOperation b,
) {
  final startCompare = a.operation.replacedRange.start.compareTo(
    b.operation.replacedRange.start,
  );
  if (startCompare != 0) return startCompare;

  final endCompare = a.operation.replacedRange.end.compareTo(
    b.operation.replacedRange.end,
  );
  if (endCompare != 0) return endCompare;

  return a.index.compareTo(b.index);
}
