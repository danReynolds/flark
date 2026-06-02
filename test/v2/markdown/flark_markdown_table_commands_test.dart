import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  group('FlarkMarkdownTableCommands', () {
    test('inserts a formatted GFM table', () {
      final result = _dispatch(
        markdown: 'alpha',
        selection: const FlarkSelection.collapsed(5),
        command: FlarkMarkdownTableCommands.insertTable,
        payload: const FlarkInsertTablePayload(),
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
        FlarkSelection.collapsed(bodyRowStart + 2),
      );
    });

    test('inserts a row below the current table row', () {
      final source = '| A | B |\n| - | - |\n| x | y |';
      final result = _dispatch(
        markdown: source,
        selection: FlarkSelection.collapsed(source.indexOf('x')),
        command: FlarkMarkdownTableCommands.insertRowBelow,
        payload: const FlarkTableMutationPayload(),
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
        FlarkSelection.collapsed(insertedRowStart + 2),
      );
    });

    test('inserts a column to the right of the current cell', () {
      final source = '| A | B |\n| - | - |\n| x | y |';
      final result = _dispatch(
        markdown: source,
        selection: FlarkSelection.collapsed(source.indexOf('x')),
        command: FlarkMarkdownTableCommands.insertColumnRight,
        payload: const FlarkTableMutationPayload(),
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
        FlarkSelection.collapsed(bodyRowStart + 8),
      );
    });

    test('deletes the current table column while keeping two columns', () {
      final source = '| A | B | C |\n| - | - | - |\n| x | y | z |';
      final result = _dispatch(
        markdown: source,
        selection: FlarkSelection.collapsed(source.indexOf('y')),
        command: FlarkMarkdownTableCommands.deleteColumn,
        payload: const FlarkTableMutationPayload(),
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
        FlarkSelection.collapsed(bodyRowStart + 8),
      );
    });

    test('deletes the current body row', () {
      final source = '| A | B |\n| - | - |\n| x | y |\n| q | r |';
      final result = _dispatch(
        markdown: source,
        selection: FlarkSelection.collapsed(source.indexOf('x')),
        command: FlarkMarkdownTableCommands.deleteRow,
        payload: const FlarkTableMutationPayload(),
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
        FlarkSelection.collapsed(bodyRowStart + 2),
      );
    });

    test(
      'row and column mutations fall through outside established tables',
      () {
        final source = 'not | a | table';
        final result = _dispatch(
          markdown: source,
          selection: const FlarkSelection.collapsed(6),
          command: FlarkMarkdownTableCommands.insertRowBelow,
          payload: const FlarkTableMutationPayload(),
        );

        expect(result.commandResult.isNotHandled, isTrue);
        expect(result.runtime.state.markdown, source);
      },
    );

    test('row and column mutations fall through inside fenced code', () {
      const source = '```\n| A | B |\n| - | - |\n```';
      final result = _dispatch(
        markdown: source,
        selection: FlarkSelection.collapsed(source.indexOf('A')),
        command: FlarkMarkdownTableCommands.insertColumnRight,
        payload: const FlarkTableMutationPayload(),
      );

      expect(result.commandResult.isNotHandled, isTrue);
      expect(result.runtime.state.markdown, source);
    });
  });
}

FlarkEditorRuntimeResult _dispatch<TPayload>({
  required String markdown,
  required FlarkSelection selection,
  required FlarkCommand<TPayload> command,
  required TPayload payload,
}) {
  final runtime = FlarkEditorRuntime(
    state: FlarkEditorState.fromMarkdown(markdown, selection: selection),
    commandRegistry: FlarkExtensionSet(const [
      FlarkMarkdownTableEditingExtension(),
    ]).commandRegistry(),
  );
  return runtime.dispatch(command: command, payload: payload);
}
