import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';

void main() {
  group('FlarkTransaction', () {
    test('applies insertion and maps a collapsed selection downstream', () {
      final state = FlarkEditorState.fromMarkdown('Hello');
      final next = state.applyTransaction(
        FlarkTransaction.single(
          FlarkSourceOperation.insert(5, '!'),
          userEvent: 'input.type',
        ),
      );

      expect(next.markdown, 'Hello!');
      expect(next.revision, 1);
      expect(next.selection, const FlarkSelection.collapsed(6));
    });

    test('applies deletion and maps selections after the deleted range', () {
      final state = FlarkEditorState.fromMarkdown(
        'abcdef',
        selection: const FlarkSelection.collapsed(5),
      );

      final next = state.applyTransaction(
        FlarkTransaction.single(FlarkSourceOperation.delete(1, 4)),
      );

      expect(next.markdown, 'aef');
      expect(next.selection, const FlarkSelection.collapsed(2));
    });

    test(
      'maps selections inside a replacement to the replacement boundary',
      () {
        final state = FlarkEditorState.fromMarkdown(
          'abcdef',
          selection: const FlarkSelection.collapsed(3),
        );

        final next = state.applyTransaction(
          FlarkTransaction.single(
            const FlarkSourceOperation.replace(
              replacedRange: FlarkSourceRange(1, 5),
              replacementText: 'Z',
            ),
          ),
        );

        expect(next.markdown, 'aZf');
        expect(next.selection, const FlarkSelection.collapsed(2));
      },
    );

    test('uses explicit transaction selection when provided', () {
      final state = FlarkEditorState.fromMarkdown('abc');
      final next = state.applyTransaction(
        FlarkTransaction.single(
          FlarkSourceOperation.insert(0, '# '),
          selectionAfter: const FlarkSelection.collapsed(2),
        ),
      );

      expect(next.markdown, '# abc');
      expect(next.selection, const FlarkSelection.collapsed(2));
    });

    test('applies multi-operation transactions against original offsets', () {
      final state = FlarkEditorState.fromMarkdown(
        'abcd',
        selection: const FlarkSelection.collapsed(4),
      );

      final next = state.applyTransaction(
        FlarkTransaction(
          operations: [
            FlarkSourceOperation.delete(3, 4),
            FlarkSourceOperation.insert(1, 'X'),
          ],
        ),
      );

      expect(next.markdown, 'aXbc');
      expect(next.selection, const FlarkSelection.collapsed(4));
    });

    test('preserves operation order for same-offset insertions', () {
      final state = FlarkEditorState.fromMarkdown('ab');

      final next = state.applyTransaction(
        FlarkTransaction(
          operations: [
            FlarkSourceOperation.insert(1, 'X'),
            FlarkSourceOperation.insert(1, 'Y'),
            FlarkSourceOperation.insert(1, 'Z'),
          ],
        ),
      );

      expect(next.markdown, 'aXYZb');
    });

    test('maps selections between atomic operations with prior deltas', () {
      final state = FlarkEditorState.fromMarkdown(
        'abcdef',
        selection: const FlarkSelection.collapsed(2),
      );

      final next = state.applyTransaction(
        FlarkTransaction(
          operations: [
            FlarkSourceOperation.insert(1, 'XX'),
            const FlarkSourceOperation.replace(
              replacedRange: FlarkSourceRange(3, 5),
              replacementText: 'Z',
            ),
          ],
        ),
      );

      expect(next.markdown, 'aXXbcZf');
      expect(next.selection, const FlarkSelection.collapsed(4));
    });

    test('maps selections inside later operations with prior deltas', () {
      final state = FlarkEditorState.fromMarkdown(
        'abcdef',
        selection: const FlarkSelection.collapsed(4),
      );

      final next = state.applyTransaction(
        FlarkTransaction(
          operations: [
            FlarkSourceOperation.insert(1, 'XX'),
            const FlarkSourceOperation.replace(
              replacedRange: FlarkSourceRange(3, 5),
              replacementText: 'Z',
            ),
          ],
        ),
      );

      expect(next.markdown, 'aXXbcZf');
      expect(next.selection, const FlarkSelection.collapsed(6));
    });

    test('supports explicit upstream mapping for insertion boundaries', () {
      final operation = FlarkSourceOperation.insert(2, '**');

      expect(operation.mapOffset(2, affinity: FlarkMapAffinity.upstream), 2);
      expect(operation.mapOffset(2, affinity: FlarkMapAffinity.downstream), 4);
    });

    test('exposes transaction offset mapping for downstream projections', () {
      final transaction = FlarkTransaction(
        operations: [
          FlarkSourceOperation.insert(1, 'XX'),
          const FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(4, 5),
            replacementText: 'Z',
          ),
        ],
      );

      expect(transaction.mapOffset(0), 0);
      expect(transaction.mapOffset(1), 3);
      expect(transaction.mapOffset(1, affinity: FlarkMapAffinity.upstream), 1);
      expect(transaction.mapOffset(5), 7);
    });

    test('source ranges support containment, intersection, and union', () {
      const range = FlarkSourceRange(2, 6);

      expect(range.containsOffset(2), isTrue);
      expect(range.containsRange(const FlarkSourceRange(3, 5)), isTrue);
      expect(range.intersects(const FlarkSourceRange(5, 8)), isTrue);
      expect(range.intersects(const FlarkSourceRange(6, 8)), isFalse);
      expect(
        range.union(const FlarkSourceRange(8, 10)),
        const FlarkSourceRange(2, 10),
      );
    });

    test('inverts a transaction back to the original markdown', () {
      final state = FlarkEditorState.fromMarkdown(
        'abcdef',
        selection: const FlarkSelection.collapsed(3),
      );
      final transaction = FlarkTransaction.single(
        const FlarkSourceOperation.replace(
          replacedRange: FlarkSourceRange(1, 4),
          replacementText: 'XY',
        ),
        selectionBefore: state.selection,
        selectionAfter: const FlarkSelection.collapsed(3),
        userEvent: 'command.replace',
      );

      final next = state.applyTransaction(transaction);
      final restored = next.applyTransaction(
        transaction.invert(state.document),
      );

      expect(next.markdown, 'aXYef');
      expect(restored.markdown, 'abcdef');
      expect(restored.selection, state.selection);
      expect(transaction.invert(state.document).metadata.addToHistory, isFalse);
    });

    test('rejects overlapping source operations', () {
      final state = FlarkEditorState.fromMarkdown('abcdef');
      final transaction = FlarkTransaction(
        operations: [
          const FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(1, 4),
            replacementText: 'X',
          ),
          const FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(3, 5),
            replacementText: 'Y',
          ),
        ],
      );

      expect(
        () => state.applyTransaction(transaction),
        throwsA(isA<StateError>()),
      );
    });
  });
}
