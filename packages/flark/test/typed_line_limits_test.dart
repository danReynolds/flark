import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  // Text typed where it changes its line's block structure is checked by
  // parsing each spelling of it, which past the live tier no parse can do:
  // there the first spelling that would leave the tier enters source mode,
  // as ordinary text does (EP1-RESULT-PRESENTATION-001), rather than being
  // refused.
  for (final (label, source, caret, command, limit, edited, at) in [
    ('an empty row', 'para\n\nb', 5, const InsertText('x'), 7, 'para\nx\nb', 6),
    ('a lazy line', '> a\nb', 5, const InsertText('c'), 5, '> a\nbc', 6),
    ('a line start', 'a\nb', 2, const InsertText(' '), 3, 'a\n b', 3),
    ('an empty row', 'para\n\nb', 5, const Paste('xy'), 8, 'para\nxy\nb', 7),
    ('a rule', '---', 3, const InsertText('x'), 4, '---\nx', 5),
    ('a hidden item marker', '1.\n', 2, const InsertText('s'), 4, '1. s\n', 4),
    // The text as it is would be the table's next row, and the spelling
    // that keeps it apart is past the tier.
    (
      'the row under a table',
      '| a |\n| - |\n| b |\n',
      18,
      const InsertText('x'),
      19,
      '| a |\n| - |\n| b |\n\nx',
      20,
    ),
    // The fence the run completes is past the tier.
    (
      'a rule, completing a fence',
      '---',
      3,
      const InsertText('```'),
      8,
      '---\n```\n\n```\n\n',
      8,
    ),
  ]) {
    test('${command.runtimeType} on $label past the live tier', () {
      final e = FlarkEditor(
        backend,
        text: source,
        caret: caret,
        syncLimit: limit,
      );
      expect(e.apply(command), isTrue);
      expect(e.lastRejection, isNull);
      expect(e.sourceMode, isTrue);
      expect(e.source, edited);
      expect(e.selection, FlarkSelection.collapsed(at));
    });
  }

  test('a spelling inside the tier still commits live', () {
    // Text under a table takes an empty line before it; with room for it
    // the edit stays live.
    final e = FlarkEditor(
      backend,
      text: '| a |\n| - |\n| b |\n',
      caret: 18,
      syncLimit: 20,
    );
    expect(e.apply(const InsertText('x')), isTrue);
    expect(e.sourceMode, isFalse);
    expect(e.source, '| a |\n| - |\n| b |\n\nx');
    expect(e.document.caretRow.kind, RowKind.paragraph);
  });
}
