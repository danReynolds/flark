import 'dart:ui' show SemanticsAction, SemanticsActionEvent;
import 'package:flutter/gestures.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart' show SemanticsNode;
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final suffix in ['\n', '\n\nAfter']) {
    testWidgets(
      'table exit with existing suffix paints the next input: $suffix',
      (tester) async {
        const table = '| a | b |\n| - | - |\n| c | d |';
        final c = FlarkController(
          FlarkEditor(
            backend,
            text: '$table$suffix',
            caret: table.lastIndexOf('d'),
          ),
        );
        final paints = <FlarkPaintObservation>[];
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                onPaint: paints.add,
              ),
            ),
          ),
        );
        await tester.pump();
        paints.clear();
        expect(c.command(const Newline()), isTrue);
        expect(c.command(const InsertText('x')), isTrue);
        await tester.pump();
        for (final p in paints) {
          expect(
            p.rows.map((r) => r.trim()),
            containsAllInOrder(['a', 'b', 'c', 'd', '', 'x']),
          );
          expect(p.caret, isNotNull);
          expect(p.snapshot.source, '$table\n\nx$suffix');
        }
        expect(paints, isNotEmpty);
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      },
    );
  }

  testWidgets(
    'byte crossing and history switch presentation in the first frame',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(backend, text: '**a**', caret: 3, syncLimit: 6),
      );
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      for (final (command, source, visible, raw)
          in <(FlarkCommand, String, String, bool)>[
            (const InsertText('xx'), '**axx**', '**axx**', true),
            (const Undo(), '**a**', 'a', false),
            (const Redo(), '**axx**', '**axx**', true),
            (const DeleteBackward(), '**ax**', 'ax', false),
          ]) {
        paints.clear();
        expect(c.command(command), isTrue);
        await tester.pump();
        expect(paints, isNotEmpty);
        for (final p in paints) {
          expect(p.snapshot.source, source);
          expect(p.rows, [visible]);
          expect(p.snapshot is FlarkSourceSnapshot, raw);
          expect(p.caret, isNotNull);
          expect(p.revision, c.editor.revision);
        }
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'pointer placement distinguishes touching non-whitespace styles',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend, text: '*ab*c', caret: 3));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkSurface),
      );
      final edge = surface.caretRect;
      for (final inside in [false, true]) {
        await tester.tapAt(
          surface.localToGlobal(
            Offset(edge.left + (inside ? -1 : 1), edge.center.dy),
          ),
        );
        expect(c.editor.typingContext, inside ? Style.emphasis : 0);
        expect(c.command(const InsertText('x')), isTrue);
        expect(c.text, inside ? '*abx*c' : '*ab*xc');
        await tester.pump();
        expect(c.command(const Undo()), isTrue);
        await tester.pump();
        expect(c.text, '*ab*c');
      }
      await tester.pumpWidget(const SizedBox());
      await tester.pump(kDoubleTapMinTime);
      c.dispose();
    },
  );

  testWidgets(
    'accessibility selection maps visible offsets into canonical source',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final c = FlarkController(
        FlarkEditor(backend, text: '**ab** cd', caret: 2),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      final node = tester.getSemantics(find.byType(FlarkSurface));
      expect(node.getSemanticsData().value, 'ab cd');
      for (final (base, extent, visible, result) in [
        (1, 2, 'b', '**ax** cd'),
        (2, 1, 'b', '**ax** cd'),
        (0, 2, 'ab', '**x** cd'),
      ]) {
        tester.binding.performSemanticsAction(
          SemanticsActionEvent(
            viewId: tester.view.viewId,
            nodeId: node.id,
            type: SemanticsAction.setSelection,
            arguments: {'base': base, 'extent': extent},
          ),
        );
        await tester.pump();
        expect(c.selectedText, visible);
        expect(c.command(const InsertText('x')), isTrue);
        await tester.pump();
        expect(
          node.getSemanticsData().value,
          result == '**x** cd' ? 'x cd' : 'ax cd',
        );
        expect(c.text, result);
        expect(c.command(const Undo()), isTrue);
        await tester.pump();
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      semantics.dispose();
    },
  );

  Future<SemanticsNode> mountForSemantics(
    WidgetTester tester,
    FlarkController c,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
    return tester.getSemantics(find.byType(FlarkSurface));
  }

  void setText(WidgetTester tester, SemanticsNode node, String text) =>
      tester.binding.performSemanticsAction(
        SemanticsActionEvent(
          viewId: tester.view.viewId,
          nodeId: node.id,
          type: SemanticsAction.setText,
          arguments: text,
        ),
      );

  testWidgets(
    'accessibility text replacement edits only the changed visible range',
    (tester) async {
      final semantics = tester.ensureSemantics();
      const note = '# Title\n\n**bold** and [link](https://x.y)';
      for (final (source, from, to, result) in [
        // Voice Access "replace and with or" sends the whole edited value.
        (note, ' and ', ' or ', '# Title\n\n**bold** or [link](https://x.y)'),
        // Text added to a word continues the word's formatting.
        (
          note,
          'bold',
          'bolder',
          '# Title\n\n**bolder** and [link](https://x.y)',
        ),
        (note, 'link', 'xlink', '# Title\n\n**bold** and [xlink](https://x.y)'),
        // A surrogate pair or combining sequence is replaced whole.
        ('**\u{1F44D}** ok', '\u{1F44D}', '\u{1F44E}', '**\u{1F44E}** ok'),
        ('**e** x', 'e', 'e\u0301', '**e\u0301** x'),
        // A replacement that reaches into formatting covers it whole.
        ('**bold** and more', 'bold and', 'it', 'it more'),
        ('see [link](u) now', 'link now', 'x', 'see x'),
      ]) {
        final c = FlarkController(FlarkEditor(backend, text: source));
        final node = await mountForSemantics(tester, c);
        final value = node.getSemanticsData().value;
        setText(tester, node, value.replaceFirst(from, to));
        await tester.pump();
        expect(c.text, result);
        expect(c.editor.sourceMode, isFalse);
        expect(node.getSemanticsData().value, value.replaceFirst(from, to));
        expect(c.command(const Undo()), isTrue);
        expect(c.text, source);
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      }
      semantics.dispose();
    },
  );

  testWidgets('accessibility insertions are typed at the caret', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    // A line break beside another: the caret tells which one is new, and the
    // list continues as Return there continues it.
    const list = '- one\n- two';
    final expected = FlarkEditor(backend, text: list, caret: 5)
      ..apply(const Newline());
    final c = FlarkController(FlarkEditor(backend, text: list, caret: 5));
    final node = await mountForSemantics(tester, c);
    setText(tester, node, 'one\n\ntwo');
    await tester.pump();
    expect(c.text, expected.source);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
    // A missing table cell takes text as typing into it does.
    const table = '| a | b | c |\n| --- | --- | --- |\n| x |\n';
    final t = FlarkController(FlarkEditor(backend, text: table));
    final cells = await mountForSemantics(tester, t);
    final value = cells.getSemanticsData().value;
    // Cells keep their padding; the two missing cells and the blank line
    // after the table follow.
    expect(value, 'a \nb \nc \nx \n\n\n');
    setText(tester, cells, value.replaceRange(12, 12, 'Z'));
    await tester.pump();
    expect(t.text, '| a | b | c |\n| --- | --- | --- |\n| x | Z|\n');
    await tester.pumpWidget(const SizedBox());
    t.dispose();
    // Dictated at the caret, text takes the style chosen for it, as typing
    // does: placing the caret there first, as a press does, dropped it.
    final styled = FlarkController(
      FlarkEditor(backend, text: 'say ', caret: 4),
    );
    final caret = await mountForSemantics(tester, styled);
    expect(styled.command(const ToggleStyle(Style.emphasis)), isTrue);
    await tester.pump();
    setText(tester, caret, 'say hi');
    await tester.pump();
    expect(styled.text, 'say *hi*');
    await tester.pumpWidget(const SizedBox());
    styled.dispose();
    semantics.dispose();
  });

  testWidgets(
    'accessibility text replacement the kernel rejects keeps the document',
    (tester) async {
      final semantics = tester.ensureSemantics();
      const source = '# Title\n\nbody';
      final c = FlarkController(FlarkEditor(backend, text: source));
      final node = await mountForSemantics(tester, c);
      expect(node.getSemanticsData().value, 'Title\n\nbody');
      // Joining a heading and a paragraph is a cross-block edit.
      setText(tester, node, 'Titlebody');
      await tester.pump();
      expect(c.text, source);
      expect(c.editor.sourceMode, isFalse);
      expect(c.notice, 'This edit needs source mode.');
      expect(c.editor.history.canUndo, isFalse);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      semantics.dispose();
    },
  );

  testWidgets('accessibility text replacement in source mode edits the page', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    const source = '# Title\n\n**bold**';
    final c = FlarkController(
      FlarkEditor(backend, text: source)..setSourceMode(true),
    );
    final node = await mountForSemantics(tester, c);
    expect(node.getSemanticsData().value, source);
    setText(tester, node, '# Title\n\n**bolder**');
    await tester.pump();
    expect(c.text, '# Title\n\n**bolder**');
    expect(c.editor.sourceMode, isTrue);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
    semantics.dispose();
  });
}
