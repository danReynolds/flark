import 'package:flark/flark.dart';
import 'package:test/test.dart';
import 'support/invariants.dart';

void main() {
  final backend = createParseBackend();
  for (final (source, visible) in [
    ('`a\nb`c`', 'a bc`'),
    ('> `a\n> b`c`', 'a bc`'),
    ('#### **foo *bar***\n].\n `!\n***-<***b`-`', '].\n! ***-<***b-`'),
  ]) {
    test(
      'multiline code owns its closing fence and paints a normalized space: $source',
      () {
        final e = FlarkEditor(backend, text: source);
        checkInvariants(
          source,
          e.document.model,
          e.projection,
          'multiline code',
        );
        expect(e.projection.rows.last.text, visible);
        e.apply(SetSelection(source.length, source.length));
        expect(e.apply(const InsertText('x')), isTrue);
        expect(e.projection.rows.last.text, '${visible}x');
        expect(e.apply(const Undo()), isTrue);
        expect(e.source, source);
      },
    );
  }
}
