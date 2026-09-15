import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flark_tree_sitter/flark_highlighting.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:test/test.dart';

typedef _Job = ({
  String source,
  CodeLanguage language,
  Completer<CodeAnalysis?> result,
});

class _Worker {
  final jobs = <_Job>[];
  int disposals = 0;
  Future<CodeAnalysis?> request(
    String source, {
    required CodeLanguage language,
  }) {
    final result = Completer<CodeAnalysis?>();
    jobs.add((source: source, language: language, result: result));
    return result.future;
  }

  void complete(_Job job) => job.result.complete(
    plainCodeAnalysis(job.source, job.language, CodeAnalysisStatus.plain),
  );
  void dispose() => disposals++;
}

Future<void> tick() => Future<void>.delayed(Duration.zero);
String fences(int count, {int size = 0}) =>
    'Intro\n\n${List.generate(count, (i) => '```dart\nfinal x$i = $i;${' ' * size}\n```').join('\n\n')}';

void main() {
  final backend = createParseBackend();
  test(
    'more than 32 fences drains once, then viewport replaces cold work',
    () async {
      final editor = FlarkEditor(backend, text: fences(40), caret: 0);
      final worker = _Worker();
      final colors = FlarkCodeHighlighting.withWorker(
        editor,
        worker.request,
        worker.dispose,
      );
      addTearDown(colors.dispose);
      for (var i = 0; i < 32; i++) {
        expect(worker.jobs.length, i + 1);
        worker.complete(worker.jobs[i]);
        await tick();
      }
      expect(worker.jobs, hasLength(32));
      expect(colors.revision, 32);
      final last = editor.projection.rows.last;
      colors.setVisibleRows([last.index]);
      expect(worker.jobs.last.source, last.text);
      worker.complete(worker.jobs.last);
      await tick();
      expect(worker.jobs, hasLength(33));
      expect(colors.analysis(last.text, 'dart'), isNotNull);
      expect(editor.history.canUndo, isFalse);
    },
  );

  test(
    'total source budget and oversized snippet cannot create eviction churn',
    () async {
      final editor = FlarkEditor(
        backend,
        text: fences(12, size: 7900),
        // Isolate the decoration budget from the default live envelope.
        syncLimit: 128 * 1024,
        liveLimits: const FlarkLiveLimits(lineCodeUnits: 10000),
        caret: 0,
      );
      final worker = _Worker();
      final colors = FlarkCodeHighlighting.withWorker(
        editor,
        worker.request,
        worker.dispose,
      );
      addTearDown(colors.dispose);
      for (var i = 0; i < 8; i++) {
        worker.complete(worker.jobs[i]);
        await tick();
      }
      expect(worker.jobs, hasLength(8));
      expect(
        worker.jobs.fold<int>(0, (n, j) => n + j.source.length),
        lessThanOrEqualTo(65536),
      );
      editor.apply(
        ReplaceRange(
          0,
          editor.source.length,
          '```dart\n${'x' * 9000}\n```\n\n```ruby\nend\n```',
        ),
      );
      expect(worker.jobs.last.source, 'end');
      worker.complete(worker.jobs.last);
      await tick();
      expect(worker.jobs, hasLength(9));
    },
  );

  test(
    'a refused current job is skipped without retrying or starving other fences',
    () async {
      final editor = FlarkEditor(backend, text: fences(3), caret: 0);
      final worker = _Worker();
      final colors = FlarkCodeHighlighting.withWorker(
        editor,
        worker.request,
        worker.dispose,
      );
      addTearDown(colors.dispose);
      worker.jobs.first.result.complete(null);
      await tick();
      worker.complete(worker.jobs[1]);
      await tick();
      worker.complete(worker.jobs[2]);
      await tick();
      editor.apply(const SetSelection.caret(1));
      expect(worker.jobs, hasLength(3));
      expect(colors.failure, isNull);
    },
  );

  test(
    'language change, source mode and disposal fence delayed responses',
    () async {
      final editor = FlarkEditor(
        backend,
        text: '```dart\nfinal x = 1;\n```',
        caret: 10,
      );
      final worker = _Worker();
      final colors = FlarkCodeHighlighting.withWorker(
        editor,
        worker.request,
        worker.dispose,
      );
      var notifications = 0;
      colors.addListener(() => notifications++);
      editor.apply(const SetCodeLanguage('ruby'));
      worker.complete(worker.jobs.first);
      await tick();
      expect(colors.revision, 0);
      editor.setSourceMode(true);
      worker.complete(worker.jobs.last);
      await tick();
      expect(notifications, 0);
      editor.setSourceMode(false);
      final pending = worker.jobs.last;
      colors.dispose();
      colors.dispose();
      worker.complete(pending);
      await tick();
      expect((worker.disposals, notifications), (1, 0));
    },
  );

  test(
    'mismatched response fails visibly and closes the worker exactly once',
    () async {
      final editor = FlarkEditor(backend, text: fences(2), caret: 0);
      final worker = _Worker();
      final colors = FlarkCodeHighlighting.withWorker(
        editor,
        worker.request,
        worker.dispose,
      );
      var notifications = 0;
      colors.addListener(() => notifications++);
      worker.jobs.first.result.complete(
        plainCodeAnalysis(
          'wrong source',
          CodeLanguage.dart,
          CodeAnalysisStatus.plain,
        ),
      );
      await tick();
      expect(colors.failure, isStateError);
      expect(notifications, 1);
      colors.dispose();
      expect(worker.disposals, 1);
    },
  );
}
