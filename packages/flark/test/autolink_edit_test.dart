import 'package:flark/flark.dart';
import 'package:test/test.dart';
import 'support/invariants.dart';

void main() {
  test('a bare URL ending in an angle is not a bracketed autolink', () {
    const source = '# < http://foo.ba&`>';
    final e = FlarkEditor(
      createParseBackend(),
      text: source,
      caret: source.length,
    );
    checkInvariants(e.source, e.document.model, e.projection, 'bare URL');
    expect(e.projection.rows.single.text, '< http://foo.ba&`>');
    expect(e.apply(const InsertText('!')), isTrue);
    checkInvariants(e.source, e.document.model, e.projection, 'next key');
    expect(e.projection.rows.single.text, '< http://foo.ba&`>!');
    e.apply(const Undo());
    expect(e.source, source);
  });
}
