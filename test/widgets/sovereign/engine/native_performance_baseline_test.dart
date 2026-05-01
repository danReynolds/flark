import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_parse_backend.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'support/bootstrap_commonmark_parse_backend.dart';
import 'support/test_paths.dart';

const int _kNativeCoreP95BudgetMicros = 8000;
const int _kNativeGfmP95BudgetMicros = 6000;
const int _kSchedulerPendingReplaceMax = 8;
const int _kSchedulerStaleDropMin = 1;
const int _kSchedulerStaleDropMax = 3;

class _FixtureCase {
  final String id;
  final String markdown;

  const _FixtureCase({required this.id, required this.markdown});

  factory _FixtureCase.fromJson(Map<String, dynamic> json) {
    return _FixtureCase(
      id: (json['id'] as String?) ?? 'unknown',
      markdown: (json['markdown'] as String?) ?? '',
    );
  }
}

List<_FixtureCase> _loadFixtureCases(String fixturePath) {
  final raw = File(fixturePath).readAsStringSync();
  final decoded = jsonDecode(raw);
  if (decoded is! List) {
    throw StateError('Fixture file must decode to a JSON array: $fixturePath');
  }

  return decoded
      .whereType<Map<String, dynamic>>()
      .map(_FixtureCase.fromJson)
      .where((fixture) => fixture.markdown.isNotEmpty)
      .toList(growable: false);
}

double _percentileMicros(List<int> sortedMicros, double percentile) {
  if (sortedMicros.isEmpty) return 0;
  final position = (sortedMicros.length - 1) * percentile;
  final lower = position.floor();
  final upper = position.ceil();
  if (lower == upper) return sortedMicros[lower].toDouble();
  final weight = position - lower;
  return sortedMicros[lower] +
      (sortedMicros[upper] - sortedMicros[lower]) * weight;
}

Future<List<int>> _measureBackendMicros({
  required CommonMarkParseBackend backend,
  required List<_FixtureCase> fixtures,
  required MarkdownSyntaxProfile profile,
  int rounds = 25,
}) async {
  final samples = <int>[];
  var revision = 1;

  // Warmup to stabilize parser and allocator paths.
  for (final fixture in fixtures) {
    await backend.parse(
      SyntaxParseRequest(
        revision: revision++,
        text: fixture.markdown,
        profile: profile,
      ),
    );
  }

  for (var round = 0; round < rounds; round++) {
    for (final fixture in fixtures) {
      final sw = Stopwatch()..start();
      final snapshot = await backend.parse(
        SyntaxParseRequest(
          revision: revision++,
          text: fixture.markdown,
          profile: profile,
        ),
      );
      sw.stop();
      expect(snapshot, isA<SyntaxSnapshot>());
      samples.add(sw.elapsedMicroseconds);
    }
  }

  return samples;
}

Future<void> _drainAsyncQueue() async {
  await Future<void>.delayed(Duration.zero);
}

void main() {
  group('Native parse baseline metrics', () {
    test('reports parse p50/p95 and enforces baseline gates', () async {
      final libPath = sovereignNativeBridgeLibraryPathForPlatform();
      if (libPath.isEmpty || !File(libPath).existsSync()) {
        return;
      }

      final coreFixtures = _loadFixtureCases(
        sovereignFixturePath('commonmark/core_cases.json'),
      );
      final gfmFixtures = _loadFixtureCases(
        sovereignFixturePath('commonmark/gfm_cases.json'),
      );
      final allFixtures = [...coreFixtures, ...gfmFixtures];

      final nativeBackend = ComrakCommonMarkParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      );
      const bootstrapBackend = BootstrapCommonMarkParseBackend(
        preferNative: false,
      );

      final nativeCoreSamples = await _measureBackendMicros(
        backend: nativeBackend,
        fixtures: coreFixtures,
        profile: MarkdownSyntaxProfile.commonMarkCore,
      );
      final nativeGfmSamples = await _measureBackendMicros(
        backend: nativeBackend,
        fixtures: gfmFixtures,
        profile: MarkdownSyntaxProfile.commonMarkGfm,
      );
      final bootstrapCoreSamples = await _measureBackendMicros(
        backend: bootstrapBackend,
        fixtures: coreFixtures,
        profile: MarkdownSyntaxProfile.commonMarkCore,
      );

      nativeCoreSamples.sort();
      nativeGfmSamples.sort();
      bootstrapCoreSamples.sort();

      final nativeCoreP50 = _percentileMicros(nativeCoreSamples, 0.50);
      final nativeCoreP95 = _percentileMicros(nativeCoreSamples, 0.95);
      final nativeGfmP50 = _percentileMicros(nativeGfmSamples, 0.50);
      final nativeGfmP95 = _percentileMicros(nativeGfmSamples, 0.95);
      final bootstrapCoreP50 = _percentileMicros(bootstrapCoreSamples, 0.50);
      final bootstrapCoreP95 = _percentileMicros(bootstrapCoreSamples, 0.95);

      debugPrint(
        'Native baseline: core p50=${(nativeCoreP50 / 1000).toStringAsFixed(3)}ms '
        'p95=${(nativeCoreP95 / 1000).toStringAsFixed(3)}ms '
        '(n=${nativeCoreSamples.length})',
      );
      debugPrint(
        'Native baseline: gfm p50=${(nativeGfmP50 / 1000).toStringAsFixed(3)}ms '
        'p95=${(nativeGfmP95 / 1000).toStringAsFixed(3)}ms '
        '(n=${nativeGfmSamples.length})',
      );
      debugPrint(
        'Bootstrap baseline: core p50=${(bootstrapCoreP50 / 1000).toStringAsFixed(3)}ms '
        'p95=${(bootstrapCoreP95 / 1000).toStringAsFixed(3)}ms '
        '(n=${bootstrapCoreSamples.length})',
      );

      final controller = SovereignController();
      addTearDown(controller.dispose);
      controller.resetParseTelemetryForTesting();
      controller.resetPredictiveTelemetryForTesting();

      final chunks = <String>[
        for (final fixture in allFixtures) fixture.markdown,
      ];
      var assembled = '';
      for (final chunk in chunks) {
        assembled += chunk;
        controller.value = TextEditingValue(
          text: assembled,
          selection: TextSelection.collapsed(offset: assembled.length),
        );
      }

      for (var i = 0; i < 25; i++) {
        await _drainAsyncQueue();
      }

      debugPrint(
        'Scheduler counters: pendingReplace=${controller.parsePendingReplaceCount} '
        'staleDrop=${controller.parseStaleDropCount} '
        'predictiveBudgetExhaustions=${controller.predictiveBudgetExhaustionCount} '
        'predictiveLocalFallbacks=${controller.predictiveLocalFallbackCount}',
      );

      expect(nativeCoreSamples, isNotEmpty);
      expect(nativeGfmSamples, isNotEmpty);
      expect(bootstrapCoreSamples, isNotEmpty);
      expect(
        nativeCoreP95,
        lessThanOrEqualTo(_kNativeCoreP95BudgetMicros.toDouble()),
      );
      expect(
        nativeGfmP95,
        lessThanOrEqualTo(_kNativeGfmP95BudgetMicros.toDouble()),
      );
      expect(
        controller.parsePendingReplaceCount,
        lessThanOrEqualTo(_kSchedulerPendingReplaceMax),
      );
      expect(
        controller.parseStaleDropCount,
        inInclusiveRange(_kSchedulerStaleDropMin, _kSchedulerStaleDropMax),
      );
    });
  });
}
