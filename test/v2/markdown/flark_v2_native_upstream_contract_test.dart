import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

import '../support/flark_test_paths.dart';

void main() {
  final coreCases = _loadCases(
    flarkFixturePath('commonmark/upstream/common_mark_tests.json'),
  );
  final gfmCases = _loadCases(
    flarkFixturePath('commonmark/upstream/gfm_tests.json'),
  );
  final deviations = _loadDeviationRegister(
    flarkFixturePath('commonmark/native_parity_deviation_register.json'),
  );
  const coreThreshold = 0.995;
  const gfmThreshold = 0.99;

  group('Flark v2 native upstream contracts', () {
    final libPath = flarkNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; v2 upstream contract suite skipped', () {
        expect(true, isTrue);
      });
      return;
    }

    final backend = FlarkNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    );

    test('core lane feeds projection and render-plan contracts', () async {
      final score = await _scoreLane(
        laneName: 'v2-core',
        cases: coreCases,
        deviations: deviations['core']!.map((item) => item.example).toSet(),
        profile: FlarkMarkdownProfile.commonMarkCore,
        backend: backend,
      );

      _printScore(score);
      expect(score.compared, greaterThan(0));
      expect(score.passRate, greaterThanOrEqualTo(coreThreshold));
    });

    test('gfm lane feeds projection and render-plan contracts', () async {
      final score = await _scoreLane(
        laneName: 'v2-gfm',
        cases: gfmCases,
        deviations: deviations['gfm']!.map((item) => item.example).toSet(),
        profile: FlarkMarkdownProfile.commonMarkGfm,
        backend: backend,
      );

      _printScore(score);
      expect(score.compared, greaterThan(0));
      expect(score.passRate, greaterThanOrEqualTo(gfmThreshold));
    });
  });
}

Future<_LaneScore> _scoreLane({
  required String laneName,
  required List<_UpstreamCase> cases,
  required Set<int> deviations,
  required FlarkMarkdownProfile profile,
  required FlarkNativeComrakParseBackend backend,
}) async {
  var skipped = 0;
  var compared = 0;
  var passed = 0;
  var failed = 0;
  var diagnosticFailures = 0;
  var rangeFailures = 0;
  var projectionFailures = 0;
  var renderPlanFailures = 0;
  var determinismFailures = 0;
  final samples = <String>[];

  for (final fixture in cases) {
    if (deviations.contains(fixture.example)) {
      skipped++;
      continue;
    }

    compared++;
    final request = FlarkMarkdownParseRequest(
      revision: fixture.example,
      markdown: fixture.markdown,
      profile: profile,
    );
    final first = await backend.parse(request);
    final second = await backend.parse(request);

    final hasErrors = first.diagnostics.any(
      (diagnostic) => diagnostic.extensions['isError'] == true,
    );
    final rangesValid = _rangesValid(first, fixture.markdown.length);
    final projectionValid = _canBuildProjection(first, fixture.markdown);
    final renderPlanValid = _canBuildRenderPlan(first);
    final deterministic = _signature(first) == _signature(second);

    if (hasErrors) diagnosticFailures++;
    if (!rangesValid) rangeFailures++;
    if (!projectionValid) projectionFailures++;
    if (!renderPlanValid) renderPlanFailures++;
    if (!deterministic) determinismFailures++;

    if (!hasErrors &&
        rangesValid &&
        projectionValid &&
        renderPlanValid &&
        deterministic) {
      passed++;
      continue;
    }

    failed++;
    if (samples.length < 12) {
      samples.add(
        '$laneName#${fixture.example} section="${fixture.section}" '
        'reason=${[if (hasErrors) 'diagnostics', if (!rangesValid) 'ranges', if (!projectionValid) 'projection', if (!renderPlanValid) 'renderPlan', if (!deterministic) 'determinism'].join('+')}',
      );
    }
  }

  return _LaneScore(
    laneName: laneName,
    total: cases.length,
    skipped: skipped,
    compared: compared,
    passed: passed,
    failed: failed,
    diagnosticFailures: diagnosticFailures,
    rangeFailures: rangeFailures,
    projectionFailures: projectionFailures,
    renderPlanFailures: renderPlanFailures,
    determinismFailures: determinismFailures,
    samples: samples,
  );
}

bool _rangesValid(FlarkMarkdownParseResult result, int textLength) {
  bool validRange(FlarkSourceRange range) {
    return range.start >= 0 &&
        range.end <= textLength &&
        range.start < range.end;
  }

  return _allBlocks(
        result.blocks,
      ).every((block) => validRange(block.sourceRange)) &&
      result.inlineTokens.every((token) => validRange(token.sourceRange)) &&
      result.hiddenRanges.every((range) => validRange(range.sourceRange)) &&
      result.replacementRanges.every(
        (range) =>
            validRange(range.sourceRange) && range.replacementText.isNotEmpty,
      ) &&
      result.ambiguityZones.every((zone) => validRange(zone.sourceRange));
}

