import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkEditableText', () {
    testWidgets('edits source text through the v2 controller', (tester) async {
      final controller = FlarkFlutterController.fromMarkdown('ab');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(controller: controller, maxLines: null),
        ),
      );

      await tester.enterText(find.byType(EditableText), 'abc');
      await tester.pump();

      expect(controller.markdown, 'abc');
      expect(controller.selection, const FlarkSelection.collapsed(3));
      expect(controller.hasAuthoritativeRenderPlan, isFalse);
    });

    testWidgets('supports mouse drag and double-click source selection', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('foo bar');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 360,
            height: 80,
            child: FlarkEditableText(
              controller: controller,
              style: const TextStyle(fontSize: 20, height: 1.4),
              maxLines: null,
            ),
          ),
        ),
      );

      final editableFinder = find.byType(EditableText);
      final rect = tester.getRect(editableFinder);
      final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await gesture.down(rect.centerLeft + const Offset(8, 0));
      await tester.pump();
      await gesture.moveTo(rect.centerLeft + const Offset(76, 0));
      await tester.pump();
      await gesture.up();
      await tester.pump();

      var editable = tester.widget<EditableText>(editableFinder);
      expect(editable.controller.selection.isCollapsed, isFalse);
      expect(controller.selection.isCollapsed, isFalse);

      controller.applySelection(const FlarkSelection.collapsed(0));
      await tester.pump();

      final wordOffset = rect.centerLeft + const Offset(18, 0);
      await tester.tapAt(wordOffset, kind: PointerDeviceKind.mouse);
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tapAt(wordOffset, kind: PointerDeviceKind.mouse);
      await tester.pump();

      editable = tester.widget<EditableText>(editableFinder);
      expect(editable.controller.selection.textInside('foo bar'), 'foo');
      expect(controller.selection.isCollapsed, isFalse);
    });

    testWidgets('syncs external controller edits into EditableText', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown(
        'ab',
        extensions: FlarkExtensionSet([const FlarkCoreEditingExtension()]),
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(controller: controller, maxLines: null),
        ),
      );

      controller.dispatch(
        command: FlarkCoreEditingCommands.insertText,
        payload: const FlarkInsertTextPayload('c'),
      );
      await tester.pump();

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, 'abc');
      expect(
        editable.controller.selection,
        const TextSelection.collapsed(offset: 3),
      );
    });

    testWidgets('merges partial style with ambient text defaults', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('ab');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: DefaultTextStyle(
            style: const TextStyle(color: Color(0xFF17202A), fontSize: 18),
            child: FlarkEditableText(
              controller: controller,
              style: const TextStyle(fontFamily: 'monospace'),
            ),
          ),
        ),
      );

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.style.color, const Color(0xFF17202A));
      expect(editable.style.fontSize, 18);
      expect(editable.style.fontFamily, 'monospace');
    });

    testWidgets('can expand to fill a document pane', (tester) async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            height: 240,
            child: FlarkEditableText(
              controller: controller,
              expands: true,
              maxLines: null,
            ),
          ),
        ),
      );

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.expands, isTrue);
      expect(editable.minLines, isNull);
      expect(editable.maxLines, isNull);
      expect(editable.textInputAction, TextInputAction.newline);
    });

    testWidgets('syncs caret movement back into the v2 controller', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('ab');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(controller: controller, maxLines: null),
        ),
      );

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      editable.controller.selection = const TextSelection.collapsed(offset: 1);
      await tester.pump();

      expect(controller.selection, const FlarkSelection.collapsed(1));
      expect(controller.runtime.canUndo, isFalse);
    });

    testWidgets(
      'upgrades platform Enter insertion through markdown input policy',
      (tester) async {
        final controller = FlarkFlutterController.fromMarkdown(
          '- item',
          extensions: FlarkExtensionSet([
            const FlarkMarkdownInputEditingExtension(),
          ]),
        );
        addTearDown(controller.dispose);

        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: FlarkEditableText(controller: controller, maxLines: null),
          ),
        );

        await tester.enterText(find.byType(EditableText), '- item\n');
        await tester.pump();

        expect(controller.markdown, '- item\n- ');
        expect(controller.selection, const FlarkSelection.collapsed(9));

        controller.undo();
        await tester.pump();
        expect(controller.markdown, '- item');
        expect(controller.selection, const FlarkSelection.collapsed(6));

        controller.redo();
        await tester.pump();
        expect(controller.markdown, '- item\n- ');
        expect(controller.selection, const FlarkSelection.collapsed(9));
      },
    );

    testWidgets(
      'upgrades platform Backspace deletion through markdown input policy',
      (tester) async {
        final controller = FlarkFlutterController.fromMarkdown(
          '- item',
          extensions: FlarkExtensionSet([
            const FlarkMarkdownInputEditingExtension(),
          ]),
        );
        addTearDown(controller.dispose);

        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: FlarkEditableText(controller: controller, maxLines: null),
          ),
        );

        controller.applySelection(
          const FlarkSelection.collapsed(2),
          userEvent: 'test',
        );
        await tester.pump();

        await tester.enterText(find.byType(EditableText), '-item');
        await tester.pump();

        expect(controller.markdown, 'item');
        expect(controller.selection, const FlarkSelection.collapsed(0));
      },
    );

    testWidgets('auto outdents platform closer insertion inside fenced code', (
      tester,
    ) async {
      const markdown = '```\n  \n```';
      final controller = FlarkFlutterController.fromMarkdown(
        markdown,
        extensions: FlarkExtensionSet([
          const FlarkMarkdownInputEditingExtension(),
        ]),
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(controller: controller, maxLines: null),
        ),
      );

      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );
      await tester.pump();

      await tester.enterText(find.byType(EditableText), '```\n  }\n```');
      await tester.pump();

      expect(controller.markdown, '```\n}\n```');
      expect(controller.selection, const FlarkSelection.collapsed(5));
    });

    testWidgets('normalizes platform multiline paste inside fenced code', (
      tester,
    ) async {
      const markdown = '```\n  \n```';
      final controller = FlarkFlutterController.fromMarkdown(
        markdown,
        extensions: FlarkExtensionSet([
          const FlarkMarkdownInputEditingExtension(),
        ]),
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(controller: controller, maxLines: null),
        ),
      );

      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );
      await tester.pump();

      await tester.enterText(
        find.byType(EditableText),
        '```\n  if (x) {\nprint(1);\n}\n```',
      );
      await tester.pump();

      const expected = '```\n  if (x) {\n  print(1);\n  }\n```';
      expect(controller.markdown, expected);
      expect(
        controller.selection,
        FlarkSelection.collapsed(expected.indexOf('\n```')),
      );
    });

    testWidgets('groups IME composition updates into one undo step', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(controller: controller, maxLines: null),
        ),
      );

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      editable.controller.value = const TextEditingValue(
        text: 'a',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );
      await tester.pump();
      editable.controller.value = const TextEditingValue(
        text: 'あ',
        selection: TextSelection.collapsed(offset: 1),
        composing: TextRange(start: 0, end: 1),
      );
      await tester.pump();
      editable.controller.value = const TextEditingValue(
        text: 'あ',
        selection: TextSelection.collapsed(offset: 1),
      );
      await tester.pump();

      expect(controller.markdown, 'あ');
      controller.undo();
      await tester.pump();
      expect(controller.markdown, isEmpty);

      controller.redo();
      await tester.pump();
      expect(controller.markdown, 'あ');
    });

    testWidgets('keeps IME composition undo separate from adjacent typing', (
      tester,
    ) async {
      final controller = FlarkFlutterController.fromMarkdown('');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkEditableText(controller: controller, maxLines: null),
        ),
      );

      await tester.enterText(find.byType(EditableText), 'x');
      await tester.pump();

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      editable.controller.value = const TextEditingValue(
        text: 'xa',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 1, end: 2),
      );
      await tester.pump();
      editable.controller.value = const TextEditingValue(
        text: 'xあ',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 1, end: 2),
      );
      await tester.pump();
      editable.controller.value = const TextEditingValue(
        text: 'xあ',
        selection: TextSelection.collapsed(offset: 2),
      );
      await tester.pump();

      expect(controller.markdown, 'xあ');
      controller.undo();
      await tester.pump();
      expect(controller.markdown, 'x');

      controller.undo();
      await tester.pump();
      expect(controller.markdown, isEmpty);
    });
  });
}
