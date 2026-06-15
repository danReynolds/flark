import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkMarkdownInlineCommands', () {
    test('wraps a selected source range with style markers', () {
      final state = FlarkEditorState.fromMarkdown(
        'hello world',
        selection: const FlarkSelection(baseOffset: 6, extentOffset: 11),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(
        result.transaction!.metadata.projectionInvalidationRange,
        const FlarkSourceRange(6, 11),
      );
      expect(next.markdown, 'hello **world**');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 8, extentOffset: 13),
      );
    });

    test('unwraps markers around a selected source range', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world**',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'world');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 0, extentOffset: 5),
      );
    });

    test('unwraps markers included in the selected source range', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world**',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 9),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'world');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 0, extentOffset: 5),
      );
    });

    test('wraps selected text with inline code markers', () {
      final state = FlarkEditorState.fromMarkdown(
        'use code',
        selection: const FlarkSelection(baseOffset: 4, extentOffset: 8),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.inlineCode,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'use `code`');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 5, extentOffset: 9),
      );
    });

    test('rejects selections that include only the opening marker', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world**',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('partially overlap'));
      expect(result.transaction, isNull);
    });

    test('rejects selections with only one surrounding marker', () {
      final state = FlarkEditorState.fromMarkdown(
        '**world',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('partially overlap'));
    });

    test('toggling emphasis inside strong nests instead of stripping', () {
      // The inner '*' of '**bold**' must not pass as an emphasis pair: a
      // 2-run carries strong only. Toggling italic adds a layer.
      final state = FlarkEditorState.fromMarkdown(
        '**bold**',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 6),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '***bold***');
    });

    test('toggling emphasis off em+strong keeps the strong pair', () {
      final state = FlarkEditorState.fromMarkdown(
        '***bold***',
        selection: const FlarkSelection(baseOffset: 3, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '**bold**');
    });

    test('toggling strong off em+strong keeps the emphasis pair', () {
      final state = FlarkEditorState.fromMarkdown(
        '***bold***',
        selection: const FlarkSelection(baseOffset: 3, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, '*bold*');
    });

    test('does not unwrap escaped surrounding markers', () {
      final state = FlarkEditorState.fromMarkdown(
        r'\*world\*',
        selection: const FlarkSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, r'\**world*\*');
      expect(
        next.selection,
        const FlarkSelection(baseOffset: 3, extentOffset: 8),
      );
    });

    test('rejects collapsed selections until active mark state exists', () {
      final state = FlarkEditorState.fromMarkdown('world');
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.emphasis,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('selected source range'));
      expect(result.transaction, isNull);
    });

    test('unwraps the run around a collapsed caret inside it', () {
      final state = FlarkEditorState.fromMarkdown(
        'a **bold** b',
        selection: const FlarkSelection.collapsed(6),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'a bold b');
      // Caret stays over the same character (offset 6 was inside "bold").
      expect(next.selection, const FlarkSelection.collapsed(4));
    });

    test('does not unwrap when the collapsed caret is outside the run', () {
      final state = FlarkEditorState.fromMarkdown(
        '**bold** plain',
        selection: const FlarkSelection.collapsed(12),
      );
      final registry = FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
    });
  });
}
