import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  test(
    'nested code and tables keep their containing quote edge on every row',
    () {
      const source =
          '> ```text\n> nested\n> ```\n>\n> | A | B |\n> | - | - |\n> | one | two |';
      const page = RgbColor(16, 20, 24), code = RgbColor(32, 36, 40);
      const theme = FlarkCellTheme(
        body: CellStyle(background: page),
        code: CellStyle(background: code),
        codePadding: 1,
        codePaddingRows: .25,
      );
      final editor = FlarkEditor(createParseBackend(), text: source, caret: 0);
      final controller = FlarkFleuryController(editor);
      final tester = FleuryTester(viewportSize: const CellSize(40, 20));
      addTearDown(() {
        tester.dispose();
        controller.dispose();
      });
      tester.pumpWidget(
        Theme(
          data: const ThemeData(),
          child: FlarkEditorView(controller: controller, theme: theme),
        ),
      );
      final layout = CellDocumentLayout(
        controller,
        40,
        theme,
        CellWidthPolicy.spec,
      );
      final frame = tester.render();
      final nested = layout.positionFor(source.indexOf('nested'));
      expect(nested.col, theme.quoteIndent + theme.codePaddingColumns);
      expect(frame.atColRow(0, nested.row).grapheme, '▎');
      expect(frame.atColRow(1, nested.row).style.background, page);
      expect(frame.atColRow(2, nested.row).style.background, code);
      expect(frame.atColRow(0, nested.row - 1).grapheme, '▎');
      expect(frame.atColRow(0, nested.row + 1).grapheme, '▎');
      final header = layout.positionFor(source.indexOf('A'));
      final last = layout.positionFor(source.indexOf('two'));
      for (var y = header.row - 1; y <= last.row + 1; y++) {
        expect(
          frame.atColRow(0, y).grapheme,
          '▎',
          reason: 'quote rail at row $y',
        );
      }
      expect(frame.atColRow(theme.quoteIndent, header.row).grapheme, '▏');
      expect(editor.source, source);
    },
  );
  final source = File(
    '../../test/fixtures/host_block_alignment.md',
  ).readAsStringSync();
  test(
    'mixed blocks share outer edges while text, markers and padding keep their own geometry',
    () {
      final editor = FlarkEditor(createParseBackend(), text: source, caret: 0);
      final controller = FlarkFleuryController(editor);
      final focus = FocusNode();
      final tester = FleuryTester(viewportSize: const CellSize(80, 48));
      addTearDown(() {
        tester.dispose();
        controller.dispose();
        focus.dispose();
      });
      const page = RgbColor(16, 20, 24), code = RgbColor(32, 36, 40);
      for (final compact in [true, false]) {
        final theme = FlarkCellTheme(
          body: const CellStyle(background: page),
          code: const CellStyle(background: code),
          codePadding: compact ? 1 : 3,
          codePaddingRows: compact ? .25 : .5,
          listIndent: compact ? 4 : 6,
          quoteIndent: compact ? 2 : 4,
        );
        tester.pumpWidget(
          Theme(
            data: const ThemeData(),
            child: FlarkEditorView(
              controller: controller,
              focusNode: focus,
              autofocus: true,
              theme: theme,
            ),
          ),
        );
        final frame = tester.render();
        final layout = CellDocumentLayout(
          controller,
          80,
          theme,
          CellWidthPolicy.spec,
        );
        CellOffset at(String word) => layout.positionFor(source.indexOf(word));
        expect(at('Paragraph.').col, 0);
        expect(at('Heading').col, 0);
        final quote = at('Quoted');
        expect(quote.col, theme.quoteIndent);
        expect(frame.atColRow(0, quote.row).grapheme, '▎');
        for (final text in ['Task item', 'Bullet item', 'Separate bullet']) {
          expect(at(text).col, theme.listIndent);
        }
        expect(at('Nested bullet').col, 2 * theme.listIndent);
        final task = tester
            .semantics()
            .byRole(SemanticRole.checkbox)
            .single
            .bounds!;
        final bulletLine = layout.lines[at('Bullet item').row];
        expect(task.left + 1, bulletLine.prefix.indexOf('●'));
        final first = at('first'), third = at('third');
        expect(first.col, theme.codePaddingColumns);
        expect(at('second').col, first.col + 2);
        for (var y = first.row; y <= third.row; y++) {
          expect(frame.atColRow(0, y).style.background, code);
          expect(frame.atColRow(79, y).style.background, code);
        }
        expect(layout.lines[first.row - 1].editable, isFalse);
        expect(layout.lines[third.row + 1].editable, isFalse);
        expect(frame.atColRow(0, first.row - 1).grapheme, compact ? '▂' : '▄');
        final table = at('Feature');
        expect(frame.atColRow(0, table.row - 1).grapheme, '🭽');
        expect(frame.atColRow(0, table.row).grapheme, '▏');
        expect(frame.atColRow(79, table.row).grapheme, '▕');
        for (final text in [
          'Quoted',
          'Task item',
          'Separate bullet',
          'Nested bullet',
          'first',
          'Feature',
        ]) {
          final point = at(text);
          for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
            tester.sendMouse(
              MouseEvent(
                kind: kind,
                button: MouseButton.left,
                col: point.col,
                row: point.row,
              ),
            );
          }
          tester.type('X');
          expect(editor.source, source.replaceFirst(text, 'X$text'));
          tester.render();
          tester.sendKey(
            const KeyEvent(KeyCode.z, modifiers: {KeyModifier.superKey}),
          );
          tester.render();
          expect(editor.source, source);
        }
        editor.apply(const SetSelection.caret(0));
        tester.render();
      }
    },
  );
}
