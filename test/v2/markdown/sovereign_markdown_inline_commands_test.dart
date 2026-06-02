import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('SovereignMarkdownInlineCommands', () {
    test('wraps a selected source range with style markers', () {
      final state = SovereignEditorState.fromMarkdown(
        'hello world',
        selection: const SovereignSelection(baseOffset: 6, extentOffset: 11),
      );
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(
        result.transaction!.metadata.projectionInvalidationRange,
        const SovereignSourceRange(6, 11),
      );
      expect(next.markdown, 'hello **world**');
      expect(next.selection,
          const SovereignSelection(baseOffset: 8, extentOffset: 13));
    });

    test('unwraps markers around a selected source range', () {
      final state = SovereignEditorState.fromMarkdown(
        '**world**',
        selection: const SovereignSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'world');
      expect(next.selection,
          const SovereignSelection(baseOffset: 0, extentOffset: 5));
    });

    test('unwraps markers included in the selected source range', () {
      final state = SovereignEditorState.fromMarkdown(
        '**world**',
        selection: const SovereignSelection(baseOffset: 0, extentOffset: 9),
      );
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, 'world');
      expect(next.selection,
          const SovereignSelection(baseOffset: 0, extentOffset: 5));
    });

    test('wraps selected text with inline code markers', () {
      final state = SovereignEditorState.fromMarkdown(
        'use code',
        selection: const SovereignSelection(baseOffset: 4, extentOffset: 8),
      );
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.inlineCode,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(next.markdown, 'use `code`');
      expect(next.selection,
          const SovereignSelection(baseOffset: 5, extentOffset: 9));
    });

    test('rejects selections that include only the opening marker', () {
      final state = SovereignEditorState.fromMarkdown(
        '**world**',
        selection: const SovereignSelection(baseOffset: 0, extentOffset: 7),
      );
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('partially overlap'));
      expect(result.transaction, isNull);
    });

    test('rejects selections with only one surrounding marker', () {
      final state = SovereignEditorState.fromMarkdown(
        '**world',
        selection: const SovereignSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('partially overlap'));
    });

    test('does not unwrap escaped surrounding markers', () {
      final state = SovereignEditorState.fromMarkdown(
        r'\*world\*',
        selection: const SovereignSelection(baseOffset: 2, extentOffset: 7),
      );
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.emphasis,
        ),
      );
      final next = state.applyTransaction(result.transaction!);

      expect(result.isHandled, isTrue);
      expect(next.markdown, r'\**world*\*');
      expect(next.selection,
          const SovereignSelection(baseOffset: 3, extentOffset: 8));
    });

    test('rejects collapsed selections until active mark state exists', () {
      final state = SovereignEditorState.fromMarkdown('world');
      final registry = SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry();

      final result = registry.dispatch(
        state: state,
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.emphasis,
        ),
      );

      expect(result.isRejected, isTrue);
      expect(result.reason, contains('selected source range'));
      expect(result.transaction, isNull);
    });
  });
}
