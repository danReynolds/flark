import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// An input method corrects a word beside the one it composes (`teh` to
/// `thee` while composing `wor`), as one composition. Its commit types it
/// all; typing a correction inside a span with the composed word is
/// refused as one edit, so text composed in more than one place stays as
/// composed rather than being withdrawn with the correction.
void main() {
  final backend = createParseBackend();

  TextEditingValue composing(String text, int caret, [TextRange? range]) =>
      TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: caret),
        composing: range ?? TextRange.empty,
      );

  for (final (name, source, wrong, right) in [
    ('emphasis', '*teh* ', 'teh', 'thee'),
    ('emphasis, same length', '*teh* ', 'teh', 'the'),
    ('strong', '**teh** ', 'teh', 'thee'),
    ('a link', '[teh](u) ', 'teh', 'thee'),
    ('code', '`teh` ', 'teh', 'thee'),
    ('a heading', '# teh ', 'teh', 'thee'),
    ('a list item', '- teh ', 'teh', 'thee'),
    ('a table cell', '| a | b |\n| - | - |\n| teh | ', 'teh', 'thee'),
  ]) {
    test('a correction in $name commits with the composed word', () {
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: source.length),
      );
      final p = source.length;
      expect(
        c.receive(
          composing('${source}wor', p + 3, TextRange(start: p, end: p + 3)),
        ),
        isTrue,
      );
      final fixed = source.replaceFirst(wrong, right);
      final q = fixed.length;
      c.receive(
        composing('${fixed}wor', q + 3, TextRange(start: q, end: q + 3)),
      );
      c.receive(composing('${fixed}wor', q + 3));
      expect(c.text, '${fixed}wor');
      expect(c.editor.composing, isFalse);
      c.dispose();
    });
  }

  // A cancel after the correction, the input method removing the text it
  // composes or Escape, removes that text only: the correction stays, one
  // undo step. A correction the length of the word kept the composing range
  // where it was, so the cancel took the correction back with the word; one
  // that moved the range left Escape committing the word.
  for (final (name, source, caret, fixed, at) in [
    ('before the word', '*teh* ', 6, '*the* ', 6),
    ('before the word, longer', '*teh* ', 6, '*thee* ', 7),
    ('after the word', 'a teh', 1, 'a the', 1),
    ('after the word, longer', 'a teh', 1, 'a thee', 1),
  ]) {
    for (final escape in [false, true]) {
      test('${escape ? 'Escape' : 'a removal'} after a correction $name '
          'keeps the correction', () {
        final c = FlarkController(
          FlarkEditor(backend, text: source, caret: caret),
        );
        c.receive(
          composing(
            source.replaceRange(caret, caret, 'wor'),
            caret + 3,
            TextRange(start: caret, end: caret + 3),
          ),
        );
        c.receive(
          composing(
            fixed.replaceRange(at, at, 'wor'),
            at + 3,
            TextRange(start: at, end: at + 3),
          ),
        );
        expect(c.text, fixed.replaceRange(at, at, 'wor'));
        if (escape) {
          c.finishComposition(cancel: true);
        } else {
          c.receive(composing(fixed, at));
        }
        expect(c.text, fixed);
        expect(c.editor.selection, FlarkSelection.collapsed(at));
        expect(c.editor.composing, isFalse);
        c.command(const Undo());
        expect(c.text, source);
        c.dispose();
      });
    }
  }
}
