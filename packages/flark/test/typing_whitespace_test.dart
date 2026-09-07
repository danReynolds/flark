import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  test('typing after a multiline link hides its destination lines', () {
    const initial = '[link](   /uri\n  "title"  )\n';
    final e = FlarkEditor(backend, text: initial, caret: initial.length);
    expect(e.apply(const InsertText(':')), isTrue);
    expect(e.source, '$initial:');
    expect(e.projection.rows.single.text, 'link\n:');
    expect(e.selection.extent, initial.length + 1);
    expect(
      e.document.displayOf(e.selection.extent),
      const DisplayPosition(0, 6),
    );
    expect(e.apply(const InsertText(' next')), isTrue);
    expect(e.projection.rows.single.text, 'link\n: next');
  });
  for (final marker in ['  ', '\\']) {
    for (final newline in ['\n', '\r\n']) {
      for (final backward in [false, true]) {
        test(
          'deleting $marker break $newline backward=$backward then typing',
          () {
            final source = 'alpha$marker${newline}next';
            final caret = backward
                ? source.indexOf('next')
                : marker == '  '
                ? 5 + marker.length
                : 5;
            final e = FlarkEditor(backend, text: source, caret: caret);
            expect(
              e.apply(
                backward ? const DeleteBackward() : const DeleteForward(),
              ),
              isTrue,
            );
            expect(e.source, 'alphanext');
            expect(e.selection.extent, 5);
            expect(e.apply(const InsertText(' X')), isTrue);
            expect(e.source, 'alpha Xnext');
            expect(e.apply(const Undo()), isTrue);
            expect(e.apply(const Undo()), isTrue);
            expect(e.source, source);
          },
        );
      }
    }
  }
  for (final initial in [
    '# alpha #',
    'alpha\n=====',
    '| alpha |\n| --- |',
    '|alpha|\n|---|',
    '**alpha**',
    'alpha\nnext',
    'alpha\r\nnext',
    '> alpha\n> next',
  ]) {
    test('space then next word stays ordered in $initial', () {
      final caret = initial.startsWith('**')
          ? initial.length
          : initial.indexOf('alpha') + 5;
      final e = FlarkEditor(backend, text: initial, caret: caret);
      var typed = '';
      for (final character in [' ', ' ', 'b', 'e', 't', 'a']) {
        typed += character;
        expect(e.apply(InsertText(character)), isTrue);
        expect(e.source, initial.replaceRange(caret, caret, typed));
        expect(e.selection.extent, caret + typed.length);
      }
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, initial);
      expect(e.apply(const Redo()), isTrue);
      expect(e.apply(const InsertText('!')), isTrue);
      expect(e.source, initial.replaceRange(caret, caret, '  beta!'));
    });
  }
  for (final prefix in ['', '- ', '> ', '# ', '> - ']) {
    for (final ending in ['', '\n', '\r\n']) {
      test('typing preserves word spacing after "$prefix" before $ending', () {
        final initial = '${prefix}alpha$ending';
        final e = FlarkEditor(backend, text: initial, caret: prefix.length + 5);
        var typed = '';
        for (final character in [' ', ' ', 'b', 'e', 't', 'a']) {
          typed += character;
          expect(e.apply(InsertText(character)), isTrue);
          expect(e.source, '${prefix}alpha$typed$ending');
          expect(e.selection.extent, prefix.length + 5 + typed.length);
          expect(e.document.rowAt(e.selection.extent).text, 'alpha$typed');
        }
        expect(e.apply(const Undo()), isTrue);
        expect(e.source, initial);
        expect(e.apply(const Redo()), isTrue);
        expect(e.source, '${prefix}alpha  beta$ending');
        expect(e.apply(const DeleteBackward()), isTrue);
        expect(e.apply(const InsertText('X')), isTrue);
        expect(e.source, '${prefix}alpha  betX$ending');
      });
    }
  }
}
