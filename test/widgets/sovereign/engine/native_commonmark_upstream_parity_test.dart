import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_parse_backend.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_types.dart';
import 'support/test_paths.dart';

class _UpstreamCase {
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

class _Deviation {
  const _Deviation({
    required this.example,
    required this.owner,
    required this.reason,
    required this.targetMilestone,
  });

  final int example;
  final String owner;
  final String reason;
  final String targetMilestone;

  factory _Deviation.fromJson(Map<String, dynamic> json) {
    return _Deviation(
      example: (json['example'] as num?)?.toInt() ?? -1,
      owner: (json['owner'] as String?) ?? '',
      reason: (json['reason'] as String?) ?? '',
      targetMilestone: (json['targetMilestone'] as String?) ?? '',
    );
  }
}

class _LaneContractScore {
  const _LaneContractScore({
    required this.total,
    required this.skipped,
    required this.compared,
    required this.passed,
    required this.failed,
    required this.passRate,
    required this.parseErrors,
    required this.fallbackDiagnosticFailures,
    required this.rangeNormalizationFailures,
    required this.determinismFailures,
    required this.failureSamples,
  });

  final int total;
  final int skipped;
  final int compared;
  final int passed;
  final int failed;
  final double passRate;
  final int parseErrors;
  final int fallbackDiagnosticFailures;
  final int rangeNormalizationFailures;
  final int determinismFailures;
  final List<String> failureSamples;
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

bool _hasNativeFallbackDiagnostics(SyntaxSnapshot snapshot) {
  return snapshot.diagnostics.any(
    (diagnostic) =>
        diagnostic.code == 'COMRAK_INLINE_FALLBACK_USED' ||
        diagnostic.code == 'COMRAK_MARKER_FALLBACK_USED',
  );
}

bool _areRangesValid(List<TextRange> ranges, int textLength) {
  for (final range in ranges) {
    if (range.start < 0 || range.end > textLength || range.start >= range.end) {
      return false;
    }
  }
  return true;
}

bool _areBlocksValid(List<BlockSpan> blocks, int textLength) {
  for (final block in blocks) {
    if (block.start < 0 || block.end > textLength || block.start >= block.end) {
      return false;
    }
  }
  return true;
}

bool _areInlineTokensNormalized(List<InlineSpanToken> tokens, int textLength) {
  for (final token in tokens) {
    if (token.start < 0 || token.end > textLength || token.start >= token.end) {
      return false;
    }
  }
  return true;
}

List<String> _blockSignature(SyntaxSnapshot snapshot) {
  return snapshot.blocks
      .map((block) => '${block.type.name}:${block.start}:${block.end}')
      .toList(growable: false);
}

List<String> _inlineSignature(SyntaxSnapshot snapshot) {
  String styleKey(InlineSpanToken token) {
    final styleNames = token.style.types
        .map((type) => type.name)
        .toList(growable: false)
      ..sort();
    return styleNames.join('+');
  }

  return snapshot.inlineTokens
      .map((token) => '${styleKey(token)}:${token.start}:${token.end}')
      .toList(growable: false);
}

Future<_LaneContractScore> _scoreLane({
  required String laneName,
  required List<_UpstreamCase> cases,
  required Set<int> deviations,
  required MarkdownSyntaxProfile profile,
  required CommonMarkParseBackend nativeBackend,
}) async {
  var skipped = 0;
  var compared = 0;
  var passed = 0;
  var failed = 0;
  var parseErrors = 0;
  var fallbackDiagnosticFailures = 0;
  var rangeNormalizationFailures = 0;
  var determinismFailures = 0;
  final failureSamples = <String>[];
  const maxSamples = 12;

  for (final fixture in cases) {
    if (deviations.contains(fixture.example)) {
      skipped++;
      continue;
    }

    compared++;
    final request = SyntaxParseRequest(
      revision: fixture.example,
      text: fixture.markdown,
      profile: profile,
    );
    final first = await nativeBackend.parse(request);
    final second = await nativeBackend.parse(request);
    final textLength = fixture.markdown.length;

    final hasError = first.diagnostics.any((diagnostic) => diagnostic.isError);
    final hasFallback = _hasNativeFallbackDiagnostics(first);
    final rangesValid = _areRangesValid(first.markerRanges, textLength) &&
        _areRangesValid(first.exclusionRanges, textLength) &&
        _areBlocksValid(first.blocks, textLength) &&
        _areInlineTokensNormalized(first.inlineTokens, textLength);
    final deterministic = listEquals(first.markerRanges, second.markerRanges) &&
        listEquals(first.exclusionRanges, second.exclusionRanges) &&
        listEquals(_blockSignature(first), _blockSignature(second)) &&
        listEquals(_inlineSignature(first), _inlineSignature(second));

    if (hasError) parseErrors++;
    if (hasFallback) fallbackDiagnosticFailures++;
    if (!rangesValid) rangeNormalizationFailures++;
    if (!deterministic) determinismFailures++;

    if (!hasError &&
        !hasFallback &&
        rangesValid &&
        deterministic &&
        first.cursorMask.snapToSafeOffset(-100) >= 0 &&
        first.cursorMask.snapToSafeOffset(textLength + 100) <= textLength) {
      passed++;
      continue;
    }

    failed++;
    if (failureSamples.length < maxSamples) {
      final reason = [
        if (hasError) 'error',
        if (hasFallback) 'fallback',
        if (!rangesValid) 'ranges',
        if (!deterministic) 'determinism',
      ].join('+');
      failureSamples.add(
        '$laneName#${fixture.example} section="${fixture.section}" reason=$reason',
      );
    }
  }

  final passRate = compared == 0 ? 1.0 : passed / compared;
  return _LaneContractScore(
    total: cases.length,
    skipped: skipped,
    compared: compared,
    passed: passed,
    failed: failed,
    passRate: passRate,
    parseErrors: parseErrors,
    fallbackDiagnosticFailures: fallbackDiagnosticFailures,
    rangeNormalizationFailures: rangeNormalizationFailures,
    determinismFailures: determinismFailures,
    failureSamples: failureSamples,
  );
}

void main() {
  final corePath = sovereignFixturePath(
    'commonmark/upstream/common_mark_tests.json',
  );
  final gfmPath = sovereignFixturePath('commonmark/upstream/gfm_tests.json');
  final deviationPath = sovereignFixturePath(
    'commonmark/native_parity_deviation_register.json',
  );
  const coreThreshold = 0.995;
  const gfmThreshold = 0.99;

  final coreCases = _loadCases(corePath);
  final gfmCases = _loadCases(gfmPath);
  final deviations = _loadDeviationRegister(deviationPath);
  final coreDeviations = deviations['core']!;
  final gfmDeviations = deviations['gfm']!;

  group('Native CommonMark upstream fixture contracts', () {
    final libPath = sovereignNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; upstream contract suite skipped', () {
        expect(true, isTrue);
      });
      return;
    }

