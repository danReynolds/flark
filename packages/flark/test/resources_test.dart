import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final (source, destination, title, image) in [
    ('[hello](/a?x=1&amp;y=2 "A &amp; B")', '/a?x=1&y=2', 'A & B', false),
    ('![cat][p]\n\n[p]: /cat.png "Cat"', '/cat.png', 'Cat', true),
    ('[Read me][]\n\n[read ME]: /guide', '/guide', '', false),
    ('[Read me]\n\n[read ME]: /guide', '/guide', '', false),
    ('<hello@example.com>', 'mailto:hello@example.com', '', false),
    ('https://example.com', 'https://example.com', '', false),
    (r'[x](a\(b\))', 'a(b)', '', false),
    ('![猫](<猫 image.png>)', '猫 image.png', '', true),
  ]) {
    test('parser resolves resource $source', () {
      final doc = FlarkEditor(backend, text: source).document;
      final resource = doc.resources.single;
      expect(resource.destination, destination);
      expect(resource.title, title);
      expect(resource.isImage, image);
    });
  }
  for (final prefix in ['', '> ', '- ', '## ']) {
    test('link creation, editing, unlink and next input in $prefix', () {
      final original = '${prefix}read **this** next';
      final e = FlarkEditor(backend, text: original);
      e.apply(SetSelection(prefix.length, prefix.length + 13));
      expect(e.apply(const SetLink('/guide', title: 'Guide')), isTrue);
      expect(e.source, '$prefix[read **this**](</guide> "Guide") next');
      expect(e.document.resources.single.text, 'read this');
      expect(e.typingContext & Style.link, Style.link);
      expect(e.apply(const InsertText('!')), isTrue);
      expect(e.source, '$prefix[read **this**!](</guide> "Guide") next');
      expect(e.apply(const SetLink('/new')), isTrue);
      expect(e.source, '$prefix[read **this**!](</new> "Guide") next');
      expect(e.apply(const RemoveLink()), isTrue);
      expect(e.source, '${prefix}read **this**! next');
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, '$prefix[read **this**!](</new> "Guide") next');
      expect(e.apply(const Redo()), isTrue);
      expect(e.apply(const InsertText('?')), isTrue);
      expect(e.source, '${prefix}read **this**!? next');
    });
  }
  test(
    'editing one reference use leaves definitions and other uses intact',
    () {
      const source = '[a][ref] and [b][ref]\n\n[ref]: /old "old"';
      final e = FlarkEditor(backend, text: source, caret: 2);
      expect(e.apply(const SetLink('/new', title: 'new')), isTrue);
      expect(e.source, '[a](</new> "new") and [b][ref]\n\n[ref]: /old "old"');
      expect(e.document.resources.map((r) => r.destination), ['/new', '/old']);
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, source);
      expect(e.selection.extent, 2);
    },
  );
  test('image values serialize literally and round trip through history', () {
    final e = FlarkEditor(backend, text: 'before  after', caret: 7);
    expect(
      e.apply(
        const SetImage(
          '/a (cat).png?x=1&y=2',
          alt: 'cat [1] *x*',
          title: 'A "cat"',
        ),
      ),
      isTrue,
    );
    final image = e.document.resources.single;
    expect(image.text, 'cat [1] *x*');
    expect(image.destination, '/a (cat).png?x=1&y=2');
    expect(image.title, 'A "cat"');
    final inserted = e.source;
    expect(e.apply(const SetImage('/new.png')), isTrue);
    expect(e.document.resources.single.text, image.text);
    expect(e.apply(const RemoveImage()), isTrue);
    expect(e.source, 'before  after');
    expect(e.selection.extent, 7);
    expect(e.apply(const Undo()), isTrue);
    expect(e.document.resources.single.destination, '/new.png');
    expect(e.apply(const Undo()), isTrue);
    expect(e.source, inserted);
    expect(e.apply(const Undo()), isTrue);
    expect(e.source, 'before  after');
  });
  test('empty image alt remains an editable image', () {
    final e = FlarkEditor(backend);
    expect(e.apply(const SetImage('/a.png', alt: '')), isTrue);
    expect(e.source, '![](</a.png>)');
    expect(e.document.resources.single.text, '');
    expect(e.apply(const InsertText('cat')), isTrue);
    expect(e.source, '![cat](</a.png>)');
  });
  for (final (source, start, end) in [
    ('```\ncode\n```', 4, 8),
    ('`code`', 2, 3),
    ('one\n\ntwo', 0, 8),
    ('[one](/a) two', 2, 11),
  ]) {
    test('unsupported resource edit rejects atomically: $source', () {
      final e = FlarkEditor(backend, text: source);
      e.apply(SetSelection(start, end));
      final snapshot = e.snapshot;
      expect(e.apply(const SetLink('/new')), isFalse);
      expect(e.snapshot, same(snapshot));
      expect(e.history.canUndo, isFalse);
    });
  }
  test('stale dialog edit cannot replace newer document state', () {
    final e = FlarkEditor(backend, text: 'hello');
    final revision = e.revision;
    e.apply(const InsertText('x'));
    expect(e.apply(const SetLink('/x'), expectedRevision: revision), isFalse);
    expect(e.source, 'xhello');
    expect(e.lastRejection, FlarkRejection.staleRevision);
  });
  for (final source in [
    'https://example.com',
    '<a@example.com>',
    '[https://example.com](/guide)',
    '[**https://example.com**](/guide)',
  ]) {
    test('unlink does not recreate an automatic URL: $source', () {
      final e = FlarkEditor(backend, text: source);
      final resource = e.document.resources.first;
      e.apply(SetSelection.caret(resource.contentStart));
      expect(e.apply(const RemoveLink()), isTrue);
      expect(e.document.resources, isEmpty);
      expect(e.projection.rows.single.text, resource.text);
      if (source.contains('**')) {
        expect(e.projection.rows.single.segments.first.styles, Style.strong);
      }
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, source);
    });
  }
  test('remove image empties its formatting owner coherently', () {
    final e = FlarkEditor(backend, text: '**![cat](/cat.png)**', caret: 5);
    expect(e.apply(const RemoveImage()), isTrue);
    expect(e.source, '');
    expect(e.apply(const InsertText('x')), isTrue);
    expect(e.source, '**x**');
  });
}
