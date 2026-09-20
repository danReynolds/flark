import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final width in [360.0, 1100.0]) {
    testWidgets('composer selection, heading and undo at $width', (t) async {
      t.view.physicalSize = Size(width, 800);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.resetPhysicalSize);
      addTearDown(t.view.resetDevicePixelRatio);
      final c = FlarkController(FlarkEditor(backend, text: 'hello world'));
      final focus = FocusNode();
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              focusNode: focus,
              autofocus: true,
            ),
          ),
        ),
      );
      await t.pump();
      c.command(const SetSelection(0, 5));
      await t.pump();
      await t.tap(find.byTooltip('Strikethrough'));
      await t.pump();
      expect(c.text, '~~hello~~ world');
      expect(
        t
            .widget<IconButton>(
              find.widgetWithIcon(IconButton, Icons.format_strikethrough),
            )
            .isSelected,
        isTrue,
      );
      expect(focus.hasFocus, isTrue);
      await t.tap(find.byTooltip('Undo'));
      await t.pump();
      expect(c.text, 'hello world');
      await t.tap(find.byTooltip('Inline code'));
      await t.pump();
      expect(c.text, '`hello` world');
      await t.tap(find.byTooltip('Undo'));
      c.command(const SetSelection.caret(3));
      await t.pump();
      await t.tap(find.byTooltip('Paragraph style'));
      await t.pumpAndSettle();
      await t.tap(find.text('Heading 2').last);
      await t.pumpAndSettle();
      expect(c.text, '## hello world');
      expect(focus.hasFocus, isTrue);
      await t.tap(find.byTooltip('Undo'));
      await t.pump();
      expect(c.text, 'hello world');
      // Changes while a menu is open invalidate its captured target.
      await t.tap(find.byTooltip('Paragraph style'));
      await t.pumpAndSettle();
      c.command(const SetSelection.caret(9));
      await t.pump();
      await t.tap(find.text('Heading 1').last);
      await t.pumpAndSettle();
      expect(c.text, 'hello world');
      expect(t.takeException(), isNull);
      await t.pumpWidget(const SizedBox());
      c.dispose();
      focus.dispose();
    });
  }

  testWidgets('start a blank document with a heading through the toolbar', (
    t,
  ) async {
    final c = FlarkController(FlarkEditor(backend));
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await t.pump();
    await t.tap(find.byTooltip('Paragraph style'));
    await t.pumpAndSettle();
    await t.tap(find.text('Heading 1').last);
    await t.pumpAndSettle();
    expect(c.text, '# ');
    c.command(const InsertText('Title'));
    expect(c.text, '# Title');
    await t.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets('code and source mode disable inline formatting', (t) async {
    final c = FlarkController(
      FlarkEditor(backend, text: '```ruby\ndef hello\nend\n```', caret: 8),
    );
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c)),
      ),
    );
    await t.pump();
    expect(find.byTooltip('Code language'), findsOneWidget);
    for (final label in ['Bold', 'Italic', 'Strikethrough', 'Inline code']) {
      expect(
        t
            .widget<IconButton>(
              find.byWidgetPredicate(
                (widget) => widget is IconButton && widget.tooltip == label,
              ),
            )
            .onPressed,
        isNull,
      );
    }
    await t.tap(find.text('Source'));
    await t.pump();
    expect(find.byTooltip('Code language'), findsNothing);
    expect(
      t.widget<PopupMenuButton<int>>(find.byType(PopupMenuButton<int>)).enabled,
      isFalse,
    );
    await t.pumpWidget(const SizedBox());
    c.dispose();
  });
}
