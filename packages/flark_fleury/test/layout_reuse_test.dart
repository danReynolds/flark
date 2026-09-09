/// Cell geometry is the expensive part of a frame: it allocates a glyph per
/// grapheme of the whole document while only the viewport is painted. It must
/// survive a repaint, a caret move and a scroll, and must not survive an edit,
/// a resize, a theme change or a new colouring result.
library;

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  final source = [for (var i = 0; i < 60; i++) 'line $i with words'].join('\n\n');

  test('geometry survives what does not change it, and no more', () {
    final editor = FlarkEditor(backend, text: source, caret: 0);
    final controller = FlarkFleuryController(editor);
    final focus = FocusNode();
    final tester = FleuryTester(viewportSize: const CellSize(40, 10));
    tester.pumpWidget(Theme(
      data: const ThemeData(),
      child: FlarkEditorView(
          controller: controller, autofocus: true, focusNode: focus),
    ));
    tester.render();

    int rebuilds(void Function() action) {
      final before = CellDocumentLayout.builds;
      action();
      tester.render();
      return CellDocumentLayout.builds - before;
    }

    expect(rebuilds(() {}), 0, reason: 'a repaint with no change');
    expect(rebuilds(() => tester.sendKey(const KeyEvent(KeyCode.arrowDown))), 0,
        reason: 'a caret move');
    expect(rebuilds(() => tester.sendKey(const KeyEvent(KeyCode.arrowRight))), 0,
        reason: 'a caret move');
    expect(rebuilds(() => editor.apply(const SelectAll())), 0,
        reason: 'a selection change');
    expect(rebuilds(() => tester.type('x')), 1, reason: 'an edit');

    tester.dispose();
    controller.dispose();
    focus.dispose();
  });

  test('the reuse test refuses geometry it no longer describes', () {
    final editor = FlarkEditor(backend, text: source, caret: 0);
    final controller = FlarkFleuryController(editor);
    const theme = FlarkCellTheme();
    final layout = CellDocumentLayout(
        controller, 40, theme, CellWidthPolicy.spec);

    expect(layout.describes(controller, 40, theme, CellWidthPolicy.spec), isTrue);
    expect(layout.describes(controller, 41, theme, CellWidthPolicy.spec), isFalse,
        reason: 'a resize');
    expect(
        layout.describes(controller, 40,
            const FlarkCellTheme(marker: CellStyle(dim: true)),
            CellWidthPolicy.spec),
        isFalse,
        reason: 'a theme change');
    expect(layout.describes(controller, 40, theme, CellWidthPolicy.cjk), isFalse,
        reason: 'a width policy change');

    // An equal theme is the same theme: the host builds a fresh one per frame.
    expect(
        layout.describes(
            controller, 40, const FlarkCellTheme(), CellWidthPolicy.spec),
        isTrue);

    editor.apply(const InsertText('y'));
    expect(layout.describes(controller, 40, theme, CellWidthPolicy.spec), isFalse,
        reason: 'an edit');

    final other = FlarkFleuryController(
        FlarkEditor(backend, text: editor.source, caret: 0));
    expect(layout.describes(other, 40, theme, CellWidthPolicy.spec), isFalse,
        reason: 'a different controller with identical text');
    controller.dispose();
    other.dispose();
  });
}
