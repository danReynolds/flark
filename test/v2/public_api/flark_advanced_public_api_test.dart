import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  test('flark_advanced exposes the full public surface', () {
    final controller = FlarkFlutterController.fromMarkdown(
      'hello',
      extensions: FlarkExtensionSet([
        const FlarkCoreEditingExtension(),
        const FlarkMarkdownInlineEditingExtension(),
        const FlarkMarkdownLinkEditingExtension(),
        const FlarkMarkdownTableEditingExtension(),
      ]),
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
    final commands = controller.commands;
    expect(commands.toggleStrong().commandResult.isHandled, isTrue);
    expect(controller.markdown, '**hello**!');
    expect(commands.strongActive, isTrue);
    expect(
      FlarkProjection(textLength: controller.markdown.length),
      isA<FlarkProjection>(),
    );
    expect(FlarkMarkdownLinkCommands.insertLink.id, 'markdown.insertLink');
    expect(FlarkMarkdownLinkCommands.removeLink.id, 'markdown.removeLink');
    expect(FlarkMarkdownTableCommands.insertTable.id, 'markdown.insertTable');
    expect(
      FlarkMarkdownBlockCommands.toggleOrderedList.id,
      'markdown.toggleOrderedList',
    );
    expect(FlarkMarkdownInputCommands.handleEnter.id, 'markdown.handleEnter');
    expect(
      FlarkMarkdownInputCommands.handleBackspace.id,
      'markdown.handleBackspace',
    );
    expect(
      FlarkMarkdownBlockCommands.setFenceLanguage.id,
      'markdown.setFenceLanguage',
    );
    expect(
      FlarkMarkdownBlockCommands.setTaskListChecked.id,
      'markdown.setTaskListChecked',
    );
    expect(FlarkNativeComrakParseBackend, isA<Type>());
    expect(NativeComrakBridgePreflightResult.available().isAvailable, isTrue);
    expect(FlarkMarkdownEditor, isA<Type>());
    expect(
      const FlarkMarkdownInteractionConfig(),
      isA<FlarkMarkdownInteractionConfig>(),
    );
    expect(
      const FlarkCodeLanguageOption(value: 'dart', label: 'Dart'),
      isA<FlarkCodeLanguageOption>(),
    );
    expect(Markdown, isA<Type>());
    expect(FlarkRenderPlanExtension, isA<Type>());
    expect(FlarkRenderPlanContext, isA<Type>());
    expect(FlarkPreviewBlockWidgetBuilder, isA<Type>());
    expect(FlarkMarkdownEditingExtensions.standard(), isA<FlarkExtensionSet>());
  });

  test('flark_advanced keeps a deliberate barrel shape', () {
    final barrel = File('lib/flark_advanced.dart').readAsStringSync();

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
    expect(barrel, isNot(contains('FlarkMarkdownField')));
    expect(barrel, isNot(contains('FlarkMarkdownPreview')));
    expect(barrel, isNot(contains('FlarkReadOnlyPreview')));
    expect(barrel, isNot(contains('FlarkEditableText')));
    expect(barrel, isNot(contains('FlarkProjectedEditableText')));
    expect(barrel, isNot(contains('FlarkLiveRenderedEditableText')));
    expect(barrel, isNot(contains('FlarkParseScheduler')));
    expect(barrel, isNot(contains('FlarkRenderPlanOverlayControls')));
    expect(barrel, isNot(contains('FlarkTextDeltaAdapter')));
  });
}
