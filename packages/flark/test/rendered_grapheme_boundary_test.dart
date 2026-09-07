import 'package:characters/characters.dart';
import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  const sources = [
    '*a*\u0301',
    '**a**\u0301',
    'a*\u0301*',
    '&#97;\u0301',
    '&fjlig;\u0301',
  ];

  for (final source in sources) {
    test('selection stays on rendered graphemes in $source', () {
      final editor = FlarkEditor(backend, text: source);
      final row = editor.projection.rows.single;
      for (var offset = 0; offset <= row.text.length; offset++) {
        for (final leading in [false, true]) {
          editor.apply(PlaceCaret(0, offset, leadingHalf: leading));
          final display = editor.document.displayOf(editor.selection.extent);
          expect(CharacterRange.at(row.text, display.offset).isEmpty, isTrue);
          expect(editor.document.isLegal(editor.selection.extent), isTrue);
        }
      }
      for (var offset = 0; offset <= source.length; offset++) {
        editor.apply(SetSelection.caret(offset));
        final display = editor.document.displayOf(editor.selection.extent);
        expect(CharacterRange.at(row.text, display.offset).isEmpty, isTrue);
      }
    });

    for (final backward in [false, true]) {
      test(
        'delete $source ${backward ? 'backward' : 'forward'}, type and undo',
        () {
          final editor = FlarkEditor(
            backend,
            text: source,
            caret: backward ? source.length : 0,
          );
          final before = editor.selection;
          final command = backward
              ? const DeleteBackward()
              : const DeleteForward();
          expect(editor.apply(command, at: Duration.zero), isTrue);
          expect(editor.source, isEmpty);
          expect(editor.projection.rows.single.text, isEmpty);
          expect(editor.selection, const FlarkSelection.collapsed(0));
          expect(editor.apply(const Undo()), isTrue);
          expect(editor.source, source);
          expect(editor.selection, before);
          expect(editor.apply(const Redo()), isTrue);
          expect(editor.source, isEmpty);
          expect(editor.apply(const InsertText('x')), isTrue);
          expect(editor.source, 'x');
          expect(editor.projection.rows.single.text, 'x');
          expect(editor.selection, const FlarkSelection.collapsed(1));
          expect(editor.apply(const Undo()), isTrue);
          expect(editor.source, isEmpty);
        },
      );
    }
  }

  test(
    'moving across a styled accent never visits the middle of the glyph',
    () {
      final editor = FlarkEditor(backend, text: '*a*\u0301');
      expect(editor.document.isLegal(2), isFalse);
      expect(editor.document.isLegal(3), isFalse);
      expect(editor.apply(const MoveCaret(MoveDirection.forward)), isTrue);
      expect(editor.document.displayOf(editor.selection.extent).offset, 2);
      expect(editor.apply(const MoveCaret(MoveDirection.backward)), isTrue);
      expect(editor.document.displayOf(editor.selection.extent).offset, 0);
      expect(editor.apply(const InsertText('x')), isTrue);
      expect(editor.projection.rows.single.text, 'xa\u0301');
    },
  );
}
