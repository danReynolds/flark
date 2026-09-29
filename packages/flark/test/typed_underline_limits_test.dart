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
}
