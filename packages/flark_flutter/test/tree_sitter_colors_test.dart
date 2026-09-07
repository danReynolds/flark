import 'dart:async';
import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

typedef Job = ({
  String source,
  CodeLanguage language,
  Completer<CodeAnalysis?> result,
});

void main() {
  final backend = createParseBackend();
  final analyzer = CodeAnalyzer();
  tearDownAll(() {
    analyzer.dispose();
  });

  test(
    'native controller worker colors every fence and disposes during a pending edit',
    () async {
      final editor = FlarkEditor(
        backend,
        text: '```dart\nfinal answer = 42;\n```\n\n```yaml\nname: example\n```',
        caret: 10,
      );
      final colors = FlarkCodeColors(editor);
      final c = FlarkController(editor, codeColors: colors);
      try {
        final deadline = Stopwatch()..start();
        while (colors.revision < 2 &&
            colors.failure == null &&
            deadline.elapsedMilliseconds < 15000) {
          await Future<void>.delayed(const Duration(milliseconds: 5));
        }
        expect(colors.failure, isNull);
        expect(colors.revision, 2);
        expect(
          colors
              .highlight('final answer = 42;', 'dart')
              .tokens
              .any((t) => t.kind == 'keyword'),
          isTrue,
        );
        expect(
          colors
              .highlight('name: example', 'yaml')
              .tokens
              .any((t) => t.kind == 'property'),
          isTrue,
        );
        expect(editor.history.canUndo, isFalse);
        c.command(const InsertText('z'));
      } finally {
        c.dispose();
      }
      await Future<void>.delayed(const Duration(milliseconds: 10));
    },
  );

  test('scrolling prioritizes a visible fence beyond the first 32', () async {
    final source =
        'Intro\n\n${List.generate(40, (i) => '```dart\nfinal x$i = $i;\n```').join('\n\n')}';
    final editor = FlarkEditor(backend, text: source);
    final jobs = <Job>[];
    final colors = FlarkCodeColors.withWorker(editor, (
      source, {
      required language,
    }) {
      final result = Completer<CodeAnalysis?>();
      jobs.add((source: source, language: language, result: result));
      return result.future;
    }, () {});
    expect(jobs.single.source, 'final x0 = 0;');
    final visible = editor.projection.rows.last;
    colors.setVisibleRows([visible.index]);
    expect(jobs.last.source, 'final x39 = 39;');
    final old = jobs.first, current = jobs.last;
    old.result.complete(analyzer.analyze(old.source, language: old.language));
    current.result.complete(
      analyzer.analyze(current.source, language: current.language),
    );
    await Future<void>.delayed(Duration.zero);
    expect(colors.revision, 1);
    expect(jobs.length, 2);
    expect(
      colors
          .highlight(current.source, 'dart')
          .tokens
          .any((t) => t.kind != null),
      isTrue,
    );
    colors.dispose();
  });

  testWidgets(
    'delayed colors preserve the first frame, geometry, selection and undo',
    (tester) async {
      final code = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
      addTearDown(code.dispose);
      const source = '> ```dart\n> final answer = 42;\n> ```\n\nafter';
      final editor = FlarkEditor(
        backend,
        text: source,
        codeEditing: code,
        caret: source.indexOf('answer'),
      );
      final jobs = <Job>[];
      var disposed = false;
      final colors = FlarkCodeColors.withWorker(editor, (
        source, {
        required language,
      }) {
        final result = Completer<CodeAnalysis?>();
        jobs.add((source: source, language: language, result: result));
        return result.future;
      }, () => disposed = true);
      final c = FlarkController(editor, codeColors: colors);
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 220,
              child: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                showToolbar: false,
                onPaint: paints.add,
              ),
            ),
          ),
        ),
      );
      c.command(
        SetSelection(source.indexOf('42') + 2, source.indexOf('answer')),
      );
      await tester.pump();
      final plain = paints.last,
          revision = editor.revision,
          snapshot = editor.snapshot;
      expect(plain.selectionRects, isNotEmpty);
      expect(plain.resolvedStyles.first.map((s) => s.color).toSet().length, 1);
      jobs.single.result.complete(
        analyzer.analyze(jobs.single.source, language: jobs.single.language),
      );
      paints.clear();
      await tester.idle();
      await tester.pump();
      expect(paints, isNotEmpty);
      expect(
        paints.last.resolvedStyles.first.map((s) => s.color).toSet().length,
        greaterThan(1),
      );
      expect(paints.last.caret, plain.caret);
      expect(paints.last.selectionRects, plain.selectionRects);
      expect(editor.snapshot, same(snapshot));
      expect(editor.revision, revision);
      expect(editor.history.canUndo, isFalse);

      // Edit while the next colors are withheld. The very next paint must show
      // current text/caret, with no old token ranges and no font-metric change.
      c.command(const InsertText('message'));
      final edited = c.text, caret = editor.selection.extent;
      paints.clear();
      await tester.pump();
      expect(paints.first.rows.first, 'final message;');
      expect(paints.first.caretSource, caret);
      expect(
        paints.first.resolvedStyles.first.map((s) => s.color).toSet().length,
        1,
      );
      final pending = jobs.last;
      c.command(const Undo());
      await tester.pump();
      expect((c.text, editor.selection), (source, snapshot.selection));
      final undoRevision = editor.revision;
      pending.result.complete(
        analyzer.analyze(pending.source, language: pending.language),
      );
      await tester.pump();
      expect(editor.revision, undoRevision);
      expect(c.text, source);
      expect(paints.last.rows.first, 'final answer = 42;');
      c.command(const Redo());
      await tester.pump();
      expect(c.text, edited);
      // Next platform character still applies to the current source and caret.
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text: edited.replaceRange(caret, caret, 'z'),
          selection: TextSelection.collapsed(offset: caret + 1),
        ),
      );
      await tester.pump();
      expect(c.text, edited.replaceRange(caret, caret, 'z'));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      expect(disposed, isTrue);
      for (final job in jobs.where((job) => !job.result.isCompleted)) {
        job.result.complete(
          analyzer.analyze(job.source, language: job.language),
        );
      }
      await tester.pump();
      expect(tester.takeException(), isNull);
    },
  );

  test(
    'language switch, source mode and deletion reject delayed results',
    () async {
      final editor = FlarkEditor(
        backend,
        text: '```dart\nfinal value = 1;\n```',
        caret: 10,
      );
      final jobs = <Job>[];
      final colors = FlarkCodeColors.withWorker(editor, (
        source, {
        required language,
      }) {
        final result = Completer<CodeAnalysis?>();
        jobs.add((source: source, language: language, result: result));
        return result.future;
      }, () {});
      var notifications = 0;
      colors.addListener(() => notifications++);
      editor.apply(const SetCodeLanguage('python'));
      expect(jobs.last.language, CodeLanguage.python);
      for (final job in jobs) {
        job.result.complete(
          analyzer.analyze(job.source, language: job.language),
        );
      }
      await Future<void>.delayed(Duration.zero);
      expect(notifications, 1);
      expect(
        colors.highlight('final value = 1;', 'dart').tokens.single.kind,
        isNull,
      );
      editor.apply(const SetCodeLanguage('yaml'));
      final delayed = jobs.last;
      editor.setSourceMode(true);
      delayed.result.complete(
        analyzer.analyze(delayed.source, language: delayed.language),
      );
      await Future<void>.delayed(Duration.zero);
      expect(notifications, 1);
      editor.setSourceMode(false);
      final deleted = jobs.last;
      editor.apply(SetSelection(0, editor.source.length));
      editor.apply(const InsertText('gone'));
      deleted.result.complete(
        analyzer.analyze(deleted.source, language: deleted.language),
      );
      await Future<void>.delayed(Duration.zero);
      expect((editor.source, notifications), ('gone', 1));
      colors.dispose();
    },
  );

  test(
    'active snippet takes priority over passive fences; failure stays plain',
    () async {
      const source = '```dart\nfinal a = 1;\n```\n\n```python\nx = 2\n```';
      final editor = FlarkEditor(
        backend,
        text: source,
        caret: source.indexOf('x ='),
      );
      final jobs = <Job>[];
      var disposed = false;
      final colors = FlarkCodeColors.withWorker(editor, (
        source, {
        required language,
      }) {
        final result = Completer<CodeAnalysis?>();
        jobs.add((source: source, language: language, result: result));
        return result.future;
      }, () => disposed = true);
      expect(jobs.single.language, CodeLanguage.python);
      final first = jobs.single;
      first.result.complete(
        analyzer.analyze(first.source, language: first.language),
      );
      await Future<void>.delayed(Duration.zero);
      expect(jobs.last.language, CodeLanguage.dart);
      jobs.last.result.completeError(StateError('worker stopped'));
      await Future<void>.delayed(Duration.zero);
      expect(colors.failure, isStateError);
      expect(disposed, isTrue);
      expect(
        colors.highlight('final a = 1;', 'dart').tokens.single.kind,
        isNull,
      );
      editor.apply(const InsertText('z'));
      expect(editor.source, contains('zx = 2'));
      colors.dispose();
    },
  );
}
