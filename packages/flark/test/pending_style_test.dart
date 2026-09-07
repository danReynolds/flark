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
}
