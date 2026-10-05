import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  // A dash typed under a paragraph gets a blank line of its own, but that
  // line must not cost the edit its admission: at a limit the dash is typed
  // as it is, and the editor stays live.
  for (final (label, create) in [
    (
      'source limit',
      () => FlarkEditor(
        backend,
        text: 'para\n',
        caret: 5,
        sourceLimit: 6,
        syncLimit: 6,
      ),
    ),
    (
      'sync limit',
      () => FlarkEditor(backend, text: 'para\n', caret: 5, syncLimit: 6),
    ),
    (
      'block limit',
      () => FlarkEditor(
        backend,
        text: 'para\n',
        caret: 5,
        liveLimits: const FlarkLiveLimits(blocks: 2),
      ),
    ),
    (
      'container depth limit',
      () => FlarkEditor(
        backend,
        text: 'para\n',
        caret: 5,
        liveLimits: const FlarkLiveLimits(containerDepth: 0),
      ),
    ),
    (
      'line limit',
      () => FlarkEditor(
        backend,
        text: 'para\n',
        caret: 5,
        liveLimits: const FlarkLiveLimits(lines: 2),
      ),
    ),
  ]) {
    test('a dash typed under a paragraph at the $label stays one edit', () {
      final e = create();
      expect(e.apply(const InsertText('-')), isTrue);
      expect(e.source, 'para\n-');
      expect(e.sourceMode, isFalse);
    });
  }
  respellingLimits();
  respellingRefusals();
}

/// The kernel's own respellings — the blank line that keeps an emptied item
/// apart from the paragraph above, or the next block apart from a heading's
/// text — are checked by the parser, which only the live tier has. Past the
/// tier a respelling is passed over, so the editor stays live and the edit
/// refuses where the plain edit would change the blocks around. The edit as
/// asked may still leave the tier into source mode.
void respellingLimits() {
  final backend = createParseBackend();
  for (final (label, source, caret, lines, command) in [
    ('an emptied item', 'a\n- b', 5, 2, const DeleteBackward() as FlarkCommand),
    ('a split heading', '# a\n    b', 2, 3, const Newline()),
    ('a lifted heading', '### a\n    b', 4, 2, const DeleteBackward()),
  ]) {
    test('$label does not leave the live tier for a respelling', () {
      final e = FlarkEditor(
        backend,
        text: source,
        caret: caret,
        liveLimits: FlarkLiveLimits(lines: lines),
      );
      expect(e.apply(command), isFalse);
      expect(e.source, source);
      expect(e.sourceMode, isFalse);
      expect(e.lastRejection, FlarkRejection.unsupportedEdit);
    });
  }
  test('a delimiter row typed at the line limit is typed as it is', () {
    // The line break that would keep `next` out of the table is a
    // respelling; past the tier the table takes `next` as Markdown reads it.
    final e = FlarkEditor(
      backend,
      text: '| a | b |\n| - | \nnext',
      caret: 16,
      liveLimits: const FlarkLiveLimits(lines: 3),
    );
    expect(e.apply(const InsertText('-')), isTrue);
    expect(e.source, '| a | b |\n| - | -\nnext');
    expect(e.sourceMode, isFalse);
  });
  test('the edit as asked still leaves the live tier', () {
    final e = FlarkEditor(
      backend,
      text: 'para',
      caret: 4,
      liveLimits: const FlarkLiveLimits(lines: 1),
    );
    expect(e.apply(const Newline()), isTrue);
    expect(e.source, 'para\n');
    expect(e.sourceMode, isTrue);
  });
}

/// A respelling the parser refuses, here past the writable source limit, is
/// passed over as one past the live tier is: only the edit as asked costs
/// the edit its admission. Where no other spelling keeps the edit, it
/// refuses as one Markdown cannot make; where a later one fits, it commits.
void respellingRefusals() {
  final backend = createParseBackend();
  FlarkEditor limited(String source, int caret, int limit) => FlarkEditor(
    backend,
    text: source,
    caret: caret,
    sourceLimit: limit,
    syncLimit: limit,
  );
  for (final (label, source, caret, limit, command) in [
    // The escape that keeps `a\` from reading as a hard break.
    ('Return', 'a\\b', 2, 4, const Newline() as FlarkCommand),
    // The prefix that keeps the lazy line in the item.
    ('a heading level', '- a\nb', 2, 8, const SetHeadingLevel(1)),
    // The escapes that keep the address from linking again.
    ('RemoveLink', '**a<https://foo.bar/?q=**>', 6, 30, const RemoveLink()),
    // The blank line that keeps `2. b` from joining the lifted text.
    ('a lift', '> > 1. a\n> > 2. b', 7, 17, const DeleteBackward()),
  ]) {
    test('$label whose respelling is past the source limit is unsupported', () {
      final e = limited(source, caret, limit);
      expect(e.apply(command), isFalse);
      expect(e.source, source);
      expect(e.lastRejection, FlarkRejection.unsupportedEdit);
    });
  }
  for (final (label, source, caret, limit, command, edited, at) in [
    // Not the blank line after the item: the next item's marker after the
    // blank lines, as Return makes it below the limit.
    (
      'Return before blank lines in an item',
      '- a\n- b\n\n  c\n- d\n',
      7,
      20,
      const Newline() as FlarkCommand,
      '- a\n- b\n\n- \n  c\n- d\n',
      11,
    ),
    // Not the escape at the end of an autolink's text: the break after it.
    (
      'Return at the end of an autolink',
      '<http://a.b>',
      11,
      13,
      const Newline(),
      '<http://a.b>\n',
      13,
    ),
    // Not the line break that would show the backslash: a hard break, the
    // backslash typed as it is.
    (
      'a backslash ending a line',
      'a\nb',
      1,
      4,
      const InsertText('\\'),
      'a\\\nb',
      3,
    ),
    // Not the line break that keeps `next` out of the table: the table
    // takes it, as past the live tier.
    (
      'a delimiter row',
      '| a | b |\n| - | \nnext',
      16,
      22,
      const InsertText('-'),
      '| a | b |\n| - | -\nnext',
      18,
    ),
  ]) {
    test('$label takes the spelling that fits the source limit', () {
      final e = limited(source, caret, limit);
      expect(e.apply(command), isTrue);
      expect(e.source, edited);
      expect(e.selection, FlarkSelection.collapsed(at));
      expect(e.lastRejection, isNull);
      expect(e.sourceMode, isFalse);
    });
  }
  test('indented text as asked past the source limit is refused', () {
    // Without its indentation the text would fit, but the text as asked
    // does not: the edit is refused for the limit.
    for (final (source, caret, limit, text) in [
      ('b', 0, 3, '  a'),
      ('## x', 3, 7, '\t- t'),
    ]) {
      final e = limited(source, caret, limit);
      expect(e.apply(Paste(text)), isFalse, reason: text);
      expect(e.lastRejection, FlarkRejection.sourceLimit, reason: text);
      expect(e.source, source, reason: text);
    }
  });
}
