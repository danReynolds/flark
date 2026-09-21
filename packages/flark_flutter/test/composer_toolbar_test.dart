import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets(
    'active fill follows toolbar, public setters, keyboard and caret',
    (t) async {
      final c = FlarkController(FlarkEditor(backend));
      final semantics = t.ensureSemantics();
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await t.pump();
      IconButton bold() => t.widget<IconButton>(
        find.byWidgetPredicate((w) => w is IconButton && w.tooltip == 'Bold'),
      );
      expect(bold().isSelected, isFalse);
      await t.tap(find.byTooltip('Bold'));
      await t.pump();
      expect(bold().isSelected, isTrue);
      final colors = Theme.of(
        t.element(find.byType(FlarkEditorWidget)),
      ).colorScheme;
      expect(
        bold().style!.backgroundColor!.resolve({WidgetState.selected}),
        colors.secondaryContainer,
      );
      var notifications = 0;
      c.addListener(() => notifications++);
      expect(c.setStyle(Style.strong, enabled: true), isFalse);
      expect(notifications, 0);
      expect(c.notice, isNull);
      expect(c.setStyle(Style.strong, enabled: false), isTrue);
      await t.pump();
      expect(bold().isSelected, isFalse);
      await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.keyB);
      await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await t.pump();
      expect(bold().isSelected, isTrue);
      c.command(const InsertText('hello'));
      c.command(const SetSelection.caret(0));
      await t.pump();
      expect(bold().isSelected, isFalse);
      c.command(const SetSelection.caret(3));
      await t.pump();
      expect(bold().isSelected, isTrue);
      await t.pumpWidget(const SizedBox());
      semantics.dispose();
      c.dispose();
    },
  );

  testWidgets('mixed style has a separate indicator and one click applies it', (
    t,
  ) async {
    final semantics = t.ensureSemantics();
    final c = FlarkController(FlarkEditor(backend, text: '**bold** plain'));
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c)),
      ),
    );
    c.command(const SelectAll());
    await t.pump();
    expect(c.styleState(Style.strong).isMixed, isTrue);
    final boldSemantics = t
        .getSemantics(
          find.ancestor(
            of: find.byTooltip('Bold'),
            matching: find.byType(MergeSemantics),
          ),
        )
        .getSemanticsData();
    expect(boldSemantics.value, 'Mixed');
    expect(boldSemantics.tooltip, 'Bold');
    expect(
      find.byWidgetPredicate(
        (w) => w is Semantics && w.properties.value == 'Mixed',
      ),
      findsOneWidget,
    );
    expect(find.byIcon(Icons.remove), findsOneWidget);
    await t.tap(find.byTooltip('Bold'));
    await t.pump();
    expect(c.text, '**bold plain**');
    expect(find.byIcon(Icons.remove), findsNothing);
    await t.tap(find.byTooltip('Undo'));
    await t.pump();
    expect(c.styleState(Style.strong).isMixed, isTrue);
    c.sourceMode(true);
    await t.pump();
    expect(
      t
          .widget<IconButton>(
            find.byWidgetPredicate(
              (w) => w is IconButton && w.tooltip == 'Bold',
            ),
          )
          .onPressed,
      isNull,
    );
    await t.pumpWidget(const SizedBox());
    semantics.dispose();
    c.dispose();
  });
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
