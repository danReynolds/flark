import 'package:flark/flark.dart';
import 'package:flark/src/kernel/edits.dart';
import 'package:test/test.dart';

void main() {
  // `abcdef` with `b` replaced by `XY`, `ZZ` inserted at 3 and `ef` removed:
  // `aXYcZZd`.
  final edits = Edits([(1, 2, 'XY'), (3, 3, 'ZZ'), (4, 6, '')]);

  test('apply makes each splice in order', () {
    expect(edits.apply('abcdef'), 'aXYcZZd');
    expect(Edits([]).apply('abc'), 'abc');
    expect(Edits([(0, 3, 'x')]).apply('abc'), 'x');
  });

  test('isDisjoint holds for sorted splices that do not overlap', () {
    expect(Edits.isDisjoint([(0, 1, ''), (1, 2, ''), (2, 2, 'x')]), isTrue);
    expect(Edits.isDisjoint([(1, 3, ''), (2, 4, '')]), isFalse);
    expect(Edits.isDisjoint([(3, 3, ''), (1, 1, '')]), isFalse);
  });

  test('forward keeps an offset where its text went', () {
    expect(
      [for (var o = 0; o <= 6; o++) edits.forward(o)],
      // Before the replacement, at its start, its end (where `c` is), at the
      // insertion (before what it put there), at the removal's start (after
      // `d`), inside it, and the end.
      [0, 1, 3, 4, 7, -1, 7],
    );
  });

  test('forward with caret goes with typed text', () {
    expect(
      [for (var o = 0; o <= 6; o++) edits.forward(o, caret: true)],
      // Past what the insertion put at 3, and from inside the removed `ef`
      // to where it was.
      [0, 1, 3, 6, 7, 7, 7],
    );
    // Inside a replaced range a caret goes to the start of what replaced it.
    expect(Edits([(1, 4, 'XY')]).forward(2, caret: true), 1);
    expect(Edits([(1, 4, 'XY')]).forward(2), -1);
  });

  test('back finds an offset of the edited source in the original', () {
    expect(
      [for (var o = 0; o <= 7; o++) edits.back(o)],
      // `XY` and `ZZ` are new text.
      [0, -1, -1, 2, -1, -1, 3, 6],
    );
  });

  test('withInsertion puts text into the edit it meets, else on its own', () {
    // Before the first edit's text: an edit of its own.
    expect(Edits([(2, 3, 'XY')]).withInsertion(1, '!').list, [
      (1, 1, '!'),
      (2, 3, 'XY'),
    ]);
    // Within, or at the end of, an edit's text: into that text.
    expect(Edits([(2, 3, 'XY')]).withInsertion(3, '!').list, [(2, 3, 'X!Y')]);
    expect(Edits([(2, 3, 'XY')]).withInsertion(4, '!').list, [(2, 3, 'XY!')]);
    // Past every edit: at the original offset it shows.
    expect(Edits([(2, 3, 'XY')]).withInsertion(6, '!').list, [
      (2, 3, 'XY'),
      (5, 5, '!'),
    ]);
    // The source it makes is the edited source with the text put at [at].
    final made = edits.apply('abcdef');
    expect(
      edits.withInsertion(5, '!').apply('abcdef'),
      made.replaceRange(5, 5, '!'),
    );
  });

  test('between is the one edit from where two sources differ', () {
    expect(Edits.between('abcd', 'aXd').list, [(1, 3, 'X')]);
    expect(Edits.between('abc', 'abc').list, [(3, 3, '')]);
    // A repeated character is taken from the front.
    expect(Edits.between('aa', 'aaa').list, [(2, 2, 'a')]);
    expect(Edits.between('', 'x').list, [(0, 0, 'x')]);
    for (final (was, now) in [('abc', 'xbz'), ('a\nb', 'a\n\nb'), ('ab', '')]) {
      expect(Edits.between(was, now).apply(was), now);
    }
  });

  test('lengths give each splice with its text length', () {
    expect(edits.lengths, [(1, 2, 2), (3, 3, 2), (4, 6, 0)]);
  });

  test('a spelling carries the selection as a caret goes', () {
    final spelling = Spelling.carrying(
      Edits([(0, 0, '# ')]),
      const FlarkSelection(0, 3),
      asAsked: true,
    );
    expect(spelling.selection, const FlarkSelection(2, 5));
    expect(spelling.asAsked, isTrue);
  });
}
