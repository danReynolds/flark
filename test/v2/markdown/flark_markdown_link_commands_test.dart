import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  group('FlarkMarkdownLinkCommands', () {
    test('resolves an existing link around the caret', () {
      final state = FlarkEditorState.fromMarkdown(
        'See [docs](https://a.b).',
        selection: const FlarkSelection.collapsed(8),
      );

      final context = FlarkMarkdownLinkCommands.resolveLinkEditContext(state);

      expect(context.isExisting, isTrue);
      expect(context.label, 'docs');
      expect(context.url, 'https://a.b');
      expect(context.replaceRange, const FlarkSourceRange(4, 23));
    });

    test('resolves selected text as a new link label', () {
      final state = FlarkEditorState.fromMarkdown(
        'hello world',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 5),
      );

      final context = FlarkMarkdownLinkCommands.resolveLinkEditContext(state);

      expect(context.isExisting, isFalse);
      expect(context.label, 'hello');
      expect(context.url, 'https://');
      expect(context.replaceRange, const FlarkSourceRange(0, 5));
    });

    test('resolves an exact selected markdown link', () {
      const text = '[docs](https://a.b)';
      final state = FlarkEditorState.fromMarkdown(
        text,
        selection: const FlarkSelection(
          baseOffset: 0,
          extentOffset: text.length,
        ),
      );

      final context = FlarkMarkdownLinkCommands.resolveLinkEditContext(state);

      expect(context.isExisting, isTrue);
      expect(context.label, 'docs');
      expect(context.url, 'https://a.b');
      expect(context.replaceRange, FlarkSourceRange(0, text.length));
    });

    test('applies a link edit over an existing link', () {
      final result = _dispatchApply(
        markdown: 'See [docs](https://a.b).',
        selection: const FlarkSelection.collapsed(8),
        label: 'guide',
        url: 'https://example.com',
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(
        result.runtime.state.markdown,
        'See [guide](https://example.com).',
      );
      expect(
        result.runtime.state.selection,
        FlarkSelection.collapsed('See [guide](https://example.com)'.length),
      );
    });

    test('applies a new link at a collapsed caret', () {
      final result = _dispatchApply(
        markdown: 'See ',
        selection: const FlarkSelection.collapsed(4),
        label: 'docs',
        url: 'https://example.com',
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(result.runtime.state.markdown, 'See [docs](https://example.com)');
      expect(
        result.runtime.state.selection,
        FlarkSelection.collapsed('See [docs](https://example.com)'.length),
      );
    });

    test('rejects empty urls without changing source', () {
      final result = _dispatchApply(
        markdown: 'See ',
        selection: const FlarkSelection.collapsed(4),
        label: 'docs',
        url: '   ',
      );

      expect(result.commandResult.isRejected, isTrue);
      expect(result.runtime.state.markdown, 'See ');
    });

    test('defaults a blank label to link', () {
      final result = _dispatchApply(
        markdown: 'See ',
        selection: const FlarkSelection.collapsed(4),
        label: '   ',
        url: 'https://example.com',
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(result.runtime.state.markdown, 'See [link](https://example.com)');
    });

    test('inserts an empty-url markdown link and selects the url slot', () {
      final result = _dispatch(
        markdown: 'See ',
        selection: const FlarkSelection.collapsed(4),
        command: FlarkMarkdownLinkCommands.insertLink,
        payload: const FlarkInsertLinkPayload(),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(result.runtime.state.markdown, 'See [link text]()');
      expect(
        result.runtime.state.selection,
        const FlarkSelection(baseOffset: 17, extentOffset: 17),
      );
    });

    test('uses selected text as the inserted link label', () {
      final result = _dispatch(
        markdown: 'hello world',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 5),
        command: FlarkMarkdownLinkCommands.insertLink,
        payload: const FlarkInsertLinkPayload(),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(result.runtime.state.markdown, '[hello]() world');
      expect(
        result.runtime.state.selection,
        const FlarkSelection(baseOffset: 9, extentOffset: 9),
      );
    });

    test('removes a markdown link while preserving its label', () {
      final result = _dispatch(
        markdown: 'See [Docs](https://example.com) now',
        selection: const FlarkSelection.collapsed(10),
        command: FlarkMarkdownLinkCommands.removeLink,
        payload: const FlarkRemoveLinkPayload(
          linkRange: FlarkSourceRange(4, 31),
        ),
      );

      expect(result.commandResult.isHandled, isTrue);
      expect(result.runtime.state.markdown, 'See Docs now');
      expect(result.runtime.state.selection, const FlarkSelection.collapsed(8));
    });

    test('rejects remove link when the range is not a markdown link', () {
      final result = _dispatch(
        markdown: 'See Docs',
        selection: const FlarkSelection.collapsed(4),
        command: FlarkMarkdownLinkCommands.removeLink,
        payload: const FlarkRemoveLinkPayload(
          linkRange: FlarkSourceRange(4, 8),
        ),
      );

      expect(result.commandResult.isRejected, isTrue);
      expect(result.runtime.state.markdown, 'See Docs');
    });
  });
}

FlarkEditorRuntimeResult _dispatchApply({
  required String markdown,
  required FlarkSelection selection,
  required String label,
  required String url,
}) {
  final runtime = _runtime(markdown, selection);
  final context = FlarkMarkdownLinkCommands.resolveLinkEditContext(
    runtime.state,
  );
  return runtime.dispatch(
    command: FlarkMarkdownLinkCommands.applyLinkEdit,
    payload: FlarkApplyLinkEditPayload(
      context: context,
      label: label,
      url: url,
    ),
  );
}

FlarkEditorRuntimeResult _dispatch<TPayload>({
  required String markdown,
  required FlarkSelection selection,
  required FlarkCommand<TPayload> command,
  required TPayload payload,
}) {
  return _runtime(
    markdown,
    selection,
  ).dispatch(command: command, payload: payload);
}

FlarkEditorRuntime _runtime(String markdown, FlarkSelection selection) {
  return FlarkEditorRuntime(
    state: FlarkEditorState.fromMarkdown(markdown, selection: selection),
    commandRegistry: FlarkExtensionSet(const [
      FlarkMarkdownLinkEditingExtension(),
    ]).commandRegistry(),
  );
}
