import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

/// Up on the first line or Down on the last has nowhere to go, as Left at
/// the document's start: it is no press, so a pending style outlasts it.
void main() {
  late FlarkParseBackend backend;
  late FleuryTester tester;
  late FlarkEditor editor;
  late FlarkFleuryController controller;
  late FocusNode focus;

  setUp(() {
    backend = createParseBackend();
    tester = FleuryTester(viewportSize: const CellSize(40, 22));
    focus = FocusNode();
  });
  tearDown(() {
    tester.dispose();
    controller.dispose();
    focus.dispose();
  });

  void mount(String source, int caret) {
    editor = FlarkEditor(backend, text: source, caret: caret);
    controller = FlarkFleuryController(editor);
    tester.pumpWidget(
      Theme(
        data: const ThemeData(),
        child: Navigator(
          home: FlarkEditorView(
            controller: controller,
            autofocus: true,
            focusNode: focus,
          ),
        ),
      ),
    );
    tester.render();
  }

  for (final (name, code, source, caret) in [
    ('Up on the first line', KeyCode.arrowUp, 'one\n\ntwo', 3),
    ('Down on the last line', KeyCode.arrowDown, 'one\n\ntwo', 8),
    ('Left at the document start', KeyCode.arrowLeft, 'one', 0),
  ]) {
    test('a pending style outlasts $name', () {
      mount(source, caret);
      tester.sendKey(
        const KeyEvent(KeyCode.b, modifiers: {KeyModifier.superKey}),
      );
      expect(editor.typingContext, isNot(0));
      final before = editor.selection;
      tester.sendKey(KeyEvent(code));
      expect(editor.selection, before);
      tester.type('x');
      expect(editor.source, contains('**x**'));
    });
  }
}
