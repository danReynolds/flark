import 'package:flark/code.dart';
import 'package:flark_tree_sitter/flark_highlighting.dart';
import 'package:test/test.dart';

List<(int, int, String?)> _ranges(CodeHighlight? colors) => [
  for (final token in colors!.tokens) (token.start, token.end, token.kind),
];

void main() {
  // `return value;` as keyword, space, variable, punctuation.
  final colors = CodeHighlight('dart', const [
    CodeToken(0, 6, 'keyword'),
    CodeToken(6, 7, null),
    CodeToken(7, 12, 'variable'),
    CodeToken(12, 13, 'punctuation'),
  ]);
  const before = 'return value;';

  test('typing continues the token it extends; later tokens shift', () {
    expect(_ranges(shiftCodeHighlight(colors, before, 'return valuex;')), [
      (0, 6, 'keyword'),
      (6, 7, null),
      (7, 13, 'variable'),
      (13, 14, 'punctuation'),
    ]);
  });

  test('deletion shrinks the token that lost text', () {
    expect(_ranges(shiftCodeHighlight(colors, before, 'return valu;')), [
      (0, 6, 'keyword'),
      (6, 7, null),
      (7, 11, 'variable'),
      (11, 12, 'punctuation'),
    ]);
  });

  test('insertion at the start continues the first token', () {
    expect(_ranges(shiftCodeHighlight(colors, before, 'xreturn value;')), [
      (0, 7, 'keyword'),
      (7, 8, null),
      (8, 13, 'variable'),
      (13, 14, 'punctuation'),
    ]);
  });

  test('a pasted block stays plain while its surroundings keep colors', () {
    const pasted = 'x = 1;\n  other(';
    final after = before.replaceRange(7, 7, pasted);
    expect(_ranges(shiftCodeHighlight(colors, before, after)), [
      (0, 6, 'keyword'),
      (6, 7 + pasted.length, null),
      (7 + pasted.length, 12 + pasted.length, 'variable'),
      (12 + pasted.length, 13 + pasted.length, 'punctuation'),
    ]);
  });

  test('an edit that keeps no text borrows no colors', () {
    expect(shiftCodeHighlight(colors, before, 'puts "new"'), isNull);
  });

  test('an unchanged text keeps identical colors', () {
    expect(
      _ranges(shiftCodeHighlight(colors, before, before)),
      _ranges(colors),
    );
  });

  test('token edges never split a surrogate pair', () {
    final emoji = CodeHighlight('dart', const [
      CodeToken(0, 1, 'keyword'),
      CodeToken(1, 3, 'string'),
      CodeToken(3, 4, 'keyword'),
    ]);
    // U+1F600 and U+1F601 share their high surrogate.
    final shifted = shiftCodeHighlight(emoji, 'a\u{1F600}b', 'a\u{1F601}b');
    for (final token in shifted!.tokens) {
      expect(token.start, isNot(2));
      expect(token.end, isNot(2));
    }
    expect(shifted.tokens.last.end, 4);
  });
}
