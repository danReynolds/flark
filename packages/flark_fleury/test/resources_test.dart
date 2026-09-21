import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury_legacy.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  late FleuryTester tester;
  late FlarkEditor editor;
  late FlarkFleuryController controller;
  late FocusNode focus;
  final opened = <Uri>[];

  setUp(() {
    tester = FleuryTester(viewportSize: const CellSize(60, 24));
    focus = FocusNode();
    opened.clear();
  });
  tearDown(() {
    tester.dispose();
    controller.dispose();
    focus.dispose();
  });

  void mount(
    String source, {
    bool readOnly = false,
    FlarkFleuryLinkPopoverBuilder? popover,
    FlarkFleuryResourcePresenter? presenter,
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
            onOpenLink: opened.add,
            baseUri: Uri.parse('https://example.com/docs/'),
            linkPopoverBuilder: popover,
            presentResourceEditor: presenter,
          ),
        ),
      ),
    );
    tester.render();
  }

  void key(KeyCode code, {bool cmd = false, bool shift = false}) =>
      tester.sendKey(
        KeyEvent(
          code,
          modifiers: {
            if (cmd) KeyModifier.superKey,
            if (shift) KeyModifier.shift,
          },
        ),
      );

  void click(int col, int row, {bool cmd = false}) {
    for (final kind in [MouseEventKind.down, MouseEventKind.up]) {
      tester.sendMouse(
        MouseEvent(
          kind: kind,
          button: MouseButton.left,
          col: col,
          row: row,
          modifiers: {if (cmd) KeyModifier.superKey},
        ),
      );
      tester.render();
    }
  }

  void button(String label) {
    final lines = tester.renderToString().split('\n');
    final row = lines.indexWhere((line) => line.contains('[ $label ]'));
    expect(row, greaterThanOrEqualTo(0), reason: lines.join('\n'));
    click(lines[row].indexOf('[ $label ]') + 2, row);
  }

  Future<void> value(String label, String text) async {
    final result = await tester.invokeSemanticAction(
      SemanticAction.setValue,
      role: SemanticRole.textField,
      label: label,
      payload: text,
    );
    expect(result.completed, isTrue);
  }

  test('short link popover fits actions instead of filling its width cap', () {
    mount('[hello](https://dart.dev)');
    click(1, 0);
    final popup = tester.semantics().byLabel('Link actions').single.bounds!;
    final lines = tester.renderToString().split('\n');
    final actions = lines.firstWhere((line) => line.contains('[ Open ]'));
    final contentStart = actions.indexOf('[ Open ]');
    final contentEnd = actions.indexOf('[ Close ]') + '[ Close ]'.length;
    expect(
      popup.size.cols,
      contentEnd - contentStart + 4,
      reason: 'one cell border and one cell padding on each side',
    );
    expect(popup.size.cols, lessThan(52));
    button('Remove');
    expect(editor.source, 'hello');
  });

  test(
    'click opens controls, Open resolves relative URI and restores typing',
    () {
      mount('Try [a link](../guide) and 界.');
      click(7, 0);
      expect(editor.source, 'Try [a link](../guide) and 界.');
      expect(tester.renderToString(), contains('[ Open ]'));
      button('Open');
      expect(opened, [Uri.parse('https://example.com/guide')]);
      expect(tester.renderToString(), isNot(contains('[ Open ]')));
      expect(focus.hasFocus, isTrue);
      tester.type('X');
      expect(editor.source, 'Try [a lXink](../guide) and 界.');
      expect(
        tester.renderToString().split('\n').first,
        startsWith('Try a lXink'),
      );
    },
  );

  test('modifier click opens; dragging a link selects without a popup', () {
    mount('[hello](https://dart.dev) world');
    click(1, 0, cmd: true);
    expect(opened, [Uri.parse('https://dart.dev')]);
    expect(tester.renderToString(), isNot(contains('[ Open ]')));
    tester.sendMouse(
      const MouseEvent(
        kind: MouseEventKind.down,
        button: MouseButton.left,
        col: 1,
        row: 0,
      ),
    );
    tester.sendMouse(
      const MouseEvent(
        kind: MouseEventKind.drag,
        button: MouseButton.left,
        col: 4,
        row: 0,
      ),
    );
    tester.sendMouse(
      const MouseEvent(
        kind: MouseEventKind.up,
        button: MouseButton.left,
        col: 4,
        row: 0,
      ),
    );
    expect(editor.selection.isCollapsed, isFalse);
    expect(tester.renderToString(), isNot(contains('[ Open ]')));
    tester.type('X');
    expect(editor.source, '[hXo](https://dart.dev) world');
    expect(tester.renderToString().split('\n').first, startsWith('hXo world'));
  });

  test(
    'Edit saves destination while preserving bold label; Undo and Cancel',
    () async {
      mount('[**hello**](https://dart.dev) world');
      click(2, 0);
      button('Edit');
      expect(tester.renderToString(), contains('Edit link'));
      expect(
        tester
            .semantics()
            .single(role: SemanticRole.textField, label: 'Destination')
            .value,
        'https://dart.dev',
      );
      key(KeyCode.a, cmd: true);
      tester.type('https://flutter.dev');
      expect(
        tester
            .semantics()
            .single(role: SemanticRole.textField, label: 'Destination')
            .value,
        'https://flutter.dev',
      );
      button('Save');
      expect(editor.source, '[**hello**](<https://flutter.dev>) world');
      expect(
        tester.renderToString().split('\n').first,
        startsWith('hello world'),
      );
      await tester.settle(); // modal completion restores focus asynchronously
      expect(focus.hasFocus, isTrue);
      key(KeyCode.z, cmd: true);
      expect(editor.source, '[**hello**](https://dart.dev) world');
      tester.render();
      key(KeyCode.k, cmd: true);
      expect(tester.renderToString(), contains('Edit link'));
      await value('Text', 'discard');
      button('Cancel');
      await tester.settle();
      expect(editor.source, '[**hello**](https://dart.dev) world');
      expect(focus.hasFocus, isTrue);
    },
  );

  test(
    'Cmd-K inserts a link over selection, blank destination keeps dialog',
    () async {
      mount('hello world');
      editor.apply(const SetSelection(0, 5));
      tester.render();
      key(KeyCode.k, cmd: true);
      expect(tester.renderToString(), contains('Insert link'));
      button('Save');
      expect(tester.renderToString(), contains('Enter a destination.'));
      expect(editor.source, 'hello world');
      await value('Destination', 'https://dart.dev');
      button('Save');
      expect(editor.source, '[hello](<https://dart.dev>) world');
      expect(
        tester.renderToString().split('\n').first,
        startsWith('hello world'),
      );
      await tester.settle();
      key(KeyCode.z, cmd: true);
      expect(editor.source, 'hello world');
    },
  );

  test(
    'Remove keeps label formatting, Escape closes without leaving editor',
    () {
      mount('[**hello**](https://dart.dev) world');
      click(2, 0);
      key(KeyCode.escape);
      expect(tester.renderToString(), isNot(contains('[ Open ]')));
      expect(focus.hasFocus, isTrue);
      click(2, 0);
      button('Remove');
      expect(editor.source, '**hello** world');
      expect(tester.render().atColRow(1, 0).style.bold, isTrue);
      key(KeyCode.z, cmd: true);
      expect(editor.source, '[**hello**](https://dart.dev) world');
    },
  );

  test('custom actions are inert after selection or source changes', () {
    late FlarkLinkActions actions;
    mount(
      '[hello](https://dart.dev) world',
      popover: (_, a) {
        actions = a;
        return FlarkLinkPopover(actions: a);
      },
    );
    click(2, 0);
    final stale = actions;
    editor.apply(SetSelection.caret(editor.source.length));
    tester.render();
    stale.open!();
    stale.remove!();
    expect(opened, isEmpty);
    expect(editor.source, '[hello](https://dart.dev) world');
    click(2, 0);
    final next = actions;
    editor.apply(const InsertText('X'));
    final source = editor.source;
    next.remove!();
    next.open!();
    expect(editor.source, source);
    expect(opened, isEmpty);
  });

  test(
    'a stale presenter completion cannot release a replacement dialog',
    () async {
      final sessions = <FlarkResourceSession>[];
      final completions = <Completer<void>>[];
      void show(FlarkFleuryController value) {
        tester.pumpWidget(
          Theme(
            data: const ThemeData(),
            child: FlarkEditorView(
              controller: value,
              focusNode: focus,
              autofocus: true,
              presentResourceEditor: (_, session) {
                sessions.add(session);
                final done = Completer<void>();
                completions.add(done);
                return done.future;
              },
            ),
          ),
        );
        tester.render();
      }

      editor = FlarkEditor(createParseBackend(), text: 'first');
      final first = FlarkFleuryController(editor);
      addTearDown(first.dispose);
      show(first);
      key(KeyCode.k, cmd: true);
      expect(sessions, hasLength(1));
      controller = FlarkFleuryController(
        FlarkEditor(createParseBackend(), text: 'second'),
      );
      show(controller);
      key(KeyCode.k, cmd: true);
      expect(sessions, hasLength(2));
      expect(sessions.first.active, isFalse);
      completions.first.complete();
      await Future<void>.delayed(Duration.zero);
      tester.render();
      key(KeyCode.k, cmd: true);
      expect(sessions, hasLength(2), reason: 'the second dialog is still open');
      expect(sessions.last.active, isTrue);
      completions.last.complete();
      await Future<void>.delayed(Duration.zero);
      tester.render();
      key(KeyCode.k, cmd: true);
      expect(sessions, hasLength(3));
      completions.last.complete();
      await Future<void>.delayed(Duration.zero);
    },
  );

  test(
    'custom edit session refuses a moved target and a closed presentation',
    () async {
      late FlarkResourceSession session;
      final done = Completer<void>();
      mount(
        '[hello](https://dart.dev) world',
        presenter: (_, s) {
          session = s;
          return done.future;
        },
      );
      click(2, 0);
      button('Edit');
      expect(session.active, isTrue);
      editor.apply(SetSelection.caret(editor.source.length));
      expect(
        session.save(destination: 'https://flutter.dev', label: 'hello'),
        isFalse,
      );
      done.complete();
      await tester.settle();
      expect(session.active, isFalse);
      expect(session.remove(), isFalse);
      expect(editor.source, '[hello](https://dart.dev) world');
    },
  );

  test(
    'read-only click opens directly; unsafe destination has no mutation actions',
    () {
      late FlarkLinkActions actions;
      mount(
        '[hello](https://dart.dev) world',
        readOnly: true,
        popover: (_, a) {
          actions = a;
          return FlarkLinkPopover(actions: a);
        },
      );
      click(2, 0);
      expect(opened, [Uri.parse('https://dart.dev')]);
      expect(tester.renderToString(), isNot(contains('[ Open ]')));
      key(KeyCode.k, cmd: true);
      tester.type('X');
      expect(editor.source, '[hello](https://dart.dev) world');
      editor.apply(
        ReplaceRange(0, editor.source.length, '[hello](javascript:alert)'),
      );
      tester.render();
      click(2, 0);
      expect(actions.open, isNull);
      expect(actions.edit, isNull);
      expect(actions.remove, isNull);
    },
  );

  test('custom popover height is measured before flipping above a link', () {
    tester.viewportSize = const CellSize(40, 18);
    mount(
      '${'line\n' * 14}[hello](https://dart.dev)',
      popover: (_, actions) => Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var i = 0; i < 9; i++) Text('Custom $i', allowSelect: false),
          FlarkLinkPopover(actions: actions),
        ],
      ),
    );
    editor.apply(SetSelection.caret(editor.source.indexOf('hello') + 2));
    tester.render();
    click(focus.caretRect!.left, focus.caretRect!.top);
    final frame = tester.renderToString();
    expect(frame, contains('Custom 0'));
    expect(frame, contains('Custom 8'));
    expect(frame, contains('[ Close ]'));
    button('Close');
    expect(focus.hasFocus, isTrue);
  });

  test('an open popover stays usable in the first frame after resize', () {
    mount('${'line\n' * 12}${'x' * 38} [hello](https://dart.dev)');
    editor.apply(SetSelection.caret(editor.source.indexOf('hello') + 2));
    tester.render();
    click(focus.caretRect!.left, focus.caretRect!.top);
    expect(tester.renderToString(), contains('[ Remove ]'));
    final source = editor.source, selection = editor.selection;
    tester.viewportSize = const CellSize(28, 8);
    expect(tester.renderToString(), contains('[ Remove ]'));
    expect(editor.source, source);
    expect(editor.selection, selection);
    button('Remove');
    expect(editor.source, '${'line\n' * 12}${'x' * 38} hello');
  });

  test('popover and editor work at narrow width and after scrolling', () {
    tester.viewportSize = const CellSize(28, 16);
    mount('${'line\n' * 18}[hello](https://dart.dev)');
    editor.apply(SetSelection.caret(editor.source.indexOf('hello') + 2));
    tester.render();
    final row = focus.caretRect!.top;
    click(2, row);
    expect(tester.renderToString(), contains('[ Remove ]'));
    button('Remove');
    expect(editor.source, '${'line\n' * 18}hello');
    expect(tester.renderToString(), contains('hello'));
  });
}
