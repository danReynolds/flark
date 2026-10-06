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
}
