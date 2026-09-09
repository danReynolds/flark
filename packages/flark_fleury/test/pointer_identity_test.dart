/// Pointer identity across the corpora: a click on a painted glyph must leave
/// the caret on that glyph. Cell geometry is host-owned, PlaceCaret is the
/// kernel's, and only the two agreeing makes a click land where the user aimed.
library;

import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:fleury/fleury_core.dart';
import 'package:test/test.dart';

void main() {
  final backend = createParseBackend();
  final corpus = <String>[
    '# Title\n\nplain **bold** and *em* `code` [link](http://x.y) tail\n',
    '- one\n- two\n  - nested\n- [x] task\n- [ ] open\n',
    '> quoted **text** here\n> more\n',
    '1. first\n2. second\n10. tenth\n',
    '```dart\nvoid main() {}\nfinal x = 1;\n```\n',
    '| a | b |\n| - | - |\n| c | d |\n',
    'a\\\nhard break\n\nsoft\nwrap\n',
    'wide 😀 emoji and é combining and\ttabs here\n',
    '*\n\nafter\n',
    '#\n\nafter\n',
    '###\n\nafter\n',
    '> *\n',
    '  -\n',
    '- a\n  *\n',
    '![img](x.png) and ![](y.png)\n',
    'Setext\n======\n\nOther\n-----\n',
    '<div>\nhtml block\n</div>\n',
    '[ref]: http://example.com "t"\n\nuse [ref] here\n',
    'a very long paragraph that will certainly wrap several times when the grid is narrow indeed\n',
    '- a very long list item that wraps across the narrow grid more than once for sure\n',
    'footnote[^1]\n\n[^1]: note body\n',
    '~~struck~~ and <https://auto.link> and **nested *both* here**\n',
  ];
  for (final name in ['common_mark_tests.json', 'gfm_tests.json']) {
    final extra = File('../../test/fixtures/commonmark/upstream/$name');
    if (!extra.existsSync()) continue;
    for (final c in (jsonDecode(extra.readAsStringSync()) as List)) {
      corpus.add((c as Map)['markdown'] as String);
    }
  }

  test('a click on a painted glyph puts the caret on that glyph', () {
    final mismatches = <String>[];
    for (final raw in corpus) {
      final source = raw.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
      for (final cols in [8, 20, 60]) {
        final editor = FlarkEditor(backend, text: source, caret: 0);
        final controller = FlarkFleuryController(editor);
        final layout = CellDocumentLayout(
            controller, cols, const FlarkCellTheme(), CellWidthPolicy.spec);
        for (var i = 0; i < layout.lines.length; i++) {
          final line = layout.lines[i];
          if (line.row == null) continue;
          for (final glyph in line.glyphs) {
            final (offset, leading) = line.hit(glyph.col);
            // Virtual spaces and replacements hold no caret; only assert on
            // display offsets an exact segment owns.
            final exact = line.row!.segments.any((s) =>
                s.exact && s.displayStart <= offset && offset < s.displayEnd);
            if (!exact) continue;
            editor.apply(PlaceCaret(line.row!.index, offset,
                leadingHalf: leading));
            final at = layout.positionFor(editor.selection.extent);
            if (at.row != i || at.col != glyph.col) {
              mismatches.add(
                  '${jsonEncode(source)} cols $cols: cell ($i,${glyph.col}) '
                  '${jsonEncode(glyph.text)} row ${line.row!.index} '
                  'offset $offset -> caret ${editor.selection.extent} '
                  'painted (${at.row},${at.col})');
            }
          }
        }
        controller.dispose();
      }
    }
    expect(mismatches, isEmpty,
        reason: '${mismatches.length} click mismatches:\n'
            '${mismatches.take(25).join('\n')}');
  });

  test('clicking a scrolled viewport lands on the painted glyph', () {
    final source = [for (var i = 0; i < 30; i++) 'line $i word$i'].join('\n\n');
    final editor = FlarkEditor(backend, text: source, caret: 0);
    final controller = FlarkFleuryController(editor);
    final focus = FocusNode();
    final tester = FleuryTester(viewportSize: const CellSize(24, 6));
    tester.pumpWidget(Theme(
      data: const ThemeData(),
      child: FlarkEditorView(
          controller: controller, autofocus: true, focusNode: focus),
    ));
    tester.render();
    // Scroll by moving the caret to the end, then click each visible cell.
    editor.apply(SetSelection.caret(source.length));
    tester.render();
    final mismatches = <String>[];
    for (var y = 0; y < 6; y++) {
      for (var c = 0; c < 24; c++) {
        final buffer = tester.render();
        final glyph = buffer.atColRow(c, y).grapheme;
        if (glyph == null || glyph == ' ') continue;
        tester.sendMouse(MouseEvent(
            button: MouseButton.left,
            kind: MouseEventKind.down,
            col: c,
            row: y));
        tester.sendMouse(MouseEvent(
            button: MouseButton.left, kind: MouseEventKind.up, col: c, row: y));
        tester.render();
        final rect = focus.caretRect;
        if (rect == null) {
          mismatches.add('($y,$c) ${jsonEncode(glyph)}: no caret');
        } else if (rect.top != y || rect.left != c) {
          mismatches.add('($y,$c) ${jsonEncode(glyph)}: caret '
              '(${rect.top},${rect.left})');
        }
      }
    }
    tester.dispose();
    controller.dispose();
    focus.dispose();
    expect(mismatches, isEmpty,
        reason: mismatches.take(12).join('\n'));
  });
}
