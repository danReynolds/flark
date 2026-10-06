import 'package:flark/flark.dart';
import 'package:flark/session.dart';
import 'package:test/test.dart';

/// FlarkEditor.canSetCodeLanguage, and the FlarkState availability that the
/// hosts' language menus read, agree with SetCodeLanguage: true exactly where
/// the selection lies on the fenced code block the caret is in.
void main() {
  final backend = createParseBackend();
  FlarkState stateOf(FlarkEditor editor) => FlarkState(
    markdown: editor.source,
    revision: editor.revision,
    status: FlarkStatus.ready,
    error: null,
    editor: editor,
  );

  // `para`, a blank line, then a fence whose body is `x` (offsets 10..11).
  const fenced = 'para\n\n```\nx\n```\n';
  for (final (name, source, base, extent, settable) in [
    ('the caret in a fence', fenced, 10, 10, true),
    ('a selection inside a fence', fenced, 10, 11, true),
    // A choice made over a paragraph and a fence would set the fence's
    // language from a selection that is not only code.
    ('a selection reaching into a fence', fenced, 1, 10, false),
    ('a selection leaving a fence', fenced, 10, 1, false),
    ('the caret in a paragraph', fenced, 1, 1, false),
    ('indented code', '    code\n', 5, 5, false),
  ]) {
    test('code language availability on $name matches the command', () {
      FlarkEditor editor() =>
          FlarkEditor(backend, text: source)..apply(SetSelection(base, extent));
      expect(editor().apply(const SetCodeLanguage('python')), settable);
      final asked = editor();
      expect(asked.canSetCodeLanguage(), settable);
      expect(stateOf(asked).code.canSetLanguage, settable);
    });
  }

  test('source mode sets no code language', () {
    final editor = FlarkEditor(backend, text: fenced, caret: 10)
      ..setSourceMode(true);
    expect(editor.canSetCodeLanguage(), isFalse);
    expect(stateOf(editor).code.canSetLanguage, isFalse);
  });
}
