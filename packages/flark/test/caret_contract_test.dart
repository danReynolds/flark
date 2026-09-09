/// Directly named cases for the reachability and erasability defects the
/// corpus sweep found: every one of these was a caret the user could see but
/// not move past, markup they could not remove, or a display position two
/// different offsets claimed.
library;

import 'package:flark/flark.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

List<int> walk(FlarkEditor e, {required bool forward}) {
  e.apply(SetSelection.caret(
    e.document.legalize(forward ? 0 : e.source.length),
  ));
  final seen = <int>[e.selection.extent];
  while (e.apply(MoveCaret(
        forward ? MoveDirection.forward : MoveDirection.backward,
      )) &&
      seen.length < 400) {
    seen.add(e.selection.extent);
  }
  return seen;
}

void main() {
  final backend = createParseBackend();

  test('the caret leaves a table whose last row is short of its columns', () {
    for (final source in [
      '| a | b |\n| - | - |\n| c |\n\nafter\n',
      '| a | b |\n| - | - |\n| c | d |\nbar\n\nafter\n',
      '| a | b | c |\n| - | - | - |\n| x |\n\nafter\n',
    ]) {
      final e = FlarkEditor(backend, text: source, caret: 0);
      expect(walk(e, forward: true).last, source.length,
          reason: 'forward stopped inside $source');
      expect(walk(e, forward: false).last, lessThanOrEqualTo(2),
          reason: 'backward stopped inside $source');
      // Down must leave the row it starts on, every time.
      e.apply(SetSelection.caret(e.document.legalize(0)));
      var row = e.document.displayOf(e.selection.extent).row;
      while (e.apply(
          const MoveCaret(MoveDirection.forward, unit: MoveUnit.row))) {
        final next = e.document.displayOf(e.selection.extent).row;
        expect(next, greaterThan(row), reason: 'Down stalled in $source');
        row = next;
      }
    }
  });

  test('the caret leaves indented code whose indent is a partial tab', () {
    final e = FlarkEditor(backend, text: '- foo\n\n\t\tbar\n', caret: 0);
    expect(walk(e, forward: true).last, 13);
    expect(walk(e, forward: false).last, 2);
  });

  test('a setext underline holds no caret', () {
    for (final source in ['Foo\n===\n\nafter\n', 'Foo\n---\n\nafter\n']) {
      final e = FlarkEditor(backend, text: source, caret: 5);
      expect(e.projection.lineSpans(1), isEmpty);
      expect(e.document.isLegal(7), isFalse);
      // A host restoring a saved offset inside the underline lands somewhere
      // it can type without destroying the heading.
      e.apply(const InsertText('x'));
      expect(e.projection.rows.first.kind, RowKind.heading);
      expect(e.source.startsWith(source.substring(0, 7)), isTrue);
    }
  });

  test('a blank line inside indented code holds no caret of its own', () {
    final e = FlarkEditor(backend, text: '  \tfoo\n', caret: 6);
    expect(e.apply(const Newline()), isTrue);
    final row = e.projection.rows.first;
    expect(row.kind, RowKind.codeBlock);
    // The line the newline made is not represented in the row's text, so it
    // does not get a caret that paints at the end of the line above.
    final shared = <int>[
      for (var o = 0; o <= e.source.length; o++)
        if (e.document.isLegal(o) &&
            e.document.displayOf(o).row == 0 &&
            e.document.displayOf(o).offset == row.text.length)
          o,
    ];
    expect(shared, hasLength(1));
  });

  test('an escape and the character it hides are one caret unit', () {
    final e = FlarkEditor(backend, text: 'a\\*b', caret: 0);
    expect(e.document.isLegal(2), isFalse);
    e.apply(SetSelection.caret(1));
    e.apply(const InsertText('Z'));
    expect(e.source, 'aZ\\*b');
    expect(e.projection.rows.single.text, 'aZ*b');
  });

  test('a line ending inside inline HTML is displayed', () {
    for (final (source, display) in [
      ('a <b\nc> d\n', 'a <b\nc> d'),
      ('<a h\nref="x">\n', '<a h\nref="x">'),
    ]) {
      final e = FlarkEditor(backend, text: source, caret: 0);
      expect(e.projection.rows.first.text, display);
      checkInvariants(source, e.document.model, e.projection, source);
    }
  });

  test('adjacent entities each display their own text', () {
    for (final (source, display) in [
      ('a&amp;&amp;b', 'a&&b'),
      ('&lt;&gt;', '<>'),
      ('a&nbsp;&nbsp;b', 'a  b'),
    ]) {
      final e = FlarkEditor(backend, text: source, caret: 0);
      final row = e.projection.rows.single;
      expect(row.text, display);
      for (final s in row.segments) {
        expect(s.displayEnd, greaterThan(s.displayStart),
            reason: 'empty display for a segment of $source');
      }
    }
  });

  test('a thematic break does not own the blank lines after it', () {
    for (final source in ['***\n\n', '---\n\n\n']) {
      final e = FlarkEditor(backend, text: source, caret: 0);
      final rule = e.projection.rows.first;
      expect(rule.kind, RowKind.thematicBreak);
      expect(rule.lineCount, 1);
      expect(e.projection.rows.where((r) => r.kind == RowKind.blank).length,
          greaterThan(1));
    }
  });

  test('whitespace on a blank line outside a container is erasable', () {
    for (final source in ['\t', ' \t', '  \t']) {
      final e = FlarkEditor(backend, text: source, caret: source.length);
      expect(e.apply(const DeleteBackward()), isTrue);
      expect(e.source, '');
    }
  });

  test('the break after a fence that displays nothing can be deleted', () {
    // The fence itself only goes whole when it is closed: an unclosed one ends
    // at its opening line, so deleting that range would orphan any closer.
    for (final (source, erased) in [
      ('~~~\n', '~~~'),
      ('a\n\n~~~\n', 'a\n\n~~~'),
      ('~~~\n~~~\n', ''),
    ]) {
      final e = FlarkEditor(backend, text: source, caret: source.length);
      var steps = 0;
      while (e.source != erased && steps < 12) {
        e.apply(SetSelection.caret(e.source.length));
        expect(e.apply(const DeleteBackward()), isTrue,
            reason: 'stuck at ${e.source} from $source');
        steps++;
      }
      expect(e.source, erased);
    }
  });

  test('a fence that displays nothing never absorbs a row', () {
    // Its lines are all delimiters, so text joined onto one becomes the info
    // string and the editor never shows it again.
    for (final (source, caret, forward) in [
      ('- ```ruby\n- def foo(x)\n- ```\n- ', 9, true),
      ('- ```\n- [foo]: /url\n- ```\n- ', 5, true),
    ]) {
      final e = FlarkEditor(backend, text: source, caret: caret);
      final visible = e.document.visibleText(0, source.length);
      expect(
        e.apply(forward ? const DeleteForward() : const DeleteBackward()),
        isFalse,
        reason: 'joined into a delimiter line of $source',
      );
      expect(e.source, source);
      expect(e.document.visibleText(0, e.source.length), visible);
    }
  });

  test('a thematic break is exactly its line, in a container too', () {
    for (final (source, erased) in [
      ('***', ''),
      ('> ---\n\n', '> \n\n'),
      ('> ***\nfoo', '> \nfoo'),
      ('- *\t*\t*\t\n- ', '- \t\n- '),
    ]) {
      final rule = FlarkEditor(backend, text: source, caret: 0).projection.rows
          .firstWhere((r) => r.kind == RowKind.thematicBreak);
      expect(source.substring(rule.sourceStart, rule.sourceEnd).trim(),
          isNot(isEmpty),
          reason: 'the rule range must be the whole rule in $source');
      final e = FlarkEditor(backend, text: source, caret: rule.sourceEnd);
      expect(e.apply(const DeleteBackward()), isTrue);
      expect(e.source, erased);
    }
  });

  test('a title closed by a literal backslash is a definition', () {
    final e = FlarkEditor(backend, text: '[foo]: /url "a\\"\n\n[foo]\n');
    expect(e.projection.rows.first.kind, RowKind.definition);
    // The kernel accepted the parse, so ordinary editing is not refused.
    final f = FlarkEditor(backend, text: '[foo]: /url "a\\"b', caret: 17);
    expect(f.apply(const DeleteBackward()), isTrue);
    expect(f.lastRejection, isNull);
  });
}
