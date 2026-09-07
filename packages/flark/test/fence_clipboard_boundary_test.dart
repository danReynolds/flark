import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();

  for (final newline in ['\n', '\r\n']) {
    for (final (openerPrefix, bodyPrefix, closingPrefix) in [
      ('', '', ''),
      ('> ', '> ', '> '),
      ('>\t', '>  ', '>\t'),
      ('> - ', '>   ', '>   '),
    ]) {
      for (final pasted in ['x', 'first\nsecond', '```\ninside']) {
        test(
          'bodyless fence paste ${openerPrefix.codeUnits} ${newline.length} $pasted',
          () {
            final before = '$openerPrefix```text$newline$closingPrefix```';
            final e = FlarkEditor(backend, text: before, caret: before.length);
            final selected = e.selection;
            final shells = e.document
                .rowAt(selected.extent)
                .shells
                .map((s) => s.kind)
                .toList();
            final publications = <String>[];
            e.addListener(() => publications.add(e.source));
            expect(e.apply(Paste(pasted)), isTrue);
            final fence = pasted.contains('```') ? '````' : '```';
            final expected =
                '$openerPrefix${fence}text$newline$bodyPrefix${pasted.replaceAll('\n', '$newline$bodyPrefix')}$newline$closingPrefix$fence';
            expect(e.source, expected);
            expect(publications, [expected]);
            expect(e.document.rowAt(e.selection.extent).text, pasted);
            expect(
              e.document.rowAt(e.selection.extent).shells.map((s) => s.kind),
              shells,
            );
            expect(
              e.selection.extent,
              expected.lastIndexOf('$newline$closingPrefix$fence'),
            );
            expect(e.apply(const InsertText('!')), isTrue);
            expect(e.document.rowAt(e.selection.extent).text, '$pasted!');
            expect(e.apply(const Undo()), isTrue);
            expect(e.source, expected);
            expect(e.apply(const Undo()), isTrue);
            expect((e.source, e.selection), (before, selected));
            expect(e.apply(const Redo()), isTrue);
            expect(e.source, expected);
          },
        );
      }
    }
  }
  for (final info in ['', 'text']) {
    for (final pasted in ['x', 'one\ntwo', '```\ninside']) {
      test('bodyless fence at EOF $info $pasted', () {
        final before = '```$info\n```';
        final e = FlarkEditor(backend, text: before, caret: before.length);
        final selected = e.selection;
        expect(e.apply(Paste(pasted)), isTrue);
        final fence = pasted.contains('```') ? '````' : '```';
        expect(e.source, '$fence$info\n$pasted\n$fence');
        expect(e.document.rowAt(e.selection.extent).text, pasted);
        expect(e.apply(const InsertText('!')), isTrue);
        expect(e.document.rowAt(e.selection.extent).text, '$pasted!');
        expect(e.apply(const Undo()), isTrue);
        expect(e.apply(const Undo()), isTrue);
        expect((e.source, e.selection), (before, selected));
      });
    }
  }
  test(
    'bodyless fence rejects oversized and invalid clipboard text atomically',
    () {
      const before = '```text\n```';
      final e = FlarkEditor(
        backend,
        text: before,
        caret: before.length,
        syncLimit: before.length,
        sourceLimit: before.length,
      );
      final snapshot = e.snapshot;
      expect(e.apply(const Paste('x')), isFalse);
      expect(e.lastRejection, FlarkRejection.sourceLimit);
      expect(e.snapshot, same(snapshot));
      expect(e.apply(const Paste('\uD800')), isFalse);
      expect(e.lastRejection, FlarkRejection.invalidSource);
      expect(e.snapshot, same(snapshot));
      expect(e.history.canUndo, isFalse);
    },
  );

  for (final boundary in ['bytes', 'lines']) {
    test(
      'bodyless partial-tab scaffold outside live $boundary rejects atomically',
      () {
        const before = '>\t```\n>\t```';
        final e = FlarkEditor(
          backend,
          text: before,
          caret: before.length,
          syncLimit: boundary == 'bytes'
              ? before.length
              : FlarkEditor.defaultSyncLimit,
          liveLimits: boundary == 'lines'
              ? const FlarkLiveLimits(lines: 2)
              : const FlarkLiveLimits(),
        );
        final snapshot = e.snapshot;
        var publications = 0;
        e.addListener(() => publications++);
        expect(e.apply(const Paste('```\ninside')), isFalse);
        expect(e.lastRejection, FlarkRejection.unsupportedEdit);
        expect(e.snapshot, same(snapshot));
        expect(e.history.canUndo, isFalse);
        expect(publications, 0);
      },
    );
  }

  for (final marker in ['`', '~']) {
    for (final newline in ['\n', '\r\n']) {
      for (final (openerPrefix, prefix) in [
        ('', ''),
        ('  ', '  '),
        ('> - ', '>   '),
        ('> - > ', '>   > '),
      ]) {
        test(
          'literal fence paste retains $marker $openerPrefix ${newline.length}',
          () {
            final fence = marker * 3;
            final pasted = '${marker * 5}\ninside\n${marker * 3}';
            final before =
                '😀$newline$newline$openerPrefix${fence}text meta \t'
                '$newline${prefix}here$newline$prefix$fence \t'
                '$newline$newline# after';
            final at = before.indexOf('here');
            final e = FlarkEditor(backend, text: before, caret: at);
            e.apply(SetSelection(at + 4, at));
            final selected = e.selection;
            final shells = e.document
                .rowAt(at)
                .shells
                .map((s) => s.kind)
                .toList();
            final published = <String>[];
            e.addListener(() => published.add(e.source));

            expect(e.apply(Paste(pasted.replaceAll('\n', newline))), isTrue);
            final expected =
                '😀$newline$newline$openerPrefix${marker * 6}text meta \t'
                '$newline$prefix${pasted.replaceAll('\n', '$newline$prefix')}'
                '$newline$prefix${marker * 6} \t$newline$newline# after';
            expect(e.source, expected);
            expect(published, [expected]);
            final row = e.document.rowAt(e.selection.extent);
            expect(row.kind, RowKind.codeBlock);
            expect(row.text, pasted);
            expect(row.shells.map((s) => s.kind), shells);
            expect(
              e.selection.extent,
              expected.lastIndexOf('$newline$prefix${marker * 6} \t'),
            );
            expect(e.projection.rows.last.kind, RowKind.heading);
            expect(e.projection.rows.last.text, 'after');

            expect(e.apply(const InsertText('!')), isTrue);
            expect(e.document.rowAt(e.selection.extent).text, '$pasted!');
            expect(e.apply(const Undo()), isTrue);
            expect(e.source, expected);
            expect(e.apply(const Undo()), isTrue);
            expect((e.source, e.selection), (before, selected));
            expect(e.apply(const Redo()), isTrue);
            expect(e.source, expected);
          },
        );
      }
    }
  }

  test(
    'a one-line fence paste preserves a longer existing closing delimiter',
    () {
      const before = '```text\nhere\n```````  \n\nafter';
      final at = before.indexOf('here');
      final e = FlarkEditor(backend, text: before, caret: at)
        ..apply(SetSelection(at, at + 4));
      expect(e.apply(const Paste('```')), isTrue);
      expect(e.source, '````text\n```\n```````  \n\nafter');
      expect(e.document.rowAt(e.selection.extent).text, '```');
      expect(e.projection.rows.last.kind, RowKind.paragraph);
      expect(e.projection.rows.last.text, 'after');
    },
  );

  test('a paste between existing marker characters also grows the fence', () {
    const before = '```text\n``\n```\n\nafter';
    final e = FlarkEditor(backend, text: before, caret: 9);
    expect(e.apply(const Paste('`')), isTrue);
    expect(e.source, '````text\n```\n````\n\nafter');
    expect(e.document.rowAt(e.selection.extent).text, '```');
    expect(e.selection.extent, 11);
  });

  for (final newline in ['\n', '\r\n']) {
    for (final prefix in ['>\t', '> \t']) {
      test(
        'a partial quote tab retains literal pasted indentation $prefix ${newline.length}',
        () {
          final before =
              '>\t```text$newline${prefix}here$newline$prefix```$newline${newline}after';
          final at = before.indexOf('here');
          final e = FlarkEditor(backend, text: before, caret: at)
            ..apply(SetSelection(at, at + 4));
          expect(e.document.rowAt(at).text, ' here');
          final selected = e.selection;
          expect(e.apply(const Paste('```\ninside')), isTrue);
          final expected =
              '>\t````text$newline$prefix```$newline>  inside$newline$prefix````$newline${newline}after';
          expect(e.source, expected);
          expect(e.document.rowAt(e.selection.extent).text, ' ```\ninside');
          expect(e.selection.extent, expected.indexOf('$newline$prefix````'));
          expect(e.projection.rows.last.text, 'after');
          expect(e.projection.rows.last.kind, RowKind.paragraph);
          expect(e.apply(const InsertText('!')), isTrue);
          expect(e.document.rowAt(e.selection.extent).text, ' ```\ninside!');
          expect(e.apply(const Undo()), isTrue);
          expect(e.source, expected);
          expect(e.apply(const Undo()), isTrue);
          expect((e.source, e.selection), (before, selected));
        },
      );
    }
  }

  test('pasting a different fence marker retains the existing spelling', () {
    const before = '~~~~text\nhere\n~~~~\n\nafter';
    final at = before.indexOf('here');
    final e = FlarkEditor(backend, text: before, caret: at)
      ..apply(SetSelection(at, at + 4));
    expect(e.apply(const Paste('```\ninside')), isTrue);
    expect(e.source, '~~~~text\n```\ninside\n~~~~\n\nafter');
    expect(e.document.rowAt(e.selection.extent).text, '```\ninside');
  });

  test(
    'an unclosed source-authored fence stays unclosed after literal paste',
    () {
      const before = '```text\nhere';
      final at = before.indexOf('here');
      final e = FlarkEditor(backend, text: before, caret: at)
        ..apply(SetSelection(at, before.length));
      expect(e.apply(const Paste('```\ninside')), isTrue);
      expect(e.source, '````text\n```\ninside');
      final row = e.document.rowAt(e.selection.extent);
      expect(row.text, '```\ninside');
      expect(e.document.model.blockAt(row.block).flags & 2, 0);
    },
  );

  test('fence growth outside writable source admission rejects atomically', () {
    const before = '```text\nhere\n```\n\nafter';
    final at = before.indexOf('here');
    final e = FlarkEditor(
      backend,
      text: before,
      caret: at,
      syncLimit: before.length,
      sourceLimit: before.length,
    )..apply(SetSelection(at, at + 4));
    final snapshot = e.snapshot;
    expect(e.apply(const Paste('```\nx')), isFalse);
    expect(identical(e.snapshot, snapshot), isTrue);
    expect(e.history.canUndo, isFalse);
    expect(e.lastRejection, FlarkRejection.sourceLimit);
  });

  for (final boundary in ['bytes', 'lines']) {
    test(
      'escaped clipboard source crosses the live $boundary boundary atomically',
      () {
        const before = '```text\nhere\n```\n\nafter';
        const after = '````text\n```\nx\n````\n\nafter';
        final at = before.indexOf('here');
        final e = FlarkEditor(
          backend,
          text: before,
          caret: at,
          syncLimit: boundary == 'bytes'
              ? before.length
              : FlarkEditor.defaultSyncLimit,
          liveLimits: boundary == 'lines'
              ? const FlarkLiveLimits(lines: 5)
              : const FlarkLiveLimits(),
        )..apply(SetSelection(at, at + 4));
        final selected = e.selection;
        final published = <FlarkEditorSnapshot>[];
        e.addListener(() => published.add(e.snapshot));
        expect(e.apply(const Paste('```\nx')), isTrue);
        expect(e.source, after);
        expect(published.single, isA<FlarkSourceSnapshot>());
        expect(e.selection.extent, after.indexOf('\n````'));
        expect(e.apply(const Undo()), isTrue);
        expect((e.source, e.selection), (before, selected));
        expect(e.sourceMode, isFalse);
        expect(e.apply(const Redo()), isTrue);
        expect(e.source, after);
        expect(e.sourceMode, isTrue);
      },
    );
  }

  for (final pasted in ['\r', '\ud800', 'line\n\udc00']) {
    test(
      'invalid code paste ${pasted.codeUnits} fails without source or history mutation',
      () {
        const before = '```text\nhere\n```\n\nafter';
        final e = FlarkEditor(
          backend,
          text: before,
          caret: before.indexOf('here'),
        );
        final snapshot = e.snapshot;
        expect(e.apply(Paste(pasted)), isFalse);
        expect(identical(e.snapshot, snapshot), isTrue);
        expect(e.history.canUndo, isFalse);
        expect(e.lastRejection, FlarkRejection.invalidSource);
      },
    );
  }
}
