import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  group('SovereignMarkdownTableCommands', () {
    test('inserts a formatted GFM table', () {
      final result = _dispatch(
        markdown: 'alpha',
        selection: const SovereignSelection.collapsed(5),
        command: SovereignMarkdownTableCommands.insertTable,
        payload: const SovereignInsertTablePayload(),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(
        result.runtime.state.markdown,
        equals(
          'alpha\n\n'
          '| Header 1 | Header 2 |\n'
          '| -------- | -------- |\n'
          '|          |          |\n',
        ),
      );
      final bodyRowStart = result.runtime.state.markdown.lastIndexOf(
        '|          |          |',
      );
      expect(
        result.runtime.state.selection,
        SovereignSelection.collapsed(bodyRowStart + 2),
      );
    });

    test('inserts a row below the current table row', () {
      final source = '| A | B |\n| - | - |\n| x | y |';
      final result = _dispatch(
        markdown: source,
        selection: SovereignSelection.collapsed(source.indexOf('x')),
        command: SovereignMarkdownTableCommands.insertRowBelow,
        payload: const SovereignTableMutationPayload(),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(
        result.runtime.state.markdown,
        equals(
          '| A   | B   |\n'
          '| --- | --- |\n'
          '| x   | y   |\n'
          '|     |     |',
        ),
      );
      final insertedRowStart = result.runtime.state.markdown.lastIndexOf(
        '|     |     |',
      );
      expect(
        result.runtime.state.selection,
        SovereignSelection.collapsed(insertedRowStart + 2),
      );
    });

    test('inserts a column to the right of the current cell', () {
      final source = '| A | B |\n| - | - |\n| x | y |';
      final result = _dispatch(
        markdown: source,
        selection: SovereignSelection.collapsed(source.indexOf('x')),
        command: SovereignMarkdownTableCommands.insertColumnRight,
        payload: const SovereignTableMutationPayload(),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(
        result.runtime.state.markdown,
        equals(
          '| A   |     | B   |\n'
          '| --- | --- | --- |\n'
          '| x   |     | y   |',
        ),
      );
      final bodyRowStart = result.runtime.state.markdown.lastIndexOf('| x');
      expect(
        result.runtime.state.selection,
        SovereignSelection.collapsed(bodyRowStart + 8),
      );
    });

    test('deletes the current table column while keeping two columns', () {
      final source = '| A | B | C |\n| - | - | - |\n| x | y | z |';
      final result = _dispatch(
        markdown: source,
        selection: SovereignSelection.collapsed(source.indexOf('y')),
        command: SovereignMarkdownTableCommands.deleteColumn,
        payload: const SovereignTableMutationPayload(),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(
        result.runtime.state.markdown,
        equals(
          '| A   | C   |\n'
          '| --- | --- |\n'
          '| x   | z   |',
        ),
      );
      final bodyRowStart = result.runtime.state.markdown.lastIndexOf('| x');
      expect(
        result.runtime.state.selection,
        SovereignSelection.collapsed(bodyRowStart + 8),
      );
    });

    test('deletes the current body row', () {
      final source = '| A | B |\n| - | - |\n| x | y |\n| q | r |';
      final result = _dispatch(
        markdown: source,
        selection: SovereignSelection.collapsed(source.indexOf('x')),
        command: SovereignMarkdownTableCommands.deleteRow,
        payload: const SovereignTableMutationPayload(),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(
        result.runtime.state.markdown,
        equals(
          '| A   | B   |\n'
          '| --- | --- |\n'
          '| q   | r   |',
        ),
      );
      final bodyRowStart = result.runtime.state.markdown.lastIndexOf('| q');
      expect(
        result.runtime.state.selection,
        SovereignSelection.collapsed(bodyRowStart + 2),
      );
    });

    test('row and column mutations fall through outside established tables',
        () {
      final source = 'not | a | table';
      final result = _dispatch(
        markdown: source,
        selection: const SovereignSelection.collapsed(6),
        command: SovereignMarkdownTableCommands.insertRowBelow,
        payload: const SovereignTableMutationPayload(),
      );

      expect(result.commandResult.isNotHandled, isTrue);
      expect(result.runtime.state.markdown, source);
    });

    test('row and column mutations fall through inside fenced code', () {
      const source = '```\n| A | B |\n| - | - |\n```';
      final result = _dispatch(
        markdown: source,
        selection: SovereignSelection.collapsed(source.indexOf('A')),
        command: SovereignMarkdownTableCommands.insertColumnRight,
        payload: const SovereignTableMutationPayload(),
      );

      expect(result.commandResult.isNotHandled, isTrue);
      expect(result.runtime.state.markdown, source);
    });
  });
}

SovereignEditorRuntimeResult _dispatch<TPayload>({
  required String markdown,
  required SovereignSelection selection,
  required SovereignCommand<TPayload> command,
  required TPayload payload,
}) {
  final runtime = SovereignEditorRuntime(
    state: SovereignEditorState.fromMarkdown(
      markdown,
      selection: selection,
    ),
    commandRegistry: SovereignExtensionSet(
      const [SovereignMarkdownTableEditingExtension()],
    ).commandRegistry(),
  );
  return runtime.dispatch(command: command, payload: payload);
}
