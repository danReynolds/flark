import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final source in ['', 'before\n\n\nafter']) {
    for (var level = 1; level <= 6; level++) {
      test(
        'choose H$level on an empty row in ${source.isEmpty ? "a new" : "an existing"} document',
        () {
          final caret = source.isEmpty ? 0 : 8;
          final editor = FlarkEditor(backend, text: source, caret: caret);
          final prefix = '${'#' * level} ';
          expect(editor.apply(SetHeadingLevel(level)), isTrue);
          expect(editor.source, source.replaceRange(caret, caret, prefix));
          expect(editor.selection.extent, caret + prefix.length);
          editor.apply(const InsertText('Title'));
          expect(
            editor.document.rowAt(editor.selection.extent).headingLevel,
            level,
          );
          expect(
            editor.source,
            source.replaceRange(caret, caret, '${prefix}Title'),
          );
          editor.apply(const Undo());
          editor.apply(const Undo());
          expect(editor.source, source);
          expect(editor.selection.extent, caret);
        },
      );
    }
  }
}
