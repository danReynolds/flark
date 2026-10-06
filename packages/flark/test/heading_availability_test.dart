import 'package:flark/flark.dart';
import 'package:flark/session.dart';
import 'package:test/test.dart';

/// FlarkEditor.canSetHeading, and the FlarkState availability that the hosts'
/// paragraph style menus read, agree with SetHeadingLevel: true exactly
/// where some level changes the caret's block.
void main() {
  final backend = createParseBackend();
  FlarkState stateOf(FlarkEditor editor) => FlarkState(
    markdown: editor.source,
    revision: editor.revision,
    status: FlarkStatus.ready,
    error: null,
    editor: editor,
  );

  for (final (name, source, base, extent, settable) in [
    ('a paragraph', 'one\n\ntwo', 6, 6, true),
    // The command heads the caret's block alone, whatever else is selected.
    ('a selection across two paragraphs', 'one\n\ntwo', 0, 8, true),
    ('a setext heading', 'Title\n=====\n\nx', 2, 2, true),
    // A heading is one line: of a paragraph of several, the first is
    // headed, and only with the caret on it.
    ('the first line of a paragraph of two', 'a\nb', 0, 0, true),
    ('the second line of a paragraph of two', 'a\nb', 2, 2, false),
    ('a selection over a paragraph of two', 'a\nb', 0, 3, false),
    // A heading of several lines takes no new level but can lose its own.
    ('a setext heading of two lines', 'a\nb\n===\n\nx', 2, 2, true),
    ('an empty line', 'a\n\n\nb', 3, 3, true),
    ('a selected empty line', 'a\n\n\nb', 2, 3, false),
    ('a bare heading marker', '#', 1, 1, true),
    ('a table cell', '| a |\n| - |\n| b |', 2, 2, false),
    ('fenced code', '```\nx\n```', 4, 4, false),
  ]) {
    test('heading availability on $name matches the command', () {
      FlarkEditor editor() =>
          FlarkEditor(backend, text: source)..apply(SetSelection(base, extent));
      final applied = [
        for (var level = 0; level <= 6; level++)
          editor().apply(SetHeadingLevel(level)),
      ];
      expect(applied.contains(true), settable, reason: '$applied');
      final asked = editor();
      expect(asked.canSetHeading(), settable);
      expect(stateOf(asked).heading.canSet, settable);
    });
  }

  test('source mode sets no heading', () {
    final editor = FlarkEditor(backend, text: 'one')..setSourceMode(true);
    expect(editor.canSetHeading(), isFalse);
    expect(stateOf(editor).heading.canSet, isFalse);
  });
}
