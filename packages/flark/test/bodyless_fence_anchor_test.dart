import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final (before, after) in [
    ('```text\n```\n\n# after', '```text\nx\n```\n\n# after'),
    (
      'before\n\n```text\n```\n\n# after',
      'before\n\n```text\nx\n```\n\n# after',
    ),
    ('> ```text\n> ```\n\n# after', '> ```text\n> x\n> ```\n\n# after'),
    ('>\t```text\n>\t```\n\n# after', '>\t```text\n>  x\n>\t```\n\n# after'),
  ]) {
    for (final command in [const InsertText('x'), const Paste('x')]) {
      test('bodyless fence pointer then ${command.runtimeType}: $before', () {
        final editor = FlarkEditor(backend, text: before);
        final code = editor.projection.rows.singleWhere((row) => row.fenced);
        editor.apply(PlaceCaret(code.index, 0));
        final selected = editor.selection;
        expect(editor.document.isLegal(selected.extent), isTrue);
        expect(editor.document.rowAt(selected.extent).index, code.index);
        expect(editor.source, before);
        expect(editor.apply(command), isTrue);
        expect(editor.source, after);
        expect(editor.document.rowAt(editor.selection.extent).text, 'x');
        expect(editor.projection.rows.last.kind, RowKind.heading);
        expect(editor.apply(const InsertText('!')), isTrue);
        expect(editor.document.rowAt(editor.selection.extent).text, 'x!');
        expect(editor.apply(const Undo()), isTrue);
        expect(editor.source, after);
        expect(editor.apply(const Undo()), isTrue);
        expect((editor.source, editor.selection), (before, selected));
        expect(editor.apply(const Redo()), isTrue);
        expect(editor.source, after);
      });
    }
  }
}
