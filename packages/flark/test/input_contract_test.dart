import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  test(
    'deep containers enter source mode before projection and can recover',
    () {
      final source = '${'> ' * 9}text';
      final e = FlarkEditor(backend, text: source, caret: 0);
      expect(e.sourceMode, isTrue);
      expect(e.source, source);
      expect(e.apply(const ReplaceRange(0, 2, '')), isTrue);
      expect(e.sourceMode, isFalse);
      expect(e.apply(const Undo()), isTrue);
      expect(e.sourceMode, isTrue);
    },
  );
  test('composition commits once and cancellation preserves redo', () {
    final e = FlarkEditor(backend, text: '**a**', caret: 3);
    e.beginComposition();
    expect(e.apply(const InsertText('n')), isTrue);
    expect(e.apply(const ReplaceRange(3, 4, '你')), isTrue);
    expect(e.history.canUndo, isFalse);
    e.commitComposition();
    expect(e.apply(const Undo()), isTrue);
    expect(
      (e.source, e.selection.extent, e.typingContext),
      ('**a**', 3, Style.strong),
    );
    e.beginComposition();
    expect(e.apply(const InsertText('z')), isTrue);
    e.cancelComposition();
    expect(e.source, '**a**');
    expect(e.apply(const Redo()), isTrue);
    expect(e.source, '**a你**');
  });
  test('stale commands reject without mutation', () {
    final e = FlarkEditor(backend);
    final old = e.revision;
    expect(e.apply(const InsertText('a'), expectedRevision: old), isTrue);
    expect(e.apply(const InsertText('a'), expectedRevision: old), isFalse);
    expect(e.lastRejection, FlarkRejection.staleRevision);
    expect(e.source, 'a');
  });
  test('shape admission and explicit source escape apply through history', () {
    final e = FlarkEditor(
      backend,
      text: 'a',
      caret: 1,
      liveLimits: const FlarkLiveLimits(lineCodeUnits: 3),
    );
    expect(e.apply(const Paste('bcd')), isTrue);
    expect(e.sourceMode, isTrue);
    expect(e.apply(const Undo()), isTrue);
    expect(e.sourceMode, isFalse);
    e.setSourceMode(true);
    expect(e.sourceMode, isTrue);
    expect(e.apply(const Redo()), isTrue);
    expect(e.source, 'abcd');
    e.setSourceMode(false);
    expect(e.sourceMode, isTrue);
    expect(e.apply(const DeleteBackward()), isTrue);
    expect(e.sourceMode, isFalse);
  });
  test(
    'block-count admission precedes projection and source size stays bounded',
    () {
      final e = FlarkEditor(
        backend,
        text: '- a\n- b',
        liveLimits: const FlarkLiveLimits(blocks: 2),
        syncLimit: 16,
        sourceLimit: 20,
      );
      expect(e.sourceMode, isTrue);
      final before = e.snapshot;
      expect(e.apply(Paste('x' * 21)), isFalse);
      expect(e.lastRejection, FlarkRejection.sourceLimit);
      expect(e.snapshot, same(before));
    },
  );
  test(
    'Return exits one nested quote then subsequent typing keeps the outer quote',
    () {
      final e = FlarkEditor(backend, text: '> > a\n> > ', caret: 10);
      expect(e.apply(const Newline()), isTrue);
      expect(e.source, '> > a\n> \n> ');
      expect(e.apply(const InsertText('x')), isTrue);
      final row = e.document.rowAt(e.selection.extent);
      expect(row.text, 'x');
      expect(row.shells.map((s) => s.kind), [ShellKind.blockQuote]);
    },
  );
  test(
    'a long logical block fails shape admission even with short physical lines',
    () {
      final e = FlarkEditor(
        backend,
        text: 'ab\ncd',
        caret: 5,
        liveLimits: const FlarkLiveLimits(lineCodeUnits: 3, blockCodeUnits: 5),
      );
      expect(e.sourceMode, isFalse);
      expect(e.apply(const InsertText('e')), isTrue);
      expect(e.sourceMode, isTrue);
      expect(e.apply(const Undo()), isTrue);
      expect(e.sourceMode, isFalse);
      expect(e.apply(const Redo()), isTrue);
      expect(e.sourceMode, isTrue);
    },
  );
}
