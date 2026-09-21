import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_homepage/demo.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();

  testWidgets(
    'editing survives source mode and page-theme changes; reset is undoable',
    (tester) async {
      final brightness = ValueNotifier(Brightness.light);
      addTearDown(brightness.dispose);
      await tester.pumpWidget(
        HomepageDemo(
          backend: backend,
          brightness: brightness,
          onOpenLink: (_) {},
        ),
      );
      await tester.pump();
      final editor = find.byType(FlarkEditorWidget);
      final controller = tester.widget<FlarkEditorWidget>(editor).controller;
      expect(tester.testTextInput.hasAnyClients, isFalse);
      await tester.tapAt(tester.getTopLeft(editor) + const Offset(70, 100));
      await tester.pump();
      controller.command(SetSelection.caret(controller.text.length));
      await tester.pump();
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text: '${controller.text}A homepage thought.',
          selection: TextSelection.collapsed(
            offset: controller.text.length + 19,
          ),
        ),
      );
      await tester.pump();
      final edited = controller.text;
      expect(edited, endsWith('A homepage thought.'));
      await tester.tap(find.text('Source'));
      await tester.pump();
      expect(controller.editor.sourceMode, isTrue);
      brightness.value = Brightness.dark;
      await tester.pump();
      expect(
        tester.widget<FlarkEditorWidget>(editor).controller,
        same(controller),
      );
      expect(controller.text, edited);
      expect(controller.editor.sourceMode, isTrue);
      await tester.tap(find.text('Rendered'));
      await tester.pump();
      await tester.tap(find.text('Reset sample'));
      await tester.pump();
      expect(controller.text, sampleMarkdown);
      await tester.tap(find.byTooltip('Undo'));
      await tester.pump();
      expect(controller.text, edited);
      await tester.pumpWidget(const SizedBox());
      // Let the gesture recognizer's minimum double-tap interval expire.
      await tester.pump(const Duration(milliseconds: 300));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('starter tabs preserve separate edits and history', (
    tester,
  ) async {
    final brightness = ValueNotifier(Brightness.light);
    addTearDown(brightness.dispose);
    await tester.pumpWidget(
      HomepageDemo(
        backend: backend,
        brightness: brightness,
        onOpenLink: (_) {},
      ),
    );
    await tester.pump();
    FlarkController current() => tester
        .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
        .controller;
    final welcome = current();
    welcome.command(SetSelection.caret(welcome.text.length));
    welcome.command(const InsertText('My welcome edit.'));
    await tester.pump();
    await tester.tap(find.text('Meeting notes'));
    await tester.pump();
    expect(current().text, meetingMarkdown);
    await tester.tap(find.text('Blank page'));
    await tester.pump();
    expect(current().text, isEmpty);
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'My own document',
        selection: TextSelection.collapsed(offset: 15),
      ),
    );
    await tester.pump();
    expect(current().text, 'My own document');
    await tester.tap(find.text('Welcome'));
    await tester.pump();
    expect(current(), same(welcome));
    expect(welcome.text, endsWith('My welcome edit.'));
    await tester.tap(find.byTooltip('Undo'));
    await tester.pump();
    expect(welcome.text, sampleMarkdown);
    await tester.tap(find.text('Blank page'));
    await tester.pump();
    expect(current().text, 'My own document');
    await tester.pumpWidget(const SizedBox());
    await tester.pump(const Duration(milliseconds: 300));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'copy exports current Markdown and narrow controls remain usable',
    (tester) async {
      tester.view.physicalSize = const Size(320, 560);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final brightness = ValueNotifier(Brightness.light);
      addTearDown(brightness.dispose);
      String? copied;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied = (call.arguments as Map)['text'] as String;
          }
          return null;
        },
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );
      await tester.pumpWidget(
        HomepageDemo(
          backend: backend,
          brightness: brightness,
          onOpenLink: (_) {},
        ),
      );
      await tester.pump();
      final controller = tester
          .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
          .controller;
      controller.command(SetSelection.caret(controller.text.length));
      controller.command(const InsertText('Copied edit.'));
      await tester.pump();
      await tester.tap(find.text('Copy Markdown'));
      await tester.pump();
      expect(copied, controller.text);
      expect(copied, endsWith('Copied edit.'));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
