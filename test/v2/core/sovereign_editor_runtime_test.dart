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
  });
}
