import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  test('sovereign_editor_v2 exposes the v2 public surface', () {
    final controller = SovereignFlutterController.fromMarkdown(
      'hello',
      extensions: SovereignExtensionSet([
        const SovereignCoreEditingExtension(),
        const SovereignMarkdownInlineEditingExtension(),
        const SovereignMarkdownLinkEditingExtension(),
        const SovereignMarkdownTableEditingExtension(),
      ]),
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
    expect(
      SovereignProjection(textLength: controller.markdown.length),
      isA<SovereignProjection>(),
    );
    expect(SovereignMarkdownLinkCommands.insertLink.id, 'markdown.insertLink');
    expect(SovereignMarkdownLinkCommands.removeLink.id, 'markdown.removeLink');
    expect(
      SovereignMarkdownTableCommands.insertTable.id,
      'markdown.insertTable',
    );
    expect(
      SovereignMarkdownBlockCommands.toggleOrderedList.id,
      'markdown.toggleOrderedList',
    );
    expect(
      SovereignMarkdownInputCommands.handleEnter.id,
      'markdown.handleEnter',
    );
    expect(
      SovereignMarkdownInputCommands.handleBackspace.id,
      'markdown.handleBackspace',
    );
    expect(
      SovereignMarkdownBlockCommands.setFenceLanguage.id,
      'markdown.setFenceLanguage',
    );
    expect(
      SovereignMarkdownBlockCommands.setTaskListChecked.id,
      'markdown.setTaskListChecked',
    );
    expect(SovereignNativeComrakParseBackend, isA<Type>());
    expect(NativeComrakBridgePreflightResult.available().isAvailable, isTrue);
    expect(MarkdownEditor, isA<Type>());
    expect(
      const SovereignMarkdownInteractionConfig(),
      isA<SovereignMarkdownInteractionConfig>(),
    );
    expect(
      const SovereignCodeLanguageOption(value: 'dart', label: 'Dart'),
      isA<SovereignCodeLanguageOption>(),
    );
    expect(Markdown, isA<Type>());
    expect(SovereignRenderPlanExtension, isA<Type>());
    expect(SovereignRenderPlanContext, isA<Type>());
    expect(SovereignPreviewBlockWidgetBuilder, isA<Type>());
    expect(
      SovereignMarkdownEditingExtensions.standard(),
      isA<SovereignExtensionSet>(),
    );
  });

  test('sovereign_editor_v2 keeps a deliberate barrel shape', () {
    final barrel = File('lib/sovereign_editor_v2.dart').readAsStringSync();

    expect(barrel, contains("export 'src/v2/core/core.dart'\n    show"));
    expect(barrel, contains("export 'src/v2/flutter/flutter.dart'\n    show"));
    expect(
      barrel,
      contains("export 'src/v2/markdown/markdown.dart'\n    show"),
    );
    expect(
      barrel,
      contains("export 'src/v2/projection/projection.dart'\n    show"),
    );
    expect(
      barrel,
      contains("export 'src/v2/render_plan/render_plan.dart'\n    show"),
    );
    expect(barrel, isNot(contains("export 'src/v2/core/core.dart';")));
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
  });
}
