import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark.dart';
import 'package:flark/flark_core.dart' as core;

void main() {
  test('top-level barrel exposes the promoted v2 app API', () {
    final controller = FlarkFlutterController.fromMarkdown(
      'hello',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);

    final result = controller.dispatch(
      command: FlarkCoreEditingCommands.insertText,
      payload: const FlarkInsertTextPayload('!'),
    );

    expect(result.commandResult.isHandled, isTrue);
    expect(controller.markdown, 'hello!');
    controller.applySelection(
      const FlarkSelection(baseOffset: 0, extentOffset: 5),
    );
    expect(controller.toggleStrong().commandResult.isHandled, isTrue);
    expect(controller.markdown, '**hello**!');
    expect(MarkdownEditor, isA<Type>());
    expect(Markdown, isA<Type>());
    expect(
      const FlarkMarkdownInteractionConfig(),
      isA<FlarkMarkdownInteractionConfig>(),
    );
    expect(
      const FlarkCodeLanguageOption(value: 'dart', label: 'Dart'),
      isA<FlarkCodeLanguageOption>(),
    );
    expect(
      FlarkMarkdownBlockCommands.setFenceLanguage.id,
      'markdown.setFenceLanguage',
    );
    expect(
      FlarkMarkdownBlockCommands.setTaskListChecked.id,
      'markdown.setTaskListChecked',
    );
    expect(FlarkMarkdownLinkCommands.removeLink.id, 'markdown.removeLink');
    expect(FlarkNativeComrakParseBackend, isA<Type>());
    expect(NativeComrakBridge, isA<Type>());
    expect(NativeComrakBridgePreflightResult.available().isAvailable, isTrue);
  });

  test('core barrel exposes headless v2 types without Flutter widgets', () {
    final state = core.FlarkEditorState.fromMarkdown('hello');
    final next = state.applyTransaction(
      core.FlarkTransaction.single(core.FlarkSourceOperation.insert(5, '!')),
    );

    expect(next.markdown, 'hello!');
    expect(core.FlarkRenderPlan, isA<Type>());
    expect(core.FlarkProjection, isA<Type>());
  });

  test(
    'top-level barrel exposes only the two promoted widget entry points',
    () {
      final barrel = File('lib/flark.dart').readAsStringSync();

      expect(barrel, contains('MarkdownEditor'));
      expect(barrel, contains('Markdown,'));
      expect(barrel, isNot(contains('FlarkMarkdownField')));
      expect(barrel, isNot(contains('FlarkMarkdownEditor')));
      expect(barrel, isNot(contains('FlarkMarkdownPreview')));
      expect(barrel, isNot(contains('FlarkReadOnlyPreview')));
      expect(barrel, isNot(contains('FlarkEditableText')));
      expect(barrel, isNot(contains('FlarkProjectedEditableText')));
      expect(barrel, isNot(contains('FlarkLiveRenderedEditableText')));
      expect(barrel, isNot(contains('FlarkParseScheduler')));
      expect(barrel, isNot(contains('FlarkRenderPlanOverlayControls')));
      expect(barrel, isNot(contains('FlarkTextDeltaAdapter')));
    },
  );

  testWidgets('top-level widgets render without legacy imports', (
    tester,
  ) async {
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title\n\n**bold**',
      extensions: FlarkMarkdownEditingExtensions.standard(),
    );
    final parseErrors = <Object>[];
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Column(
            children: [
              Expanded(
                child: MarkdownEditor(
                  controller: controller,
                  parseBackend: const _IdentityParseBackend(),
                  onParseError: (error, stackTrace) {
                    parseErrors.add(error);
                  },
                  editingMode: FlarkMarkdownEditingMode.source,
                ),
              ),
              Expanded(
                child: Markdown(
                  markdown: '# Preview',
                  parseBackend: const _IdentityParseBackend(),
                  onParseError: (error, stackTrace) {
                    parseErrors.add(error);
                  },
                ),
              ),
            ],
          ),
        ),
      ),
    );

    expect(find.byType(MarkdownEditor), findsOneWidget);
    expect(find.byType(Markdown), findsOneWidget);
    expect(parseErrors, isEmpty);
  });
}

final class _IdentityParseBackend implements FlarkMarkdownParseBackend {
  const _IdentityParseBackend();

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'identity-test',
        schemaVersion: 1,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(
    FlarkMarkdownParseRequest request,
  ) async {
    return FlarkMarkdownParseResult(
      schemaVersion: 1,
      revision: request.revision,
      sourceTextLength: request.markdown.length,
      blocks: const [],
      inlineTokens: const [],
    );
  }
}
