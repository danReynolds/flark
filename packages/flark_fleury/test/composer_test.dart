import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  late FleuryTester tester;
  late FlarkEditor editor;
  late FlarkFleuryController controller;
  late FocusNode focus;
  void mount(
    String source, {
    int width = 100,
    bool toolbar = true,
    bool readOnly = false,
  }) {
    tester = FleuryTester(viewportSize: CellSize(width, 30));
    editor = FlarkEditor(createParseBackend(), text: source, caret: 0);
    controller = FlarkFleuryController(editor);
    focus = FocusNode();
    tester.pumpWidget(
      FleuryApp(
        title: 'Composer test',
        home: FlarkEditorView(
          controller: controller,
          focusNode: focus,
          autofocus: true,
          showToolbar: toolbar,
          readOnly: readOnly,
          theme: const FlarkCellTheme(
            thematicBreak: CellStyle(foreground: RgbColor(100, 120, 140)),
          ),
        ),
      ),
    );
    tester.render();
  }

  tearDown(() {
    tester.dispose();
    controller.dispose();
    focus.dispose();
  });
  Future<void> press(
    String label, {
    SemanticRole role = SemanticRole.button,
  }) async {
    final result = await tester.invokeSemanticAction(
      SemanticAction.activate,
      role: role,
      label: label,
    );
    expect(result.completed, isTrue, reason: label);
    tester.render();
  }

  for (final width in [24, 100]) {
    test(
      'mixed and pending formatting remain observable at $width columns',
      () async {
        mount('**bold** plain', width: width);
        editor.apply(const SelectAll());
        tester.render();
        expect(controller.styleState(Style.strong).isMixed, isTrue);
        expect(tester.semantics().byLabel('Bold').single.value, 'Mixed');
        expect(
          tester.semantics().byLabel('Bold').single.hint,
          'Mixed formatting',
        );
        expect(tester.renderToString(), contains('B−'));
        await press('Bold');
        expect(editor.source, '**bold plain**');
        expect(tester.semantics().byLabel('Bold').single.selected, isTrue);
        await press('Undo');
        expect(controller.styleState(Style.strong).isMixed, isTrue);
        editor.apply(ReplaceRange(0, editor.source.length, ''));
        var notifications = 0;
        controller.addListener(() => notifications++);
        expect(controller.setStyle(Style.strong, enabled: true), isTrue);
        expect(notifications, 1);
        expect(controller.setStyle(Style.strong, enabled: true), isFalse);
        expect(notifications, 1);
        tester.render();
        expect(tester.semantics().byLabel('Bold').single.selected, isTrue);
        tester.type('hello');
        expect(editor.source, '**hello**');
        editor.apply(const SetSelection.caret(0));
        tester.render();
        expect(tester.semantics().byLabel('Bold').single.selected, isFalse);
      },
    );

    test(
      'rule paints in its container without changing source at $width columns',
      () {
        const source =
            'above\n\n---\n\n> before\n>\n> ***\n>\n> after\n\nbelow';
        mount(source, width: width, toolbar: false);
        focus.unfocus();
        final lines = tester.renderToString().split('\n');
        final rules = lines.indexed
            .where((line) => line.$2.contains('──'))
            .toList();
        expect(rules, hasLength(2));
        expect(rules.first.$2.trimRight(), '─' * width);
        expect(rules.last.$2, startsWith('▎ '));
        expect(rules.last.$2.substring(2).trimRight(), '─' * (width - 2));
        expect(
          tester.render().atColRow(4, rules.first.$1).style.foreground,
          const RgbColor(100, 120, 140),
        );
        expect(editor.source, source);
        editor.setSourceMode(true);
        expect(tester.renderToString(), contains('---'));
        expect(tester.renderToString(), isNot(contains('──')));
        expect(editor.source, source);
      },
    );

    test(
      'format, continue typing, heading and undo at $width columns',
      () async {
        mount('hello world', width: width);
        editor.apply(const SetSelection(0, 5));
        tester.render();
        await press('Bold');
        expect(editor.source, '**hello** world');
        expect(tester.semantics().byLabel('Bold').single.selected, isTrue);
        expect(focus.hasFocus, isTrue);
        // Replace the selected content through real host text input.
        tester.type('Hi');
        expect(editor.source, '**Hi** world');
        await press('Undo');
        expect(editor.source, '**hello** world');
        await press('Undo');
        expect(editor.source, 'hello world');
        editor.apply(const SetSelection.caret(3));
        tester.render();
        await press('Paragraph style');
        await press('Heading 2', role: SemanticRole.menuItem);
        expect(editor.source, '## hello world');
        expect(focus.hasFocus, isTrue);
        await press('Undo');
        expect(editor.source, 'hello world');
      },
    );
  }

  test(
    'language picker persists choice, restores input, and shares history',
    () async {
      mount('```\ndef hello\nend\n```\n\nafter');
      editor.apply(const SetSelection.caret(4));
      tester.render();
      await press('Code language');
      await press('Ruby', role: SemanticRole.menuItem);
      expect(editor.source, '```ruby\ndef hello\nend\n```\n\nafter');
      expect(focus.hasFocus, isTrue);
      expect(tester.semantics().byLabel('Bold').single.enabled, isFalse);
      await press('Code language');
      await press('Plain text', role: SemanticRole.menuItem);
      expect(editor.source, startsWith('```text\n'));
      await press('Undo');
      expect(editor.source, startsWith('```ruby\n'));
      await press('Code language');
      await press('Automatic', role: SemanticRole.menuItem);
      expect(editor.source, startsWith('```\n'));
      // An open choice must not follow the caret into another fence.
      await press('Code language');
      editor.apply(SetSelection.caret(editor.source.length));
      tester.render();
      expect(tester.semantics().byLabel('Ruby'), isEmpty);
      expect(editor.source, '```\ndef hello\nend\n```\n\nafter');
    },
  );

  test('start a blank document with a heading through the toolbar', () async {
    mount('');
    await press('Paragraph style');
    await press('Heading 1', role: SemanticRole.menuItem);
    tester.type('Title');
    expect(editor.source, '# Title');
    expect(focus.hasFocus, isTrue);
  });

  test(
    'source mode disables formatting and read-only omits authoring controls',
    () async {
      mount('hello');
      await press('Source');
      expect(tester.semantics().byLabel('Bold').single.enabled, isFalse);
      expect(
        tester.semantics().byLabel('Paragraph style').single.enabled,
        isFalse,
      );
      await press('Rendered');
      expect(editor.source, 'hello');
      tester.pumpWidget(
        FleuryApp(
          title: 'Composer test',
          home: FlarkEditorView(
            controller: controller,
            showToolbar: true,
            readOnly: true,
          ),
        ),
      );
      expect(tester.semantics().byLabel('Bold'), isEmpty);
    },
  );
}
