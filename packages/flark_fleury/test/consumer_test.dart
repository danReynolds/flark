import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  test('public toolbar state, save routing and remount', () async {
    final c = FlarkController(markdown: 'hello');
    await c.ready;
    final tester = FleuryTester(viewportSize: const CellSize(100, 30));
    final saved = <String>[];
    void mount() => tester.pumpWidget(
      FleuryApp(
        title: 'Consumer',
        home: FlarkEditor(controller: c, onChanged: saved.add),
      ),
    );
    mount();
    tester.render();
    c.selectAll();
    tester.render();
    final press = await tester.invokeSemanticAction(
      SemanticAction.activate,
      role: SemanticRole.button,
      label: 'Bold',
    );
    expect(press.completed, isTrue);
    tester.render();
    expect(c.markdown, '**hello**');
    expect(c.state.styles.bold.isOn, isTrue);
    expect(saved, ['**hello**']);
    c.loadMarkdown('fetched');
    expect(saved.length, 1);
    tester.pumpWidget(const Text('unmounted'));
    tester.render();
    c.replaceMarkdown('offline');
    mount();
    tester.render();
    expect(c.markdown, 'offline');
    expect(saved.length, 1);
    tester.dispose();
    c.dispose();
    c.dispose();
  });
  test('dedicated reader renders and reacts', () async {
    final tester = FleuryTester(viewportSize: const CellSize(80, 25));
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: const FlarkMarkdown(markdown: '# Article\n\n**body**'),
      ),
    );
    await Future<void>.delayed(Duration.zero);
    tester.render();
    expect(tester.renderToString(), contains('Article'));
    expect(tester.renderToString(), contains('body'));
    final semantics = tester.semantics().single(
      role: SemanticRole.textArea,
      label: 'Markdown',
    );
    expect(semantics.value, contains('body'));
    expect(semantics.actions, isEmpty);
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: const FlarkMarkdown(markdown: '# Changed'),
      ),
    );
    tester.render();
    expect(tester.renderToString(), contains('Changed'));
    tester.dispose();
  });
  test('a selectable reader selects all and copies with Command', () async {
    final tester = FleuryTester(viewportSize: const CellSize(80, 25));
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: const FlarkMarkdown(
          markdown: '# Article\n\n**body**',
          selectable: true,
        ),
      ),
    );
    await Future<void>.delayed(Duration.zero);
    tester.render();
    for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
      tester.sendMouse(
        MouseEvent(button: MouseButton.left, kind: kind, col: 1, row: 0),
      );
    }
    // Command arrives as super under the Kitty keyboard protocol.
    for (final code in [KeyCode.a, KeyCode.c]) {
      tester.sendKey(KeyEvent(code, modifiers: const {KeyModifier.superKey}));
    }
    await tester.settle();
    final copied = (tester.clipboard as InProcessClipboard).lastWritten;
    expect(copied, contains('Article'));
    expect(copied, contains('body'));
    tester.dispose();
  });

  test('a reader paints a first image as any other, its label only while '
      'selected', () async {
    // An editor shows a standalone image's label while its caret is at the
    // image. A reader paints no caret, and its selection rests at the
    // document's start: a document that began with an image showed that
    // image's label, and no other.
    final tester = FleuryTester(viewportSize: const CellSize(40, 30));
    void read(String markdown) => tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: FlarkMarkdown(
          markdown: markdown,
          selectable: true,
          imagePreviewBuilder: (_, _, _) => const Text('PREVIEW'),
        ),
      ),
    );
    for (final markdown in [
      '![Photo label](demo.png)\n\nafter',
      'intro\n\n![Photo label](demo.png)\n\nafter',
    ]) {
      read(markdown);
      await Future<void>.delayed(Duration.zero);
      tester.render();
      expect(tester.renderToString(), contains('PREVIEW'));
      expect(tester.renderToString(), isNot(contains('Photo label')));
    }
    // A selection that takes the image shows its label, as in an editor.
    final rows = tester.renderToString().split('\n');
    final after = rows.indexWhere((row) => row.startsWith('after'));
    expect(after, isPositive);
    for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
      tester.sendMouse(
        MouseEvent(button: MouseButton.left, kind: kind, col: 1, row: after),
      );
    }
    expect(tester.renderToString(), isNot(contains('Photo label')));
    tester.sendKey(
      const KeyEvent(KeyCode.a, modifiers: {KeyModifier.superKey}),
    );
    expect(tester.renderToString(), contains('Photo label'));
    tester.dispose();
  });

  test('a reader opens a link only from the link text it paints', () async {
    final opened = <Uri>[];
    final tester = FleuryTester(viewportSize: const CellSize(40, 4));
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: FlarkMarkdown(
          markdown: 'x[ab](https://a.example)c',
          onOpenLink: opened.add,
        ),
      ),
    );
    await Future<void>.delayed(Duration.zero);
    expect(tester.renderToString(), startsWith('xabc'));
    void click(int col) {
      for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
        tester.sendMouse(
          MouseEvent(button: MouseButton.left, kind: kind, col: col, row: 0),
        );
      }
    }

    click(3); // "c", after the link
    click(20); // blank, past the end of the line
    expect(opened, isEmpty);
    click(2); // "b"
    expect(opened, [Uri.parse('https://a.example')]);
    // A preview opens the link around its image, as in Flutter.
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: FlarkMarkdown(
          markdown: '[![pic](i.png)](https://b.example)',
          onOpenLink: opened.add,
          imagePreviewBuilder: (_, _, _) => const Text('PREVIEW'),
        ),
      ),
    );
    tester.render();
    final preview = tester
        .renderToString()
        .split('\n')
        .indexWhere((row) => row.contains('PREVIEW'));
    for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
      tester.sendMouse(
        MouseEvent(button: MouseButton.left, kind: kind, col: 2, row: preview),
      );
    }
    expect(opened.last, Uri.parse('https://b.example'));
    // A link in an image's alt text opens from its own text.
    tester.pumpWidget(
      FleuryApp(
        title: 'Reader',
        home: FlarkMarkdown(
          markdown: 'see ![x <https://c.example>](i.png)',
          onOpenLink: opened.add,
          imagePreviewBuilder: (_, _, _) => const Text('PREVIEW'),
        ),
      ),
    );
    final row = tester.renderToString().split('\n').first;
    for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
      tester.sendMouse(
        MouseEvent(
          button: MouseButton.left,
          kind: kind,
          col: row.indexOf('c.example'),
          row: 0,
        ),
      );
    }
    expect(opened.last, Uri.parse('https://c.example'));
    tester.dispose();
  });
  test(
    'a composition the controller ended cannot overwrite loaded text',
    () async {
      final c = FlarkController(markdown: 'word');
      await c.ready;
      final focus = FocusNode();
      final tester = FleuryTester(viewportSize: const CellSize(100, 30));
      tester.pumpWidget(
        FleuryApp(
          title: 'Composition',
          home: FlarkEditor(controller: c, focusNode: focus, autofocus: true),
        ),
      );
      tester.render();
      c.setSelection(4, 4);
      tester.dispatcher.dispatch(const TextCompositionEvent.update('xyz'));
      expect(c.markdown, 'wordxyz');
      // Fetched content replaces the document while an input method composes.
      c.loadMarkdown('fresh');
      tester.dispatcher.dispatch(const TextCompositionEvent.update('xyz!'));
      expect(c.markdown, contains('fresh'));
      tester.dispatcher.dispatch(const TextCompositionEvent.cancel());
      expect(c.markdown, 'fresh');
      tester.dispose();
      c.dispose();
      focus.dispose();
    },
  );

  test(
    'the session opens no resource editor where the kernel cannot set one',
    () async {
      // Its presenter follows the kernel's canSetResource, as the toolbar's
      // buttons do, rather than opening a form whose Save can only fail.
      final c = FlarkController(markdown: 'one\n\ntwo');
      await c.ready;
      final tester = FleuryTester(viewportSize: const CellSize(60, 20));
      tester.pumpWidget(
        FleuryApp(
          title: 'Resources',
          home: FlarkEditor(controller: c),
        ),
      );
      tester.render();
      c.setSelection(0, 8);
      tester.render();
      expect(c.state.link.canSet, isFalse);
      for (final open in [c.showLinkEditor, c.showImageEditor]) {
        final result = open();
        tester.render();
        expect(tester.renderToString(), isNot(contains('Insert')));
        expect((await result).reason, FlarkEditRejection.unavailable);
      }
      expect(c.markdown, 'one\n\ntwo');
      tester.dispose();
      c.dispose();
    },
  );
}
