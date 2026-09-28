import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  final code = FlarkCodeMirror();

  Future<(FlarkController, List<FlarkPaintObservation>)> mount(
    WidgetTester tester,
    String source, {
    int? caret,
    double width = 400,
  }) async {
    final c = FlarkController(
      FlarkEditor(backend, text: source, codeEditing: code, caret: caret ?? 0),
    );
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: width,
            height: 600,
            child: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              showToolbar: false,
              onPaint: paints.add,
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    return (c, paints);
  }

  int colorsIn(FlarkPaintObservation paint, int row) =>
      paint.resolvedStyles[row].map((s) => s.color).toSet().length;

  testWidgets(
    'a fence is colored from its first frame through edits, undo and input',
    (tester) async {
      const source = '> ```dart\n> final answer = 42;\n> ```\n\nafter';
      final (c, paints) = await mount(
        tester,
        source,
        caret: source.indexOf('answer'),
        width: 220,
      );
      final editor = c.editor;
      expect(paints.first.rows.first, 'final answer = 42;');
      expect(colorsIn(paints.first, 0), greaterThan(1));
      c.command(
        SetSelection(source.indexOf('42') + 2, source.indexOf('answer')),
      );
      await tester.pump();
      final colored = paints.last;
      expect(colored.selectionRects, isNotEmpty);
      final keyword = colored.resolvedStyles.first.first;
      expect(editor.history.canUndo, isFalse);

      // The paint after an edit shows the current text in its own colors:
      // `final` keeps its keyword color, and fonts never change.
      c.command(const InsertText('message'));
      final edited = c.text, caret = editor.selection.extent;
      paints.clear();
      await tester.pump();
      expect(paints.first.rows.first, 'final message;');
      expect(paints.first.caretSource, caret);
      final styles = paints.first.resolvedStyles.first;
      expect(styles.first.color, keyword.color);
      expect(colorsIn(paints.first, 0), greaterThan(1));
      expect(styles.map((s) => s.fontFamily).toSet(), {keyword.fontFamily});
      c.command(const Undo());
      await tester.pump();
      expect(c.text, source);
      expect(paints.last.rows.first, 'final answer = 42;');
      expect(colorsIn(paints.last, 0), greaterThan(1));
      c.command(const Redo());
      await tester.pump();
      expect(c.text, edited);
      // The next platform character applies to the current source and caret.
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text: edited.replaceRange(caret, caret, 'z'),
          selection: TextSelection.collapsed(offset: caret + 1),
        ),
      );
      await tester.pump();
      expect(c.text, edited.replaceRange(caret, caret, 'z'));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('an untagged fence is colored as the language it looks like', (
    tester,
  ) async {
    final (c, paints) = await mount(
      tester,
      '```\ndef greet(name):\n    return name\n```\n\nafter',
    );
    expect(paints.last.rows.first, 'def greet(name):\n    return name');
    expect(colorsIn(paints.last, 0), greaterThan(1));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets('a fence in an unknown language is plain', (tester) async {
    final (c, paints) = await mount(
      tester,
      '```haskell\nmain = print 1\n```\n\nafter',
    );
    expect(paints.last.rows.first, 'main = print 1');
    expect(colorsIn(paints.last, 0), 1);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets('every fence is colored, however far down', (tester) async {
    final text = StringBuffer();
    for (var i = 0; i < 60; i++) {
      text.write('Paragraph $i\n\n```dart\nfinal fence$i = $i;\n```\n\n');
    }
    final (c, paints) = await mount(tester, text.toString());
    final scroll = tester.state<ScrollableState>(find.byType(Scrollable).last);
    scroll.position.jumpTo(scroll.position.maxScrollExtent);
    await tester.pump();
    final last = paints.last;
    final row = last.rows.lastIndexOf('final fence59 = 59;');
    expect(row, isNot(-1));
    expect(colorsIn(last, row), greaterThan(1));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
}
