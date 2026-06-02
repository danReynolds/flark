import '../document/flark_document.dart';
import '../selection/flark_selection.dart';
import 'flark_source_operation.dart';
import 'flark_source_range.dart';
import 'flark_transaction_metadata.dart';

final class FlarkTransaction {
  FlarkTransaction({
    required List<FlarkSourceOperation> operations,
    this.selectionBefore,
    this.selectionAfter,
    FlarkTransactionMetadata? metadata,
    String? userEvent,
    int? undoGroupId,
    bool addToHistory = true,
  }) : metadata =
           metadata ??
           FlarkTransactionMetadata(
             userEvent: userEvent,
             undoGroupId: undoGroupId,
             addToHistory: addToHistory,
           ),
       operations = List<FlarkSourceOperation>.unmodifiable(operations);

  factory FlarkTransaction.single(
    FlarkSourceOperation operation, {
    FlarkSelection? selectionBefore,
    FlarkSelection? selectionAfter,
    FlarkTransactionMetadata? metadata,
    String? userEvent,
    int? undoGroupId,
    bool addToHistory = true,
  }) {
    return FlarkTransaction(
      operations: [operation],
      selectionBefore: selectionBefore,
      selectionAfter: selectionAfter,
      metadata: metadata,
      userEvent: userEvent,
      undoGroupId: undoGroupId,
      addToHistory: addToHistory,
    );
  }

  final List<FlarkSourceOperation> operations;
  final FlarkSelection? selectionBefore;
  final FlarkSelection? selectionAfter;
  final FlarkTransactionMetadata metadata;

  String? get userEvent => metadata.userEvent;

  int? get undoGroupId => metadata.undoGroupId;

  bool get addToHistory => metadata.addToHistory;

  bool get changesDocument => operations.isNotEmpty;

  FlarkDocument applyToDocument(FlarkDocument document) {
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

  FlarkSelection mapSelection(FlarkSelection selection) {
    final sorted = _sortedOperations();
    final mappedBase = _mapOffset(selection.baseOffset, sorted: sorted);
    final mappedExtent = _mapOffset(selection.extentOffset, sorted: sorted);
    return FlarkSelection(baseOffset: mappedBase, extentOffset: mappedExtent);
  }

  int mapOffset(
    int offset, {
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
  }) {
    return _mapOffset(offset, affinity: affinity);
  }

  FlarkTransaction invert(FlarkDocument before) {
    final text = before.markdown;
    final sorted = _validatedOperations(text.length);
    var delta = 0;
    final inverseOperations = <FlarkSourceOperation>[];

    for (final operation in sorted) {
      final range = operation.replacedRange;
      final deletedText = text.substring(range.start, range.end);
      final mappedStart = range.start + delta;
      final mappedEnd = mappedStart + operation.insertedLength;
      inverseOperations.add(
        FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(mappedStart, mappedEnd),
          replacementText: deletedText,
        ),
      );
      delta += operation.delta;
    }

    return FlarkTransaction(
      operations: inverseOperations,
      selectionBefore: selectionAfter,
      selectionAfter: selectionBefore,
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.undo,
        userEvent: metadata.userEvent == null
            ? null
            : 'undo:${metadata.userEvent}',
        undoGroupId: metadata.undoGroupId,
        addToHistory: false,
      ),
    );
  }

  List<FlarkSourceOperation> _validatedOperations(int textLength) {
    final sorted = _sortedOperations();

    var previousEnd = 0;
    for (final operation in sorted) {
      operation.validate(textLength);
      if (operation.replacedRange.start < previousEnd) {
        throw StateError(
          'Flark transactions cannot contain overlapping '
          'source operations.',
        );
      }
      previousEnd = operation.replacedRange.end;
    }
    return sorted;
  }

  List<FlarkSourceOperation> _sortedOperations() {
    final indexed = [
      for (var index = 0; index < operations.length; index += 1)
        _IndexedSourceOperation(index, operations[index]),
    ]..sort(_compareIndexedOperations);

    return [for (final indexedOperation in indexed) indexedOperation.operation];
  }

  static String _applyAtomic(
    String text,
    List<FlarkSourceOperation> operations,
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
    FlarkMapAffinity affinity = FlarkMapAffinity.downstream,
    List<FlarkSourceOperation>? sorted,
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
          FlarkMapAffinity.upstream => start + delta,
          FlarkMapAffinity.downstream =>
            start + delta + operation.insertedLength,
        };
      }

      return switch (affinity) {
        FlarkMapAffinity.upstream => start + delta,
        FlarkMapAffinity.downstream => start + delta + operation.insertedLength,
      };
    }
    return offset + delta;
  }
}

final class _IndexedSourceOperation {
  const _IndexedSourceOperation(this.index, this.operation);

  final int index;
  final FlarkSourceOperation operation;
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
