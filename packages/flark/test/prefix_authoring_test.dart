import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

void main() {
  final backend = createParseBackend();

  for (final marker in ['*', '-', '+', '#', '##', '######']) {
    for (final outer in ['', '> ', '  ']) {
      test(
        'bare $outer$marker stays editable with its parser model intact',
        () {
          final source = '$outer$marker\n\nafter';
          final editor = FlarkEditor(
            backend,
            text: source,
            caret: outer.length,
          );
          final row = editor.projection.rows.first;
          expect(row.text, marker);
          expect(row.kind, RowKind.paragraph);
          expect(row.sourceStart, outer.length);
          expect(row.sourceEnd, outer.length + marker.length);
          expect(
            row.shells.map((s) => s.kind),
            outer == '> ' ? [ShellKind.blockQuote] : isEmpty,
          );
          expect(
            editor.document.model.blockAt(row.block).kind,
            marker.startsWith('#') ? BlockKind.heading : BlockKind.item,
          );
          checkInvariants(
            source,
            editor.document.model,
            editor.projection,
            source,
          );

          editor.apply(SetSelection.caret(outer.length + marker.length));
          editor.apply(const InsertText(' '));
          editor.apply(const InsertText('x'));
          final committed = '$outer$marker x\n\nafter';
          expect(editor.source, committed);
          expect(editor.selection.extent, outer.length + marker.length + 2);
          expect(editor.projection.rows.first.text, 'x');
          checkInvariants(
            committed,
            editor.document.model,
            editor.projection,
            committed,
          );
          editor.apply(const Undo());
          expect(editor.source, source);
          expect(editor.projection.rows.first.text, marker);
          editor.apply(const Redo());
          expect(editor.source, committed);
          editor.apply(const InsertText('y'));
          expect(editor.source, '$outer$marker xy\n\nafter');
        },
      );
    }
  }

  test('a bare item on a continuation line retains its existing container', () {
    final editor = FlarkEditor(backend, text: '- a\n  *', caret: 7);
    expect(editor.projection.rows.single.text, 'a\n*');
    expect(editor.projection.rows.single.shells.last.kind, ShellKind.item);
    editor.apply(const InsertText('*'));
    editor.apply(const InsertText('b'));
    editor.apply(const InsertText('*'));
    editor.apply(const InsertText('*'));
    expect(editor.source, '- a\n  **b**');
    expect(editor.projection.rows.single.text, 'a\nb');
    expect(editor.projection.rows.single.segments.last.styles, Style.strong);
  });

  test('closing heading markers and empty committed items remain rendered', () {
    for (final source in ['# #', '## ##', '* ', '- ', '+ ', '1. ']) {
      final editor = FlarkEditor(backend, text: source);
      expect(editor.projection.rows.single.text, '');
    }
  });
}
