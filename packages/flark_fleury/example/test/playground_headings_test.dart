import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury_example/playground.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  test('heading controls preserve the draft and export overrides', () async {
    final editor = FlarkEditor(
      createParseBackend(),
      text: headingSample,
      caret: 0,
    );
    final controller = FlarkFleuryController(editor);
    final tester = FleuryTester(viewportSize: const CellSize(100, 70));
    addTearDown(() {
      tester.dispose();
      controller.dispose();
    });
    tester.pumpWidget(Playground(controller: controller));
    final selection = editor.selection;
    Future<void> press(String label) async {
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: label,
      );
      tester.render();
    }

    CellOffset at(String text) {
      final rows = tester.renderToString().split('\n');
      final row = rows.indexWhere((r) => r.contains(text));
      expect(row, greaterThanOrEqualTo(0), reason: text);
      return CellOffset(rows[row].indexOf(text), row);
    }

    CellStyle styleAt(String text) {
      final point = at(text);
      return tester.render().atColRow(point.col, point.row).style;
    }

    final title = at('A weekend');
    final body = at('Two days');
    // Compare the painted document, not only the theme configuration. H1 and
    // H2 differ by color; H3 adds italics. Every block keeps the same surface.
    for (final text in ['A weekend', 'Before you go', 'Packing light']) {
      expect(styleAt(text).background, styleAt('Two days').background);
      expect(styleAt(text).underline, isFalse);
      expect(styleAt(text).bold, isTrue);
    }
    expect(styleAt('Packing light').italic, isTrue);
    expect(styleAt('Before you go').italic, isFalse);
    expect(
      styleAt('A weekend').foreground,
      isNot(styleAt('Before you go').foreground),
    );
    expect(styleAt('Packing light').foreground, styleAt('Two days').foreground);
    await press('Heading styles');
    await press('Band: off');
    expect(
      styleAt('A weekend').background,
      isNot(styleAt('Two days').background),
    );
    await press('Band: on');
    await press('Level gutter: off');
    expect(at('A weekend').col, title.col + 3);
    expect(at('Two days').col, body.col + 3);
    await press('Heading level: H1');
    await press('Heading level: H2');
    await press('Italic: on');
    await press('Underline: off');
    final red = tester
        .semantics()
        .byLabel('Level color')
        .single
        .children
        .singleWhere(
          (node) => node.role == SemanticRole.radio && node.label == 'Red',
        );
    await tester.invokeSemanticAction(SemanticAction.activate, id: red.id);
    tester.render();
    expect(styleAt('Packing light').italic, isFalse);
    expect(styleAt('Packing light').underline, isTrue);
    expect(styleAt('Packing light').foreground, const RgbColor(252, 165, 165));
    expect(styleAt('A weekend').foreground, const RgbColor(147, 197, 253));
    await press('Dark / light');
    expect(styleAt('Packing light').foreground, const RgbColor(185, 28, 28));
    expect(styleAt('A weekend').foreground, const RgbColor(29, 78, 216));
    expect(styleAt('Before you go').foreground, const RgbColor(70, 94, 123));
    expect(editor.source, headingSample);
    expect(editor.selection, selection);
    await press('Copy theme Dart');
    await tester.settle(); // clipboard completion only
    final copied = (tester.clipboard as InProcessClipboard).lastWritten!;
    expect(copied, contains('headingGutter: true'));
    expect(
      copied,
      contains(
        '1: FlarkHeadingStyle(style: CellStyle(foreground: RgbColor(29, 78, 216))',
      ),
    );
    expect(
      copied,
      contains(
        '3: FlarkHeadingStyle(style: CellStyle(foreground: RgbColor(185, 28, 28), italic: false, underline: true)',
      ),
    );
    expect(copied, contains('6: FlarkHeadingStyle('));
    await press('Reset theme');
    expect(at('A weekend').col, title.col);
    expect(styleAt('Packing light').italic, isTrue);
    expect(styleAt('Packing light').underline, isFalse);
    expect(styleAt('Packing light').foreground, styleAt('Two days').foreground);
    expect(editor.source, headingSample);
    expect(editor.history.undoTarget, isNull);
  });
}
