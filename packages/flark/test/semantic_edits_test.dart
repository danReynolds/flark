import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final caret in [3, 4]) {
    test(
      'Return at either side of whitespace retains both strong fragments $caret',
      () {
        final e = FlarkEditor(backend, text: '**a b**', caret: caret);
        expect(e.apply(const Newline()), isTrue);
        final left = caret == 4 ? 'a ' : 'a';
        expect(e.projection.rows.single.text, '$left\nb');
        expect(
          e.projection.rows.single.segments
              .where(
                (s) =>
                    !s.lineBreak &&
                    e.source
                        .substring(s.sourceStart, s.sourceEnd)
                        .trim()
                        .isNotEmpty,
              )
              .map((s) => s.styles),
          everyElement(Style.strong),
        );
        expect(e.apply(const InsertText('x')), isTrue);
        expect(e.projection.rows.single.text, '$left\nxb');
        expect(e.typingContext, Style.strong);
      },
    );
  }
  const styles = [
    ('*', Style.emphasis),
    ('**', Style.strong),
    ('~~', Style.strikethrough),
    ('`', Style.code),
    ('***', Style.emphasis | Style.strong),
  ];
  for (final (marker, style) in styles) {
    for (final backward in [true, false]) {
      test(
        'joining matching $marker owners keeps glyphs and style backward=$backward',
        () {
          final original = '${marker}a$marker\n\n${marker}b$marker';
          final e = FlarkEditor(backend, text: original);
          e.apply(
            PlaceCaret(
              backward ? 2 : 0,
              backward ? 0 : 1,
              leadingHalf: backward,
            ),
          );
          final command = backward
              ? const DeleteBackward()
              : const DeleteForward();
          expect(e.apply(command), isTrue);
          expect(e.apply(command), isTrue);
          expect(e.source, '${marker}ab$marker');
          expect(e.projection.rows.single.text, 'ab');
          expect(
            e.projection.rows.single.segments.map((s) => s.styles),
            everyElement(style),
          );
          expect(e.apply(const InsertText('x')), isTrue);
          expect(e.source, '${marker}axb$marker');
          expect(e.apply(const Undo()), isTrue);
          expect(e.apply(const Undo()), isTrue);
          expect(e.apply(const Undo()), isTrue);
          expect(e.source, original);
        },
      );
    }
    for (final prefix in ['', '- ', '> ', '# ']) {
      for (final middle in [false, true]) {
        test(
          'Return preserves $marker in "$prefix" at ${middle ? 'middle' : 'end'}',
          () {
            final source = '$prefix${marker}ab$marker';
            final caret = prefix.length + marker.length + (middle ? 1 : 2);
            final e = FlarkEditor(backend, text: source, caret: caret);
            expect(e.apply(const Newline()), isTrue);
            final continuation = prefix == '# ' ? '' : prefix;
            expect(
              e.source,
              middle
                  ? '$prefix${marker}a$marker\n$continuation${marker}b$marker'
                  : '$source\n$continuation',
            );
            final glyphs = [
              for (final r in e.projection.rows)
                for (final s in r.segments)
                  if (!s.lineBreak)
                    (r.text.substring(s.displayStart, s.displayEnd), s.styles),
            ];
            expect(
              glyphs,
              middle ? [('a', style), ('b', style)] : [('ab', style)],
            );
            expect(e.typingContext, middle ? style : 0);
            expect(e.apply(const InsertText('x')), isTrue);
            expect(
              e.document.rowAt(e.selection.extent).text,
              contains(middle ? 'xb' : 'x'),
            );
            expect(e.typingContext, middle ? style : 0);
            expect(e.apply(const Undo()), isTrue);
            expect(e.apply(const Undo()), isTrue);
            expect(
              (e.source, e.selection.extent, e.typingContext),
              (source, caret, style),
            );
          },
        );
      }
    }
    for (final backward in [true, false]) {
      for (final inside in [true, false]) {
        test(
          'empty $marker ${backward ? 'backward' : 'forward'} retains only starting context $inside',
          () {
            final source = '${marker}t$marker';
            final caret = backward
                ? (inside ? marker.length + 1 : source.length)
                : (inside ? marker.length : 0);
            final e = FlarkEditor(backend, text: source, caret: caret);
            expect(
              e.apply(
                backward ? const DeleteBackward() : const DeleteForward(),
              ),
              isTrue,
            );
            expect(e.source, '');
            expect(e.typingContext, inside ? style : 0);
            expect(e.apply(const InsertText('x')), isTrue);
            expect(e.source, inside ? '${marker}x$marker' : 'x');
            expect(
              e.projection.rows.single.segments.single.styles,
              inside ? style : 0,
            );
          },
        );
      }
    }
  }
  for (final start in [2, 3]) {
    for (final command in <FlarkCommand>[
      const InsertText('X'),
      const Paste('X'),
      ReplaceRange(start, 8, 'X'),
      const DeleteBackward(),
      const DeleteForward(),
      const Newline(),
    ]) {
      test(
        'partial owner range from $start rejects ${command.runtimeType} atomically',
        () {
          final e = FlarkEditor(backend, text: '**ab** cd');
          e.apply(SetSelection(start, 8));
          final before = e.snapshot;
          expect(e.apply(command), isFalse);
          expect(e.snapshot, same(before));
          expect(e.history.canUndo, isFalse);
        },
      );
    }
  }
  for (final source in ['- one\n- ', '> one\n> ']) {
    test('empty container exits before subsequent typing: $source', () {
      final e = FlarkEditor(backend, text: source, caret: source.length);
      expect(e.apply(const Newline()), isTrue);
      expect(e.apply(const InsertText('p')), isTrue);
      final row = e.document.rowAt(e.selection.extent);
      expect(row.text, 'p');
      expect(row.shells, isEmpty);
      expect(e.apply(const Undo()), isTrue);
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, source);
    });
  }
}
