import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkCommandActions', () {
    testWidgets('invokes typed commands through Flutter Actions', (
      tester,
    ) async {
      final controller = _markdownController();
      late BuildContext actionContext;
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkCommandActions(
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

      final result =
          Actions.invoke(
                actionContext,
                const FlarkCommandIntent(
                  FlarkTypedCommandInvocation(
                    command: FlarkMarkdownInlineCommands.toggleInlineStyle,
                    payload: FlarkToggleInlineStylePayload(
                      FlarkMarkdownInlineStyle.strong,
                    ),
                  ),
                ),
              )
              as FlarkEditorRuntimeResult;

      expect(result.commandResult.isHandled, isTrue);
      expect(controller.markdown, '**bold**');
    });

    testWidgets('FlarkEditableText installs command actions for descendants', (
      tester,
    ) async {
      final controller = _markdownController();
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(
            controller: controller,
            shortcuts: const {
              SingleActivator(
                LogicalKeyboardKey.keyB,
                meta: true,
              ): FlarkCommandIntent(
                FlarkTypedCommandInvocation(
                  command: FlarkMarkdownInlineCommands.toggleInlineStyle,
                  payload: FlarkToggleInlineStylePayload(
                    FlarkMarkdownInlineStyle.strong,
                  ),
                ),
              ),
            },
          ),
        ),
      );

      final result =
          Actions.invoke(
                tester.element(find.byType(EditableText)),
                const FlarkCommandIntent(
                  FlarkTypedCommandInvocation(
                    command: FlarkMarkdownInlineCommands.toggleInlineStyle,
                    payload: FlarkToggleInlineStylePayload(
                      FlarkMarkdownInlineStyle.strong,
                    ),
                  ),
                ),
              )
              as FlarkEditorRuntimeResult;

      expect(result.commandResult.isHandled, isTrue);
      expect(controller.markdown, '**bold**');
      expect(find.byType(Shortcuts), findsOneWidget);
    });

    testWidgets('FlarkMarkdownShortcuts intents drive the command surface', (
      tester,
    ) async {
      final controller = _markdownController();
      late BuildContext actionContext;
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkCommandActions(
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

      final result =
          Actions.invoke(actionContext, FlarkMarkdownShortcuts.toggleStrong())
              as FlarkEditorRuntimeResult;

      expect(result.commandResult.isHandled, isTrue);
      expect(controller.markdown, '**bold**');
    });

    test('default shortcut map binds inline formatting accelerators', () {
      final macDefaults = FlarkMarkdownShortcuts.defaults(
        platform: TargetPlatform.macOS,
      );
      final linuxDefaults = FlarkMarkdownShortcuts.defaults(
        platform: TargetPlatform.linux,
      );

      SingleActivator boldOf(Map<ShortcutActivator, FlarkCommandIntent> map) {
        return map.keys.whereType<SingleActivator>().firstWhere(
          (activator) => activator.trigger == LogicalKeyboardKey.keyB,
        );
      }

      final macBold = boldOf(macDefaults);
      expect(macBold.meta, isTrue);
      expect(macBold.control, isFalse);

      final linuxBold = boldOf(linuxDefaults);
      expect(linuxBold.control, isTrue);
      expect(linuxBold.meta, isFalse);

      expect(macDefaults, hasLength(4));
    });
  });
}

FlarkFlutterController _markdownController() {
  return FlarkFlutterController(
    runtime: FlarkEditorRuntime(
      state: FlarkEditorState.fromMarkdown(
        'bold',
        selection: const FlarkSelection(baseOffset: 0, extentOffset: 4),
      ),
      commandRegistry: FlarkExtensionSet([
        const FlarkMarkdownInlineEditingExtension(),
      ]).commandRegistry(),
    ),
  );
}
