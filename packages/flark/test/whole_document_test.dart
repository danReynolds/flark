import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  test(
    'whole selection rejects over-limit input and collapses to a legal caret',
    () {
      final e = FlarkEditor(
        backend,
        text: '# Heading',
        syncLimit: 12,
        sourceLimit: 12,
      );
      e.apply(const SetSelection(0, 9));
      final before = e.snapshot;
      expect(e.apply(Paste('x' * 13)), isFalse);
      expect(e.snapshot, same(before));
      expect(e.lastRejection, FlarkRejection.sourceLimit);
      expect(e.history.canUndo, isFalse);
      expect(e.apply(const MoveCaret(MoveDirection.backward)), isTrue);
      expect(e.selection, const FlarkSelection.collapsed(2));
      expect(e.document.isLegal(e.selection.extent), isTrue);
      expect(e.apply(const InsertText('x')), isTrue);
      expect(e.source, '# xHeading');
    },
  );
  for (final original in [
    'first\n\n**second**',
    '# Heading\n\n- one\n- two',
    '| a | b |\n| - | - |\n| 1 | 2 |',
    '```dart\ncode\n```',
  ]) {
    for (final reverse in [false, true]) {
      for (final command in <FlarkCommand>[
        const Paste('fresh **text**'),
        const InsertText('fresh **text**'),
        ReplaceRange(0, original.length, 'fresh **text**'),
        const DeleteBackward(),
        const DeleteForward(),
        const Newline(),
      ]) {
        test(
          'whole document ${command.runtimeType} reverse=$reverse: $original',
          () {
            final e = FlarkEditor(backend, text: original);
            final selected = reverse
                ? FlarkSelection(original.length, 0)
                : FlarkSelection(0, original.length);
            expect(
              e.apply(SetSelection(selected.base, selected.extent)),
              isTrue,
            );
            expect(e.selection, selected);
            expect(e.apply(command), isTrue);
            final expected = switch (command) {
              DeleteBackward() || DeleteForward() => '',
              Newline() => '\n',
              _ => 'fresh **text**',
            };
            expect(e.source, expected);
            expect(e.apply(const InsertText('!')), isTrue);
            expect(e.source, '$expected!');
            expect(e.apply(const Undo()), isTrue);
            expect(e.source, expected);
            expect(e.apply(const Undo()), isTrue);
            expect(e.source, original);
            expect(e.selection, selected);
            expect(e.apply(const Redo()), isTrue);
            expect(e.source, expected);
            expect(e.document.model.bytes, backend.parse(expected).bytes);
          },
        );
      }
    }
  }
}
