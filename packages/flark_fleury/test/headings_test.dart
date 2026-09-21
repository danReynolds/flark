import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  late FlarkEditor editor;
  late FlarkFleuryController controller;
  late FleuryTester tester;
  late FocusNode focus;
  late FlarkCellTheme theme;

  void mount(String source, {FlarkCellTheme? styling, int width = 40}) {
    theme = styling ?? const FlarkCellTheme();
    editor = FlarkEditor(createParseBackend(), text: source, caret: 0);
    controller = FlarkFleuryController(editor);
    tester = FleuryTester(viewportSize: CellSize(width, 40));
    focus = FocusNode();
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
  }

  CellDocumentLayout layout() => CellDocumentLayout(
    controller,
    tester.viewportSize.cols,
    theme,
    CellWidthPolicy.spec,
  );

  void click(CellOffset p) {
    for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
      tester.sendMouse(
        MouseEvent(
          kind: kind,
          button: MouseButton.left,
          col: p.col,
          row: p.row,
        ),
      );
    }
  }

  void key(KeyCode key, {bool cmd = false, bool shift = false}) =>
      tester.sendKey(
        KeyEvent(
          key,
          modifiers: {
            if (cmd) KeyModifier.superKey,
            if (shift) KeyModifier.shift,
          },
        ),
      );

  tearDown(() {
    tester.dispose();
    controller.dispose();
    focus.dispose();
  });

  for (final colors in [
    null,
    (const RgbColor(220, 230, 240), const RgbColor(18, 24, 32)),
    (const RgbColor(25, 35, 45), const RgbColor(250, 250, 252)),
  ]) {
    test('heading defaults use typography without decorations: $colors', () {
      final source = [
        for (var n = 1; n <= 6; n++) '${'#' * n} Title',
      ].join('\n');
      mount(
        source,
        styling: FlarkCellTheme(
          body: colors == null
              ? CellStyle.none
              : CellStyle(foreground: colors.$1, background: colors.$2),
        ),
      );
      focus.unfocus();
      final geometry = layout();
      final frame = tester.render();
      expect(geometry.lines, hasLength(6));
      for (final row in editor.projection.rows.where(
        (r) => r.kind == RowKind.heading,
      )) {
        final at = geometry.positionFor(row.sourceStart);
        expect(at.col, 0);
        final cell = frame.atColRow(0, at.row);
        final label = tester.renderToString().split('\n')[at.row].trimRight();
        expect(label, 'Title');
        expect(cell.style.bold, row.headingLevel <= 3);
        expect(cell.style.italic, [3, 4].contains(row.headingLevel));
        expect(cell.style.dim, row.headingLevel == 6);
        expect(cell.style.underline, isFalse);
        expect(cell.style.inverse, isFalse);
        expect(cell.style.background, theme.body.background);
        expect(frame.atColRow(39, at.row).style, theme.body);
      }
      expect(editor.source, source);
    });
  }

  test(
    'wrapped heading divider is outside selection, copying and arrow movement',
    () async {
      const source = '## Long heading with 界 and more words\nafter';
      mount(
        source,
        width: 16,
        styling: const FlarkCellTheme(
          headingStyles: {2: FlarkHeadingStyle(divider: true)},
        ),
      );
      final initial = layout();
      final headingEnd = source.indexOf('\n');
      editor.apply(SetSelection.caret(headingEnd));
      tester.render();
      final end = initial.positionFor(headingEnd);
      expect(end.row, greaterThan(0));
      expect(initial.lines[end.row + 1].headingRule, isTrue);
      key(KeyCode.arrowDown);
      tester.render();
      expect(editor.selection.extent, greaterThan(headingEnd));
      key(KeyCode.arrowUp);
      tester.render();
      expect(layout().positionFor(editor.selection.extent).row, end.row);
      click(CellOffset(0, end.row + 1));
      tester.type('X');
      expect(editor.source.substring(headingEnd + 1), '\nafter');
      expect(editor.source.substring(0, headingEnd + 1), contains('X'));
      key(KeyCode.z, cmd: true);
      tester.render();
      expect(editor.source, source);
      editor.apply(
        SetSelection(editor.projection.rows.first.sourceStart, headingEnd),
      );
      tester.render();
      key(KeyCode.c, cmd: true);
      await tester.settle(); // clipboard completion only
      expect(
        (tester.clipboard as InProcessClipboard).lastWritten,
        source.substring(3, headingEnd),
      );
      expect(layout().lines.length, initial.lines.length);
    },
  );

  test('monochrome title band leaves caret and selection visible', () {
    mount(
      '# Title',
      styling: const FlarkCellTheme(
        headingStyles: {1: FlarkHeadingStyle(band: true)},
      ),
    );
    var frame = tester.render();
    expect(frame.atColRow(0, 0).style.inverse, isFalse);
    expect(frame.atColRow(1, 0).style.inverse, isTrue);
    editor.apply(const SetSelection(2, 4));
    frame = tester.render();
    expect(frame.atColRow(0, 0).style.inverse, isFalse);
    expect(frame.atColRow(2, 0).style.inverse, isTrue);
    editor.apply(SetSelection.caret(editor.source.length));
    frame = tester.render();
    expect(frame.atColRow(5, 0).style.inverse, isFalse);
    tester.type('X');
    expect(editor.source, '# TitleX');
  });

  test(
    'level labels wrap with text and clicks never insert into decoration',
    () {
      const source = '###### Long 界 heading that wraps';
      mount(
        source,
        width: 12,
        styling: const FlarkCellTheme(
          headingStyles: {6: FlarkHeadingStyle(showLevel: true)},
        ),
      );
      var geometry = layout();
      final labelLine = geometry.lines.singleWhere(
        (l) => l.headingLabelColumn != null,
      );
      final labelRow = geometry.lines.indexOf(labelLine);
      final labelCol = labelLine.headingLabelColumn!;
      expect(labelCol + 2, lessThanOrEqualTo(12));
      for (final line in geometry.lines) {
        expect(line.glyphs.first.col, 0);
      }
      click(CellOffset(labelCol, labelRow));
      expect(editor.selection.extent, source.length);
      tester.type('X');
      expect(editor.source, '${source}X');
      tester.render();
      geometry = layout();
      expect(
        geometry.positionFor(editor.selection.extent).col,
        lessThan(geometry.lines.last.headingLabelColumn!),
      );
    },
  );

  test(
    'gutter aligns all blocks, keeps quote rails and maps clicks to source',
    () {
      const source =
          '# Title\nParagraph\n\n> ## Quote\n> text\n\n- [ ] Task\n\n```\ncode\n```\n\n| A | B |\n| - | - |\n| x | y |';
      mount(source, styling: const FlarkCellTheme(headingGutter: true));
      final geometry = layout();
      final frame = tester.render();
      CellOffset at(String word) => geometry.positionFor(source.indexOf(word));
      expect(at('Title').col, 3);
      expect(at('Paragraph').col, 3);
      expect(at('Quote').col, 5);
      expect(at('code').col, 5);
      expect(frame.atColRow(3, at('Quote').row).grapheme, '▎');
      expect(frame.atColRow(3, at('Quote').row + 1).grapheme, '▎');
      expect(frame.atColRow(0, at('Quote').row).grapheme, 'H');
      expect(frame.atColRow(1, at('Quote').row).grapheme, '2');
      final task = tester
          .semantics()
          .byRole(SemanticRole.checkbox)
          .single
          .bounds!;
      expect(task.left, 3);
      click(CellOffset(0, at('Quote').row));
      tester.type('X');
      expect(editor.source, source.replaceFirst('Quote', 'XQuote'));
      key(KeyCode.z, cmd: true);
      tester.render();
      for (final word in ['Task', 'code', 'A', 'x']) {
        click(at(word));
        tester.type('X');
        expect(editor.source, source.replaceFirst(word, 'X$word'));
        key(KeyCode.z, cmd: true);
        tester.render();
      }
      editor.setSourceMode(true);
      tester.render();
      expect(layout().headingGutter, 0);
    },
  );

  test(
    'per-level override changes layout without changing source or history',
    () {
      mount('## Section\ntext');
      final before = layout();
      final custom = FlarkCellTheme(
        headingStyles: {
          2: const FlarkHeadingStyle(
            style: CellStyle(underline: true),
            showLevel: true,
          ),
        },
      );
      final changed = CellDocumentLayout(
        controller,
        40,
        custom,
        CellWidthPolicy.spec,
      );
      expect(
        before.describes(controller, 40, custom, CellWidthPolicy.spec),
        isFalse,
      );
      expect(changed.lines.any((l) => l.headingRule), isFalse);
      expect(changed.lines.first.glyphs.first.style.underline, isTrue);
      expect(changed.lines.first.headingLabelColumn, 8);
      expect(editor.source, '## Section\ntext');
      expect(editor.history.undoTarget != null, isFalse);
    },
  );
}
