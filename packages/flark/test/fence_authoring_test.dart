import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final tail in ['', '\nafter', '\n# Heading', '\n```dart\nold\n```']) {
    test('third backtick creates an empty bounded block before $tail', () {
      final e = FlarkEditor(backend, text: tail);
      expect(e.apply(const InsertText('`')), isTrue);
      expect(e.source, '`$tail');
      expect(e.apply(const InsertText('`')), isTrue);
      expect(e.source, '``$tail');
      final published = <String>[];
      e.addListener(() => published.add(e.source));
      expect(e.apply(const InsertText('`')), isTrue);
      final completed = '```\n\n```\n${tail.isEmpty ? '\n' : tail}';
      expect(published, [completed]);
      expect(e.selection.extent, 4);
      expect(e.document.rowAt(4).kind, RowKind.codeBlock);
      expect(e.document.rowAt(4).text, '');
      final block = e.document.model.blockAt(e.document.rowAt(4).block);
      expect(block.flags & 3, 3);
      expect(e.apply(const InsertText('hello')), isTrue);
      expect(e.document.rowAt(e.selection.extent).text, 'hello');
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection.extent), (completed, 4));
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection.extent), ('``$tail', 2));
      expect(e.apply(const Redo()), isTrue);
      expect((e.source, e.selection.extent), (completed, 4));
    });
  }
  for (final (prefix, continuation) in [
    ('', ''),
    ('  ', '  '),
    ('> ', '> '),
    ('- ', '  '),
    ('> - ', '>   '),
    ('-\t', ' \t'),
    ('> - > ', '>   > '),
    ('1. ', '   '),
    ('  - ', '    '),
    ('- - ', '    '),
  ]) {
    test('create, Return, type, and empty Backspace in $prefix', () {
      final e = FlarkEditor(
        backend,
        text: '$prefix``\n\nafter',
        caret: prefix.length + 2,
      );
      expect(e.apply(const InsertText('`')), isTrue);
      final completed =
          '$prefix```\n$continuation\n$continuation```\n$continuation\n\nafter';
      expect(e.source, completed);
      final body = prefix.length + 4 + continuation.length;
      expect(e.selection.extent, body);
      final shells = e.document.rowAt(body).shells.map((s) => s.kind).toList();
      expect(e.document.rowAt(body).text, '');
      expect(e.apply(const Newline()), isTrue);
      expect(e.source, completed.replaceRange(body, body, '\n$continuation'));
      expect(e.apply(const InsertText('x')), isTrue);
      final row = e.document.rowAt(e.selection.extent);
      expect(row.kind, RowKind.codeBlock);
      expect(row.text, '\nx');
      expect(row.shells.map((s) => s.kind), shells);
      expect(e.apply(const Undo()), isTrue);
      expect(e.apply(const Undo()), isTrue);
      expect((e.source, e.selection.extent), (completed, body));
      expect(e.apply(const DeleteBackward()), isTrue);
      expect(
        e.projection.rows.any((r) => r.kind == RowKind.codeBlock),
        isFalse,
      );
      expect(e.source.endsWith('\n\nafter'), isTrue);
      expect(e.apply(const InsertText('prose')), isTrue);
      expect(e.document.rowAt(e.selection.extent).kind, RowKind.paragraph);
      expect(
        e.document.rowAt(e.selection.extent).shells.map((s) => s.kind),
        shells,
      );
    });
  }
  test('a tilde opener uses the same bounded authoring rule', () {
    final e = FlarkEditor(backend, text: '~~\nafter', caret: 2);
    expect(e.apply(const InsertText('~')), isTrue);
    expect((e.source, e.selection.extent), ('~~~\n\n~~~\n\nafter', 4));
  });
  test('trailing whitespace stays on the opening fence line', () {
    final e = FlarkEditor(backend, text: '`` \t\nafter', caret: 2);
    expect(e.apply(const InsertText('`')), isTrue);
    expect((e.source, e.selection.extent), ('``` \t\n\n```\n\nafter', 6));
  });
  test('UTF-8 prefixes and CRLF preserve offsets and surrounding source', () {
    final e = FlarkEditor(backend, text: '😀\r\n\r\n``\r\n# après', caret: 8);
    expect(e.apply(const InsertText('`')), isTrue);
    expect(e.source, '😀\r\n\r\n```\r\n\r\n```\r\n\r\n# après');
    expect(e.selection.extent, 11);
    expect(e.document.rowAt(11).text, '');
  });
  test('paste, source input, composition and code content stay literal', () {
    final pasted = FlarkEditor(backend);
    expect(pasted.apply(const Paste('```\nafter')), isTrue);
    expect(pasted.source, '```\nafter');
    final raw = FlarkEditor(backend)..setSourceMode(true);
    for (var i = 0; i < 3; i++) {
      raw.apply(const InsertText('`'));
    }
    expect(raw.source, '```');
    final ime = FlarkEditor(backend, text: '``', caret: 2)..beginComposition();
    expect(ime.apply(const InsertText('`')), isTrue);
    expect(ime.source, '```');
    ime.cancelComposition();
    expect(ime.source, '``');
    final code = FlarkEditor(backend, text: '~~~~\n``\n~~~~', caret: 7);
    expect(code.apply(const InsertText('`')), isTrue);
    expect(code.source, '~~~~\n```\n~~~~');
    expect(code.document.rowAt(8).text, '```');
    final inline = FlarkEditor(backend, text: 'word ``', caret: 7);
    expect(inline.apply(const InsertText('`')), isTrue);
    expect(inline.source, 'word ```');
  });
  test('completion exceeding the writable limit rejects atomically', () {
    final e = FlarkEditor(
      backend,
      text: '``',
      caret: 2,
      syncLimit: 8,
      sourceLimit: 9,
    );
    final before = e.snapshot;
    expect(e.apply(const InsertText('`')), isFalse);
    expect(e.lastRejection, FlarkRejection.sourceLimit);
    expect(e.snapshot, same(before));
    expect(e.history.canUndo, isFalse);
    expect(e.revision, 0);
  });
  test('Return on imported fence syntax without a body rejects safely', () {
    for (final source in ['```', '```\n```']) {
      final e = FlarkEditor(backend, text: source);
      expect(e.apply(const Newline()), isFalse);
      expect(e.source, source);
      expect(e.history.canUndo, isFalse);
    }
  });
}
