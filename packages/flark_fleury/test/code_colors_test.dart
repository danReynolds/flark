/// Code colors come synchronously from the editor's code delegate: every
/// frame paints the colors of the text it shows.
library;

import 'package:flark/code.dart';
import 'package:flark/flark.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  late FleuryTester tester;
  late FlarkEditor editor;
  late FlarkFleuryController controller;
  late FocusNode focus;
  const keyword = Colors.magenta, string = Colors.green;

  /// Mounts without painting, so the next render is the first frame.
  void mount(String source, {required int caret}) {
    editor = FlarkEditor(
      createParseBackend(),
      text: source,
      caret: caret,
      codeEditing: FlarkCodeMirror(),
    );
    controller = FlarkFleuryController(editor);
    focus = FocusNode();
    tester = FleuryTester(viewportSize: const CellSize(40, 16));
    tester.mountWidget(
      Theme(
        data: const ThemeData(),
        child: FlarkEditorView(
          controller: controller,
          focusNode: focus,
          autofocus: true,
          theme: const FlarkCellTheme(
            codePadding: 0,
            syntax: {
              CodeSyntaxRole.keyword: CellStyle(foreground: keyword),
              CodeSyntaxRole.string: CellStyle(foreground: string),
            },
          ),
        ),
      ),
    );
  }

  tearDown(() {
    tester.dispose();
    controller.dispose();
    focus.dispose();
  });

  /// The foreground of each cell of every place [frame] shows [text].
  List<List<Color?>> painted(CellBuffer frame, String text) {
    final found = <List<Color?>>[];
    for (var row = 0; row < frame.size.rows; row++) {
      final line = [
        for (var col = 0; col < frame.size.cols; col++)
          frame.atColRow(col, row).grapheme ?? ' ',
      ].join();
      for (
        var col = line.indexOf(text);
        col >= 0;
        col = line.indexOf(text, col + 1)
      ) {
        found.add([
          for (var i = 0; i < text.length; i++)
            frame.atColRow(col + i, row).style.foreground,
        ]);
      }
    }
    return found;
  }

  test('a fence is colored in the first frame that shows it', () {
    const source = '```ruby\ndef hello\n  puts "hi"\nend\n```\n\nprose';
    mount(source, caret: source.indexOf('hello'));
    final frame = tester.render();
    expect(painted(frame, 'def'), [everyElement(keyword)]);
    expect(painted(frame, 'hello'), [everyElement(isNot(keyword))]);
    expect(painted(frame, '"hi"'), [everyElement(string)]);
    expect(painted(frame, 'end'), [everyElement(keyword)]);
    expect(painted(frame, 'prose'), [everyElement(isNot(keyword))]);
  });

  test('each edit re-colors the fence in the frame that shows it', () {
    const source = '```ruby\ndef hello\nend\n```';
    const quoted = '```ruby\ndef hello"\nend\n```';
    mount(source, caret: source.indexOf('hello') + 5);
    tester.pump();
    void undo({bool redo = false}) => tester.sendKey(
      KeyEvent(
        KeyCode.z,
        modifiers: {KeyModifier.superKey, if (redo) KeyModifier.shift},
      ),
    );

    tester.type('"');
    expect(editor.source, quoted);
    // An open string runs on through the next line.
    var frame = tester.render();
    expect(painted(frame, 'hello"'), [
      [...List.filled(5, isNot(anyOf(keyword, string))), string],
    ]);
    expect(painted(frame, 'end'), [everyElement(string)]);
    undo();
    expect(editor.source, source);
    frame = tester.render();
    expect(painted(frame, 'def'), [everyElement(keyword)]);
    expect(painted(frame, 'end'), [everyElement(keyword)]);
    undo(redo: true);
    expect(editor.source, quoted);
    expect(painted(tester.render(), 'end'), [everyElement(string)]);
    tester.sendKey(const KeyEvent(KeyCode.backspace));
    expect(editor.source, source);
    expect(painted(tester.render(), 'end'), [everyElement(keyword)]);
  });

  test('a fence in an unknown language is plain', () {
    const source =
        '```klingon\ndef hello\nend\n```\n\n```ruby\ndef hello\nend\n```';
    mount(source, caret: source.length);
    final frame = tester.render();
    final plain = isNot(anyOf(keyword, string));
    expect(painted(frame, 'def'), [everyElement(plain), everyElement(keyword)]);
    expect(painted(frame, 'end'), [everyElement(plain), everyElement(keyword)]);
  });

  test('choosing another language re-colors the fence', () {
    const source = '```ruby\ndef hello\nend\n```';
    mount(source, caret: source.indexOf('hello'));
    expect(painted(tester.render(), 'end'), [everyElement(keyword)]);
    expect(editor.apply(const SetCodeLanguage('python')), isTrue);
    expect(editor.source, '```python\ndef hello\nend\n```');
    final frame = tester.render();
    expect(painted(frame, 'def'), [everyElement(keyword)]);
    expect(painted(frame, 'end'), [everyElement(isNot(keyword))]);
  });

  test('an edit lays out only its own row and keeps other fences colored', () {
    // More fences than FlarkCodeMirror's highlight cache holds.
    final source = [
      'prose',
      for (var i = 0; i < 40; i++)
        i.isEven
            ? '```ruby\ndef f$i\nend\n```'
            : '```python\ndef g$i():\n    return "x"\n```',
    ].join('\n\n');
    mount(source, caret: 'prose'.length);
    tester.pump();
    int laidOut(void Function() edit) {
      final before = CellDocumentLayout.rowLayouts;
      edit();
      final frame = tester.render();
      expect(
        painted(frame, 'def'),
        allOf(isNotEmpty, everyElement(everyElement(keyword))),
      );
      expect(
        painted(frame, '"x"'),
        allOf(isNotEmpty, everyElement(everyElement(string))),
      );
      return CellDocumentLayout.rowLayouts - before;
    }

    expect(laidOut(() => tester.type('!')), 1, reason: 'typing in prose');
    expect(editor.source, startsWith('prose!\n'));
    expect(
      laidOut(() {
        editor.apply(SetSelection.caret(editor.source.indexOf('f0') + 2));
        tester.type('x');
      }),
      1,
      reason: 'typing in a fence',
    );
    expect(editor.source, contains('def f0x\n'));
    expect(laidOut(() {}), 0, reason: 'a frame without an edit');
  });
}
