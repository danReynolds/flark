/// A viewport can be one cell. Mounting, typing, moving and clicking every
/// cell must still work there: the geometry reserves a caret column, and no
/// clamp inverts.
library;

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  for (final size in [
    const CellSize(1, 1),
    const CellSize(2, 2),
    const CellSize(3, 1),
    const CellSize(6, 3),
    const CellSize(80, 1),
  ]) {
    test('mounts and edits at $size', () {
      final editor = FlarkEditor(backend,
          text: '# H\n\n- a [link](http://x.y) b\n\n```\nc\n```\n', caret: 0);
      final controller = FlarkFleuryController(editor);
      final focus = FocusNode();
      final tester = FleuryTester(viewportSize: size);
      tester.pumpWidget(Theme(
        data: const ThemeData(),
        child: FlarkEditorView(
            controller: controller, autofocus: true, focusNode: focus),
      ));
      tester.render();
      tester.type('x');
      tester.render();
      for (final code in [KeyCode.arrowDown, KeyCode.arrowRight, KeyCode.end]) {
        tester.sendKey(KeyEvent(code));
        tester.render();
      }
      // Click every cell.
      for (var c = 0; c < size.cols; c++) {
        for (var y = 0; y < size.rows; y++) {
          tester.sendMouse(MouseEvent(
              button: MouseButton.left,
              kind: MouseEventKind.down,
              col: c,
              row: y));
          tester.sendMouse(MouseEvent(
              button: MouseButton.left, kind: MouseEventKind.up, col: c, row: y));
          tester.render();
        }
      }
      editor.apply(SetSelection.caret(editor.source.indexOf('link') + 1));
      tester.render();
      expect(editor.source, contains('x'));
      tester.dispose();
      controller.dispose();
      focus.dispose();
    });
  }
}
