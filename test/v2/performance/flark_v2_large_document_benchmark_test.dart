@Tags(<String>['benchmark'])
library;

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

const _enforceTrackingBudgets = bool.fromEnvironment(
  'FLARK_BENCHMARK_ENFORCE_BUDGETS',
);

int _blackHole = 0;

void main() {
  group('Flark v2 large document tracking benchmarks', () {
    test('rebuilds line-indexed text buffers after localized edits', () {
      final markdown = List.filled(9000, 'paragraph line').join('\n');
      final buffer = FlarkTextBuffer(markdown);

      final result = _measure(
        'text_buffer_replace_126k_middle',
        iterations: 40,
        warmups: 8,
        body: () {
          final next = buffer.replaceRange(
            markdown.length ~/ 2,
            markdown.length ~/ 2,
            'x',
          );
          return next.length + next.lineCount;
        },
      );

      _report(result);
      _expectTrackingBudget(
        result,
        median: const Duration(milliseconds: 75),
        p95: const Duration(milliseconds: 150),
      );
    });

    test('predicts dense projections through localized edits', () {
      const markerCount = 5000;
      const stride = 24;
      final projection = FlarkProjection(
        textLength: markerCount * stride,
        hiddenRanges: [
          for (var i = 0; i < markerCount; i += 1)
            FlarkHiddenRange(
              range: FlarkSourceRange(i * stride, i * stride + 2),
              kind: FlarkHiddenRangeKind.inlineMarker,
            ),
        ],
      );
      final transaction = FlarkTransaction.single(
        FlarkSourceOperation.insert((markerCount * stride) ~/ 2, 'x'),
      );

      final result = _measure(
        'projection_predict_5000_markers',
        iterations: 30,
        warmups: 6,
        body: () {
          final prediction = projection.predictAfter(
            transaction,
            textLengthAfter: markerCount * stride + 1,
          );
          return prediction.projection.displayLength;
        },
      );

      _report(result);
      _expectTrackingBudget(
        result,
        median: const Duration(milliseconds: 200),
        p95: const Duration(milliseconds: 400),
      );
    });

    test('builds render plans from large parsed block and inline sets', () {
      const blockCount = 5000;
      const stride = 40;
      final parseResult = FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: 1,
        sourceTextLength: blockCount * stride,
        blocks: [
          for (var i = 0; i < blockCount; i += 1)
            FlarkMarkdownBlockNode(
              kind: FlarkMarkdownBlockKind.paragraph,
              type: 'paragraph',
              sourceRange: FlarkSourceRange(i * stride, i * stride + 32),
            ),
        ],
        inlineTokens: [
          for (var i = 0; i < blockCount; i += 1)
            FlarkMarkdownInlineToken(
              kind: FlarkMarkdownInlineKind.strong,
              type: 'strong',
              sourceRange: FlarkSourceRange(i * stride + 2, i * stride + 16),
            ),
        ],
      );

      final result = _measure(
        'render_plan_5000_blocks_5000_inlines',
        iterations: 20,
        warmups: 4,
        body: () {
          final plan = FlarkRenderPlan.fromParseResult(
            parseResult: parseResult,
          );
          return plan.blocks.length + plan.allInlineRuns.length;
        },
      );

      _report(result);
      _expectTrackingBudget(
        result,
        median: const Duration(milliseconds: 250),
        p95: const Duration(milliseconds: 500),
      );
    });

    test(
      'parses and decodes large markdown through native Comrak when present',
      () async {
        final backend = FlarkNativeComrakParseBackend.tryLoad();
        if (backend == null) {
          debugPrint('flark_benchmark native_comrak_120k skipped=no_bridge');
          return;
        }
        final markdown = _realisticMarkdown(sectionCount: 900);

        final result = await _measureAsync(
          'native_comrak_parse_decode_${markdown.length}_chars',
          iterations: 1,
          warmups: 0,
          body: () async {
            final parseResult = await backend.parse(
              FlarkMarkdownParseRequest(
                revision: 1,
                markdown: markdown,
                profile: FlarkMarkdownProfile.commonMarkGfm,
              ),
            );
            return parseResult.blocks.length +
                parseResult.inlineTokens.length +
                parseResult.hiddenRanges.length;
          },
        );

        _report(result);
        _expectTrackingBudget(
          result,
          median: const Duration(seconds: 5),
          p95: const Duration(seconds: 5),
        );
      },
    );
  });
}

_BenchmarkResult _measure(
  String name, {
  required int iterations,
  required int warmups,
  required int Function() body,
}) {
  for (var i = 0; i < warmups; i += 1) {
    _consume(body());
  }

  final samples = <Duration>[];
  for (var i = 0; i < iterations; i += 1) {
    final stopwatch = Stopwatch()..start();
    _consume(body());
    stopwatch.stop();
    samples.add(stopwatch.elapsed);
  }
  return _BenchmarkResult(name: name, samples: samples);
}

Future<_BenchmarkResult> _measureAsync(
  String name, {
  required int iterations,
  required int warmups,
  required Future<int> Function() body,
}) async {
  for (var i = 0; i < warmups; i += 1) {
    _consume(await body());
  }

  final samples = <Duration>[];
  for (var i = 0; i < iterations; i += 1) {
    final stopwatch = Stopwatch()..start();
    _consume(await body());
    stopwatch.stop();
    samples.add(stopwatch.elapsed);
  }
  return _BenchmarkResult(name: name, samples: samples);
}

void _consume(int value) {
  _blackHole = (_blackHole + value) & 0x3fffffff;
}

void _report(_BenchmarkResult result) {
  debugPrint('flark_benchmark ${result.summary}');
}

void _expectTrackingBudget(
  _BenchmarkResult result, {
  required Duration median,
  required Duration p95,
}) {
  if (!_enforceTrackingBudgets) return;
  expect(
    result.median,
    lessThan(median),
    reason: '${result.name} median was ${_formatDuration(result.median)}',
  );
  expect(
    result.p95,
    lessThan(p95),
    reason: '${result.name} p95 was ${_formatDuration(result.p95)}',
  );
}

String _realisticMarkdown({required int sectionCount}) {
  final buffer = StringBuffer();
  for (var i = 0; i < sectionCount; i += 1) {
    buffer
      ..writeln('## Section $i')
      ..writeln()
      ..writeln(
        'A paragraph with **strong text**, _emphasis_, `inline code`, and '
        '[a link](https://example.com/$i).',
      )
      ..writeln()
      ..writeln('- [ ] Task item $i')
      ..writeln('- [x] Completed item $i')
      ..writeln()
      ..writeln('```dart')
      ..writeln('final value$i = $i;')
      ..writeln('```')
      ..writeln();
  }
  return buffer.toString();
}

final class _BenchmarkResult {
  _BenchmarkResult({required this.name, required Iterable<Duration> samples})
    : samples = List<Duration>.unmodifiable(
        [...samples]..sort((left, right) => left.compareTo(right)),
      );

  final String name;
  final List<Duration> samples;

  Duration get min => samples.first;

  Duration get median => samples[samples.length ~/ 2];

  Duration get p95 => samples[((samples.length - 1) * 0.95).ceil()];

  Duration get max => samples.last;

  String get summary {
    return '$name iterations=${samples.length} '
        'min=${_formatDuration(min)} median=${_formatDuration(median)} '
        'p95=${_formatDuration(p95)} max=${_formatDuration(max)} '
        'blackHole=$_blackHole';
  }
}

String _formatDuration(Duration duration) {
  final micros = duration.inMicroseconds;
  if (micros < 1000) return '${micros}us';
  return '${(micros / 1000).toStringAsFixed(2)}ms';
}
