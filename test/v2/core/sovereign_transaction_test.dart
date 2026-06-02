import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';

void main() {
  group('SovereignTransaction', () {
    test('applies insertion and maps a collapsed selection downstream', () {
      final state = SovereignEditorState.fromMarkdown('Hello');
      final next = state.applyTransaction(
        SovereignTransaction.single(
          SovereignSourceOperation.insert(5, '!'),
          userEvent: 'input.type',
        ),
      );

      expect(next.markdown, 'Hello!');
      expect(next.revision, 1);
      expect(next.selection, const SovereignSelection.collapsed(6));
    });

    test('applies deletion and maps selections after the deleted range', () {
      final state = SovereignEditorState.fromMarkdown(
        'abcdef',
        selection: const SovereignSelection.collapsed(5),
      );

      final next = state.applyTransaction(
        SovereignTransaction.single(SovereignSourceOperation.delete(1, 4)),
      );

      expect(next.markdown, 'aef');
      expect(next.selection, const SovereignSelection.collapsed(2));
    });

    test(
      'maps selections inside a replacement to the replacement boundary',
      () {
        final state = SovereignEditorState.fromMarkdown(
          'abcdef',
          selection: const SovereignSelection.collapsed(3),
        );

        final next = state.applyTransaction(
          SovereignTransaction.single(
            const SovereignSourceOperation.replace(
              replacedRange: SovereignSourceRange(1, 5),
              replacementText: 'Z',
            ),
          ),
        );

        expect(next.markdown, 'aZf');
        expect(next.selection, const SovereignSelection.collapsed(2));
      },
    );

    test('uses explicit transaction selection when provided', () {
      final state = SovereignEditorState.fromMarkdown('abc');
      final next = state.applyTransaction(
        SovereignTransaction.single(
          SovereignSourceOperation.insert(0, '# '),
          selectionAfter: const SovereignSelection.collapsed(2),
        ),
      );

      expect(next.markdown, '# abc');
      expect(next.selection, const SovereignSelection.collapsed(2));
    });

    test('applies multi-operation transactions against original offsets', () {
      final state = SovereignEditorState.fromMarkdown(
        'abcd',
        selection: const SovereignSelection.collapsed(4),
      );

      final next = state.applyTransaction(
        SovereignTransaction(
          operations: [
            SovereignSourceOperation.delete(3, 4),
            SovereignSourceOperation.insert(1, 'X'),
          ],
        ),
      );

      expect(next.markdown, 'aXbc');
      expect(next.selection, const SovereignSelection.collapsed(4));
    });

    test('preserves operation order for same-offset insertions', () {
      final state = SovereignEditorState.fromMarkdown('ab');

      final next = state.applyTransaction(
        SovereignTransaction(
          operations: [
            SovereignSourceOperation.insert(1, 'X'),
            SovereignSourceOperation.insert(1, 'Y'),
            SovereignSourceOperation.insert(1, 'Z'),
          ],
        ),
      );

      expect(next.markdown, 'aXYZb');
    });

    test('maps selections between atomic operations with prior deltas', () {
      final state = SovereignEditorState.fromMarkdown(
        'abcdef',
        selection: const SovereignSelection.collapsed(2),
      );

      final next = state.applyTransaction(
        SovereignTransaction(
          operations: [
            SovereignSourceOperation.insert(1, 'XX'),
            const SovereignSourceOperation.replace(
              replacedRange: SovereignSourceRange(3, 5),
              replacementText: 'Z',
            ),
          ],
        ),
      );

      expect(next.markdown, 'aXXbcZf');
      expect(next.selection, const SovereignSelection.collapsed(4));
    });

    test('maps selections inside later operations with prior deltas', () {
      final state = SovereignEditorState.fromMarkdown(
        'abcdef',
        selection: const SovereignSelection.collapsed(4),
      );

      final next = state.applyTransaction(
        SovereignTransaction(
          operations: [
            SovereignSourceOperation.insert(1, 'XX'),
            const SovereignSourceOperation.replace(
              replacedRange: SovereignSourceRange(3, 5),
              replacementText: 'Z',
            ),
          ],
        ),
      );

      expect(next.markdown, 'aXXbcZf');
      expect(next.selection, const SovereignSelection.collapsed(6));
    });

    test('supports explicit upstream mapping for insertion boundaries', () {
      final operation = SovereignSourceOperation.insert(2, '**');

      expect(
        operation.mapOffset(2, affinity: SovereignMapAffinity.upstream),
        2,
      );
      expect(
        operation.mapOffset(2, affinity: SovereignMapAffinity.downstream),
        4,
      );
    });

    test('exposes transaction offset mapping for downstream projections', () {
      final transaction = SovereignTransaction(
        operations: [
          SovereignSourceOperation.insert(1, 'XX'),
          const SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(4, 5),
            replacementText: 'Z',
          ),
        ],
      );

      expect(transaction.mapOffset(0), 0);
      expect(transaction.mapOffset(1), 3);
      expect(
        transaction.mapOffset(1, affinity: SovereignMapAffinity.upstream),
        1,
      );
      expect(transaction.mapOffset(5), 7);
    });

    test('source ranges support containment, intersection, and union', () {
      const range = SovereignSourceRange(2, 6);

      expect(range.containsOffset(2), isTrue);
      expect(range.containsRange(const SovereignSourceRange(3, 5)), isTrue);
      expect(range.intersects(const SovereignSourceRange(5, 8)), isTrue);
      expect(range.intersects(const SovereignSourceRange(6, 8)), isFalse);
      expect(
        range.union(const SovereignSourceRange(8, 10)),
        const SovereignSourceRange(2, 10),
      );
    });

    test('inverts a transaction back to the original markdown', () {
      final state = SovereignEditorState.fromMarkdown(
        'abcdef',
        selection: const SovereignSelection.collapsed(3),
      );
      final transaction = SovereignTransaction.single(
        const SovereignSourceOperation.replace(
          replacedRange: SovereignSourceRange(1, 4),
          replacementText: 'XY',
        ),
        selectionBefore: state.selection,
        selectionAfter: const SovereignSelection.collapsed(3),
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
      final state = SovereignEditorState.fromMarkdown('abcdef');
      final transaction = SovereignTransaction(
        operations: [
          const SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(1, 4),
            replacementText: 'X',
          ),
          const SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(3, 5),
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
