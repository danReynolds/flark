import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';

void main() {
  group('FlarkCommandRegistry', () {
    const insertText = FlarkCommand<String>('test.insertText');
    const maybeInsert = FlarkCommand<String>('test.maybeInsert');

    test('returns not handled when no command handler is registered', () {
      final state = FlarkEditorState.fromMarkdown('');
      final result = const FlarkCommandRegistry().dispatch(
        state: state,
        command: insertText,
        payload: 'a',
      );

      expect(result.isNotHandled, isTrue);
    });

    test('dispatches the highest-priority handled command', () {
      final state = FlarkEditorState.fromMarkdown('');
      final registry = const FlarkCommandRegistry()
          .register<String>(
            insertText,
            (context) => FlarkCommandResult.handled(
              transaction: FlarkTransaction.single(
                FlarkSourceOperation.insert(0, 'low'),
              ),
            ),
            priority: FlarkCommandPriority.normal,
          )
          .register<String>(
            insertText,
            (context) => FlarkCommandResult.handled(
              transaction: FlarkTransaction.single(
                FlarkSourceOperation.insert(0, context.payload),
              ),
            ),
            priority: FlarkCommandPriority.high,
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

    test('payload-type mismatches fall through to typed handlers', () {
      final state = FlarkEditorState.fromMarkdown('');
      const intCommand = FlarkCommand<int>('shared.command');
      const stringCommand = FlarkCommand<String>('shared.command');
      final registry = const FlarkCommandRegistry()
          .register<int>(
            intCommand,
            (context) => FlarkCommandResult.handled(
              transaction: FlarkTransaction.single(
                FlarkSourceOperation.insert(0, 'int'),
              ),
            ),
            priority: FlarkCommandPriority.high,
          )
          .register<String>(
            stringCommand,
            (context) => FlarkCommandResult.handled(
              transaction: FlarkTransaction.single(
                FlarkSourceOperation.insert(0, context.payload),
              ),
            ),
            priority: FlarkCommandPriority.normal,
          );

      // The higher-priority int handler does not apply to a String payload;
      // it must not terminally reject and shadow the typed handler below.
      final result = registry.dispatch(
        state: state,
        command: stringCommand,
        payload: 'typed',
      );

      expect(result.isHandled, isTrue);
      expect(state.applyTransaction(result.transaction!).markdown, 'typed');
    });

    test('falls through not-handled handlers by priority order', () {
      final state = FlarkEditorState.fromMarkdown('');
      final registry = const FlarkCommandRegistry()
          .register<String>(
            maybeInsert,
            (context) => const FlarkCommandResult.notHandled(),
            priority: FlarkCommandPriority.high,
          )
          .register<String>(
            maybeInsert,
            (context) => FlarkCommandResult.handled(
              transaction: FlarkTransaction.single(
                FlarkSourceOperation.insert(0, context.payload),
              ),
            ),
            priority: FlarkCommandPriority.normal,
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
      final state = FlarkEditorState.fromMarkdown('');
      final registry = const FlarkCommandRegistry()
          .register<String>(
            insertText,
            (context) => FlarkCommandResult.rejected('blocked'),
            priority: FlarkCommandPriority.high,
          )
          .register<String>(
            insertText,
            (context) => FlarkCommandResult.handled(
              transaction: FlarkTransaction.single(
                FlarkSourceOperation.insert(0, context.payload),
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
