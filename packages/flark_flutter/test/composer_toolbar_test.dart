import 'dart:ui' show SemanticsAction;
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart' show SemanticsNode;
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

  testWidgets('the paragraph style menu is its own semantics node', (t) async {
    // It has no node of its own. Merged into the editor's (which carries the
    // Link actions custom action), its tap and label covered the whole
    // editor: with accessibility on, a browser's press anywhere in the
    // document opened the menu, and a screen reader read the editor as it.
    final c = FlarkController(FlarkEditor(backend, text: 'abc'));
    final semantics = t.ensureSemantics();
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await t.pump();
    // The editor's own node is the one with its Link actions.
    SemanticsNode? editor;
    bool visit(SemanticsNode node) {
      if (node.getSemanticsData().customSemanticsActionIds?.isNotEmpty ??
          false) {
        editor = node;
        return false;
      }
      node.visitChildren(visit);
      return true;
    }

    visit(
      t.binding.renderViews.first.owner!.semanticsOwner!.rootSemanticsNode!,
    );
    final menu = t.getSemantics(find.byTooltip('Paragraph style'));
    expect(editor, isNotNull);
    expect(menu, isNot(same(editor)));
    expect(menu.getSemanticsData().hasAction(SemanticsAction.tap), isTrue);
    expect(menu.rect.height, lessThan(100));
    final data = editor!.getSemanticsData();
    expect(data.hasAction(SemanticsAction.tap), isFalse);
    expect(data.label, isNot(contains('Paragraph')));
    semantics.dispose();
    await t.pumpWidget(const SizedBox());
    c.dispose();
  });

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

  // The toolbar reads the FlarkState a consumer is given, whose heading
  // availability is the kernel's canSetHeading: the menu is on exactly where
  // SetHeadingLevel would change the caret's block. That block alone, so a
  // selection across paragraphs heads the caret's; and only a paragraph's
  // first line can be headed.
  for (final (where, source, base, extent, enabled, label) in [
    (
      'a selection across two paragraphs',
      'one\n\ntwo',
      0,
      8,
      true,
      'Paragraph',
    ),
    (
      'a selection across a heading and a paragraph',
      '# one\n\ntwo',
      0,
      10,
      true,
      'Mixed',
    ),
    ('the second line of a paragraph', 'one\ntwo', 6, 6, false, 'Paragraph'),
  ]) {
    testWidgets('the paragraph style menu on $where follows the kernel', (
      t,
    ) async {
      final c = FlarkController(FlarkEditor(backend, text: source));
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(body: FlarkEditorWidget(controller: c)),
        ),
      );
      c.command(SetSelection(base, extent));
      await t.pump();
      expect(c.editor.canSetHeading(), enabled);
      expect(
        t
            .widget<PopupMenuButton<int>>(find.byType(PopupMenuButton<int>))
            .enabled,
        enabled,
      );
      expect(find.text(label), findsOneWidget);
      await t.pumpWidget(const SizedBox());
      c.dispose();
    });
  }

  testWidgets(
    'a selection reaching into a fence leaves its language menu off',
    (t) async {
      const source = 'intro\n\n```dart\nmain\n```';
      final inside = source.indexOf('main') + 2;
      final c = FlarkController(FlarkEditor(backend, text: source));
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(body: FlarkEditorWidget(controller: c)),
        ),
      );
      PopupMenuButton<String> menu() => t.widget<PopupMenuButton<String>>(
        find.byType(PopupMenuButton<String>),
      );
      c.command(SetSelection(0, inside));
      await t.pump();
      expect(menu().enabled, isFalse);
      c.command(SetSelection.caret(inside));
      await t.pump();
      expect(menu().enabled, isTrue);
      await t.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
