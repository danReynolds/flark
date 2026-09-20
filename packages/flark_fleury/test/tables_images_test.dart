import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  late FleuryTester tester;
  late FlarkEditor editor;
  late FlarkFleuryController controller;
  late FocusNode focus;
  setUp(() {
    tester = FleuryTester(viewportSize: const CellSize(40, 22));
    focus = FocusNode();
  });
  tearDown(() {
    tester.dispose();
    controller.dispose();
    focus.dispose();
  });
  void mount(
    String source, {
    FlarkFleuryImagePreviewBuilder? images,
    bool readOnly = false,
    void Function(Uri)? onOpenLink,
    Uri? baseUri,
  }) {
    editor = FlarkEditor(createParseBackend(), text: source, caret: 0);
    controller = FlarkFleuryController(editor);
    tester.pumpWidget(
      Theme(
        data: const ThemeData(),
        child: Navigator(
          home: FlarkEditorView(
            controller: controller,
            focusNode: focus,
            autofocus: true,
            readOnly: readOnly,
            imagePreviewBuilder: images,
            onOpenLink: onOpenLink,
            baseUri: baseUri,
          ),
        ),
      ),
    );
    tester.render();
  }

  CellDocumentLayout layout() => CellDocumentLayout(
    controller,
    tester.viewportSize.cols,
    const FlarkCellTheme(),
    CellWidthPolicy.spec,
  );
  void key(KeyCode code, {bool shift = false, bool cmd = false}) =>
      tester.sendKey(
        KeyEvent(
          code,
          modifiers: {
            if (shift) KeyModifier.shift,
            if (cmd) KeyModifier.superKey,
          },
        ),
      );
  void click(int x, int y) {
    for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
      tester.sendMouse(
        MouseEvent(button: MouseButton.left, kind: kind, col: x, row: y),
      );
    }
  }

  const table =
      '| Left | Center | Right |\n| :--- | :---: | ---: |\n'
      '| alpha beta gamma | 界 | 42 |\n| tail | value | 7 |\n\nafter';

  test(
    'pointer and Tab target missing cells without editing the previous column',
    () {
      const source = '| a | b | c |\n| --- | --- | --- |\n| x |\n';
      mount(source);
      final l = layout();
      final a = l.positionFor(source.indexOf('x'));
      final b = l.positionFor(source.indexOf('b'));
      click(b.col, a.row);
      expect(editor.document.caretRow.column, 1);
      expect(editor.source, source);
      tester.render();
      expect(focus.caretRect!.left, b.col);
      key(KeyCode.tab);
      tester.render();
      expect(editor.document.caretRow.column, 2);
      final cellCaret = focus.caretRect;
      tester.type('Z');
      final frame = tester.render();
      expect(editor.document.caretRow.column, 2);
      expect(editor.document.caretRow.text.trim(), 'Z');
      expect(frame.atColRow(cellCaret!.left, cellCaret.top).grapheme, 'Z');
      key(KeyCode.z, cmd: true);
      tester.render();
      expect(editor.source, source);
      expect(focus.caretRect, cellCaret);
      key(KeyCode.tab, shift: true);
      tester.type('Y');
      expect(editor.document.caretRow.column, 1);
      expect(tester.renderToString(), contains('Y'));
      expect(
        editor.projection.rows
            .firstWhere((r) => r.kind == RowKind.tableCell && !r.header)
            .text
            .trim(),
        'x',
      );
    },
  );

  test(
    'columns share rows, respect alignment, wrap and remain pointer-editable',
    () {
      mount(table);
      final text = tester.renderToString().split('\n');
      expect(text[1], contains('Left'));
      expect(text[1], contains('Center'));
      expect(text[1], contains('Right'));
      expect(text[3], contains('alpha beta'));
      expect(text[4], contains('gamma'));
      expect(text[3].indexOf('界'), greaterThan(14));
      expect(tester.render().atColRow(36, 3).grapheme, '4');
      final p = layout().positionFor(table.indexOf('界'));
      click(p.col, p.row);
      expect(editor.selection.extent, table.indexOf('界'));
      tester.type('中');
      expect(editor.source, contains('| 中界 |'));
      expect(tester.renderToString(), contains('中界'));
      key(KeyCode.z, cmd: true);
      expect(editor.source, table);
      expect(tester.renderToString(), contains('界'));
    },
  );

  test(
    'Tab, Shift-Tab, Enter and arrows use cell geometry on their first frame',
    () {
      mount(table);
      editor.apply(SetSelection.caret(table.indexOf('Left')));
      tester.render();
      key(KeyCode.tab);
      expect(editor.selection.extent, table.indexOf('Center'));
      tester.render();
      key(KeyCode.tab, shift: true);
      expect(editor.selection.extent, table.indexOf('Left'));
      key(KeyCode.arrowDown);
      expect(editor.selection.extent, table.indexOf('alpha'));
      tester.render();
      key(KeyCode.enter);
      expect(editor.selection.extent, table.indexOf('tail'));
      tester.render();
      key(KeyCode.enter);
      expect(
        editor.document.rowAt(editor.selection.extent).kind,
        isNot(RowKind.tableCell),
      );
      tester.type('Outside');
      expect(tester.renderToString(), contains('Outside'));
    },
  );

  test(
    'explicit empty table cells remain addressable and narrow cells stay visible',
    () {
      mount('| A | B |\n| - | - |\n|  | z |');
      final row = editor.projection.rows.firstWhere(
        (r) => r.kind == RowKind.tableCell && !r.header,
      );
      final p = layout().positionFor(row.sourceStart);
      click(p.col, p.row);
      tester.type('x');
      expect(editor.source, contains('|  x| z |'));
      expect(tester.renderToString(), contains('x'));
      tester.render(size: const CellSize(6, 22));
      final narrow = tester.renderToString();
      expect(narrow, contains('A'));
      expect(narrow, contains('B'));
      expect(narrow, contains('z'));
    },
  );

  test('cell padding clicks and End append directly to visible text', () {
    const source =
        '| Left | Center | Right |\n| :--- | :---: | ---: |\n'
        '| **bold**   | yes  | right   |';
    for (final word in ['bold', 'yes', 'right']) {
      mount(source);
      final l = layout();
      final p = l.positionFor(source.indexOf(word));
      final line = l.lines[p.row].cellAt(p.col);
      final last = line.glyphs.last;
      click(line.right - 1, p.row);
      expect(focus.caretRect!.left, last.col + last.width);
      tester.type('X');
      expect(editor.source, contains('${word}X'));
      expect(tester.renderToString(), contains('${word}X'));
      key(KeyCode.z, cmd: true);
      click(p.col, p.row);
      key(KeyCode.end);
      tester.type('Y');
      expect(
        editor.source,
        contains(word == 'bold' ? '**bold**Y' : '${word}Y'),
      );
      expect(tester.renderToString(), contains('${word}Y'));
      // Dispose between independent column journeys; teardown owns the last.
      if (word != 'right') {
        tester.dispose();
        controller.dispose();
        focus.dispose();
        tester = FleuryTester(viewportSize: const CellSize(40, 22));
        focus = FocusNode();
      }
    }
  });

  test(
    'standalone preview owns its group and exposes alt text only while active',
    () {
      const source = 'before\n\n![Photo label](demo.png)\n\nafter';
      mount(source, images: (_, _, _) => const Text('PREVIEW'));
      final l = layout(), slot = l.images.single;
      final label = l.positionFor(source.indexOf('Photo label'));
      expect(label.row, slot.top + slot.height);
      expect(tester.renderToString(), isNot(contains('Photo label')));
      final after = l.positionFor(source.indexOf('after'));
      click(slot.left + 1, slot.top + 1);
      expect(tester.renderToString(), contains('[ Edit ]'));
      key(KeyCode.escape);
      expect(tester.renderToString(), contains('Photo label'));
      expect(
        tester.render().atColRow(label.col, label.row).style.underline,
        isFalse,
      );
      tester.type('A ');
      expect(editor.source, contains('![A Photo label](demo.png)'));
      expect(tester.renderToString(), contains('A Photo label'));
      expect(layout().positionFor(editor.source.indexOf('after')), after);
      click(after.col, after.row);
      expect(tester.renderToString(), isNot(contains('Photo label')));
      expect(layout().positionFor(editor.source.indexOf('after')), after);
    },
  );

  test(
    'image slots keep geometry, alt text and image actions while previewing',
    () {
      final opened = <Uri>[];
      mount(
        '![Landscape](demo.png)\n\nafter',
        images: (_, resource, _) => const Text('PREVIEW'),
        baseUri: Uri.parse('https://example.test/docs/'),
        onOpenLink: opened.add,
      );
      expect(tester.renderToString(), contains('PREVIEW'));
      final geometry = layout();
      expect(geometry.images.single.height, 8);
      expect(geometry.positionFor(editor.source.indexOf('after')).row, 10);
      click(2, 2);
      expect(tester.renderToString(), contains('[ Edit ]'));
      final actions = tester.renderToString().split('\n');
      final openRow = actions.indexWhere((r) => r.contains('[ Open ]'));
      click(actions[openRow].indexOf('[ Open ]') + 2, openRow);
      expect(opened, [Uri.parse('https://example.test/docs/demo.png')]);
      click(2, 2);
      final rows = tester.renderToString().split('\n');
      final y = rows.indexWhere((r) => r.contains('[ Remove ]'));
      click(rows[y].indexOf('[ Remove ]') + 2, y);
      expect(editor.source, '\n\nafter');
      expect(tester.renderToString(), isNot(contains('PREVIEW')));
      key(KeyCode.z, cmd: true);
      expect(tester.renderToString(), contains('PREVIEW'));
      expect(layout().positionFor(editor.source.indexOf('after')).row, 10);
      editor.setSourceMode(true);
      expect(tester.renderToString(), contains('![Landscape](demo.png)'));
      expect(tester.renderToString(), isNot(contains('PREVIEW')));
    },
  );

  test(
    'only visible image slots mount and vertical movement skips the preview',
    () {
      final mounted = <String>[];
      mount(
        '![one](one.png)\n\nafter\n\n${'gap\n\n' * 18}![two](two.png)\n\nlast',
        images: (_, resource, _) {
          mounted.add(resource.destination);
          return const Text('PREVIEW');
        },
      );
      expect(mounted, contains('one.png'));
      expect(mounted, isNot(contains('two.png')));
      editor.apply(SetSelection.caret(editor.source.indexOf('one]')));
      tester.render();
      key(KeyCode.arrowDown);
      expect(
        editor.selection.extent,
        greaterThan(editor.source.indexOf('one.png')),
      );
      expect(layout().positionFor(editor.selection.extent).row, 9);
      editor.apply(SetSelection.caret(editor.source.indexOf('last')));
      tester.render();
      expect(mounted, contains('two.png'));
    },
  );
  test('selection spans cells and links inside cells keep their actions', () {
    mount('| A | B |\n| - | - |\n| **bold** | [link](https://dart.dev) |');
    final start = editor.source.indexOf('bold');
    final end = editor.source.indexOf('link]') + 4;
    editor.apply(SetSelection(start, end));
    final buffer = tester.render();
    final a = layout().positionFor(start), b = layout().positionFor(end - 1);
    expect(buffer.atColRow(a.col, a.row).style.inverse, isTrue);
    expect(buffer.atColRow(b.col, b.row).style.inverse, isTrue);
    click(b.col, b.row);
    expect(tester.renderToString(), contains('[ Edit ]'));
    key(KeyCode.escape);
    tester.type('x');
    expect(editor.source, contains('linxk'));
    expect(tester.renderToString(), contains('linxk'));
  });

  test(
    'images in table cells share row height and retain adjacent cell geometry',
    () {
      mount(
        '| Picture | Note |\n| - | - |\n| ![alt](demo.png) | text |\n\nafter',
        images: (_, _, _) => const Text('PREVIEW'),
      );
      final l = layout(), slot = layout().images.single;
      expect(slot.left, 1);
      expect(slot.width, lessThan(22));
      expect(slot.top, 4);
      expect(l.positionFor(editor.source.indexOf('text')).row, 3);
      expect(l.positionFor(editor.source.indexOf('text') + 4).row, 3);
      expect(tester.renderToString(), contains('PREVIEW'));
      click(22, 6);
      tester.type('!');
      expect(editor.source, contains('text !'));
      expect(tester.renderToString(), contains('text !'));
    },
  );
}
