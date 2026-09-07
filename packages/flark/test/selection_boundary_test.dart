import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final range in [
    const FlarkSelection(7, 13),
    const FlarkSelection(15, 9),
    const FlarkSelection(7, 15),
  ]) {
    for (final command in <FlarkCommand>[
      const Paste('new'),
      ReplaceRange(range.start, range.end, 'new'),
      const DeleteBackward(),
      const DeleteForward(),
      const Newline(),
      const ToggleStyle(Style.strong),
    ]) {
      test(
        'boundary selection $range ${command.runtimeType} and next input',
        () {
          const source = 'before **bold** after';
          final e = FlarkEditor(backend, text: source);
          e.apply(SetSelection(range.base, range.extent));
          expect(e.apply(command), isTrue);
          final expected = switch (command) {
            DeleteBackward() || DeleteForward() => 'before  after',
            Newline() => 'before \n after',
            ToggleStyle() => 'before bold after',
            _ => 'before **new** after',
          };
          expect(e.source, expected);
          expect(e.apply(const InsertText('!')), isTrue);
          final next = switch (command) {
            DeleteBackward() || DeleteForward() =>
              range.extent >= 9 && range.extent <= 13
                  ? 'before **!** after'
                  : 'before ! after',
            // The next row's leading indentation is outside its caret span.
            Newline() => 'before \n !after',
            ToggleStyle() => 'before ! after',
            _ => 'before **new!** after',
          };
          expect(e.source, next);
          e.apply(const Undo());
          e.apply(const Undo());
          expect(e.source, source);
          expect(e.selection, range);
          expect(e.document.model.bytes, backend.parse(source).bytes);
        },
      );
    }
  }
  for (final marker in ['*', '**', '~~', '`', '***']) {
    for (final edges in ['left', 'right', 'both']) {
      for (final reverse in [false, true]) {
        test('replace $marker word from $edges edge reverse=$reverse', () {
          final source = 'before ${marker}bold$marker after';
          final start = edges == 'right' ? 7 + marker.length : 7;
          final end = edges == 'left'
              ? 7 + marker.length + 4
              : 7 + marker.length * 2 + 4;
          final e = FlarkEditor(backend, text: source);
          final selected = reverse
              ? FlarkSelection(end, start)
              : FlarkSelection(start, end);
          e.apply(SetSelection(selected.base, selected.extent));
          expect(e.apply(const InsertText('new')), isTrue);
          expect(e.source, 'before ${marker}new$marker after');
          expect(e.apply(const InsertText('!')), isTrue);
          expect(e.source, 'before ${marker}new!$marker after');
          e.apply(const Undo());
          e.apply(const Undo());
          expect(e.source, source);
          expect(e.selection, selected);
          e.apply(const Redo());
          expect(e.source, 'before ${marker}new$marker after');
        });
      }
    }
  }
}
