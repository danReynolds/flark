@Tags(<String>['benchmark'])
library;

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

// Measures the REAL per-keystroke render cost (rebuild + layout + paint) of the
// live-rendered editor as the number of on-screen block widgets grows. The
// question this answers: at realistic block counts, is the current full-Column
// rebuild perceptibly slow (relative to a 16ms / 8ms frame budget), or is the
// 6/6 build fan-out cheap enough that rebuild isolation is premature?
//
// Numbers are debug-mode Dart-test-VM timings (no GPU, assertions on) — strictly
// PESSIMISTIC vs profile/release. Read them for absolute upper bound + scaling,
// not exact device latency.
void main() {
  for (final blockCount in const [10, 20, 40, 80]) {
    testWidgets('per-edit rebuild cost at $blockCount blocks', (tester) async {
      final backend = FlarkNativeComrakParseBackend.tryLoad();
      if (backend == null) {
        debugPrint('flark_benchmark rebuild_${blockCount}blocks skipped=no_bridge');
        return;
      }

      final markdown = _taskListMarkdown(blockCount);
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final parsed = await tester.runAsync(
        () => backend.parse(
          FlarkMarkdownParseRequest(
            revision: controller.state.revision,
            markdown: markdown,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        ),
      );
      expect(controller.applyParseResult(parsed!), isTrue);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 600,
            height: 100000, // tall: render every block (no viewport clipping)
            child: FlarkLiveRenderedEditableText(
              controller: controller,
              style: const TextStyle(fontSize: 14),
            ),
          ),
        ),
      );
      await tester.pump();

      final renderedBlocks = find.byType(EditableText).evaluate().length;

      // Warm up.
      for (var i = 0; i < 5; i += 1) {
        controller.applyTransaction(_insertAt(5));
        await tester.pump();
      }

      const iterations = 40;
      final samples = <Duration>[];
      var totalBuilds = 0;
      for (var i = 0; i < iterations; i += 1) {
        // Apply one source edit near the document start (worst case: shifts
        // every following block), then time the resulting frame.
        controller.applyTransaction(_insertAt(5));
        flarkDebugLiveBlockBuildCount = 0;
        final stopwatch = Stopwatch()..start();
        await tester.pump();
        stopwatch.stop();
        samples.add(stopwatch.elapsed);
        totalBuilds += flarkDebugLiveBlockBuildCount;
      }

      samples.sort();
      final median = samples[samples.length ~/ 2];
      final p95 = samples[((samples.length - 1) * 0.95).ceil()];
      final buildsPerEdit = (totalBuilds / iterations).toStringAsFixed(1);

      debugPrint(
        'flark_benchmark rebuild_${blockCount}blocks '
        'rendered=$renderedBlocks builds_per_edit=$buildsPerEdit '
        'pump_median=${_fmt(median)} pump_p95=${_fmt(p95)}',
      );
    });
  }

  // Realistic viewport: 80-block document in an 600px-tall scroll view (only
  // ~10 blocks visible). Tests whether the current SingleChildScrollView+Column
  // already avoids building off-screen blocks (it does not — Column is not
  // lazy), which is what a viewport-virtualized list would fix.
  testWidgets('per-edit cost at 80 blocks in a realistic viewport', (
    tester,
  ) async {
    final backend = FlarkNativeComrakParseBackend.tryLoad();
    if (backend == null) return;

    final markdown = _taskListMarkdown(80);
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    final parsed = await tester.runAsync(
      () => backend.parse(
        FlarkMarkdownParseRequest(
          revision: controller.state.revision,
          markdown: markdown,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      ),
    );
    expect(controller.applyParseResult(parsed!), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 600,
          height: 600, // realistic editor viewport — ~10 blocks visible
          child: FlarkLiveRenderedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );
    await tester.pump();

    for (var i = 0; i < 5; i += 1) {
      controller.applyTransaction(_insertAt(5));
      await tester.pump();
    }

    const iterations = 40;
    final samples = <Duration>[];
    var totalBuilds = 0;
    for (var i = 0; i < iterations; i += 1) {
      controller.applyTransaction(_insertAt(5));
      flarkDebugLiveBlockBuildCount = 0;
      final stopwatch = Stopwatch()..start();
      await tester.pump();
      stopwatch.stop();
      samples.add(stopwatch.elapsed);
      totalBuilds += flarkDebugLiveBlockBuildCount;
    }

    samples.sort();
    final median = samples[samples.length ~/ 2];
    final buildsPerEdit = (totalBuilds / iterations).toStringAsFixed(1);
    debugPrint(
      'flark_benchmark rebuild_80blocks_600pxviewport '
      'builds_per_edit=$buildsPerEdit pump_median=${_fmt(median)}',
    );
  });

  // Baseline: the SAME 40-block document in source mode (one EditableText, no
  // per-block widgets). The live-rendered/source ratio normalizes out debug
  // overhead and isolates the block-fan-out cost specifically.
  testWidgets('per-edit cost baseline source mode at 40 blocks', (
    tester,
  ) async {
    final markdown = _taskListMarkdown(40);
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 600,
          height: 100000,
          child: FlarkEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      ),
    );
    await tester.pump();

    for (var i = 0; i < 5; i += 1) {
      controller.applyTransaction(_insertAt(5));
      await tester.pump();
    }

    const iterations = 40;
    final samples = <Duration>[];
    for (var i = 0; i < iterations; i += 1) {
      controller.applyTransaction(_insertAt(5));
      final stopwatch = Stopwatch()..start();
      await tester.pump();
      stopwatch.stop();
      samples.add(stopwatch.elapsed);
    }

    samples.sort();
    final median = samples[samples.length ~/ 2];
    final p95 = samples[((samples.length - 1) * 0.95).ceil()];
    debugPrint(
      'flark_benchmark rebuild_source_40blocks '
      'pump_median=${_fmt(median)} pump_p95=${_fmt(p95)}',
    );
  });
}

FlarkTransaction _insertAt(int offset) {
  return FlarkTransaction.single(
    FlarkSourceOperation.insert(offset, 'x'),
    metadata: const FlarkTransactionMetadata(
      intent: FlarkTransactionIntent.input,
      userEvent: 'benchmark.insert',
    ),
  );
}

String _taskListMarkdown(int count) {
  final buffer = StringBuffer();
  for (var i = 0; i < count; i += 1) {
    buffer.writeln('- [ ] task item number $i with a little inline text');
  }
  return buffer.toString();
}

String _fmt(Duration duration) {
  final micros = duration.inMicroseconds;
  if (micros < 1000) return '${micros}us';
  return '${(micros / 1000).toStringAsFixed(2)}ms';
}
