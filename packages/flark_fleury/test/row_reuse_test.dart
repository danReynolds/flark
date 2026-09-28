/// A layout built from its predecessor must be exactly the layout built from
/// scratch, while an edit lays out only the rows it changed.
library;

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:test/test.dart';

String _style(CellStyle s) =>
    '${s.foreground}/${s.background}/${s.boldOrNull}/${s.dimOrNull}/'
    '${s.italicOrNull}/${s.underlineOrNull}/${s.inverseOrNull}/'
    '${s.strikethroughOrNull}/${s.linkUri}';

String _dump(CellDocumentLayout layout) {
  final out = StringBuffer();
  void line(CellLine l, String indent) {
    out.write(
      '$indent${l.row?.index}|${l.row?.sourceStart}|${l.start}|${l.end}|'
      '${l.prefix}|${l.left}|${l.right}|${l.rule}|${l.blockLeft}|'
      '${l.codeEdgeTop}|${l.headingRule}|${l.headingLabelColumn}|'
      '${l.caretLine}|${l.image?.start}|${l.imageLabel?.start}|',
    );
    for (final g in l.glyphs) {
      out.write('[${g.text},${g.col},${g.width},${g.start},${g.end},');
      out.write('${_style(g.style)}]');
    }
    out.writeln();
    for (final cell in l.cells ?? const <CellLine>[]) {
      line(cell, '$indent  ');
    }
  }

  for (final l in layout.lines) {
    line(l, '');
  }
  for (final slot in layout.images) {
    out.writeln(
      'image ${slot.resource.start} ${slot.left} ${slot.top} '
      '${slot.width} ${slot.height}',
    );
  }
  return out.toString();
}

void main() {
  final backend = createParseBackend();
  const theme = FlarkCellTheme();
  const policy = CellWidthPolicy.spec;

  test('a reused layout equals a fresh one after every kind of edit', () {
    final source = [
      '# Title',
      'Plain paragraph with **bold** and *emphasis*.',
      for (var i = 1; i <= 9; i++) '$i. item $i',
      '> quoted *words*',
      '- [ ] task',
      '- bullet',
      '```dart\nfinal x = 1;\n```',
      '| a | b |\n| - | - |\n| 1 | **2** |',
      '![alt](image.png)',
      '## Tail heading',
      'Last paragraph.',
    ].join('\n\n');
    final editor = FlarkEditor(backend, text: source, caret: 0);
    final controller = FlarkFleuryController(editor);
    for (final cols in [12, 40, 100]) {
      var layout = CellDocumentLayout(controller, cols, theme, policy);
      void check(String step, void Function() edit) {
        edit();
        final reused = CellDocumentLayout(
          controller,
          cols,
          theme,
          policy,
          previous: layout,
        );
        final fresh = CellDocumentLayout(controller, cols, theme, policy);
        expect(_dump(reused), _dump(fresh), reason: '$step at $cols cols');
        layout = reused;
      }

      int at(String text) => editor.source.indexOf(text);
      check('typing', () {
        editor.apply(SetSelection.caret(at('Plain')));
        editor.apply(const InsertText('x'));
      });
      check(
        'a new paragraph',
        () => editor.apply(const Newline(paragraph: true)),
      );
      check('a join', () => editor.apply(const DeleteBackward()));
      check('undo', () => editor.apply(const Undo()));
      check('redo', () => editor.apply(const Redo()));
      check('a tenth ordered item widens every marker', () {
        editor.apply(SetSelection.caret(at('item 9') + 6));
        editor.apply(const Newline());
        editor.apply(const InsertText('ten'));
      });
      check('a paragraph entering a quote', () {
        editor.apply(SetSelection.caret(at('Last paragraph')));
        editor.apply(const InsertText('>'));
      });
      check('a task toggle changes only a marker', () {
        editor.apply(SetSelection.caret(at('task')));
        expect(editor.apply(const ToggleTask()), isTrue);
      });
      check('a heading edit', () {
        editor.apply(SetSelection.caret(at('Tail heading')));
        editor.apply(const InsertText('New '));
      });
      check('a table cell edit', () {
        editor.apply(SetSelection.caret(at('| 1 |') + 2));
        editor.apply(const InsertText('0'));
      });
      check('a code edit', () {
        editor.apply(SetSelection.caret(at('final x')));
        editor.apply(const InsertText('var '));
      });
      check('undo everything', () {
        while (editor.apply(const Undo())) {}
      });
    }
    controller.dispose();
  });

  test('a new paragraph near the top lays out only its own rows', () {
    final source = [
      for (var i = 0; i < 400; i++) 'Paragraph $i with *some* words.',
    ].join('\n\n');
    final editor = FlarkEditor(backend, text: source, caret: 5);
    final controller = FlarkFleuryController(editor);
    var layout = CellDocumentLayout(controller, 60, theme, policy);
    int laidOut(FlarkCommand command) {
      expect(editor.apply(command), isTrue);
      final before = CellDocumentLayout.rowLayouts;
      layout = CellDocumentLayout(
        controller,
        60,
        theme,
        policy,
        previous: layout,
      );
      return CellDocumentLayout.rowLayouts - before;
    }

    expect(laidOut(const InsertText('x')), 1);
    expect(laidOut(const Newline(paragraph: true)), lessThanOrEqualTo(3));
    expect(laidOut(const DeleteBackward()), lessThanOrEqualTo(3));
    expect(laidOut(const Undo()), lessThanOrEqualTo(3));
    controller.dispose();
  });
}
