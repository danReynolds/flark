import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flark/code.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_tree_sitter/flark_highlighting.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

void main() {
  test(
    'delayed colors preserve Fleury input, first paint, selection and undo',
    () async {
      const source = '```ruby\ndef hello\nend\n```';
      final editor = FlarkEditor(
        createParseBackend(),
        text: source,
        caret: source.indexOf('hello'),
      );
      final analyzer = CodeAnalyzer();
      final jobs =
          <
            ({
              String source,
              CodeLanguage language,
              Completer<CodeAnalysis?> result,
            })
          >[];
      var disposals = 0;
      final colors = FlarkCodeHighlighting.withWorker(editor, (
        source, {
        required language,
      }) {
        final result = Completer<CodeAnalysis?>();
        jobs.add((source: source, language: language, result: result));
        return result.future;
      }, () => disposals++);
      final controller = FlarkFleuryController(editor, codeColors: colors);
      final focus = FocusNode();
      final tester = FleuryTester(viewportSize: const CellSize(40, 10));
      addTearDown(() {
        tester.dispose();
        controller.dispose();
        focus.dispose();
        analyzer.dispose();
      });
      tester.pumpWidget(
        Theme(
          data: const ThemeData(),
          child: FlarkEditorView(
            controller: controller,
            focusNode: focus,
            autofocus: true,
            theme: const FlarkCellTheme(
              codePadding: 0,
              syntax: {
                CodeSyntaxRole.keyword: CellStyle(foreground: Colors.magenta),
              },
            ),
          ),
        ),
      );
      var frame = tester.render();
      expect(frame.atColRow(0, 0).style.foreground, isNot(Colors.magenta));
      final before = editor.snapshot, caret = focus.caretRect;
      jobs.first.result.complete(
        analyzer.analyze(jobs.first.source, language: jobs.first.language),
      );
      await Future<void>.delayed(
        Duration.zero,
      ); // only asynchronous decoration work
      frame = tester.render();
      expect(frame.atColRow(0, 0).style.foreground, Colors.magenta);
      expect(editor.snapshot, same(before));
      expect(focus.caretRect, caret);
      expect(editor.history.canUndo, isFalse);

      tester.sendKey(KeyEvent(KeyCode.a, modifiers: {KeyModifier.superKey}));
      expect(
        editor.source.substring(editor.selection.start, editor.selection.end),
        'def hello\nend',
      );
      tester.paste('puts "new"');
      frame = tester.render(); // edited first frame, no settle
      expect(editor.source, '```ruby\nputs "new"\n```');
      expect(tester.renderToString(), contains('puts "new"'));
      expect(controller.colorsFor(editor.projection.rows.first), isNull);
      final pending = jobs.last;
      tester.sendKey(KeyEvent(KeyCode.z, modifiers: {KeyModifier.superKey}));
      tester.render();
      expect(editor.source, source);
      expect(
        editor.source.substring(editor.selection.start, editor.selection.end),
        'def hello\nend',
      );
      final undo = editor.snapshot;
      pending.result.complete(
        analyzer.analyze(pending.source, language: pending.language),
      );
      await Future<void>.delayed(Duration.zero);
      tester.render();
      expect(editor.snapshot, same(undo));
      expect(
        controller.colorsFor(editor.projection.rows.first)?.source,
        'def hello\nend',
      );
      tester.type('x');
      expect(editor.source, contains('x'));
      tester.render();
      tester.pumpWidget(const SizedBox());
      controller.dispose();
      expect(disposals, 1);
      for (final job in jobs.where((job) => !job.result.isCompleted)) {
        job.result.complete(null);
      }
      await Future<void>.delayed(Duration.zero);
      expect(disposals, 1);
    },
  );
}
