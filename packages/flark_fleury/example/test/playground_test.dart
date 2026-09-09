import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury_example/playground.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  test(
    'picker focus keeps the export button fixed and pointer copy works',
    () async {
      final editor = FlarkEditor(createParseBackend(), text: sample);
      final controller = FlarkFleuryController(editor);
      final tester = FleuryTester(viewportSize: const CellSize(100, 40));
      addTearDown(() {
        tester.dispose();
        controller.dispose();
      });
      tester.pumpWidget(Playground(controller: controller));
      int copyRow() => tester
          .renderToString()
          .split('\n')
          .indexWhere((line) => line.contains('[ Copy theme Dart ]'));
      final before = copyRow();
      expect(before, greaterThan(0));
      for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
        tester.sendMouse(
          MouseEvent(kind: kind, button: MouseButton.left, col: 3, row: 5),
        );
      }
      expect(copyRow(), before);
      for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
        tester.sendMouse(
          MouseEvent(kind: kind, button: MouseButton.left, col: 5, row: before),
        );
        tester
            .render(); // focus/blur can change layout between press and release
      }
      await tester.settle();
      final copied = (tester.clipboard as InProcessClipboard).lastWritten;
      expect(copied, startsWith('FlarkCellTheme('));
      expect(copied, contains('syntax:'));
      expect(editor.source, sample);
    },
  );
  for (final size in [const CellSize(100, 32), const CellSize(40, 24)]) {
    test('playground theme and editor remain usable at $size', () async {
      final editor = FlarkEditor(createParseBackend(), text: sample);
      final controller = FlarkFleuryController(editor);
      final tester = FleuryTester(viewportSize: size);
      addTearDown(() {
        tester.dispose();
        controller.dispose();
      });
      tester.pumpWidget(Playground(controller: controller));
      expect(tester.renderToString(), contains('Flark / Fleury'));
      final selection = editor.selection;
      if (size.cols < 64) {
        final result = await tester.invokeSemanticAction(
          SemanticAction.activate,
          role: SemanticRole.button,
          label: 'Customize theme',
        );
        expect(result.status, SemanticActionInvocationStatus.completed);
        expect(tester.renderToString(), contains('THEME'));
      }
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: 'Dark / light',
      );
      tester.render();
      expect(editor.source, sample);
      expect(editor.selection, selection);
      if (size.cols < 64) {
        await tester.invokeSemanticAction(
          SemanticAction.activate,
          role: SemanticRole.button,
          label: 'Back to editor',
        );
        tester.render();
      }
      await tester.invokeSemanticAction(
        SemanticAction.focus,
        role: SemanticRole.textArea,
        label: 'Markdown editor',
      );
      tester.type('X');
      expect(editor.source, startsWith('# XFlark'));
      expect(tester.renderToString(), contains('XFlark / Fleury'));
      tester.sendKey(const KeyEvent(KeyCode.z, modifiers: {KeyModifier.ctrl}));
      expect(editor.source, sample);
      expect(tester.renderToString(), contains('Flark / Fleury'));
    });
  }
}