    final nativeBackend = ComrakCommonMarkParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    );

    test('core lane meets upstream contract threshold', () async {
      final score = await _scoreLane(
        laneName: 'core',
        cases: coreCases,
        deviations: coreDeviations.map((item) => item.example).toSet(),
        profile: MarkdownSyntaxProfile.commonMarkCore,
        nativeBackend: nativeBackend,
      );

      debugPrint(
        'Native core contract: passed=${score.passed} failed=${score.failed} '
        'compared=${score.compared} skipped=${score.skipped} total=${score.total} '
        'passRate=${score.passRate.toStringAsFixed(4)} parseErrors=${score.parseErrors} '
        'fallback=${score.fallbackDiagnosticFailures} ranges=${score.rangeNormalizationFailures} '
        'determinism=${score.determinismFailures}',
      );
      if (score.failureSamples.isNotEmpty) {
        debugPrint(
          'Native core contract samples: ${score.failureSamples.join(', ')}',
        );
      }

      expect(score.compared, greaterThan(0));
      expect(score.passRate, greaterThanOrEqualTo(coreThreshold));
    });

    test('gfm lane meets upstream contract threshold', () async {
      final score = await _scoreLane(
        laneName: 'gfm',
        cases: gfmCases,
        deviations: gfmDeviations.map((item) => item.example).toSet(),
        profile: MarkdownSyntaxProfile.commonMarkGfm,
        nativeBackend: nativeBackend,
      );

      debugPrint(
        'Native gfm contract: passed=${score.passed} failed=${score.failed} '
        'compared=${score.compared} skipped=${score.skipped} total=${score.total} '
        'passRate=${score.passRate.toStringAsFixed(4)} parseErrors=${score.parseErrors} '
        'fallback=${score.fallbackDiagnosticFailures} ranges=${score.rangeNormalizationFailures} '
        'determinism=${score.determinismFailures}',
      );
      if (score.failureSamples.isNotEmpty) {
        debugPrint(
          'Native gfm contract samples: ${score.failureSamples.join(', ')}',
        );
      }

      expect(score.compared, greaterThan(0));
      expect(score.passRate, greaterThanOrEqualTo(gfmThreshold));
    });
  });
}
