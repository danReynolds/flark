import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:flark_fleury_example/playground.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  test(
    'preset text colors remain readable on both surfaces in both themes',
    () async {
      final editor = FlarkEditor(createParseBackend(), text: sample);
      final controller = FlarkFleuryController(editor);
      final tester = FleuryTester(viewportSize: const CellSize(100, 40));
      addTearDown(() {
        tester.dispose();
        controller.dispose();
      });
      tester.pumpWidget(Playground(controller: controller));
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: 'Customize theme',
      );
      tester.render();
      final initialSelection = editor.selection;

      double luminance(Color color) {
        final rgb = color.toRgb();
        double channel(int value) {
          final c = value / 255;
          return c <= 0.04045
              ? c / 12.92
              : math.pow((c + 0.055) / 1.055, 2.4).toDouble();
        }

        return 0.2126 * channel(rgb.r) +
            0.7152 * channel(rgb.g) +
            0.0722 * channel(rgb.b);
      }

      double contrast(Color a, Color b) {
        final x = luminance(a), y = luminance(b);
        return (math.max(x, y) + 0.05) / (math.min(x, y) + 0.05);
      }

      Color paintedBackground(String text) {
        final lines = tester.renderToString().split('\n');
        final row = lines.indexWhere((line) => line.contains(text));
        return tester
            .render()
            .atColRow(lines[row].indexOf(text), row)
            .style
            .background!;
      }

      for (final light in [false, true]) {
        if (light) {
          await tester.invokeSemanticAction(
            SemanticAction.activate,
            role: SemanticRole.button,
            label: 'Dark / light',
          );
        }
        final page = paintedBackground('One Markdown');
        final code = paintedBackground('def hello');
        for (final label in ['Heading color', 'Link color', 'Keyword color']) {
          final frame = tester.render();
          final swatches = tester
              .semantics()
              .byLabel(label)
              .single
              .children
              .where((node) => node.role == SemanticRole.radio);
          for (final swatch in swatches) {
            final bounds = swatch.bounds!;
            final color = frame
                .atColRow(bounds.left + 1, bounds.top)
                .style
                .foreground!;
            expect(
              contrast(color, page),
              greaterThanOrEqualTo(4.5),
              reason:
                  '$label ${swatch.label} on ${light ? "light" : "dark"} page',
            );
            expect(
              contrast(color, code),
              greaterThanOrEqualTo(4.5),
              reason: '$label ${swatch.label} on code background',
            );
          }
        }
        expect(
          tester.semantics().byLabel('Heading color').single.value,
          'Blue',
        );
        if (!light) {
          // Move away first so selecting Blue installs an explicit override.
          for (final name in ['Red', 'Blue']) {
            final preset = tester
                .semantics()
                .byLabel('Heading color')
                .single
                .children
                .singleWhere(
                  (node) =>
                      node.role == SemanticRole.radio && node.label == name,
                );
            await tester.invokeSemanticAction(
              SemanticAction.activate,
              id: preset.id,
            );
            tester.render();
            expect(
              tester.semantics().byLabel('Heading color').single.value,
              name,
            );
          }
        }
      }
      expect(editor.source, sample);
      expect(editor.selection, initialSelection);
    },
  );

  test(
    'light theme paints readable editor, controls and resource surfaces',
    () async {
      final editor = FlarkEditor(createParseBackend(), text: sample);
      final controller = FlarkFleuryController(editor);
      final tester = FleuryTester(viewportSize: const CellSize(100, 32));
      addTearDown(() {
        tester.dispose();
        controller.dispose();
      });
      tester.pumpWidget(Playground(controller: controller));
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: 'Customize theme',
      );
      tester.render();
      final selection = editor.selection;
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: 'Dark / light',
      );
      final frame = tester.render();
      for (final label in ['Heading color', 'Link color', 'Keyword color']) {
        final palette = tester.semantics().byLabel(label).single;
        final swatches = palette.children
            .where((node) => node.role == SemanticRole.radio)
            .toList();
        expect(
          swatches.length,
          16,
          reason: 'custom RGB defaults stay inside the two-row palette',
        );
        expect(swatches.where((node) => node.checked == true).length, 1);
      }
      expect(frame.atColRow(1, 0).style.foreground, const RgbColor(25, 35, 45));
      expect(
        frame.atColRow(99, 30).style.background,
        const RgbColor(250, 250, 252),
      );
      CellOffset findText(String value) {
        final lines = tester.renderToString().split('\n');
        final row = lines.indexWhere((line) => line.contains(value));
        expect(row, greaterThanOrEqualTo(0), reason: value);
        return CellOffset(lines[row].indexOf(value), row);
      }

      final body = findText('One Markdown');
      expect(
        frame.atColRow(body.col, body.row).style.foreground,
        const RgbColor(25, 35, 45),
      );
      final code = findText('def hello');
      expect(
        code.col,
        body.col + 1,
        reason: 'code text and its caret stay on the cell grid',
      );
      expect(
        findText('[ ] Click').col + 1,
        findText('●').col,
        reason: 'checkbox and unordered marker share the gutter',
      );
      expect(
        findText('Click this task').col,
        findText('Try Enter').col,
        reason: 'both list labels start in the same column',
      );
      expect(
        findText('puts').col,
        code.col + 2,
        reason: 'Ruby source indentation is still preserved',
      );
      expect(
        frame.atColRow(code.col, code.row).style.foreground,
        const RgbColor(25, 35, 45),
      );
      expect(
        frame.atColRow(code.col, code.row).style.background,
        const RgbColor(235, 239, 244),
      );
      final codeEdge = frame.atColRow(code.col - 1, code.row);
      expect(codeEdge.grapheme ?? ' ', ' ');
      expect(codeEdge.style.background, const RgbColor(235, 239, 244));
      final link = findText('a link');
      expect(
        frame.atColRow(link.col, link.row).style.foreground,
        const RgbColor(0, 95, 130),
      );
      expect(editor.source, sample);
      expect(editor.selection, selection);
      for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
        tester.sendMouse(
          MouseEvent(
            kind: kind,
            button: MouseButton.left,
            col: link.col + 2,
            row: link.row,
          ),
        );
        tester.render();
      }
      final destination = findText('https://dart.dev');
      final popup = tester.render().atColRow(destination.col, destination.row);
      expect(popup.style.foreground, const RgbColor(25, 35, 45));
      expect(popup.style.background, const RgbColor(250, 250, 252));
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: 'Edit',
      );
      final title = findText('Edit link');
      final dialog = tester.render().atColRow(title.col, title.row);
      expect(dialog.style.foreground, const RgbColor(25, 35, 45));
      expect(dialog.style.background, const RgbColor(250, 250, 252));
      final dialogLines = tester.renderToString().split('\n');
      final labelRow = dialogLines.lastIndexWhere(
        (line) => line.contains('a link'),
      );
      final labelCol = dialogLines[labelRow].indexOf('a link');
      expect(labelRow, greaterThan(title.row));
      final field = tester.render().atColRow(labelCol, labelRow);
      expect(
        field.style.foreground,
        const RgbColor(25, 35, 45),
        reason: 'unfocused TextInput must not fall back to dark-page defaults',
      );
      expect(field.style.background, const RgbColor(250, 250, 252));
    },
  );

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
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: 'Customize theme',
      );
      tester.render();
      int copyRow() => tester
          .renderToString()
          .split('\n')
          .indexWhere((line) => line.contains('[ Copy theme Dart ]'));
      final before = copyRow();
      expect(before, greaterThan(0));
      final palette = tester.semantics().byLabel('Heading color').single;
      final swatches = palette.children
          .where((node) => node.role == SemanticRole.radio)
          .toList();
      expect(
        swatches[8].bounds!.top - swatches[0].bounds!.top,
        2,
        reason: 'palette rows have one blank row between them',
      );
      final swatch = swatches[1].bounds!;
      for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
        tester.sendMouse(
          MouseEvent(
            kind: kind,
            button: MouseButton.left,
            col: swatch.left + 1,
            row: swatch.top,
          ),
        );
      }
      expect(copyRow(), before);
      expect(tester.semantics().byLabel('Heading color').single.value, 'Red');
      for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
        tester.sendMouse(
          MouseEvent(
            kind: kind,
            button: MouseButton.left,
            col:
                tester
                    .semantics()
                    .byLabel('Copy theme Dart')
                    .single
                    .bounds!
                    .left +
                2,
            row: before,
          ),
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
      await tester.invokeSemanticAction(
        SemanticAction.activate,
        role: SemanticRole.button,
        label: 'Customize theme',
      );
      tester.render();
      expect(tester.renderToString(), contains('Flark / Fleury'));
      final selection = editor.selection;
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
          label: 'Close theme',
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
