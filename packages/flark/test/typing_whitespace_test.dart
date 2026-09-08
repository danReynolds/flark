import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  test('author bold, backspace, space and continue keeps formatting', () {
    const prefix = 'This is text that really works for a change. ';
    final e = FlarkEditor(backend, text: prefix, caret: prefix.length);
    expect(e.apply(const ToggleStyle(Style.strong)), isTrue);
    for (final character in 'what'.split('')) {
      expect(e.apply(InsertText(character)), isTrue);
    }
    expect(e.source, '$prefix**what**');
    expect(e.apply(const DeleteBackward()), isTrue);
    expect(e.source, '$prefix**wha**');
    expect(e.apply(const InsertText(' ')), isTrue);
    expect(e.source, '$prefix**wha** ');
    expect(e.projection.rows.single.text, '${prefix}wha ');
    expect(e.selection.extent, e.source.length);
    expect(e.typingContext, Style.strong);
    expect(e.apply(const InsertText('x')), isTrue);
    expect(e.projection.rows.single.text, '${prefix}wha x');
    expect(e.typingContext, Style.strong);
  });
  for (final (marker, style) in [
    ('*', Style.emphasis),
    ('_', Style.emphasis),
    ('**', Style.strong),
    ('__', Style.strong),
    ('~~', Style.strikethrough),
    ('***', Style.strong | Style.emphasis),
    ('**~~', Style.strong | Style.strikethrough),
  ]) {
    final close = marker.split('').reversed.join();
    for (final deletedSpaces in [1, 2]) {
      test(
        'erase $deletedSpaces separating spaces after $marker then continue',
        () {
          final initial = '${marker}what$close';
          final e = FlarkEditor(
            backend,
            text: initial,
            caret: marker.length + 4,
          );
          e.apply(const InsertText(' '));
          e.apply(const InsertText(' '));
          for (var i = 0; i < deletedSpaces; i++) {
            expect(e.apply(const DeleteBackward()), isTrue);
            expect(e.typingContext, style);
          }
          expect(e.apply(const InsertText('x')), isTrue);
          expect(
            e.source,
            deletedSpaces == 2
                ? '${marker}whatx$close'
                : '$initial ${marker}x$close',
          );
          expect(
            e.projection.rows.single.text,
            deletedSpaces == 2 ? 'whatx' : 'what x',
          );
          expect(e.typingContext, style);
        },
      );
    }
    for (final prefix in ['say ', '> ', '- ', '# ']) {
      for (final outside in [false, true]) {
        test(
          'shorten $marker then spaces and word in $prefix outside=$outside',
          () {
            final initial = '$prefix${marker}what$close';
            final e = FlarkEditor(
              backend,
              text: initial,
              caret: initial.length - (outside ? 0 : close.length),
            );
            final rowKind = e.projection.rows.single.kind;
            final shells = e.projection.rows.single.shells
                .map((s) => s.kind)
                .toList();
            final displayPrefix = prefix == 'say ' ? prefix : '';
            var tick = 0;
            void step(
              FlarkCommand command,
              String source,
              String visible,
              int caret,
            ) {
              final before = (e.source, e.selection, e.typingContext);
              expect(
                e.apply(command, at: Duration(seconds: ++tick * 2)),
                isTrue,
              );
              void check() {
                expect(e.source, source);
                expect(e.selection.extent, caret);
                expect(e.typingContext, style);
                final row = e.projection.rows.single;
                expect(row.text, '$displayPrefix$visible');
                expect(row.kind, rowKind);
                expect(row.shells.map((s) => s.kind), shells);
                expect(
                  e.document.displayOf(caret),
                  DisplayPosition(0, row.text.length),
                );
                for (var i = displayPrefix.length; i < row.text.length; i++) {
                  if (row.text[i] == ' ') continue;
                  expect(
                    row.segments
                        .firstWhere(
                          (s) => s.displayStart <= i && s.displayEnd > i,
                        )
                        .styles,
                    style,
                  );
                }
              }

              check();
              expect(e.apply(const Undo()), isTrue);
              expect((e.source, e.selection, e.typingContext), before);
              expect(e.apply(const Redo()), isTrue);
              check();
            }

            final shortened = '$prefix${marker}wha$close';
            step(
              const DeleteBackward(),
              shortened,
              'wha',
              shortened.length - close.length,
            );
            step(
              const InsertText(' '),
              '$shortened ',
              'wha ',
              shortened.length + 1,
            );
            step(
              const InsertText(' '),
              '$shortened  ',
              'wha  ',
              shortened.length + 2,
            );
            final next = '$shortened  ${marker}x$close';
            step(
              const InsertText('x'),
              next,
              'wha  x',
              next.length - close.length,
            );
            step(
              const InsertText('y'),
              '$shortened  ${marker}xy$close',
              'wha  xy',
              next.length + 1 - close.length,
            );
          },
        );
      }
    }
  }
  for (final (initial, at, expected, visible, style) in [
    ('say **what**', 6, 'say  **what**', 'say  what', Style.strong),
    (
      'say ***what***',
      7,
      'say  ***what***',
      'say  what',
      Style.strong | Style.emphasis,
    ),
    ('**what**', 4, '**wh at**', 'wh at', Style.strong),
    ('`what`', 5, '`what `', 'what ', Style.code),
    ('[what](/url)', 5, '[what ](/url)', 'what ', Style.link),
  ]) {
    test('space retains its intended owner at $at in $initial', () {
      final e = FlarkEditor(backend, text: initial, caret: at);
      expect(e.apply(const InsertText(' ')), isTrue);
      expect(e.source, expected);
      expect(e.projection.rows.single.text, visible);
      expect(e.typingContext, style);
      expect(e.apply(const InsertText('x')), isTrue);
      expect(e.typingContext, style);
      expect(e.projection.rows.single.text, contains(' x'));
    });
  }
  for (final command in <FlarkCommand>[
    const InsertText('  '),
    const Paste('  '),
    const ReplaceRange(5, 6, '  '),
  ]) {
    test(
      'strong edge replacement and continuation via ${command.runtimeType}',
      () {
        final e = FlarkEditor(backend, text: '**what**', caret: 6);
        e.apply(const SetSelection(5, 6));
        expect(e.apply(command), isTrue);
        expect(e.source, '**wha**  ');
        expect(e.apply(const InsertText('next ')), isTrue);
        expect(e.source, '**wha**  **next** ');
        expect(e.projection.rows.single.text, 'wha  next ');
        expect(e.typingContext, Style.strong);
        expect(e.apply(const InsertText('x')), isTrue);
        expect(e.projection.rows.single.text, 'wha  next x');
        expect(e.typingContext, Style.strong);
      },
    );
  }
  test('Source mode retains a literal space before closing markers', () {
    final e = FlarkEditor(backend, text: '**what**', caret: 6);
    e.setSourceMode(true);
    expect(e.apply(const InsertText(' ')), isTrue);
    expect(e.source, '**what **');
    expect(e.selection.extent, 7);
  });
  test('typing after a multiline link hides its destination lines', () {
    const initial = '[link](   /uri\n  "title"  )\n';
    final e = FlarkEditor(backend, text: initial, caret: initial.length);
    expect(e.apply(const InsertText(':')), isTrue);
    expect(e.source, '$initial:');
    expect(e.projection.rows.single.text, 'link\n:');
    expect(e.selection.extent, initial.length + 1);
    expect(
      e.document.displayOf(e.selection.extent),
      const DisplayPosition(0, 6),
    );
    expect(e.apply(const InsertText(' next')), isTrue);
    expect(e.projection.rows.single.text, 'link\n: next');
  });
  for (final marker in ['  ', '\\']) {
    for (final newline in ['\n', '\r\n']) {
      for (final backward in [false, true]) {
        test(
          'deleting $marker break $newline backward=$backward then typing',
          () {
            final source = 'alpha$marker${newline}next';
            final caret = backward
                ? source.indexOf('next')
                : marker == '  '
                ? 5 + marker.length
                : 5;
            final e = FlarkEditor(backend, text: source, caret: caret);
            expect(
              e.apply(
                backward ? const DeleteBackward() : const DeleteForward(),
              ),
              isTrue,
            );
            expect(e.source, 'alphanext');
            expect(e.selection.extent, 5);
            expect(e.apply(const InsertText(' X')), isTrue);
            expect(e.source, 'alpha Xnext');
            expect(e.apply(const Undo()), isTrue);
            expect(e.apply(const Undo()), isTrue);
            expect(e.source, source);
          },
        );
      }
    }
  }
  for (final initial in [
    '# alpha #',
    'alpha\n=====',
    '| alpha |\n| --- |',
    '|alpha|\n|---|',
    '**alpha**',
    'alpha\nnext',
    'alpha\r\nnext',
    '> alpha\n> next',
  ]) {
    test('space then next word stays ordered in $initial', () {
      final caret = initial.startsWith('**')
          ? initial.length
          : initial.indexOf('alpha') + 5;
      final e = FlarkEditor(backend, text: initial, caret: caret);
      var typed = '';
      for (final character in [' ', ' ', 'b', 'e', 't', 'a']) {
        typed += character;
        expect(e.apply(InsertText(character)), isTrue);
        expect(e.source, initial.replaceRange(caret, caret, typed));
        expect(e.selection.extent, caret + typed.length);
      }
      expect(e.apply(const Undo()), isTrue);
      expect(e.source, initial);
      expect(e.apply(const Redo()), isTrue);
      expect(e.apply(const InsertText('!')), isTrue);
      expect(e.source, initial.replaceRange(caret, caret, '  beta!'));
    });
  }
  for (final prefix in ['', '- ', '> ', '# ', '> - ']) {
    for (final ending in ['', '\n', '\r\n']) {
      test('typing preserves word spacing after "$prefix" before $ending', () {
        final initial = '${prefix}alpha$ending';
        final e = FlarkEditor(backend, text: initial, caret: prefix.length + 5);
        var typed = '';
        for (final character in [' ', ' ', 'b', 'e', 't', 'a']) {
          typed += character;
          expect(e.apply(InsertText(character)), isTrue);
          expect(e.source, '${prefix}alpha$typed$ending');
          expect(e.selection.extent, prefix.length + 5 + typed.length);
          expect(e.document.rowAt(e.selection.extent).text, 'alpha$typed');
        }
        expect(e.apply(const Undo()), isTrue);
        expect(e.source, initial);
        expect(e.apply(const Redo()), isTrue);
        expect(e.source, '${prefix}alpha  beta$ending');
        expect(e.apply(const DeleteBackward()), isTrue);
        expect(e.apply(const InsertText('X')), isTrue);
        expect(e.source, '${prefix}alpha  betX$ending');
      });
    }
  }
}
