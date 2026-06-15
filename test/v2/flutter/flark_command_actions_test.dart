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

      SingleActivator boldOf(Map<ShortcutActivator, Intent> map) {
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

      // 4 inline + link + 7 heading levels + 3 list/quote + 2 Tab indents.
      expect(macDefaults, hasLength(17));
      expect(
        macDefaults[const SingleActivator(LogicalKeyboardKey.tab)],
        isA<FlarkIndentListIntent>(),
      );
    });

    testWidgets('default heading/list/link shortcuts drive the commands', (
      tester,
    ) async {
      Future<BuildContext> pump(FlarkFlutterController controller) async {
        late BuildContext actionContext;
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
        return actionContext;
      }

      final heading = FlarkFlutterController.fromMarkdown('title');
      addTearDown(heading.dispose);
      heading.applySelection(const FlarkSelection.collapsed(0), userEvent: 't');
      Actions.invoke(
        await pump(heading),
        FlarkMarkdownShortcuts.setHeadingLevel(2),
      );
      expect(heading.markdown, '## title');

      final bullet = FlarkFlutterController.fromMarkdown('item');
      addTearDown(bullet.dispose);
      bullet.applySelection(const FlarkSelection.collapsed(0), userEvent: 't');
      Actions.invoke(
        await pump(bullet),
        FlarkMarkdownShortcuts.toggleBulletList(),
      );
      expect(bullet.markdown, '- item');

      final link = FlarkFlutterController.fromMarkdown('go');
      addTearDown(link.dispose);
      link.applySelection(
        const FlarkSelection(baseOffset: 0, extentOffset: 2),
        userEvent: 't',
      );
      Actions.invoke(await pump(link), FlarkMarkdownShortcuts.insertLink());
      expect(link.markdown, '[go]()');
    });

    testWidgets('Tab indents the list item under the caret', (tester) async {
      final controller = FlarkFlutterController.fromMarkdown('- item');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(4),
        userEvent: 'test',
      );
      late BuildContext actionContext;

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

      final action = Actions.find<FlarkIndentListIntent>(actionContext);
      expect(action.isEnabled(const FlarkIndentListIntent()), isTrue);

      Actions.invoke(actionContext, const FlarkIndentListIntent());
      expect(controller.markdown, '  - item');
    });

    testWidgets('Tab is disabled outside a list, so it can fall through', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('plain');
      addTearDown(controller.dispose);
      controller.applySelection(
        const FlarkSelection.collapsed(2),
        userEvent: 'test',
      );
      late BuildContext actionContext;

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

      final action = Actions.find<FlarkIndentListIntent>(actionContext);
      expect(action.isEnabled(const FlarkIndentListIntent()), isFalse);
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
