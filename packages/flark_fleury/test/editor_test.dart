import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/cell_layout.dart';
import 'package:flark_tree_sitter/flark.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_tree_sitter/highlight_worker.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  late FlarkParseBackend backend;
  late FleuryTester tester;
  late FlarkEditor editor;
  late FlarkFleuryController controller;
  late FocusNode focus;

  setUp(() {
    backend = createParseBackend();
    tester = FleuryTester(viewportSize: const CellSize(40, 12));
    focus = FocusNode();
  });
  tearDown(() {
    tester.dispose();
    controller.dispose();
    focus.dispose();
  });

  void mount(
    String source, {
    int? caret,
    bool readOnly = false,
    FlarkTreeSitter? code,
    CodeHighlightWorker? worker,
  }) {
    editor = FlarkEditor(
      backend,
      text: source,
      caret: caret ?? source.length,
      codeEditing: code,
    );
    controller = FlarkFleuryController(editor, highlightWorker: worker);
    tester.pumpWidget(
      Theme(
        data: const ThemeData(),
        child: FlarkEditorView(
          controller: controller,
          autofocus: true,
          focusNode: focus,
          readOnly: readOnly,
        ),
      ),
    );
    tester.render();
  }

  List<String> lines() {
    final buffer = tester.render();
    return [
      for (var y = 0; y < tester.viewportSize.rows; y++)
        [
          for (var x = 0; x < tester.viewportSize.cols; x++)
            if (buffer.atColRow(x, y).role != CellRole.continuation)
              buffer.atColRow(x, y).grapheme ?? ' ',
        ].join().trimRight(),
    ];
  }

  void key(KeyCode key, {bool shift = false, bool cmd = false}) {
    tester.sendKey(
      KeyEvent(
        key,
        modifiers: {
          if (shift) KeyModifier.shift,
          if (cmd) KeyModifier.superKey,
        },
      ),
    );
  }

  void click(int col, int row, {bool shift = false}) {
    tester.sendMouse(
      MouseEvent(
        button: MouseButton.left,
        kind: MouseEventKind.down,
        col: col,
        row: row,
        modifiers: {if (shift) KeyModifier.shift},
      ),
    );
    tester.sendMouse(
      MouseEvent(
        button: MouseButton.left,
        kind: MouseEventKind.up,
        col: col,
        row: row,
      ),
    );
  }

  for (var level = 1; level <= 6; level++) {
    test('author heading $level from an empty paragraph, then continue', () {
      mount('');
      for (var i = 1; i <= level; i++) {
        tester.type('#');
        expect(editor.source, '#' * i);
        expect(editor.selection.extent, i);
        expect(lines().first, '#' * i);
        expect(focus.caretRect!.left, i);
        expect(tester.render().atColRow(0, 0).style.bold, isFalse);
      }
      tester.type(' ');
      expect(editor.source, '${'#' * level} ');
      expect(editor.projection.rows.single.kind, RowKind.heading);
      expect(editor.projection.rows.single.headingLevel, level);
      expect(editor.projection.rows.single.text, '');
      expect(lines().first, '');
      expect(editor.selection.extent, level + 1);
      expect(focus.caretRect!.left, 0);
      expect(tester.render().atColRow(0, 0).style.inverse, isTrue);
      expect(tester.render().atColRow(1, 0).style.inverse, isFalse);
      tester.type('H');
      expect(editor.source, '${'#' * level} H');
      expect(editor.selection.extent, level + 2);
      expect(lines().first, 'H');
      expect(tester.render().atColRow(0, 0).style.bold, isTrue);
      key(KeyCode.enter);
      tester.type('x');
      expect(editor.source, '${'#' * level} H\nx');
      expect(lines().take(2), ['H', 'x']);
      expect(tester.render().atColRow(0, 1).style.bold, isFalse);
    });
  }

  for (final delimiter in ['*', '**']) {
    test('author $delimiter inline syntax without an intermediate list', () {
      mount('');
      var source = '';
      final input = '${delimiter}word$delimiter';
      for (var i = 0; i < input.length; i++) {
        final char = input[i];
        tester.type(char);
        source += char;
        expect(editor.source, source);
        expect(editor.selection.extent, source.length);
        expect(editor.projection.rows.single.shells, isEmpty);
        expect(
          lines().first,
          i == input.length - 1
              ? 'word'
              : delimiter == '**' && i == input.length - 2
              ? '*word'
              : source,
        );
      }
      final style = tester.render().atColRow(1, 0).style;
      expect(delimiter == '**' ? style.bold : style.italic, isTrue);
      tester.type(' ');
      tester.type('x');
      expect(editor.source, '$input x');
      expect(lines().first, 'word x');
      expect(tester.render().atColRow(5, 0).style.bold, isFalse);
      expect(tester.render().atColRow(5, 0).style.italic, isFalse);
    });
  }

  test(
    'empty heading backspace and undo preserve the content-origin caret',
    () {
      mount('');
      tester.type('##');
      tester.type(' ');
      expect(lines().first, '');
      expect(focus.caretRect!.left, 0);
      key(KeyCode.backspace);
      expect(editor.source, '');
      expect(lines().first, '');
      expect(focus.caretRect!.left, 0);
      key(KeyCode.z, cmd: true);
      expect(editor.source, '## ');
      expect(lines().first, '');
      expect(focus.caretRect!.left, 0);
      tester.type('x');
      expect(editor.source, '## x');
      expect(lines().first, 'x');
      expect(focus.caretRect!.left, 1);
    },
  );

  test('bullet and wrapped text share a row and stable content columns', () {
    tester.viewportSize = const CellSize(8, 8);
    mount('- abcdefghij');
    expect(lines().take(2), ['● abcde', '  fghij']);
    final buffer = tester.render();
    expect(buffer.atColRow(0, 0).style.dim, isFalse);
    expect(buffer.atColRow(0, 0).style, buffer.atColRow(2, 0).style);
    expect(buffer.atColRow(1, 0).grapheme, ' ');
    click(2, 0);
    expect(editor.selection.extent, 2);
    tester.type('X');
    expect(editor.source, '- Xabcdefghij');
    expect(lines().take(3), ['● Xabcd', '  efghi', '  j']);
    expect(focus.caretRect!.left, 3);

    final wide = CellDocumentLayout(
      controller,
      8,
      const FlarkCellTheme(),
      CellWidthPolicy.cjk,
    );
    expect(wide.lines.first.prefix, '* ');
    expect(wide.lines.first.glyphs.first.col, 2);
    expect(wide.lines[1].prefix, '  ');
    expect(wide.positionFor(3).col, 3);
  });

  for (final marker in ['*', '-', '+']) {
    test('space commits $marker as a list; Enter continues and exits', () {
      mount('');
      tester.type(marker);
      expect(lines().first, marker);
      expect(editor.selection.extent, 1);
      key(KeyCode.backspace);
      expect(editor.source, '');
      expect(lines().first, '');
      tester.type(marker);
      tester.type(' ');
      expect(editor.source, '$marker ');
      expect(lines().first, '●');
      tester.type('x');
      expect(lines().first, '● x');
      key(KeyCode.enter);
      expect(editor.source, '$marker x\n$marker ');
      expect(lines().take(2), ['● x', '●']);
      key(KeyCode.enter);
      tester.type('p');
      expect(editor.source, '$marker x\n\np');
      expect(lines().take(3), ['● x', '', 'p']);
      expect(editor.projection.rows.last.shells, isEmpty);
    });
  }

  test('clicking a task continuation indent does not toggle its checkbox', () {
    mount('- [ ] first\n\n  second');
    expect(lines().take(3), ['[ ] first', '', '    second']);
    click(1, 2);
    expect(editor.source, '- [ ] first\n\n  second');
    tester.type('X');
    expect(editor.source, '- [ ] first\n\n  Xsecond');
    expect(lines()[2], '    Xsecond');
  });

  test('quote bar is substantial and code uses padding through wrapping', () {
    tester.viewportSize = const CellSize(12, 12);
    mount('> quoted words wrap\n\n```text\nabcdefghi jkl\n```');
    expect(lines().take(6), [
      '▎ quoted wo',
      '▎ rds wrap',
      '',
      '  abcdefghi',
      '   jkl',
      '',
    ]);
    expect(tester.render().atColRow(0, 0).grapheme, '▎');
    expect(tester.render().atColRow(0, 3).grapheme ?? ' ', ' ');
    click(2, 3);
    tester.type('X');
    expect(
      editor.source,
      '> quoted words wrap\n\n```text\nXabcdefghi jkl\n```',
    );
    expect(lines()[3], '  Xabcdefgh');
  });

  test('typed bold, backspace, space and next character paint immediately', () {
    mount('**what**', caret: 6);
    key(KeyCode.backspace);
    expect(editor.source, '**wha**');
    expect(lines().first, 'wha');
    expect(tester.render().atColRow(1, 0).style.bold, isTrue);
    tester.type(' ');
    expect(editor.source, '**wha** ');
    expect(lines().first, 'wha');
    tester.type('x');
    expect(editor.source, '**wha** **x**');
    expect(lines().first, 'wha x');
    expect(tester.render().atColRow(4, 0).style.bold, isTrue);
  });

  test('clicking bold end restores its context and next character', () {
    mount('**word** next');
    click(4, 0);
    tester.type('s');
    expect(editor.source, '**words** next');
    expect(lines().first, 'words next');
    expect(tester.render().atColRow(4, 0).style.bold, isTrue);
  });

  test('up through consecutive empty rows, then type at the shown caret', () {
    mount('a\n\n\nb', caret: 3);
    key(KeyCode.arrowUp);
    expect(editor.selection.extent, 2);
    expect(focus.caretRect, isNotNull);
    lines();
    expect(focus.caretRect!.top, 1);
    tester.type('x');
    expect(editor.source, 'a\nx\n\nb');
    expect(lines().take(4), ['a', 'x', '', 'b']);
  });

  test('wrapped vertical selection uses cells and preserves goal column', () {
    tester.viewportSize = const CellSize(8, 8);
    mount('abcdefghijk', caret: 10);
    expect(lines().take(2), ['abcdefg', 'hijk']);
    key(KeyCode.arrowUp, shift: true);
    expect(editor.selection, const FlarkSelection(10, 3));
    expect(tester.render().atColRow(3, 0).style.inverse, isTrue);
    tester.type('Z');
    expect(editor.source, 'abcZk');
    expect(lines().first, 'abcZk');
  });

  test('typing then moving before a paint uses the current source geometry', () {
    tester.viewportSize = const CellSize(8, 8);
    mount('abc', caret: 3);
    tester.type('defghijk');
    // Deliberately no render/pump between these events in the same input batch.
    key(KeyCode.arrowUp);
    expect(editor.selection.extent, 4);
    tester.type('X');
    expect(editor.source, 'abcdXefghijk');
    expect(lines().take(2), ['abcdXef', 'ghijk']);
  });

  test(
    'pointer drag selects across code lines and replacement stays in fence',
    () {
      mount('```text\nabc\ndef\n```');
      tester.sendMouse(
        const MouseEvent(
          kind: MouseEventKind.down,
          button: MouseButton.left,
          col: 3,
          row: 0,
        ),
      );
      tester.sendMouse(
        const MouseEvent(
          kind: MouseEventKind.drag,
          button: MouseButton.left,
          col: 4,
          row: 1,
        ),
      );
      tester.sendMouse(
        const MouseEvent(
          kind: MouseEventKind.up,
          button: MouseButton.left,
          col: 4,
          row: 1,
        ),
      );
      expect(
        editor.source.substring(editor.selection.start, editor.selection.end),
        'bc\nde',
      );
      expect(tester.render().atColRow(3, 0).style.inverse, isTrue);
      tester.type('X');
      expect(editor.source, '```text\naXf\n```');
      expect(lines().first, '  aXf');
    },
  );

  test('task click and list Enter use kernel commands, then undo', () {
    mount('- [ ] task');
    click(1, 0);
    expect(editor.source, '- [x] task');
    expect(lines().first, '[x] task');
    key(KeyCode.end);
    key(KeyCode.enter);
    expect(editor.source, '- [x] task\n- [ ] ');
    expect(lines().take(2), ['[x] task', '[ ]']);
    tester.type('next');
    expect(lines().take(2), ['[x] task', '[ ] next']);
    key(KeyCode.z, cmd: true);
    expect(lines().take(2), ['[x] task', '[ ]']);
  });

  test(
    'source mode keeps raw marker editing and undo in the same document',
    () {
      mount('**word**');
      editor.setSourceMode(true);
      expect(lines().first, '**word**');
      key(KeyCode.a, cmd: true);
      tester.paste('# Heading');
      expect(lines().first, '# Heading');
      editor.setSourceMode(false);
      expect(lines().first, 'Heading');
      expect(tester.render().atColRow(0, 0).style.bold, isTrue);
      key(KeyCode.z, cmd: true);
      expect(editor.source, '**word**');
      expect(lines().first, 'word');
    },
  );

  test(
    'scrolling and focus changes publish the visible caret and next input',
    () {
      tester.viewportSize = const CellSize(12, 4);
      mount('0\n1\n2\n3\n4\n5\n6\n7\n8\n9');
      expect(lines(), ['6', '7', '8', '9']);
      expect(focus.caretRect!.top, 3);
      key(KeyCode.escape);
      expect(focus.hasFocus, isFalse);
      lines();
      expect(focus.caretRect, isNull);
      click(1, 0);
      tester.type('x');
      expect(editor.source, contains('6x\n7'));
      expect(lines().first, '6x');
      expect(focus.caretRect!.top, 0);
    },
  );

  test('cached editor moves pointer and caret geometry with its ancestor', () {
    mount('abc', caret: 1);
    final scroll = ScrollController();
    addTearDown(scroll.dispose);
    final child = RepaintBoundary(
      child: SizedBox(
        height: 3,
        child: FlarkEditorView(
          controller: controller,
          focusNode: focus,
          autofocus: true,
        ),
      ),
    );
    tester.viewportSize = const CellSize(20, 6);
    tester.pumpWidget(
      ScrollView(
        controller: scroll,
        child: Column(
          children: [
            const SizedBox(height: 3),
            child,
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
    tester.render();
    expect(focus.caretRect!.top, 3);
    scroll.jumpTo(2);
    tester.render();
    expect(focus.caretRect!.top, 1);
    click(2, 1);
    tester.type('X');
    expect(editor.source, 'abXc');
    expect(lines(), contains('abXc'));
  });

  test('wide and combining glyphs map pointer, selection and deletion', () {
    mount('a界e\u0301z');
    click(2, 0); // trailing cell of the CJK glyph
    expect(editor.selection.extent, 2);
    tester.type('X');
    expect(editor.source, 'a界Xe\u0301z');
    expect(lines().first, 'a界Xe\u0301z');
    key(KeyCode.arrowRight);
    lines();
    key(KeyCode.backspace);
    expect(editor.source, 'a界Xz');
    expect(lines().first, 'a界Xz');
  });

  test(
    'segmented paste is one transaction and selection changes invalidate it',
    () {
      mount('before');
      tester.dispatcher.dispatch(
        const PasteEvent.segment(
          '\n**one',
          pasteId: 1,
          phase: PasteEventPhase.start,
        ),
      );
      expect(editor.source, 'before');
      tester.dispatcher.dispatch(
        const PasteEvent.segment(
          '**\r\ntwo',
          pasteId: 1,
          phase: PasteEventPhase.end,
        ),
      );
      expect(lines().take(3), ['before', 'one', 'two']);
      expect(editor.source, 'before\n**one**\ntwo');
      key(KeyCode.z, cmd: true);
      expect(editor.source, 'before');
      expect(lines().first, 'before');
      tester.dispatcher.dispatch(
        const PasteEvent.segment(
          'bad',
          pasteId: 2,
          phase: PasteEventPhase.start,
        ),
      );
      editor.apply(const SetSelection.caret(0));
      tester.dispatcher.dispatch(
        const PasteEvent.segment(
          'paste',
          pasteId: 2,
          phase: PasteEventPhase.end,
        ),
      );
      expect(editor.source, 'before');
      tester.type('X');
      expect(editor.source, 'Xbefore');
    },
  );

  test(
    'fence scoped select all and Ruby indentation use the shared engine',
    () {
      final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
      addTearDown(code.dispose);
      mount(
        'above\n\n```ruby\ndef hello\n\n```\n\nbelow',
        caret: 24,
        code: code,
      );
      editor.apply(SetSelection.caret(editor.source.indexOf('hello') + 5));
      lines();
      key(KeyCode.enter);
      expect(editor.source, contains('def hello\n  \n'));
      expect(lines(), contains('    '.trimRight()));
      for (final letter in ['e', 'n', 'd']) {
        tester.type(letter);
        lines();
      }
      expect(editor.source, contains('def hello\nend\n'));
      expect(lines(), contains('  end'));
      key(KeyCode.a, cmd: true);
      expect(
        editor.source.substring(editor.selection.start, editor.selection.end),
        startsWith('def hello\nend'),
      );
      expect(editor.selection.start, greaterThan(0));
      tester.paste('puts "ok"');
      expect(lines(), contains('  puts "ok"'));
      expect(editor.source, contains('```ruby\nputs "ok"\n```'));
      key(KeyCode.z, cmd: true);
      expect(editor.source, contains('def hello\nend'));
    },
  );

  test('composition replacement, cancel and commit preserve one undo unit', () {
    mount('a');
    focus.textCompositionClaimant!.onTextCompositionUpdate('n');
    expect(lines().first, 'an');
    focus.textCompositionClaimant!.onTextCompositionUpdate('に');
    expect(lines().first, 'aに');
    focus.textCompositionClaimant!.onTextCompositionCancel();
    expect(lines().first, 'a');
    focus.textCompositionClaimant!.onTextCompositionUpdate('日本');
    expect(lines().first, 'a日本');
    focus.textCompositionClaimant!.onTextCompositionCommit(null);
    key(KeyCode.z, cmd: true);
    expect(lines().first, 'a');
  });

  test(
    'read only declines every mutation route but permits selection and copy',
    () async {
      mount('**hello**', readOnly: true);
      tester.type('X');
      tester.paste('bad');
      key(KeyCode.backspace);
      key(KeyCode.b, cmd: true);
      focus.textCompositionClaimant!.onTextCompositionUpdate('bad');
      expect(editor.source, '**hello**');
      key(KeyCode.a, cmd: true);
      expect(editor.selection, const FlarkSelection(0, 9));
      expect(lines().first, 'hello');
      key(KeyCode.c, cmd: true);
      await tester
          .settle(); // clipboard completion, never used for edited-frame checks
      expect((tester.clipboard as InProcessClipboard).lastWritten, '**hello**');
    },
  );

  test(
    'theme and resize preserve source, selection, history and next input',
    () {
      mount('**word**', caret: 6);
      tester.type('s');
      expect(lines().first, 'words');
      final selection = editor.selection;
      tester.pumpWidget(
        Theme(
          data: const ThemeData(
            extensions: [
              FlarkCellTheme(body: CellStyle(foreground: Colors.cyan)),
            ],
          ),
          child: FlarkEditorView(controller: controller, focusNode: focus),
        ),
      );
      tester.viewportSize = const CellSize(5, 8);
      expect(lines().take(2), ['word', 's']);
      expect(editor.selection, selection);
      expect(tester.render().atColRow(0, 0).style.foreground, Colors.cyan);
      focus.requestFocus();
      key(KeyCode.z, cmd: true);
      expect(editor.source, '**word**');
      expect(lines().first, 'word');
      tester.type('!');
      expect(editor.source, '**word!**');
    },
  );

  test(
    'real worker colors only exact current source, without holding edits',
    () async {
      final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
      addTearDown(code.dispose);
      final worker = await CodeHighlightWorker.start();
      mount(
        '```ruby\ndef hello\nend\n```',
        caret: 17,
        code: code,
        worker: worker,
      );
      for (
        var i = 0;
        i < 100 && controller.colorsFor(editor.projection.rows.first) == null;
        i++
      ) {
        await tester.settle();
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      final row = editor.projection.rows.first;
      expect(controller.colorsFor(row)?.language, CodeLanguage.ruby);
      editor.apply(SetSelection.caret(editor.source.indexOf('hello') + 5));
      tester.type('x');
      expect(
        editor.source,
        contains(
          'hello'
          'x',
        ),
      );
      expect(lines().first, '  def hellox');
      expect(controller.colorsFor(editor.projection.rows.first), isNull);
    },
  );
}
