import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final (open, close, style) in [
    ('**', '**', Style.strong),
    ('__', '__', Style.strong),
    ('*', '*', Style.emphasis),
    ('***', '***', Style.strong | Style.emphasis),
    ('~~', '~~', Style.strikethrough),
    ('`', '`', Style.code),
    ('[', '](/url)', Style.link),
  ]) {
    for (final prefix in ['', 'say ', '> ']) {
      for (final suffix in ['', ' next']) {
        for (final leading in [false, true]) {
          test(
            'pointer edge continues $open in $prefix / $suffix half=$leading',
            () {
              final original = '$prefix${open}what$close$suffix';
              final e = FlarkEditor(
                backend,
                text: original,
                caret: original.length,
              );
              final displayPrefix = prefix == 'say ' ? prefix : '';
              final contentEnd = prefix.length + open.length + 4;
              e.apply(
                PlaceCaret(0, displayPrefix.length + 4, leadingHalf: leading),
              );
              expect(e.selection.extent, contentEnd);
              expect(e.typingContext, style);
              expect(e.apply(const InsertText('x')), isTrue);
              expect(e.source, '$prefix${open}whatx$close$suffix');
              expect(
                e.projection.rows.single.text,
                '${displayPrefix}whatx$suffix',
              );
              expect(e.typingContext, style);
              expect(e.apply(const Undo()), isTrue);
              expect(e.source, original);
              expect(e.selection.extent, contentEnd);
              expect(e.apply(const Redo()), isTrue);
              expect(e.apply(const InsertText('y')), isTrue);
              expect(e.source, '$prefix${open}whatxy$close$suffix');
              expect(e.typingContext, style);
              if (suffix.isNotEmpty) {
                e.apply(
                  PlaceCaret(0, displayPrefix.length + 7, leadingHalf: leading),
                );
                expect(e.typingContext, 0);
                expect(e.apply(const InsertText('z')), isTrue);
                expect(e.source, '$prefix${open}whatxy$close znext');
              }
            },
          );
        }
      }
    }
  }
  for (final leading in [false, true]) {
    test(
      'touching non-whitespace styles retain explicit hit side $leading',
      () {
        final e = FlarkEditor(backend, text: '**bold***italic*');
        e.apply(PlaceCaret(0, 4, leadingHalf: leading));
        expect(e.typingContext, leading ? Style.emphasis : Style.strong);
        expect(e.apply(const InsertText('x')), isTrue);
        expect(e.source, leading ? '**bold***xitalic*' : '**boldx***italic*');
      },
    );
  }
}
