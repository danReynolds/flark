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

    // Document-size sweep: the two headline numbers (keystroke-apply latency
    // and native parse+decode) at 1KB / 100KB / 1MB. Printed as `flark_benchmark`
    // lines for the README perf table.
    for (final size in _sweepSizes) {
      test('keystroke apply latency at ${size.label}', () {
        final markdown = _markdownOfSize(size.targetChars);
        final state = FlarkEditorState.fromMarkdown(markdown);
        final transaction = FlarkTransaction.single(
          FlarkSourceOperation.insert(markdown.length ~/ 2, 'x'),
        );

        final result = _measure(
          'keystroke_apply_${size.label}_${markdown.length}chars',
          iterations: size.iterations,
          warmups: size.warmups,
          body: () {
            final next = state.applyTransaction(transaction);
            return next.document.length + next.selection.extentOffset;
          },
        );

        _report(result);
        _expectTrackingBudget(
          result,
          median: size.applyMedian,
          p95: size.applyP95,
        );
      });
    }

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

    for (final size in _sweepSizes) {
      test('native parse and decode at ${size.label} when present', () async {
        final backend = FlarkNativeComrakParseBackend.tryLoad();
        if (backend == null) {
          debugPrint(
            'flark_benchmark native_parse_${size.label} skipped=no_bridge',
          );
          return;
        }
        final markdown = _markdownOfSize(size.targetChars);

        final result = await _measureAsync(
          'native_parse_decode_${size.label}_${markdown.length}chars',
          iterations: size.parseIterations,
          warmups: size.parseWarmups,
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
          median: size.parseMedian,
          p95: size.parseP95,
        );
      });
    }

    for (final size in _profiledParseSizes) {
      test(
        'native parse phase profile at ${size.label} when present',
        () async {
          final backend = FlarkNativeComrakParseBackend.tryLoad();
          if (backend == null) {
            debugPrint(
              'flark_benchmark native_parse_profile_${size.label} '
              'skipped=no_bridge',
            );
            return;
          }
          final markdown = _markdownOfSize(size.targetChars);

          FlarkNativeComrakProfiledParseResult? latest;
          for (var i = 0; i < size.parseWarmups; i += 1) {
            latest = await backend.parseWithProfile(
              FlarkMarkdownParseRequest(
                revision: i + 1,
                markdown: markdown,
                profile: FlarkMarkdownProfile.commonMarkGfm,
              ),
            );
            _consume(_parseProfileBlackHole(latest));
          }

          final profiles = <FlarkNativeComrakParseProfile>[];
          for (var i = 0; i < size.parseIterations; i += 1) {
            latest = await backend.parseWithProfile(
              FlarkMarkdownParseRequest(
                revision: i + 1 + size.parseWarmups,
                markdown: markdown,
                profile: FlarkMarkdownProfile.commonMarkGfm,
              ),
            );
            profiles.add(latest.profile);
            _consume(_parseProfileBlackHole(latest));
          }

          _reportParseProfile(
            'native_parse_profile_${size.label}_${markdown.length}chars',
            profiles,
          );
        },
      );
    }
  });
}

const _sweepSizes = <_SweepSize>[
  // Budgets are deliberately generous regression trackers (≈10× headroom over
  // observed medians), not tight SLAs — native parse timing varies widely with
  // machine speed.
  _SweepSize(
    label: '1KB',
    targetChars: 1000,
    iterations: 60,
    warmups: 10,
    applyMedian: Duration(milliseconds: 10),
    applyP95: Duration(milliseconds: 25),
    parseIterations: 12,
    parseWarmups: 3,
    parseMedian: Duration(milliseconds: 50),
    parseP95: Duration(milliseconds: 120),
  ),
  _SweepSize(
    label: '100KB',
    targetChars: 100000,
    iterations: 40,
    warmups: 8,
    applyMedian: Duration(milliseconds: 30),
    applyP95: Duration(milliseconds: 80),
    parseIterations: 5,
    parseWarmups: 1,
    parseMedian: Duration(milliseconds: 1500),
    parseP95: Duration(milliseconds: 2500),
  ),
  _SweepSize(
    label: '1MB',
    targetChars: 1000000,
    iterations: 20,
    warmups: 4,
    applyMedian: Duration(milliseconds: 200),
    applyP95: Duration(milliseconds: 400),
    // Parse+decode is linear after the O(n^2) fix (~0.5s for 1MB locally); the
    // budget guards against a quadratic regression with generous CI headroom.
    parseIterations: 2,
    parseWarmups: 1,
    parseMedian: Duration(seconds: 3),
    parseP95: Duration(seconds: 5),
  ),
];

