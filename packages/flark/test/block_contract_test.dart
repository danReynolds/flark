import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final source in [
    '# **title**',
    '###### **title**',
    '**title**\n===',
    '**title**\n---',
  ]) {
    test('Backspace lifts heading and retains inline formatting: $source', () {
      final e = FlarkEditor(backend, text: source);
      final caret = e.projection.rows.first.sourceStart;
      e.apply(SetSelection(caret, caret));
      expect(e.selection.extent, caret);
      expect(e.apply(const DeleteBackward()), isTrue);
      expect(e.source, '**title**');
      expect(e.projection.rows.first.kind, RowKind.paragraph);
      expect(e.projection.rows.first.segments.single.styles, Style.strong);
      expect(e.apply(const InsertText('x')), isTrue);
      expect(e.projection.rows.single.text, 'xtitle');
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, '**title**');
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, source);
    });
  }
  const table = '| a | b |\n| - | - |\n| c | **d** |';
  test(
    'Return advances in the same table column then exits without changing cells',
    () {
      final e = FlarkEditor(backend, text: table);
      final first = e.projection.rows.first;
      e.apply(SetSelection.caret(first.sourceStart));
      expect(e.apply(const Newline()), isTrue);
      expect(e.source, table);
      expect(e.document.rowAt(e.selection.extent).text.trim(), 'c');
      expect(e.apply(const Newline()), isTrue);
      expect(e.source, '$table\n\n');
      expect(e.projection.rows.last.kind, RowKind.blank);
      expect(e.apply(const InsertText('x')), isTrue);
      expect(e.source, '$table\n\nx');
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, '$table\n\n');
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, table);
      expect(e.document.rowAt(e.selection.extent).text.trim(), 'c');
    },
  );
  test(
    'table cell boundary deletion rejects without lifting a pipe or delimiter row',
    () {
      final e = FlarkEditor(backend, text: table);
      for (final row in e.projection.rows.where(
        (r) => r.kind == RowKind.tableCell,
      )) {
        for (final backward in [true, false]) {
          e.apply(
            SetSelection.caret(backward ? row.sourceStart : row.sourceEnd),
          );
          final before = e.snapshot;
          expect(
            e.apply(backward ? const DeleteBackward() : const DeleteForward()),
            isFalse,
          );
          expect(e.snapshot, same(before));
          expect(e.source, table);
        }
      }
    },
  );
  for (final suffix in ['', '\n', '\n\nAfter']) {
    test(
      'table exit keeps a blank separator before subsequent typing: ${suffix.length}',
      () {
        final original = table + suffix;
        final e = FlarkEditor(backend, text: original);
        final cell = e.projection.rows.lastWhere(
          (r) => r.kind == RowKind.tableCell,
        );
        e.apply(SetSelection.caret(cell.sourceEnd));
        expect(e.apply(const Newline()), isTrue);
        expect(e.apply(const InsertText('x')), isTrue);
        expect(e.source, '$table\n\nx$suffix');
        expect(e.document.rowAt(e.selection.extent).kind, RowKind.paragraph);
        expect(e.apply(const Undo()), isTrue);
        expect(e.apply(const Undo()), isTrue);
        expect(e.source, original);
      },
    );
  }
}
