import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final marker in ['*', '**', '~~', '***']) {
    for (final backward in [false, true]) {
      test(
        'deletion preserves $marker beside exposed whitespace backward=$backward',
        () {
          final source = backward
              ? '${marker}two x$marker'
              : '${marker}x two$marker';
          final caret = marker.length + (backward ? 5 : 0);
          final e = FlarkEditor(backend, text: source, caret: caret);
          expect(
            e.apply(backward ? const DeleteBackward() : const DeleteForward()),
            isTrue,
          );
          final expected = backward
              ? '${marker}two$marker '
              : ' ${marker}two$marker';
          expect(e.source, expected);
          expect(e.projection.rows.single.text.trim(), 'two');
          expect(e.apply(const InsertText('y')), isTrue);
          expect(
            e.source,
            backward
                ? '${marker}two$marker ${marker}y$marker'
                : ' ${marker}ytwo$marker',
          );
          e.apply(const Undo());
          e.apply(const Undo());
          expect(e.source, source);
          expect(e.selection.extent, caret);
        },
      );
    }
  }
  for (final source in ['one two three', 'one **two three**', 'café 👩🏽‍💻']) {
    for (final sourceMode in [false, true]) {
      test('word deletion, next key and undo: $source source=$sourceMode', () {
        final e = FlarkEditor(
          backend,
          text: source,
          caret: source.endsWith('**') && !sourceMode
              ? source.length - 2
              : source.length,
        );
        if (sourceMode) e.setSourceMode(true);
        final before = e.selection;
        expect(e.apply(const DeleteBackward(word: true)), isTrue);
        final expected = source == 'one two three'
            ? 'one two '
            : source == 'café 👩🏽‍💻'
            ? 'café '
            : sourceMode
            ? 'one **two '
            : 'one **two** ';
        expect(e.source, expected);
        expect(e.apply(const InsertText('x')), isTrue);
        expect(
          e.source,
          source == 'one **two three**' && !sourceMode
              ? 'one **two** **x**'
              : '${expected}x',
        );
        e.apply(const Undo());
        e.apply(const Undo());
        expect(e.source, source);
        expect(e.selection, before);
      });
    }
  }
  for (final sourceMode in [false, true]) {
    test('forward word deletion source=$sourceMode', () {
      final e = FlarkEditor(backend, text: 'one two three', caret: 4);
      if (sourceMode) e.setSourceMode(true);
      expect(e.apply(const DeleteForward(word: true)), isTrue);
      expect(e.source, 'one  three');
      e.apply(const InsertText('x'));
      expect(e.source, 'one x three');
      e.apply(const Undo());
      e.apply(const Undo());
      expect(e.source, 'one two three');
      expect(e.selection.extent, 4);
    });
  }
}