bool _canBuildProjection(FlarkMarkdownParseResult result, String markdown) {
  try {
    final projection = FlarkProjection.fromParseResult(result);
    projection.projectText(markdown);
    return true;
  } catch (_) {
    return false;
  }
}

bool _canBuildRenderPlan(FlarkMarkdownParseResult result) {
  try {
    FlarkRenderPlan.fromParseResult(parseResult: result);
    return true;
  } catch (_) {
    return false;
  }
}

String _signature(FlarkMarkdownParseResult result) {
  final blockSignature = [
    for (final block in _allBlocks(result.blocks))
      '${block.type}:${block.sourceRange.start}:${block.sourceRange.end}',
  ];
  final inlineSignature = [
    for (final token in result.inlineTokens)
      '${token.type}:${token.sourceRange.start}:${token.sourceRange.end}',
  ];
  final hiddenSignature = [
    for (final range in result.hiddenRanges)
      '${range.type}:${range.sourceRange.start}:${range.sourceRange.end}',
  ];
  return [
    ...blockSignature,
    '|',
    ...inlineSignature,
    '|',
    ...hiddenSignature,
  ].join(',');
}

Iterable<FlarkMarkdownBlockNode> _allBlocks(
  Iterable<FlarkMarkdownBlockNode> blocks,
) sync* {
  for (final block in blocks) {
    yield block;
    yield* _allBlocks(block.children);
  }
}

void _printScore(_LaneScore score) {
  debugPrint(
    '${score.laneName}: passed=${score.passed} failed=${score.failed} '
    'compared=${score.compared} skipped=${score.skipped} total=${score.total} '
    'passRate=${score.passRate.toStringAsFixed(4)} '
    'diagnostics=${score.diagnosticFailures} ranges=${score.rangeFailures} '
    'projection=${score.projectionFailures} renderPlan=${score.renderPlanFailures} '
    'determinism=${score.determinismFailures}',
  );
  if (score.samples.isNotEmpty) {
    debugPrint('${score.laneName} samples: ${score.samples.join(', ')}');
  }
}

List<_UpstreamCase> _loadCases(String path) {
  final decoded = jsonDecode(File(path).readAsStringSync());
  if (decoded is! List) {
    throw StateError('Expected list fixture file: $path');
  }

  return decoded
      .whereType<Map<String, dynamic>>()
      .map(_UpstreamCase.fromJson)
      .where((item) => item.example >= 0)
      .toList(growable: false);
}

Map<String, List<_Deviation>> _loadDeviationRegister(String path) {
  final decoded = jsonDecode(File(path).readAsStringSync());
  if (decoded is! Map<String, dynamic>) {
    throw StateError('Expected object deviation register: $path');
  }

  List<_Deviation> readLane(String lane) {
    final entries = decoded[lane];
    if (entries is! List) return const [];
    return entries
        .whereType<Map<String, dynamic>>()
        .map(_Deviation.fromJson)
        .toList(growable: false);
  }

  return <String, List<_Deviation>>{
    'core': readLane('core'),
    'gfm': readLane('gfm'),
  };
}

final class _LaneScore {
  const _LaneScore({
    required this.laneName,
    required this.total,
    required this.skipped,
    required this.compared,
    required this.passed,
    required this.failed,
    required this.diagnosticFailures,
    required this.rangeFailures,
    required this.projectionFailures,
    required this.renderPlanFailures,
    required this.determinismFailures,
    required this.samples,
  });

  final String laneName;
  final int total;
  final int skipped;
  final int compared;
  final int passed;
  final int failed;
  final int diagnosticFailures;
  final int rangeFailures;
  final int projectionFailures;
  final int renderPlanFailures;
  final int determinismFailures;
  final List<String> samples;

  double get passRate => compared == 0 ? 1.0 : passed / compared;
}

final class _UpstreamCase {
  const _UpstreamCase({
    required this.example,
    required this.markdown,
    required this.section,
  });

  final int example;
  final String markdown;
  final String section;

  factory _UpstreamCase.fromJson(Map<String, dynamic> json) {
    return _UpstreamCase(
      example: (json['example'] as num?)?.toInt() ?? -1,
      markdown: (json['markdown'] as String?) ?? '',
      section: (json['section'] as String?) ?? '',
    );
  }
}

final class _Deviation {
  const _Deviation({required this.example});

  final int example;

  factory _Deviation.fromJson(Map<String, dynamic> json) {
    return _Deviation(example: (json['example'] as num?)?.toInt() ?? -1);
  }
}
