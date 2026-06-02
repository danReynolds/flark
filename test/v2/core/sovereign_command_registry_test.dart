import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';

void main() {
  group('SovereignCommandRegistry', () {
    const insertText = SovereignCommand<String>('test.insertText');
    const maybeInsert = SovereignCommand<String>('test.maybeInsert');

    test('returns not handled when no command handler is registered', () {
      final state = SovereignEditorState.fromMarkdown('');
      final result = const SovereignCommandRegistry().dispatch(
        state: state,
        command: insertText,
        payload: 'a',
      );

      expect(result.isNotHandled, isTrue);
    });

    test('dispatches the highest-priority handled command', () {
      final state = SovereignEditorState.fromMarkdown('');
      final registry = const SovereignCommandRegistry()
          .register<String>(
            insertText,
            (context) => SovereignCommandResult.handled(
              transaction: SovereignTransaction.single(
                SovereignSourceOperation.insert(0, 'low'),
              ),
            ),
            priority: SovereignCommandPriority.normal,
          )
          .register<String>(
            insertText,
            (context) => SovereignCommandResult.handled(
              transaction: SovereignTransaction.single(
                SovereignSourceOperation.insert(0, context.payload),
              ),
            ),
            priority: SovereignCommandPriority.high,
          );

      final result = registry.dispatch(
        state: state,
        command: insertText,
        payload: 'high',
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'high');
    });

    test('falls through not-handled handlers by priority order', () {
      final state = SovereignEditorState.fromMarkdown('');
      final registry = const SovereignCommandRegistry()
          .register<String>(
            maybeInsert,
            (context) => const SovereignCommandResult.notHandled(),
            priority: SovereignCommandPriority.high,
          )
          .register<String>(
            maybeInsert,
            (context) => SovereignCommandResult.handled(
              transaction: SovereignTransaction.single(
                SovereignSourceOperation.insert(0, context.payload),
              ),
            ),
            priority: SovereignCommandPriority.normal,
          );

      final result = registry.dispatch(
        state: state,
        command: maybeInsert,
        payload: 'fallback',
      );

      expect(result.isHandled, isTrue);
      expect(state.applyTransaction(result.transaction!).markdown, 'fallback');
    });

    test('rejected handlers stop dispatch', () {
      final state = SovereignEditorState.fromMarkdown('');
      final registry = const SovereignCommandRegistry()
          .register<String>(
            insertText,
            (context) => SovereignCommandResult.rejected('blocked'),
            priority: SovereignCommandPriority.high,
          )
          .register<String>(
            insertText,
            (context) => SovereignCommandResult.handled(
              transaction: SovereignTransaction.single(
                SovereignSourceOperation.insert(0, context.payload),
              ),
            ),
          );

      final result = registry.dispatch(
        state: state,
        command: insertText,
        payload: 'ignored',
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, 'blocked');
      expect(result.transaction, isNull);
    });
  });
}
