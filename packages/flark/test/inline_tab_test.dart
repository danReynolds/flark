import 'package:flark/flark.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

void main() {
  test('tabs in a pipeless cell retain distinct source positions', () {
    const source = ' h\n --- |\n**<\t\t**';
    final e = FlarkEditor(createParseBackend(), text: source);
    expect(e.projection.rows.last.text, '**<\t\t**');
    checkInvariants(source, e.document.model, e.projection, 'inline tabs');
    final at = source.indexOf('**<') + 1;
    e.apply(SetSelection.caret(at));
    e.apply(const InsertText('x'));
    expect(e.source, source.replaceRange(at, at, 'x'));
    e.apply(const Undo());
    expect((e.source, e.selection.extent), (source, at));
  });
}
