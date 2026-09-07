import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final (source, continuation) in [
    ('-\tfoo', '-\t'),
    ('1.\tfoo', '2. '),
    ('9)\tfoo', '10) '),
    ('> -\tfoo', '> -\t'),
    ('- a\n\t- b', '\t- '),
    ('- a\r\n\t- b', '\t- '),
    ('-\t[x] foo', '-\t[ ] '),
    ('-   foo', '-   '),
  ]) {
    test('Enter uses source marker range: $source', () {
      final e = FlarkEditor(backend, text: source, caret: source.length);
      expect(e.apply(const Newline()), isTrue);
      final expected = '$source\n$continuation';
      expect(e.source, expected);
      expect(e.selection, FlarkSelection.collapsed(expected.length));
      expect(e.apply(const InsertText('next')), isTrue);
      expect(e.source, '${expected}next');
      expect(
        e.document
            .rowAt(e.selection.extent)
            .shells
            .where((s) => s.kind == ShellKind.item),
        isNotEmpty,
      );
      expect(e.projection.rows.last.text, 'next');
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, expected);
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, source);
      expect(e.apply(const Redo()), isTrue);
      expect(e.source, expected);
    });
  }
}
