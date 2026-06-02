import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('SovereignCommandActions', () {
    testWidgets('invokes typed commands through Flutter Actions',
        (tester) async {
      final controller = _markdownController();
      late BuildContext actionContext;
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignCommandActions(
            controller: controller,
            child: Builder(
              builder: (context) {
                actionContext = context;
                return const SizedBox();
              },
            ),
          ),
        ),
      );

      final result = Actions.invoke(
        actionContext,
        const SovereignCommandIntent(
          SovereignTypedCommandInvocation(
            command: SovereignMarkdownInlineCommands.toggleInlineStyle,
            payload: SovereignToggleInlineStylePayload(
              SovereignMarkdownInlineStyle.strong,
            ),
          ),
        ),
      ) as SovereignEditorRuntimeResult;

      expect(result.commandResult.isHandled, isTrue);
      expect(controller.markdown, '**bold**');
    });

    testWidgets(
        'SovereignEditableText installs command actions for descendants',
        (tester) async {
      final controller = _markdownController();
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignEditableText(
            controller: controller,
            shortcuts: const {
              SingleActivator(LogicalKeyboardKey.keyB, meta: true):
                  SovereignCommandIntent(
                SovereignTypedCommandInvocation(
                  command: SovereignMarkdownInlineCommands.toggleInlineStyle,
                  payload: SovereignToggleInlineStylePayload(
                    SovereignMarkdownInlineStyle.strong,
                  ),
                ),
              ),
            },
          ),
        ),
      );

      final result = Actions.invoke(
        tester.element(find.byType(EditableText)),
        const SovereignCommandIntent(
          SovereignTypedCommandInvocation(
            command: SovereignMarkdownInlineCommands.toggleInlineStyle,
            payload: SovereignToggleInlineStylePayload(
              SovereignMarkdownInlineStyle.strong,
            ),
          ),
        ),
      ) as SovereignEditorRuntimeResult;

      expect(result.commandResult.isHandled, isTrue);
      expect(controller.markdown, '**bold**');
      expect(find.byType(Shortcuts), findsOneWidget);
    });
  });
}

SovereignFlutterController _markdownController() {
  return SovereignFlutterController(
    runtime: SovereignEditorRuntime(
      state: SovereignEditorState.fromMarkdown(
        'bold',
        selection: const SovereignSelection(baseOffset: 0, extentOffset: 4),
      ),
      commandRegistry: SovereignExtensionSet([
        const SovereignMarkdownInlineEditingExtension(),
      ]).commandRegistry(),
    ),
  );
}
