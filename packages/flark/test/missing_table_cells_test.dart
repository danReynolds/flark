import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:test/test.dart';

final class _PreparationFaultBackend implements FlarkParseBackend {
  _PreparationFaultBackend(this.delegate, this.error);

  final FlarkParseBackend delegate;
  final Object error;
  var fail = true;

  @override
  int get schemaVersion => delegate.schemaVersion;

  @override
  RenderModel parse(String source) {
    if (fail && source.endsWith('| x | | |\n')) throw error;
    return delegate.parse(source);
  }
}

void main() {
  final backend = createParseBackend();
  const table = '| a | b | c |\n| --- | --- | --- |\n| x |\n';
  for (final prefix in ['', '> ', '- ']) {
    for (final newline in ['\n', '\r\n']) {
      for (final closing in [true, false]) {
        for (final column in [1, 2]) {
          test(
            'unwritten column $column: prefix=$prefix CRLF=${newline.length == 2} closing=$closing',
            () {
              final lines = table.trimRight().split('\n');
              if (!closing) lines[2] = '| x';
              final source =
                  [
                    for (var i = 0; i < lines.length; i++)
                      '${prefix == '- ' && i > 0 ? '  ' : prefix}${lines[i]}',
                  ].join(newline) +
                  newline;
              final e = FlarkEditor(backend, text: source);
              final rows = e.projection.rows
                  .where((r) => r.kind == RowKind.tableCell && !r.header)
                  .toList();
              expect(rows, hasLength(3));
              final cell = rows[column];
              expect(e.apply(PlaceCaret(cell.index, 0)), isTrue);
              expect(e.document.caretRow.column, column);
              expect(e.selection.tableCell, cell.index);
              expect(
                e.source,
                source,
                reason: 'navigation must preserve exact Markdown',
              );
              expect(e.history.canUndo, isFalse);
              final states = <String>[];
              e.addListener(() => states.add(e.source));
              expect(e.apply(const InsertText('Z')), isTrue);
              expect(
                states,
                hasLength(1),
                reason: 'no intermediate delimiter edit',
              );
              final written = e.projection.rows
                  .where((r) => r.kind == RowKind.tableCell && !r.header)
                  .toList();
              expect(written[0].text.trim(), 'x');
              expect(written[column].text.trim(), 'Z');
              expect(e.document.caretRow.column, column);
              final after = e.source;
              expect(e.apply(const Undo()), isTrue);
              expect(e.source, source);
              expect(e.selection.tableCell, cell.index);
              expect(e.document.caretRow.column, column);
              expect(e.apply(const Redo()), isTrue);
              expect(e.source, after);
            },
          );
        }
      }
    }
  }

  test(
    'Tab, backwards Tab, arrows, line edges and Return preserve cell identity',
    () {
      final e = FlarkEditor(backend, text: '$table| y |\n');
      final cells = e.projection.rows
          .where((r) => r.kind == RowKind.tableCell && !r.header)
          .toList();
      e.apply(PlaceCaret(cells[0].index, 0));
      e.apply(const MoveTableCell());
      expect(e.document.caretRow.column, 1);
      e.apply(const MoveTableCell());
      expect(e.document.caretRow.column, 2);
      e.apply(const MoveTableCell(backward: true));
      expect(e.document.caretRow.column, 1);
      e.apply(const MoveCaret(MoveDirection.forward));
      expect(e.document.caretRow.column, 2);
      e.apply(const MoveCaret(MoveDirection.backward));
      expect(e.document.caretRow.column, 1);
      e.apply(const MoveCaret(MoveDirection.forward, unit: MoveUnit.line));
      expect(e.document.caretRow.column, 1);
      e.apply(const Newline());
      expect(e.document.caretRow.index, cells[4].index);
      expect(e.source, '$table| y |\n');
    },
  );

  test(
    'composition cancellation, pending style and resource insertion are atomic',
    () {
      final e = FlarkEditor(backend, text: table);
      final target = e.projection.rows
          .lastWhere((r) => r.kind == RowKind.tableCell)
          .index;
      e.apply(PlaceCaret(target, 0));
      final before = e.selection;
      e.beginComposition();
      expect(
        e.apply(ReplaceRange(e.selection.extent, e.selection.extent, '中')),
        isTrue,
      );
      expect(e.document.caretRow.text.trim(), '中');
      e.cancelComposition();
      expect(e.source, table);
      expect(e.selection, before);
      expect(e.history.canUndo, isFalse);
      e.apply(const ToggleStyle(Style.strong));
      expect(e.typingContext & Style.strong, Style.strong);
      e.apply(const Paste('word'));
      expect(e.document.caretRow.text.trim(), 'word');
      expect(e.source, contains('**word**'));
      e.apply(const Undo());
      expect(e.source, table);
      e.apply(const SetLink('https://dart.dev', text: 'Dart'));
      expect(e.document.caretRow.column, 2);
      expect(e.document.caretRow.text.trim(), 'Dart');
      e.apply(const Undo());
      expect(e.source, table);
    },
  );

  test(
    'refused edits, deletion, source mode and external selection clear safely',
    () {
      final e = FlarkEditor(
        backend,
        text: table,
        syncLimit: table.length,
        sourceLimit: table.length,
      );
      final target = e.projection.rows
          .lastWhere((r) => r.kind == RowKind.tableCell)
          .index;
      e.apply(PlaceCaret(target, 0));
      final selected = e.selection;
      expect(e.apply(const InsertText('Z')), isFalse);
      expect(e.lastRejection, FlarkRejection.sourceLimit);
      expect(e.selection, selected);
      expect(e.source, table);
      expect(e.apply(const SetLink('')), isFalse);
      expect(e.apply(const DeleteBackward()), isFalse);
      expect(e.source, table);
      expect(e.history.canUndo, isFalse);
      e.setSourceMode(true);
      expect(e.selection.tableCell, isNull);
      e.setSourceMode(false);
      expect(e.selection.tableCell, isNull);
    },
  );

  test('first insertion can cross into source mode as one undoable edit', () {
    final e = FlarkEditor(backend, text: table, syncLimit: table.length);
    final target = e.projection.rows
        .lastWhere((r) => r.kind == RowKind.tableCell)
        .index;
    e.apply(PlaceCaret(target, 0));
    final selected = e.selection;
    final published = <String>[];
    e.addListener(() => published.add(e.source));
    expect(e.apply(const InsertText('Z')), isTrue);
    expect(e.sourceMode, isTrue);
    expect(published, [e.source]);
    expect(e.source, contains('| x | | Z|'));
    expect(e.apply(const Undo()), isTrue);
    expect(e.sourceMode, isFalse);
    expect(e.source, table);
    expect(e.selection, selected);
  });

  for (final (style, marker) in [
    (Style.strong, '**'),
    (Style.emphasis, '*'),
    (Style.code, '`'),
  ]) {
    test(
      'pending $marker survives a missing-cell edit crossing the live limit',
      () {
        final e = FlarkEditor(backend, text: table, syncLimit: table.length);
        final target = e.projection.rows
            .lastWhere((r) => r.kind == RowKind.tableCell)
            .index;
        e.apply(PlaceCaret(target, 0));
        e.apply(ToggleStyle(style));
        final selected = e.selection;
        final published = <String>[];
        e.addListener(() => published.add(e.source));
        expect(e.apply(const InsertText('Z')), isTrue);
        expect(e.sourceMode, isTrue);
        expect(
          e.source,
          table.replaceFirst('| x |', '| x | | ${marker}Z$marker|'),
        );
        expect(published, [e.source]);
        expect(e.apply(const Undo()), isTrue);
        expect(e.source, table);
        expect(e.selection, selected);
        expect(e.typingContext & style, style);
      },
    );
  }

  for (final deviation in [true, false]) {
    test('private preparation rolls back after parser fault: $deviation', () {
      final error = deviation
          ? const FlarkParseException(
              FlarkParseException.extractionDeviationCode,
              'synthetic extraction deviation',
            )
          : StateError('synthetic backend fault');
      final failing = _PreparationFaultBackend(backend, error);
      final e = FlarkEditor(failing, text: table, syncLimit: table.length);
      final target = e.projection.rows
          .lastWhere((r) => r.kind == RowKind.tableCell)
          .index;
      e.apply(PlaceCaret(target, 0));
      e.apply(const ToggleStyle(Style.strong));
      final before = e.snapshot;
      final published = <String>[];
      e.addListener(() => published.add(e.source));
      if (deviation) {
        expect(e.apply(const InsertText('Z')), isFalse);
        expect(e.lastRejection, FlarkRejection.extractionDeviation);
      } else {
        expect(() => e.apply(const InsertText('Z')), throwsA(same(error)));
      }
      expect(e.snapshot, same(before));
      expect(e.typingContext & Style.strong, Style.strong);
      expect(e.history.canUndo, isFalse);
      expect(published, isEmpty);
      failing.fail = false;
      expect(e.apply(const InsertText('Z')), isTrue);
      expect(e.source, contains('| x | | **Z**|'));
      expect(published, [e.source]);
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, before.source);
      expect(e.selection, before.selection);
    });
  }

  test('resource validation still refuses a final source-mode result', () {
    for (final command in [
      const SetLink('https://dart.dev', text: 'Dart'),
      const SetImage('https://dart.dev/logo.png', alt: 'Dart'),
    ]) {
      for (final source in [table, table.replaceFirst('| x |', '| x | | |')]) {
        final e = FlarkEditor(backend, text: source, syncLimit: source.length);
        final target = e.projection.rows
            .lastWhere((r) => r.kind == RowKind.tableCell)
            .index;
        e.apply(PlaceCaret(target, 0));
        final before = e.snapshot;
        final published = <String>[];
        e.addListener(() => published.add(e.source));
        expect(e.apply(command), isFalse);
        expect(e.snapshot, same(before));
        expect(e.history.canUndo, isFalse);
        expect(published, isEmpty);
      }
    }
  });
}