const _profiledParseSizes = <_SweepSize>[
  _SweepSize(
    label: '100KB',
    targetChars: 100000,
    iterations: 0,
    warmups: 0,
    applyMedian: Duration.zero,
    applyP95: Duration.zero,
    parseIterations: 3,
    parseWarmups: 1,
    parseMedian: Duration.zero,
    parseP95: Duration.zero,
  ),
  _SweepSize(
    label: '1MB',
    targetChars: 1000000,
    iterations: 0,
    warmups: 0,
    applyMedian: Duration.zero,
    applyP95: Duration.zero,
    parseIterations: 3,
    parseWarmups: 1,
    parseMedian: Duration.zero,
    parseP95: Duration.zero,
  ),
];

final class _SweepSize {
  const _SweepSize({
    required this.label,
    required this.targetChars,
    required this.iterations,
    required this.warmups,
    required this.applyMedian,
    required this.applyP95,
    required this.parseIterations,
    required this.parseWarmups,
    required this.parseMedian,
    required this.parseP95,
  });

  final String label;
  final int targetChars;
  final int iterations;
  final int warmups;
  final Duration applyMedian;
  final Duration applyP95;
  final int parseIterations;
  final int parseWarmups;
  final Duration parseMedian;
  final Duration parseP95;
}

String _markdownOfSize(int targetChars) {
  final buffer = StringBuffer();
  var section = 0;
  while (buffer.length < targetChars) {
    buffer
      ..writeln('## Section $section')
      ..writeln()
      ..writeln(
        'A paragraph with **strong text**, _emphasis_, `inline code`, and '
        '[a link](https://example.com/$section).',
      )
      ..writeln()
      ..writeln('- [ ] Task item $section')
      ..writeln('- [x] Completed item $section')
      ..writeln()
      ..writeln('```dart')
      ..writeln('final value$section = $section;')
      ..writeln('```')
      ..writeln();
    section += 1;
  }
  return buffer.toString();
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

void _reportParseProfile(
  String name,
  List<FlarkNativeComrakParseProfile> profiles,
) {
  if (profiles.isEmpty) return;
  final latest = profiles.last;
  debugPrint(
    'flark_benchmark $name iterations=${profiles.length} '
    'total_median=${_formatDuration(_medianDuration(profiles, (p) => p.total))} '
    'total_p95=${_formatDuration(_p95Duration(profiles, (p) => p.total))} '
    'utf8_encode_median=${_formatDuration(_medianDuration(profiles, (p) => p.utf8Encode))} '
    'bridge_total_median=${_formatDuration(_medianDuration(profiles, (p) => p.bridgeTotal))} '
    'input_copy_median=${_formatDuration(_medianDuration(profiles, (p) => p.bridgeInputCopy))} '
    'native_parse_median=${_formatDuration(_medianDuration(profiles, (p) => p.nativeParse))} '
    'payload_copy_median=${_formatDuration(_medianDuration(profiles, (p) => p.payloadCopy))} '
    'payload_decode_median=${_formatDuration(_medianDuration(profiles, (p) => p.payloadDecode))} '
    'result_mapping_median=${_formatDuration(_medianDuration(profiles, (p) => p.resultMapping))} '
    'input_bytes=${latest.inputBytes} payload_bytes=${latest.payloadBytes} '
    'native_blocks=${latest.nativeBlockCount} '
    'native_inlines=${latest.nativeInlineTokenCount} '
    'native_markers=${latest.nativeMarkerRangeCount} '
    'bridge_profile=${latest.hasBridgeProfile} blackHole=$_blackHole',
  );
}

Duration _medianDuration(
  List<FlarkNativeComrakParseProfile> profiles,
  Duration Function(FlarkNativeComrakParseProfile profile) select,
) {
  return _sortedDurations(profiles, select)[profiles.length ~/ 2];
}

Duration _p95Duration(
  List<FlarkNativeComrakParseProfile> profiles,
  Duration Function(FlarkNativeComrakParseProfile profile) select,
) {
  final sorted = _sortedDurations(profiles, select);
  return sorted[((sorted.length - 1) * 0.95).ceil()];
}

List<Duration> _sortedDurations(
  List<FlarkNativeComrakParseProfile> profiles,
  Duration Function(FlarkNativeComrakParseProfile profile) select,
) {
  return [for (final profile in profiles) select(profile)]
    ..sort((left, right) => left.compareTo(right));
}

int _parseProfileBlackHole(FlarkNativeComrakProfiledParseResult result) {
  final profile = result.profile;
  return result.result.blocks.length +
      result.result.inlineTokens.length +
      result.result.hiddenRanges.length +
      profile.inputBytes +
      profile.payloadBytes +
      profile.nativeBlockCount +
      profile.nativeInlineTokenCount +
      profile.nativeMarkerRangeCount;
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
