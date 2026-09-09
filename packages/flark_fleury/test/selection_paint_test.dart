/// The painted selection must be exactly the glyphs the source selection
/// covers: a real render, over projected rows, wrapped lines, hidden markup,
/// wide glyphs and tabs, rather than a second copy of the paint arithmetic.
library;

import 'dart:convert';
import 'dart:math';

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  const theme = FlarkCellTheme(
    body: CellStyle(bold: false),
    selection: CellStyle(inverse: true),
    caret: CellStyle(inverse: true),
  );
  final sources = <String>[
    '# Title here\n\nplain **bold** and *em* `code` [link](http://x.y) tail\n',
    '- one\n- two\n  - nested deep item text\n- [x] task done\n',
    '> quoted **text** here\n> second line of it\n',
    '```dart\nvoid main() {}\nfinal x = 1;\n```\n',
    '| a | b |\n| - | - |\n| c | d |\n| e |\n',
    'wide 😀 emoji and é and\ttabs plus a long tail that wraps\n',
    'a\\\nhard break line\n\nsoft\nwrap here\n',
    '*\n\nafter the bare marker\n',
    '###\n\nafter the bare hashes\n',
  ];

  test('painted selection equals the selected source glyphs', () {
    final failures = <String>[];
    var selectedCells = 0;
    final random = Random(11);
    for (final source in sources) {
      for (final cols in [16, 34]) {
        for (var trial = 0; trial < 24; trial++) {
          final a = random.nextInt(source.length + 1);
          final b = random.nextInt(source.length + 1);
          final editor = FlarkEditor(backend, text: source, caret: 0);
          final controller = FlarkFleuryController(editor);
          final focus = FocusNode();
          final tester =
              FleuryTester(viewportSize: CellSize(cols, 24));
          tester.pumpWidget(Theme(
            data: const ThemeData(),
            child: FlarkEditorView(
              controller: controller,
              theme: theme,
              autofocus: true,
              focusNode: focus,
            ),
          ));
          editor.apply(SetSelection(min(a, b), max(a, b)));
          final buffer = tester.render();
          final layout = CellDocumentLayout(
              controller, cols, theme, CellWidthPolicy.spec);
          if (editor.selection.isCollapsed) {
            tester.dispose();
            controller.dispose();
            focus.dispose();
            continue;
          }
          for (var y = 0; y < layout.lines.length && y < 24; y++) {
            final line = layout.lines[y];
            for (final glyph in line.glyphs) {
              if (glyph.col >= cols) continue;
              final start = line.row!.sourceForDisplay(glyph.start,
                  anchor: Anchor.after);
              final end = line.row!
                  .sourceForDisplay(glyph.end, anchor: Anchor.before);
              final covered = editor.selection.start < end &&
                  editor.selection.end > start;
              final painted = buffer.atColRow(glyph.col, y).style.inverse;
              if (painted) selectedCells++;
              if (covered != painted) {
                failures.add('${jsonEncode(source)} cols $cols '
                    'sel ${editor.selection.start}..${editor.selection.end} '
                    'cell ($y,${glyph.col}) ${jsonEncode(glyph.text)} '
                    'src $start..$end covered $covered painted $painted');
              }
            }
          }
          tester.dispose();
          controller.dispose();
          focus.dispose();
        }
      }
    }
    expect(selectedCells, greaterThan(500),
        reason: 'the trials must actually paint a selection');
    expect(failures, isEmpty,
        reason: '${failures.length} paint mismatches:\n'
            '${failures.take(20).join('\n')}');
  });
}
