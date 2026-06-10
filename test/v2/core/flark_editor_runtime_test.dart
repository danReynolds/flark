import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkEditorRuntime', () {
    test('dispatches extension commands and records history', () {
      final runtime = FlarkEditorRuntime.fromMarkdown(
        'hello',
        extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
      );

      final result = runtime.dispatch(
        command: FlarkCoreEditingCommands.insertText,
        payload: const FlarkInsertTextPayload('!'),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(result.runtime.state.markdown, 'hello!');
      expect(result.runtime.canUndo, isTrue);
      expect(runtime.state.markdown, 'hello');
    });

    test('does not mutate runtime for rejected commands', () {
      final runtime = FlarkEditorRuntime.fromMarkdown(
        'hello',
        extensions: FlarkExtensionSet([
          const FlarkMarkdownInlineEditingExtension(),
        ]),
      );

      final result = runtime.dispatch(
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );

      expect(result.commandResult.isRejected, isTrue);
      expect(identical(result.runtime, runtime), isTrue);
      expect(result.runtime.state.markdown, 'hello');
    });

    test('undoes and redoes runtime changes', () {
      var runtime = FlarkEditorRuntime.fromMarkdown(
        '',
        extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
      );

      runtime = runtime
          .dispatch(
            command: FlarkCoreEditingCommands.insertText,
            payload: const FlarkInsertTextPayload('a'),
          )
          .runtime;
      runtime = runtime
          .dispatch(
            command: FlarkCoreEditingCommands.insertText,
            payload: const FlarkInsertTextPayload('b'),
          )
          .runtime;

      expect(runtime.state.markdown, 'ab');
      expect(runtime.canUndo, isTrue);

      final undone = runtime.undo().runtime;
      expect(undone.state.markdown, 'a');
      expect(undone.canRedo, isTrue);

      final redone = undone.redo().runtime;
      expect(redone.state.markdown, 'ab');
      expect(redone.canRedo, isFalse);
    });

    test('normalizes CRLF and CR line endings at document ingest', () {
      final runtime = FlarkEditorRuntime.fromMarkdown('a\r\nb\rc');
      expect(runtime.state.markdown, 'a\nb\nc');
    });

    test(
      'results expose the applied transactions for edits, undo, and redo',
      () {
        var runtime = FlarkEditorRuntime.fromMarkdown('ab');

        final transaction = FlarkTransaction.single(
          FlarkSourceOperation.insert(2, 'c'),
          selectionAfter: const FlarkSelection.collapsed(3),
        );
        final applyResult = runtime.applyTransaction(transaction);
        expect(applyResult.appliedTransactions, [transaction]);
        runtime = applyResult.runtime;

        final undoResult = runtime.undo();
        expect(undoResult.appliedTransactions, hasLength(1));
        expect(
          undoResult.appliedTransactions.single.operations.single.replacedRange,
          const FlarkSourceRange(2, 3),
        );
        expect(undoResult.runtime.state.markdown, 'ab');
        runtime = undoResult.runtime;

        final redoResult = runtime.redo();
        expect(redoResult.appliedTransactions, [transaction]);
        expect(redoResult.runtime.state.markdown, 'abc');

        // An exhausted stack applies nothing.
        expect(redoResult.runtime.redo().appliedTransactions, isEmpty);
      },
    );

    test(
      'undo and redo on an exhausted stack return the identical runtime',
      () {
        final runtime = FlarkEditorRuntime.fromMarkdown('abc');

        final undoResult = runtime.undo();
        expect(identical(undoResult.runtime, runtime), isTrue);
        expect(undoResult.appliedTransactions, isEmpty);

        final redoResult = runtime.redo();
        expect(identical(redoResult.runtime, runtime), isTrue);
        expect(redoResult.appliedTransactions, isEmpty);
      },
    );

    test('grouped entries expose all inverse transactions on undo', () {
      var runtime = FlarkEditorRuntime.fromMarkdown('x');

      runtime = runtime
          .applyTransaction(
            FlarkTransaction.single(
              FlarkSourceOperation.insert(1, 'y'),
              selectionAfter: const FlarkSelection.collapsed(2),
              undoGroupId: 3,
            ),
          )
          .runtime;
      runtime = runtime
          .applyTransaction(
            FlarkTransaction.single(
              FlarkSourceOperation.insert(2, 'z'),
              selectionAfter: const FlarkSelection.collapsed(3),
              undoGroupId: 3,
            ),
          )
          .runtime;
      expect(runtime.state.markdown, 'xyz');

      final undoResult = runtime.undo();
      expect(undoResult.appliedTransactions, hasLength(2));
      expect(undoResult.runtime.state.markdown, 'x');
    });
  });
}
