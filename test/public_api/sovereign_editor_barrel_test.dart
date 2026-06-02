import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';
import 'package:sovereign_editor/sovereign_editor_core.dart' as core;

void main() {
  test('top-level barrel exposes the promoted v2 app API', () {
    final controller = SovereignFlutterController.fromMarkdown(
      'hello',
      extensions: SovereignMarkdownEditingExtensions.standard(),
    );
    addTearDown(controller.dispose);

    final result = controller.dispatch(
      command: SovereignCoreEditingCommands.insertText,
      payload: const SovereignInsertTextPayload('!'),
    );

    expect(result.commandResult.isHandled, isTrue);
    expect(controller.markdown, 'hello!');
    controller.applySelection(
      const SovereignSelection(baseOffset: 0, extentOffset: 5),
    );
    expect(controller.toggleStrong().commandResult.isHandled, isTrue);
    expect(controller.markdown, '**hello**!');
    expect(MarkdownEditor, isA<Type>());
    expect(Markdown, isA<Type>());
    expect(
      const SovereignMarkdownInteractionConfig(),
      isA<SovereignMarkdownInteractionConfig>(),
    );
    expect(
      const SovereignCodeLanguageOption(value: 'dart', label: 'Dart'),
      isA<SovereignCodeLanguageOption>(),
    );
    expect(
      SovereignMarkdownBlockCommands.setFenceLanguage.id,
      'markdown.setFenceLanguage',
    );
    expect(
      SovereignMarkdownBlockCommands.setTaskListChecked.id,
      'markdown.setTaskListChecked',
    );
    expect(SovereignMarkdownLinkCommands.removeLink.id, 'markdown.removeLink');
    expect(SovereignNativeComrakParseBackend, isA<Type>());
    expect(NativeComrakBridge, isA<Type>());
    expect(NativeComrakBridgePreflightResult.available().isAvailable, isTrue);
  });

  test('core barrel exposes headless v2 types without Flutter widgets', () {
    final state = core.SovereignEditorState.fromMarkdown('hello');
    final next = state.applyTransaction(
      core.SovereignTransaction.single(
        core.SovereignSourceOperation.insert(5, '!'),
      ),
    );

    expect(next.markdown, 'hello!');
    expect(core.SovereignRenderPlan, isA<Type>());
    expect(core.SovereignProjection, isA<Type>());
  });

  test(
    'top-level barrel exposes only the two promoted widget entry points',
    () {
      final barrel = File('lib/sovereign_editor.dart').readAsStringSync();

      expect(barrel, contains('MarkdownEditor'));
      expect(barrel, contains('Markdown,'));
      expect(barrel, isNot(contains('SovereignMarkdownField')));
      expect(barrel, isNot(contains('SovereignMarkdownEditor')));
      expect(barrel, isNot(contains('SovereignMarkdownPreview')));
      expect(barrel, isNot(contains('SovereignReadOnlyPreview')));
      expect(barrel, isNot(contains('SovereignEditableText')));
      expect(barrel, isNot(contains('SovereignProjectedEditableText')));
      expect(barrel, isNot(contains('SovereignLiveRenderedEditableText')));
      expect(barrel, isNot(contains('SovereignParseScheduler')));
      expect(barrel, isNot(contains('SovereignRenderPlanOverlayControls')));
      expect(barrel, isNot(contains('SovereignTextDeltaAdapter')));
    },
  );

  testWidgets('top-level widgets render without legacy imports', (
    tester,
  ) async {
    final controller = SovereignFlutterController.fromMarkdown(
      '# Title\n\n**bold**',
      extensions: SovereignMarkdownEditingExtensions.standard(),
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
                  editingMode: SovereignMarkdownEditingMode.source,
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

final class _IdentityParseBackend implements SovereignMarkdownParseBackend {
  const _IdentityParseBackend();

  @override
  SovereignMarkdownParserCapabilities get capabilities =>
      SovereignMarkdownParserCapabilities(
        parserName: 'identity-test',
        schemaVersion: 1,
        supportedProfiles: const [SovereignMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<SovereignMarkdownParseResult> parse(
    SovereignMarkdownParseRequest request,
  ) async {
    return SovereignMarkdownParseResult(
      schemaVersion: 1,
      revision: request.revision,
      sourceTextLength: request.markdown.length,
      blocks: const [],
      inlineTokens: const [],
    );
  }
}
