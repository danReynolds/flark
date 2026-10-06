import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (var mask = 1; mask < 16; mask++) {
    test(
      'pending style combination $mask materializes once and survives Undo',
      () {
        final e = FlarkEditor(backend);
        for (final style in [
          Style.strong,
          Style.emphasis,
          Style.strikethrough,
          Style.code,
        ]) {
          if (mask & style == 0) continue;
          expect(e.apply(ToggleStyle(style)), isTrue);
          expect(e.source, '');
        }
        expect(e.typingContext, mask);
        expect(e.history.canUndo, isFalse);
        expect(e.apply(const InsertText('x')), isTrue);
        expect(e.projection.rows.single.text, 'x');
        expect(e.projection.rows.single.segments.single.styles, mask);
        expect(e.apply(const Undo()), isTrue);
        expect(e.source, '');
        expect(e.typingContext, mask);
        expect(e.apply(const InsertText(' ')), isTrue);
        expect(e.source, ' ');
        expect(e.typingContext, 0);
      },
    );
  }
  test(
    'toggling one pending style off retains the other without changing source',
    () {
      final e = FlarkEditor(backend);
      expect(e.apply(const ToggleStyle(Style.strong)), isTrue);
      expect(e.apply(const ToggleStyle(Style.emphasis)), isTrue);
      expect(e.apply(const ToggleStyle(Style.strong)), isTrue);
      expect(e.source, '');
      expect(e.typingContext, Style.emphasis);
      expect(e.apply(const InsertText('x')), isTrue);
      expect(e.source, '*x*');
    },
  );

  test('a pending style that would pair with a run beside it types plain', () {
    // Its delimiters would join the run (`*~😀***é**` paints the emphasis's
    // delimiters) or pair with literal ones (`*foo bar *é**` hides them), so
    // the row would show more than the typed text; the text goes in alone.
    for (final (source, caret, style, text, typed) in [
      ('*~😀*', 5, Style.strong, 'é', '*~😀*é'),
      ('*foo bar *', 9, Style.emphasis, 'é', '*foo bar é*'),
      ('# *[**', 6, Style.strong, 'a', '# *[**a'),
    ]) {
      final e = FlarkEditor(backend, text: source, caret: caret);
      expect(e.apply(ToggleStyle(style)), isTrue, reason: source);
      expect(e.apply(InsertText(text)), isTrue, reason: source);
      expect(e.source, typed, reason: source);
    }
    // Where the style pairs on its own, it wraps the text.
    final wrapped = FlarkEditor(backend, text: 'a **b** c', caret: 7);
    expect(wrapped.apply(const ToggleStyle(Style.emphasis)), isTrue);
    expect(wrapped.apply(const InsertText('x')), isTrue);
    expect(wrapped.source, 'a **b***x* c');
    expect(wrapped.document.caretRow.text, 'a bx c');
  });
}
