import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();

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
    for (final newline in ['\n', '\r\n']) {
      test('Enter twice exits code in $prefix with ${newline.codeUnits}', () {
        final e = FlarkEditor(
          backend,
          text: '$prefix``$newline${newline}after',
          caret: prefix.length + 2,
        );
        expect(e.apply(const InsertText('`')), isTrue);
        expect(e.apply(const InsertText('  hello 😀')), isTrue);
        final code = e.source;
        final shells = e.document.caretRow.shells.map((s) => s.kind).toList();
        expect(e.apply(const Newline()), isTrue);
        expect(e.document.caretRow.text, '  hello 😀\n  ');
        final beforeExit = e.snapshot;
        final published = <String>[];
        e.addListener(() => published.add(e.source));
        expect(e.apply(const Newline()), isTrue);
        expect(published, [e.source]);
        expect(e.source, code); // Reuses the existing following blank line.
        expect(e.document.caretRow.kind, RowKind.blank);
        final exited = e.snapshot;
        expect(e.apply(const Undo()), isTrue);
        expect(e.source, beforeExit.source);
        expect(e.selection, beforeExit.selection);
        expect(e.apply(const Redo()), isTrue);
        expect(e.source, exited.source);
        expect(e.selection, exited.selection);
        expect(e.apply(const InsertText('prose')), isTrue);
        expect(e.document.caretRow.kind, RowKind.paragraph);
        expect(e.document.caretRow.text, 'prose');
        expect(e.document.caretRow.shells.map((s) => s.kind), shells);
        expect(e.source, contains('$newline${continuation}prose$newline'));
        expect(e.source.endsWith('$newline${newline}after'), isTrue);
        expect(
          e.projection.rows.where((r) => r.fenced).single.text,
          '  hello 😀',
        );
      });
    }
  }

  for (final closer in ['```', '````` \t']) {
    for (final tail in ['', '\n', '\nfollowing', '\n\nfollowing']) {
      test('exit preserves closer $closer and tail ${tail.codeUnits}', () {
        final e = FlarkEditor(
          backend,
          text: '```ruby metadata\ncode\n\t \n$closer$tail',
          caret: 23,
        );
        expect(e.document.caretRow.text, 'code\n\t ');
        expect(e.apply(const Newline()), isTrue);
        expect(e.projection.rows.where((r) => r.fenced).single.text, 'code');
        expect(e.source, startsWith('```ruby metadata\ncode\n$closer\n'));
        expect(e.apply(const InsertText('outside')), isTrue);
        expect(e.document.caretRow.text, 'outside');
        expect(e.document.caretRow.kind, RowKind.paragraph);
        if (tail.contains('following')) {
          expect(e.source, endsWith('\nfollowing'));
        }
      });
    }
  }

  for (final prefix in ['', '> ', '  ']) {
    for (final marker in ['```', '~~~~']) {
      test('imported unclosed $marker in $prefix acquires a closer', () {
        final source = '$prefix$marker ruby\n${prefix}code\n$prefix  ';
        final e = FlarkEditor(backend, text: source, caret: source.length);
        expect(e.apply(const Newline()), isTrue);
        expect(
          e.source,
          '$prefix$marker ruby\n${prefix}code\n$prefix$marker\n$prefix',
        );
        expect(e.apply(const InsertText('outside')), isTrue);
        expect(e.document.caretRow.kind, RowKind.paragraph);
        expect(e.document.caretRow.text, 'outside');
      });
    }
  }

  test('newly empty fence needs two Enters and remains as an empty fence', () {
    final e = FlarkEditor(backend);
    for (var i = 0; i < 3; i++) {
      expect(e.apply(const InsertText('`')), isTrue);
    }
    expect(e.apply(const Newline()), isTrue);
    expect(e.document.caretRow.kind, RowKind.codeBlock);
    expect(e.document.caretRow.text, '\n');
    expect(e.apply(const Newline()), isTrue);
    expect(e.document.caretRow.kind, RowKind.blank);
    expect(e.projection.rows.where((r) => r.fenced).single.text, '');
    expect(e.apply(const InsertText('outside')), isTrue);
    expect(e.document.caretRow.kind, RowKind.paragraph);
  });

  test(
    'interior blank line, Shift-Enter and selected replacement stay code',
    () {
      final interior = FlarkEditor(backend, text: '```\na\n\nb\n```', caret: 6);
      expect(interior.apply(const Newline()), isTrue);
      expect(interior.document.caretRow.text, 'a\n\n\nb');
      final shifted = FlarkEditor(backend, text: '```\na\n\n```', caret: 6);
      expect(shifted.apply(const Newline(paragraph: true)), isTrue);
      expect(shifted.document.caretRow.text, 'a\n\n');
      final selected = FlarkEditor(backend, text: '```\na\n  \n```');
      selected.apply(const SetSelection(6, 8));
      expect(selected.apply(const Newline()), isTrue);
      expect(selected.document.caretRow.text, 'a\n\n');
    },
  );

  test('paste, source mode and composition keep literal line breaks', () {
    const source = '```\na\n\n```';
    final pasted = FlarkEditor(backend, text: source, caret: 6);
    expect(pasted.apply(const Paste('\n')), isTrue);
    expect(pasted.document.caretRow.text, 'a\n\n');
    final raw = FlarkEditor(backend, text: source, caret: 6)
      ..setSourceMode(true);
    expect(raw.apply(const Newline()), isTrue);
    expect(raw.source, '```\na\n\n\n```');
    final ime = FlarkEditor(backend, text: source, caret: 6)
      ..beginComposition();
    expect(ime.apply(const Newline()), isTrue);
    expect(ime.document.caretRow.text, 'a\n\n');
    ime.cancelComposition();
    expect(ime.source, source);
  });

  test('exit exceeding the source limit rejects without falling through', () {
    const source = '```ruby\ncode\n ';
    final e = FlarkEditor(
      backend,
      text: source,
      caret: source.length,
      syncLimit: source.length,
      sourceLimit: source.length,
    );
    final before = e.snapshot;
    expect(e.apply(const Newline()), isFalse);
    expect(e.lastRejection, FlarkRejection.sourceLimit);
    expect(e.snapshot, same(before));
    expect(e.history.canUndo, isFalse);
    expect(e.revision, 0);
  });
}
