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
