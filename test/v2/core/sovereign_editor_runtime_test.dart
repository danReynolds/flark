import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('SovereignEditorRuntime', () {
    test('dispatches extension commands and records history', () {
      final runtime = SovereignEditorRuntime.fromMarkdown(
        'hello',
        extensions: SovereignExtensionSet([
          const SovereignCoreEditingExtension(),
        ]),
      );

      final result = runtime.dispatch(
        command: SovereignCoreEditingCommands.insertText,
        payload: const SovereignInsertTextPayload('!'),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(result.runtime.state.markdown, 'hello!');
      expect(result.runtime.canUndo, isTrue);
      expect(runtime.state.markdown, 'hello');
    });

    test('does not mutate runtime for rejected commands', () {
      final runtime = SovereignEditorRuntime.fromMarkdown(
        'hello',
        extensions: SovereignExtensionSet([
          const SovereignMarkdownInlineEditingExtension(),
        ]),
      );

      final result = runtime.dispatch(
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      );

      expect(result.commandResult.isRejected, isTrue);
      expect(identical(result.runtime, runtime), isTrue);
      expect(result.runtime.state.markdown, 'hello');
    });

    test('undoes and redoes runtime changes', () {
      var runtime = SovereignEditorRuntime.fromMarkdown(
        '',
        extensions: SovereignExtensionSet([
          const SovereignCoreEditingExtension(),
        ]),
      );

      runtime = runtime
          .dispatch(
            command: SovereignCoreEditingCommands.insertText,
            payload: const SovereignInsertTextPayload('a'),
          )
          .runtime;
      runtime = runtime
          .dispatch(
            command: SovereignCoreEditingCommands.insertText,
            payload: const SovereignInsertTextPayload('b'),
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
