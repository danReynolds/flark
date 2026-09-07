import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final prefix in ['', '> ', '>   ']) {
    for (final newline in ['\n', '\r\n']) {
      test(
        'code scope preserves containers $prefix and ${newline.length}-unit newlines',
        () {
          final opener = prefix == '>   ' ? '> - ```ruby' : '$prefix```ruby';
          final source = [
            'Before.',
            '',
            opener,
            '${prefix}def test',
            '$prefix  puts "😀"',
            '${prefix}end',
            '$prefix```',
            '',
            'After.',
          ].join(newline);
          final first = source.indexOf('def test'),
              last = source.indexOf('$newline$prefix```', first);
          final e = FlarkEditor(backend, text: source, caret: first + 4);
          e.apply(SetSelection(first + 5, first + 1));
          e.apply(const SelectAll());
          expect(e.selection, FlarkSelection(first, last));
          expect(e.source, source);
          e.apply(const Paste('one\ntwo'));
          e.apply(const InsertText('!'));
          expect(
            e.source,
            source.replaceRange(first, last, 'one$newline${prefix}two!'),
          );
          expect(e.selection.extent, first + 'one$newline${prefix}two!'.length);
          e.apply(const Undo());
          e.apply(const Undo());
          expect(
            (e.source, e.selection),
            (source, FlarkSelection(first, last)),
          );
        },
      );
    }
  }
  test(
    'empty code scope is inert once, then expands, and pointer movement resets it',
    () {
      const source = 'Before.\n\n```\n\n```\n\nAfter.';
      final at = source.indexOf('```\n') + 4;
      final e = FlarkEditor(backend, text: source, caret: at);
      final revision = e.revision;
      expect(e.apply(const SelectAll()), isFalse);
      expect(e.selection, FlarkSelection.collapsed(at));
      expect(e.lastRejection, isNull);
      expect(e.revision, revision);
      e.apply(const SelectAll());
      expect(e.selection, const FlarkSelection(0, source.length));
      e.apply(SetSelection.caret(at));
      e.apply(const SelectAll());
      e.apply(const Paste('inside'));
      expect(e.source, source.replaceRange(at, at, 'inside'));
    },
  );
  test(
    'one empty fence cannot turn its first Select All into document replacement',
    () {
      const source = '```\n\n```';
      final e = FlarkEditor(backend, text: source, caret: 4);
      e.apply(const SelectAll());
      e.apply(const Paste('inside'));
      expect(e.source, '```\ninside\n```');
    },
  );
  test('unclosed fence scope and repeated document selection are stable', () {
    const source = 'Before.\n\n```ruby\ndef test';
    final e = FlarkEditor(backend, text: source, caret: source.length);
    e.apply(const SelectAll());
    expect(e.selection, FlarkSelection(source.indexOf('def'), source.length));
    for (var i = 0; i < 3; i++) {
      e.apply(const SelectAll());
      expect(e.selection, const FlarkSelection(0, source.length));
      expect(e.lastRejection, isNull);
    }
  });
  test(
    'prose, cross-fence selections and source mode select the document immediately',
    () {
      const source = 'Before.\n\n```\none\n```\n\n```\ntwo\n```\n\nAfter.';
      final e = FlarkEditor(backend, text: source);
      e.apply(const SelectAll());
      expect(e.selection, const FlarkSelection(0, source.length));
      e.apply(SetSelection(source.indexOf('one'), source.indexOf('two')));
      e.apply(const SelectAll());
      expect(e.selection, const FlarkSelection(0, source.length));
      e.setSourceMode(true);
      e.apply(SetSelection.caret(source.indexOf('one')));
      e.apply(const SelectAll());
      expect(e.selection, const FlarkSelection(0, source.length));
    },
  );
}
