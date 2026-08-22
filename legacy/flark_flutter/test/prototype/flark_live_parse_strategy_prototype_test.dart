import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter_advanced.dart';

import '../v2/support/flark_test_paths.dart';

/// Temporary architecture probe. This is deliberately not production code.
///
/// It compares three states for each edit:
///
/// 1. today's mapped/structural prediction;
/// 2. a conservative synchronous Comrak parse of the affected authoritative
///    top-level block;
/// 3. the next full-document Comrak parse, used as the oracle.
///
/// The local strategy makes no claim for structural or context-dependent
/// edits. Those become explicit whole-document fallbacks instead of guessed
/// grammar.
void main() {
  final libraryPath = flarkNativeBridgeLibraryPathForPlatform();

  test('compare provisional and block-local parser strategies', () async {
    expect(
      libraryPath.isNotEmpty && File(libraryPath).existsSync(),
      isTrue,
      reason: 'Run ./scripts/build_comrak_all.sh --host-only first.',
    );
    final backend = FlarkNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: libraryPath,
    );
    final report = _StrategyReport();

    for (final editCase in _curatedCases) {
      await _probeCase(backend, editCase, report);
    }
    for (final editCase in _generatedInlineCases()) {
      await _probeCase(backend, editCase, report);
    }
    for (final editCase in _generatedMixedCases()) {
      await _probeCase(backend, editCase, report);
    }

    final sizeResults = <_SizeResult>[];
    for (final target in const [100000, 1000000]) {
      sizeResults.add(await _probeLargeDocument(backend, target));
    }
    final fullDocumentSweep = _probeFullDocumentPipeline(backend);

    // ignore: avoid_print
    print(report.format(sizeResults, fullDocumentSweep));

    expect(
      report.localMismatches,
      isEmpty,
      reason:
          'A locally eligible edit must exactly match the corresponding '
          'slice of the full Comrak parse.',
    );
    expect(
      report.stitchMismatches,
      isEmpty,
      reason:
          'Replacing the affected block in the prior parse snapshot must '
          'produce the same whole-document projection and render plan as '
          'the full Comrak parse.',
    );
    expect(report.localEligible, greaterThan(50));
    expect(
      report.currentPredictionMismatches,
      isNotEmpty,
      reason:
          'The corpus must retain grammar-changing controls that expose '
          'today\'s provisional divergence.',
    );
    for (final result in sizeResults) {
      expect(result.localExact, isTrue, reason: result.label);
      expect(result.stitchExact, isTrue, reason: '${result.label} stitch');
      expect(
        result.localP95Micros,
        lessThan(16000),
        reason: '${result.label} local parsing should fit a 60 fps frame',
      );
    }
  });
}

List<_FullDocumentSizeResult> _probeFullDocumentPipeline(
  FlarkSyncCapableParseBackend backend,
) {
  final results = <_FullDocumentSizeResult>[];
  for (final targetLength in const [
    1024,
    4096,
    8192,
    16384,
    32768,
    49152,
    65536,
    98304,
    131072,
  ]) {
    final markdown = _markdownOfSize(targetLength);
    final controller = FlarkFlutterController.fromMarkdown(
      markdown,
      parseBackend: backend,
    );
    final parseSamples = <int>[];
    final adoptionSamples = <int>[];
    final totalSamples = <int>[];
    try {
      const warmupIterations = 5;
      final measuredIterations = targetLength >= 98304 ? 15 : 25;
      for (
        var iteration = 0;
        iteration < warmupIterations + measuredIterations;
        iteration += 1
      ) {
        final totalWatch = Stopwatch()..start();
        final parseWatch = Stopwatch()..start();
        final parsed = backend.parseSync(
          FlarkMarkdownParseRequest(
            revision: controller.state.revision,
            markdown: markdown,
            profile: FlarkMarkdownProfile.commonMarkGfm,
            maxSyncUtf8Bytes: 1 << 20,
          ),
        );
        parseWatch.stop();
        if (parsed == null) {
          throw StateError('Full-document sync parse unexpectedly declined.');
        }
        final adoptionWatch = Stopwatch()..start();
        if (!controller.applyParseResult(parsed)) {
          throw StateError('Full-document parse result was rejected.');
        }
        adoptionWatch.stop();
        totalWatch.stop();
        if (iteration >= warmupIterations) {
          parseSamples.add(parseWatch.elapsedMicroseconds);
          adoptionSamples.add(adoptionWatch.elapsedMicroseconds);
          totalSamples.add(totalWatch.elapsedMicroseconds);
        }
      }
    } finally {
      controller.dispose();
    }
    results.add(
      _FullDocumentSizeResult(
        documentLength: markdown.length,
        parseMedianMicros: _percentile(parseSamples, 0.5),
        parseP95Micros: _percentile(parseSamples, 0.95),
        adoptionMedianMicros: _percentile(adoptionSamples, 0.5),
        adoptionP95Micros: _percentile(adoptionSamples, 0.95),
        totalMedianMicros: _percentile(totalSamples, 0.5),
        totalP95Micros: _percentile(totalSamples, 0.95),
      ),
    );
  }
  return results;
}

Future<void> _probeCase(
  FlarkMarkdownParseBackend backend,
  _EditCase editCase,
  _StrategyReport report,
) async {
  final before = await _parse(backend, editCase.before, revision: 0);
  final afterText = editCase.apply();
  final fullWatch = Stopwatch()..start();
  final after = await _parse(backend, afterText, revision: 1);
  fullWatch.stop();
  report.fullParseMicros.add(fullWatch.elapsedMicroseconds);

  final transaction = editCase.transaction;
  var currentPredictionDiverged = false;
  final controller = FlarkFlutterController.fromMarkdown(
    editCase.before,
    parseBackend: backend,
  );
  try {
    expect(controller.applyParseResult(before), isTrue);
    controller.applyTransaction(transaction);
    final predicted = _renderSignature(
      source: controller.markdown,
      projection: controller.projection,
      renderPlan: controller.renderPlan,
    );
    final authoritativeProjection = FlarkProjection.fromParseResult(after);
    final authoritative = _renderSignature(
      source: afterText,
      projection: authoritativeProjection,
      renderPlan: FlarkRenderPlan.fromParseResult(
        parseResult: after,
        projection: authoritativeProjection,
      ),
    );
    report.total += 1;
    if (predicted == authoritative) {
      report.currentPredictionExact += 1;
    } else {
      report.currentPredictionMismatches.add(editCase.id);
      currentPredictionDiverged = true;
    }
  } finally {
    controller.dispose();
  }

  _probeExpandedLocal(
    backend: backend as FlarkSyncCapableParseBackend,
    editCase: editCase,
    before: before,
    after: after,
    afterText: afterText,
    report: report,
  );

  final eligibility = _localEligibility(
    markdown: editCase.before,
    parseResult: before,
    operation: editCase.operation,
  );
  if (eligibility case _LocalFallback(:final reason)) {
    if (currentPredictionDiverged) {
      report.currentDivergenceExplicitFallback += 1;
    }
    report.fallbackReasons.update(
      reason,
      (value) => value + 1,
      ifAbsent: () => 1,
    );
    return;
  }
  final eligible = eligibility as _LocalEligible;
  report.localEligible += 1;
  final fragment = afterText.substring(
    eligible.rangeAfter.start,
    eligible.rangeAfter.end,
  );
  final localWatch = Stopwatch()..start();
  final local = backend.parseSync(
    FlarkMarkdownParseRequest(
      revision: 2,
      markdown: fragment,
      profile: FlarkMarkdownProfile.commonMarkGfm,
      maxSyncUtf8Bytes: 1 << 20,
    ),
  );
  localWatch.stop();
  report.localParseMicros.add(localWatch.elapsedMicroseconds);
  if (local == null) {
    report.localMismatches.add('${editCase.id}: sync parse declined');
    return;
  }

  final localSignature = _parseSliceSignature(
    result: local,
    source: fragment,
    range: FlarkSourceRange(0, fragment.length),
    rangeShift: eligible.rangeAfter.start,
  );
  final oracleSignature = _parseSliceSignature(
    result: after,
    source: afterText,
    range: eligible.rangeAfter,
  );
  if (localSignature == oracleSignature) {
    report.localExact += 1;
    if (currentPredictionDiverged) {
      report.currentDivergenceResolvedLocally += 1;
    }

    final stitched = _stitchParseResult(
      before: before,
      local: local,
      transaction: transaction,
      replacedRangeBefore: eligible.blockBefore.sourceRange,
      replacementStartAfter: eligible.rangeAfter.start,
      sourceTextLengthAfter: afterText.length,
      revisionAfter: after.revision,
    );
    final fullRange = FlarkSourceRange(0, afterText.length);
    final stitchedProjection = FlarkProjection.fromParseResult(stitched);
    final oracleProjection = FlarkProjection.fromParseResult(after);
    final stitchedSignature =
        '${_parseSliceSignature(result: stitched, source: afterText, range: fullRange)}||render=${_renderSignature(
          source: afterText,
          projection: stitchedProjection,
          renderPlan: FlarkRenderPlan.fromParseResult(parseResult: stitched, projection: stitchedProjection),
        )}';
    final fullSignature =
        '${_parseSliceSignature(result: after, source: afterText, range: fullRange)}||render=${_renderSignature(
          source: afterText,
          projection: oracleProjection,
          renderPlan: FlarkRenderPlan.fromParseResult(parseResult: after, projection: oracleProjection),
        )}';
    if (stitchedSignature == fullSignature) {
      report.stitchExact += 1;
    } else {
      report.stitchMismatches.add(editCase.id);
    }
  } else {
    report.localMismatches.add(
      '${editCase.id}\n  local:  $localSignature\n  oracle: $oracleSignature',
    );
  }
}

void _probeExpandedLocal({
  required FlarkSyncCapableParseBackend backend,
  required _EditCase editCase,
  required FlarkMarkdownParseResult before,
  required FlarkMarkdownParseResult after,
  required String afterText,
  required _StrategyReport report,
}) {
  final candidate = _expandedLocalCandidate(
    markdown: editCase.before,
    parseResult: before,
    operation: editCase.operation,
  );
  if (candidate == null) return;
  report.expandedEligible += 1;
  final fragment = afterText.substring(
    candidate.rangeAfter.start,
    candidate.rangeAfter.end,
  );
  final local = backend.parseSync(
    FlarkMarkdownParseRequest(
      revision: 3,
      markdown: fragment,
      profile: FlarkMarkdownProfile.commonMarkGfm,
      maxSyncUtf8Bytes: 1 << 20,
    ),
  );
  if (local == null) {
    report.expandedMismatches.add('${editCase.id}: sync parse declined');
    return;
  }
  final stitched = _stitchParseResult(
    before: before,
    local: local,
    transaction: editCase.transaction,
    replacedRangeBefore: candidate.blockBefore.sourceRange,
    replacementStartAfter: candidate.rangeAfter.start,
    sourceTextLengthAfter: afterText.length,
    revisionAfter: after.revision,
  );
  if (_wholeParseAndRenderSignature(stitched, afterText) ==
      _wholeParseAndRenderSignature(after, afterText)) {
    report.expandedExact += 1;
  } else {
    report.expandedMismatches.add(
      '${editCase.id} before=${jsonEncode(editCase.before)} '
      'range=${editCase.operation.replacedRange} '
      'replacement=${jsonEncode(editCase.operation.replacementText)}',
    );
  }
}

String _wholeParseAndRenderSignature(
  FlarkMarkdownParseResult result,
  String source,
) {
  final projection = FlarkProjection.fromParseResult(result);
  return '${_parseSliceSignature(result: result, source: source, range: FlarkSourceRange(0, source.length))}||render=${_renderSignature(
    source: source,
    projection: projection,
    renderPlan: FlarkRenderPlan.fromParseResult(parseResult: result, projection: projection),
  )}';
}

_LocalEligible? _expandedLocalCandidate({
  required String markdown,
  required FlarkMarkdownParseResult parseResult,
  required FlarkSourceOperation operation,
}) {
  if (parseResult.hiddenRanges.any(
    (hidden) => hidden.kind == FlarkMarkdownHiddenRangeKind.referenceDefinition,
  )) {
    return null;
  }
  final range = operation.replacedRange;
  final candidates =
      parseResult.blocks.where((block) {
        final blockRange = block.sourceRange;
        if (range.isCollapsed) {
          return range.start >= blockRange.start &&
              range.start <= blockRange.end;
        }
        return range.start >= blockRange.start && range.end <= blockRange.end;
      }).toList()..sort(
        (left, right) =>
            left.sourceRange.length.compareTo(right.sourceRange.length),
      );
  if (candidates.isEmpty) return null;
  if (candidates.length > 1 &&
      candidates[0].sourceRange.length == candidates[1].sourceRange.length) {
    return null;
  }
  final block = candidates.first;
  final transaction = FlarkTransaction.single(operation);
  final startAfter = transaction.mapOffset(
    block.sourceRange.start,
    affinity: FlarkMapAffinity.upstream,
  );
  final endAfter = transaction.mapOffset(
    block.sourceRange.end,
    affinity: FlarkMapAffinity.downstream,
  );
  if (startAfter > endAfter) return null;
  return _LocalEligible(
    blockBefore: block,
    rangeAfter: FlarkSourceRange(startAfter, endAfter),
  );
}

FlarkMarkdownParseResult _stitchParseResult({
  required FlarkMarkdownParseResult before,
  required FlarkMarkdownParseResult local,
  required FlarkTransaction transaction,
  required FlarkSourceRange replacedRangeBefore,
  required int replacementStartAfter,
  required int sourceTextLengthAfter,
  required int revisionAfter,
}) {
  bool replaced(FlarkSourceRange range) =>
      _contains(replacedRangeBefore, range);

  final blocks = <FlarkMarkdownBlockNode>[
    for (final block in before.blocks)
      if (!replaced(block.sourceRange)) _mapBlock(block, transaction),
    for (final block in local.blocks) _shiftBlock(block, replacementStartAfter),
  ]..sort(_compareBlocks);
  final inlineTokens = <FlarkMarkdownInlineToken>[
    for (final token in before.inlineTokens)
      if (!replaced(token.sourceRange)) _mapInlineToken(token, transaction),
    for (final token in local.inlineTokens)
      _shiftInlineToken(token, replacementStartAfter),
  ]..sort((left, right) => _compareRanges(left.sourceRange, right.sourceRange));
  final hiddenRanges = <FlarkMarkdownHiddenRange>[
    for (final hidden in before.hiddenRanges)
      if (!replaced(hidden.sourceRange)) _mapHiddenRange(hidden, transaction),
    for (final hidden in local.hiddenRanges)
      _shiftHiddenRange(hidden, replacementStartAfter),
  ]..sort((left, right) => _compareRanges(left.sourceRange, right.sourceRange));
  final replacementRanges = <FlarkMarkdownReplacementRange>[
    for (final replacement in before.replacementRanges)
      if (!replaced(replacement.sourceRange))
        _mapReplacementRange(replacement, transaction),
    for (final replacement in local.replacementRanges)
      _shiftReplacementRange(replacement, replacementStartAfter),
  ]..sort((left, right) => _compareRanges(left.sourceRange, right.sourceRange));
  final ambiguityZones = <FlarkMarkdownAmbiguityZone>[
    for (final zone in before.ambiguityZones)
      if (!replaced(zone.sourceRange)) _mapAmbiguityZone(zone, transaction),
    for (final zone in local.ambiguityZones)
      _shiftAmbiguityZone(zone, replacementStartAfter),
  ]..sort((left, right) => _compareRanges(left.sourceRange, right.sourceRange));

  return FlarkMarkdownParseResult(
    schemaVersion: before.schemaVersion,
    revision: revisionAfter,
    sourceTextLength: sourceTextLengthAfter,
    blocks: blocks,
    inlineTokens: inlineTokens,
    hiddenRanges: hiddenRanges,
    replacementRanges: replacementRanges,
    ambiguityZones: ambiguityZones,
    extensions: const {'prototype': 'block-local-stitch'},
  );
}

FlarkMarkdownBlockNode _mapBlock(
  FlarkMarkdownBlockNode block,
  FlarkTransaction transaction,
) {
  return FlarkMarkdownBlockNode(
    kind: block.kind,
    type: block.type,
    sourceRange: _mapRange(block.sourceRange, transaction),
    attributes: block.attributes,
    children: [
      for (final child in block.children) _mapBlock(child, transaction),
    ],
    extensions: block.extensions,
  );
}

FlarkMarkdownBlockNode _shiftBlock(FlarkMarkdownBlockNode block, int amount) {
  return FlarkMarkdownBlockNode(
    kind: block.kind,
    type: block.type,
    sourceRange: _shift(block.sourceRange, amount),
    attributes: block.attributes,
    children: [for (final child in block.children) _shiftBlock(child, amount)],
    extensions: block.extensions,
  );
}

FlarkMarkdownInlineToken _mapInlineToken(
  FlarkMarkdownInlineToken token,
  FlarkTransaction transaction,
) {
  return FlarkMarkdownInlineToken(
    kind: token.kind,
    type: token.type,
    sourceRange: _mapRange(token.sourceRange, transaction),
    attributes: token.attributes,
    extensions: token.extensions,
  );
}

FlarkMarkdownInlineToken _shiftInlineToken(
  FlarkMarkdownInlineToken token,
  int amount,
) {
  return FlarkMarkdownInlineToken(
    kind: token.kind,
    type: token.type,
    sourceRange: _shift(token.sourceRange, amount),
    attributes: token.attributes,
    extensions: token.extensions,
  );
}

FlarkMarkdownHiddenRange _mapHiddenRange(
  FlarkMarkdownHiddenRange hidden,
  FlarkTransaction transaction,
) {
  return FlarkMarkdownHiddenRange(
    kind: hidden.kind,
    type: hidden.type,
    sourceRange: _mapRange(hidden.sourceRange, transaction),
    attributes: hidden.attributes,
    extensions: hidden.extensions,
  );
}

FlarkMarkdownHiddenRange _shiftHiddenRange(
  FlarkMarkdownHiddenRange hidden,
  int amount,
) {
  return FlarkMarkdownHiddenRange(
    kind: hidden.kind,
    type: hidden.type,
    sourceRange: _shift(hidden.sourceRange, amount),
    attributes: hidden.attributes,
    extensions: hidden.extensions,
  );
}

FlarkMarkdownReplacementRange _mapReplacementRange(
  FlarkMarkdownReplacementRange replacement,
  FlarkTransaction transaction,
) {
  return FlarkMarkdownReplacementRange(
    kind: replacement.kind,
    type: replacement.type,
    sourceRange: _mapRange(replacement.sourceRange, transaction),
    replacementText: replacement.replacementText,
    attributes: replacement.attributes,
    extensions: replacement.extensions,
  );
}

FlarkMarkdownReplacementRange _shiftReplacementRange(
  FlarkMarkdownReplacementRange replacement,
  int amount,
) {
  return FlarkMarkdownReplacementRange(
    kind: replacement.kind,
    type: replacement.type,
    sourceRange: _shift(replacement.sourceRange, amount),
    replacementText: replacement.replacementText,
    attributes: replacement.attributes,
    extensions: replacement.extensions,
  );
}

FlarkMarkdownAmbiguityZone _mapAmbiguityZone(
  FlarkMarkdownAmbiguityZone zone,
  FlarkTransaction transaction,
) {
  return FlarkMarkdownAmbiguityZone(
    kind: zone.kind,
    type: zone.type,
    sourceRange: _mapRange(zone.sourceRange, transaction),
    preferredAffinity: zone.preferredAffinity,
    attributes: zone.attributes,
    extensions: zone.extensions,
  );
}

FlarkMarkdownAmbiguityZone _shiftAmbiguityZone(
  FlarkMarkdownAmbiguityZone zone,
  int amount,
) {
  return FlarkMarkdownAmbiguityZone(
    kind: zone.kind,
    type: zone.type,
    sourceRange: _shift(zone.sourceRange, amount),
    preferredAffinity: zone.preferredAffinity,
    attributes: zone.attributes,
    extensions: zone.extensions,
  );
}

FlarkSourceRange _mapRange(
  FlarkSourceRange range,
  FlarkTransaction transaction,
) {
  return FlarkSourceRange(
    transaction.mapOffset(range.start, affinity: FlarkMapAffinity.upstream),
    transaction.mapOffset(range.end, affinity: FlarkMapAffinity.downstream),
  );
}

int _compareBlocks(FlarkMarkdownBlockNode left, FlarkMarkdownBlockNode right) {
  final rangeCompare = _compareRanges(left.sourceRange, right.sourceRange);
  if (rangeCompare != 0) return rangeCompare;
  return left.kind.index.compareTo(right.kind.index);
}

int _compareRanges(FlarkSourceRange left, FlarkSourceRange right) {
  final startCompare = left.start.compareTo(right.start);
  if (startCompare != 0) return startCompare;
  return left.length.compareTo(right.length);
}

Future<_SizeResult> _probeLargeDocument(
  FlarkMarkdownParseBackend backend,
  int targetLength,
) async {
  final beforeText = _markdownOfSize(targetLength);
  final anchor = beforeText.indexOf('content', beforeText.length ~/ 2);
  final editCase = _EditCase(
    id: '${targetLength ~/ 1000}KB localized inline edit',
    before: beforeText,
    operation: FlarkSourceOperation.insert(anchor + 3, '*'),
  );
  final before = await _parse(backend, beforeText, revision: 10);
  final afterText = editCase.apply();

  final fullSamples = <int>[];
  FlarkMarkdownParseResult? after;
  final fullIterations = targetLength >= 1000000 ? 2 : 4;
  for (var iteration = 0; iteration < fullIterations; iteration += 1) {
    final watch = Stopwatch()..start();
    after = await _parse(backend, afterText, revision: 11 + iteration);
    watch.stop();
    fullSamples.add(watch.elapsedMicroseconds);
  }

  final eligibility = _localEligibility(
    markdown: beforeText,
    parseResult: before,
    operation: editCase.operation,
  );
  if (eligibility is! _LocalEligible) {
    throw StateError(
      'Large document edit unexpectedly fell back: $eligibility',
    );
  }
  final fragment = afterText.substring(
    eligibility.rangeAfter.start,
    eligibility.rangeAfter.end,
  );
  final localSamples = <int>[];
  FlarkMarkdownParseResult? local;
  for (var iteration = 0; iteration < 80; iteration += 1) {
    final watch = Stopwatch()..start();
    local = (backend as FlarkSyncCapableParseBackend).parseSync(
      FlarkMarkdownParseRequest(
        revision: 20 + iteration,
        markdown: fragment,
        profile: FlarkMarkdownProfile.commonMarkGfm,
        maxSyncUtf8Bytes: 1 << 20,
      ),
    );
    watch.stop();
    if (iteration >= 10) localSamples.add(watch.elapsedMicroseconds);
  }
  if (local == null || after == null) {
    throw StateError('Parser unexpectedly declined a prototype sample.');
  }

  final pipelineSamples = <int>[];
  final segmentPipelineSamples = <int>[];
  final stitchSamples = <int>[];
  final projectionSamples = <int>[];
  final renderPlanSamples = <int>[];
  FlarkMarkdownParseResult? stitched;
  final pipelineIterations = targetLength >= 1000000 ? 12 : 30;
  for (var iteration = 0; iteration < pipelineIterations; iteration += 1) {
    final totalWatch = Stopwatch()..start();
    final pipelineLocal = (backend as FlarkSyncCapableParseBackend).parseSync(
      FlarkMarkdownParseRequest(
        revision: 120 + iteration,
        markdown: fragment,
        profile: FlarkMarkdownProfile.commonMarkGfm,
        maxSyncUtf8Bytes: 1 << 20,
      ),
    );
    if (pipelineLocal == null) {
      throw StateError('Parser declined an end-to-end pipeline sample.');
    }
    final localProjection = FlarkProjection.fromParseResult(pipelineLocal);
    final localRenderPlan = FlarkRenderPlan.fromParseResult(
      parseResult: pipelineLocal,
      projection: localProjection,
    );
    final segmentElapsed = totalWatch.elapsedMicroseconds;

    final stitchWatch = Stopwatch()..start();
    stitched = _stitchParseResult(
      before: before,
      local: pipelineLocal,
      transaction: editCase.transaction,
      replacedRangeBefore: eligibility.blockBefore.sourceRange,
      replacementStartAfter: eligibility.rangeAfter.start,
      sourceTextLengthAfter: afterText.length,
      revisionAfter: after.revision,
    );
    stitchWatch.stop();

    final projectionWatch = Stopwatch()..start();
    final projection = FlarkProjection.fromParseResult(stitched);
    projectionWatch.stop();

    final renderPlanWatch = Stopwatch()..start();
    final renderPlan = FlarkRenderPlan.fromParseResult(
      parseResult: stitched,
      projection: projection,
    );
    renderPlanWatch.stop();
    if (renderPlan.allBlocks.isEmpty && stitched.blocks.isNotEmpty) {
      throw StateError('Render-plan construction unexpectedly lost blocks.');
    }
    if (localRenderPlan.allBlocks.isEmpty && pipelineLocal.blocks.isNotEmpty) {
      throw StateError('Segment render-plan construction lost blocks.');
    }
    totalWatch.stop();
    if (iteration >= 3) {
      pipelineSamples.add(totalWatch.elapsedMicroseconds);
      segmentPipelineSamples.add(segmentElapsed);
      stitchSamples.add(stitchWatch.elapsedMicroseconds);
      projectionSamples.add(projectionWatch.elapsedMicroseconds);
      renderPlanSamples.add(renderPlanWatch.elapsedMicroseconds);
    }
  }
  if (stitched == null) {
    throw StateError('Pipeline did not produce a stitched result.');
  }
  final localSignature = _parseSliceSignature(
    result: local,
    source: fragment,
    range: FlarkSourceRange(0, fragment.length),
    rangeShift: eligibility.rangeAfter.start,
  );
  final oracleSignature = _parseSliceSignature(
    result: after,
    source: afterText,
    range: eligibility.rangeAfter,
  );
  final stitchedProjection = FlarkProjection.fromParseResult(stitched);
  final oracleProjection = FlarkProjection.fromParseResult(after);
  final stitchExact =
      _renderSignature(
        source: afterText,
        projection: stitchedProjection,
        renderPlan: FlarkRenderPlan.fromParseResult(
          parseResult: stitched,
          projection: stitchedProjection,
        ),
      ) ==
      _renderSignature(
        source: afterText,
        projection: oracleProjection,
        renderPlan: FlarkRenderPlan.fromParseResult(
          parseResult: after,
          projection: oracleProjection,
        ),
      );
  return _SizeResult(
    label: editCase.id,
    documentLength: beforeText.length,
    fragmentLength: fragment.length,
    localExact: localSignature == oracleSignature,
    stitchExact: stitchExact,
    localMedianMicros: _percentile(localSamples, 0.5),
    localP95Micros: _percentile(localSamples, 0.95),
    pipelineMedianMicros: _percentile(pipelineSamples, 0.5),
    pipelineP95Micros: _percentile(pipelineSamples, 0.95),
    segmentPipelineMedianMicros: _percentile(segmentPipelineSamples, 0.5),
    segmentPipelineP95Micros: _percentile(segmentPipelineSamples, 0.95),
    stitchMedianMicros: _percentile(stitchSamples, 0.5),
    projectionMedianMicros: _percentile(projectionSamples, 0.5),
    renderPlanMedianMicros: _percentile(renderPlanSamples, 0.5),
    fullMedianMicros: _percentile(fullSamples, 0.5),
    fullP95Micros: _percentile(fullSamples, 0.95),
  );
}

sealed class _LocalEligibility {
  const _LocalEligibility();
}

final class _LocalEligible extends _LocalEligibility {
  const _LocalEligible({required this.blockBefore, required this.rangeAfter});

  final FlarkMarkdownBlockNode blockBefore;
  final FlarkSourceRange rangeAfter;
}

final class _LocalFallback extends _LocalEligibility {
  const _LocalFallback(this.reason);

  final String reason;

  @override
  String toString() => reason;
}

_LocalEligibility _localEligibility({
  required String markdown,
  required FlarkMarkdownParseResult parseResult,
  required FlarkSourceOperation operation,
}) {
  final range = operation.replacedRange;
  final replacedText = markdown.substring(range.start, range.end);
  if (operation.replacementText.contains('\n') || replacedText.contains('\n')) {
    return const _LocalFallback('line-count change');
  }
  if (parseResult.hiddenRanges.any(
    (hidden) => hidden.kind == FlarkMarkdownHiddenRangeKind.referenceDefinition,
  )) {
    return const _LocalFallback('document context: reference definitions');
  }

  final candidates =
      parseResult.blocks.where((block) {
        final blockRange = block.sourceRange;
        if (range.isCollapsed) {
          return range.start > blockRange.start && range.start < blockRange.end;
        }
        return range.start >= blockRange.start && range.end <= blockRange.end;
      }).toList()..sort(
        (left, right) =>
            left.sourceRange.length.compareTo(right.sourceRange.length),
      );
  if (candidates.isEmpty) return const _LocalFallback('no unique block');
  final block = candidates.first;
  if (candidates.length > 1 &&
      candidates[1].sourceRange.length == block.sourceRange.length) {
    return const _LocalFallback('ambiguous overlapping blocks');
  }
  if (!const {
    FlarkMarkdownBlockKind.paragraph,
    FlarkMarkdownBlockKind.heading,
    FlarkMarkdownBlockKind.listItem,
    FlarkMarkdownBlockKind.blockquote,
  }.contains(block.kind)) {
    return _LocalFallback('unsupported block: ${block.kind.name}');
  }

  final lineStart = markdown.lastIndexOf('\n', max(0, range.start - 1)) + 1;
  if (range.start == lineStart) {
    return const _LocalFallback('line-prefix edit');
  }
  for (final hidden in parseResult.hiddenRanges) {
    if (hidden.kind != FlarkMarkdownHiddenRangeKind.blockMarker &&
        hidden.kind != FlarkMarkdownHiddenRangeKind.markdownMarker) {
      continue;
    }
    if (_operationTouches(range, hidden.sourceRange)) {
      return const _LocalFallback('structural marker edit');
    }
  }

  final transaction = FlarkTransaction.single(operation);
  final startAfter = transaction.mapOffset(
    block.sourceRange.start,
    affinity: FlarkMapAffinity.upstream,
  );
  final endAfter = transaction.mapOffset(
    block.sourceRange.end,
    affinity: FlarkMapAffinity.downstream,
  );
  if (startAfter >= endAfter) return const _LocalFallback('block deleted');
  return _LocalEligible(
    blockBefore: block,
    rangeAfter: FlarkSourceRange(startAfter, endAfter),
  );
}

bool _operationTouches(FlarkSourceRange edit, FlarkSourceRange target) {
  if (edit.isCollapsed) {
    return edit.start >= target.start && edit.start <= target.end;
  }
  return edit.intersects(target);
}

Future<FlarkMarkdownParseResult> _parse(
  FlarkMarkdownParseBackend backend,
  String markdown, {
  required int revision,
}) {
  return backend.parse(
    FlarkMarkdownParseRequest(
      revision: revision,
      markdown: markdown,
      profile: FlarkMarkdownProfile.commonMarkGfm,
    ),
  );
}

String _parseSliceSignature({
  required FlarkMarkdownParseResult result,
  required String source,
  required FlarkSourceRange range,
  int rangeShift = 0,
}) {
  final items = <String>[];
  for (final block in result.blocks) {
    if (!_contains(range, block.sourceRange)) continue;
    _appendBlock(items, block, rangeShift);
  }
  for (final token in result.inlineTokens) {
    if (!_contains(range, token.sourceRange)) continue;
    items.add(
      'inline:${token.kind.name}:${_shift(token.sourceRange, rangeShift)}:'
      '${_canonicalJson(token.attributes)}',
    );
  }
  for (final hidden in result.hiddenRanges) {
    if (!_contains(range, hidden.sourceRange)) continue;
    items.add(
      'hidden:${hidden.kind.name}:${_shift(hidden.sourceRange, rangeShift)}:'
      '${_canonicalJson(hidden.attributes)}',
    );
  }
  for (final replacement in result.replacementRanges) {
    if (!_contains(range, replacement.sourceRange)) continue;
    items.add(
      'replacement:${replacement.kind.name}:'
      '${_shift(replacement.sourceRange, rangeShift)}:'
      '${replacement.replacementText}',
    );
  }
  for (final zone in result.ambiguityZones) {
    if (!_contains(range, zone.sourceRange)) continue;
    items.add(
      'ambiguity:${zone.kind.name}:${_shift(zone.sourceRange, rangeShift)}',
    );
  }
  items.sort();

  final projection = FlarkProjection.fromParseResult(result);
  final projected = projection.projectText(source);
  final displayStart = projection.sourceToDisplayOffset(range.start);
  final displayEnd = projection.sourceToDisplayOffset(range.end);
  final display = projected.substring(displayStart, displayEnd);
  return '${items.join('|')}||display=${jsonEncode(display)}';
}

void _appendBlock(List<String> items, FlarkMarkdownBlockNode block, int shift) {
  items.add(
    'block:${block.kind.name}:${_shift(block.sourceRange, shift)}:'
    '${_canonicalJson(block.attributes)}',
  );
  for (final child in block.children) {
    _appendBlock(items, child, shift);
  }
}

String _renderSignature({
  required String source,
  required FlarkProjection projection,
  required FlarkRenderPlan renderPlan,
}) {
  final blocks = <String>[];
  for (final block in renderPlan.allBlocks) {
    blocks.add(
      '${block.kind.name}:${block.sourceRange}:${block.displayRange}:'
      'list=${block.listItem?.kind.name}:task=${block.taskListItem?.checked}:'
      'code=${block.codeBlock?.language}:table=${block.table != null}:'
      'runs=${block.inlineRuns.map((run) => '${run.kind.name}:${run.sourceRange}').join(',')}',
    );
  }
  return '${projection.projectText(source)}||${blocks.join('|')}';
}

bool _contains(FlarkSourceRange outer, FlarkSourceRange inner) {
  return inner.start >= outer.start && inner.end <= outer.end;
}

FlarkSourceRange _shift(FlarkSourceRange range, int amount) {
  return FlarkSourceRange(range.start + amount, range.end + amount);
}

String _canonicalJson(Object? value) => jsonEncode(_canonicalize(value));

Object? _canonicalize(Object? value) {
  if (value is Map) {
    final keys = value.keys.map((key) => key.toString()).toList()..sort();
    return <String, Object?>{
      for (final key in keys) key: _canonicalize(value[key]),
    };
  }
  if (value is Iterable) return value.map(_canonicalize).toList();
  return value;
}

final class _EditCase {
  const _EditCase({
    required this.id,
    required this.before,
    required this.operation,
  });

  final String id;
  final String before;
  final FlarkSourceOperation operation;

  String apply() => before.replaceRange(
    operation.replacedRange.start,
    operation.replacedRange.end,
    operation.replacementText,
  );

  FlarkTransaction get transaction => FlarkTransaction.single(
    operation,
    selectionAfter: FlarkSelection.collapsed(
      operation.replacedRange.start + operation.replacementText.length,
    ),
    metadata: FlarkTransactionMetadata(
      intent: FlarkTransactionIntent.input,
      userEvent: 'prototype.$id',
      parseInvalidationRange: operation.replacedRange,
      projectionInvalidationRange: operation.replacedRange,
    ),
  );
}

final _curatedCases = <_EditCase>[
  _EditCase(
    id: 'plain interior insert',
    before: 'hello world',
    operation: FlarkSourceOperation.insert(5, '!'),
  ),
  _EditCase(
    id: 'strong content insert',
    before: '**bold text** and tail',
    operation: FlarkSourceOperation.insert(6, '!'),
  ),
  _EditCase(
    id: 'strong delimiter deletion',
    before: '**bold** and tail',
    operation: FlarkSourceOperation.replace(
      replacedRange: FlarkSourceRange(7, 8),
      replacementText: '',
    ),
  ),
  _EditCase(
    id: 'link label edit',
    before: '[label](https://example.com) and tail',
    operation: FlarkSourceOperation.insert(3, 'x'),
  ),
  _EditCase(
    id: 'entity edit',
    before: 'a &amp; b and tail',
    operation: FlarkSourceOperation.insert(3, 'x'),
  ),
  _EditCase(
    id: 'heading body edit',
    before: '# Heading body',
    operation: FlarkSourceOperation.insert(5, 'x'),
  ),
  _EditCase(
    id: 'list body edit',
    before: '* item body',
    operation: FlarkSourceOperation.insert(5, 'x'),
  ),
  _EditCase(
    id: 'blockquote body edit',
    before: '> quote body',
    operation: FlarkSourceOperation.insert(5, 'x'),
  ),
  _EditCase(
    id: 'reference-context edit',
    before: '[label][ref] and tail\n\n[ref]: /url',
    operation: FlarkSourceOperation.insert(3, 'x'),
  ),
  _EditCase(
    id: 'code body edit',
    before: '```dart\ncode body\n```',
    operation: FlarkSourceOperation.insert(13, 'x'),
  ),
  _EditCase(
    id: 'table cell edit',
    before: '| A | B |\n| --- | --- |\n| x | y |',
    operation: FlarkSourceOperation.insert(29, 'z'),
  ),
  _EditCase(
    id: 'paragraph to heading',
    before: 'hello',
    operation: FlarkSourceOperation.insert(0, '# '),
  ),
  _EditCase(
    id: 'paragraph to list',
    before: 'hello',
    operation: FlarkSourceOperation.insert(0, '* '),
  ),
  _EditCase(
    id: 'list to paragraph',
    before: '* item',
    operation: FlarkSourceOperation.replace(
      replacedRange: FlarkSourceRange(0, 2),
      replacementText: '',
    ),
  ),
  _EditCase(
    id: 'paragraph split',
    before: 'hello world',
    operation: FlarkSourceOperation.insert(5, '\n\n'),
  ),
  _EditCase(
    id: 'open fence',
    before: 'tail',
    operation: FlarkSourceOperation.insert(0, '```\n'),
  ),
  _EditCase(
    id: 'close distant fence',
    before: '```\nbody\ntext',
    operation: FlarkSourceOperation.insert(13, '\n```'),
  ),
  _EditCase(
    id: 'open fence consumes following blocks',
    before: 'intro\n\nmiddle\n\ntail',
    operation: FlarkSourceOperation.insert(7, '```\n'),
  ),
  _EditCase(
    id: 'introduce reference definition with backward effect',
    before: '[label][new]\n\nplaceholder',
    operation: FlarkSourceOperation.replace(
      replacedRange: FlarkSourceRange(14, 25),
      replacementText: '[new]: /url',
    ),
  ),
];

Iterable<_EditCase> _generatedInlineCases() sync* {
  final random = Random(22022);
  const templates = <String>[
    'A plain paragraph with enough content for edits.',
    'A **strong paragraph** with trailing content.',
    'A paragraph with *emphasis* and `code` content.',
    '# A heading with **strong** content',
    '* A list item with [a link](https://example.com)',
    '> A quote with ~~struck~~ content',
  ];
  const insertions = <String>['x', '*', '_', '~', '`', ' '];
  for (var index = 0; index < 90; index += 1) {
    final template = templates[index % templates.length];
    final wordOffsets = <int>[
      for (var offset = 1; offset < template.length - 1; offset += 1)
        if (_isAsciiLetter(template.codeUnitAt(offset - 1)) &&
            _isAsciiLetter(template.codeUnitAt(offset)))
          offset,
    ];
    final offset = wordOffsets[random.nextInt(wordOffsets.length)];
    yield _EditCase(
      id: 'generated $index',
      before: template,
      operation: FlarkSourceOperation.insert(
        offset,
        insertions[random.nextInt(insertions.length)],
      ),
    );
  }
}

Iterable<_EditCase> _generatedMixedCases() sync* {
  final random = Random(94317);
  const templates = <String>[
    'First paragraph with **bold** content.\n\nSecond paragraph with '
        '[a link](https://example.com).',
    '# Heading text\n\nParagraph after the heading.',
    '* outer item\n  * nested item\n* sibling item',
    '> quoted first\n> quoted second\n\nTail paragraph.',
    'Setext heading\n---\n\nTail paragraph.',
    '```dart\ncode body\n```\n\nTail paragraph.',
    '| A | B |\n| --- | --- |\n| x | y |',
    '[label][ref] and tail\n\n[ref]: /url',
    '<div>\nhtml block\n</div>\n\nTail paragraph.',
    '- [ ] task body\n\nTail paragraph.',
  ];
  const replacements = <String>['x', '*', '_', '~', '`', ' ', '\n', '# ', '* '];
  for (var index = 0; index < 120; index += 1) {
    final template = templates[index % templates.length];
    final start = random.nextInt(template.length + 1);
    final replaceLength = random.nextInt(4) == 0 && start < template.length
        ? 1 + random.nextInt(min(3, template.length - start))
        : 0;
    yield _EditCase(
      id: 'mixed $index',
      before: template,
      operation: FlarkSourceOperation.replace(
        replacedRange: FlarkSourceRange(start, start + replaceLength),
        replacementText: replacements[random.nextInt(replacements.length)],
      ),
    );
  }
}

bool _isAsciiLetter(int codeUnit) {
  return (codeUnit >= 0x41 && codeUnit <= 0x5A) ||
      (codeUnit >= 0x61 && codeUnit <= 0x7A);
}

String _markdownOfSize(int targetLength) {
  final buffer = StringBuffer();
  var index = 0;
  while (buffer.length < targetLength) {
    buffer
      ..writeln('## Section $index')
      ..writeln()
      ..writeln(
        'Paragraph content $index with **bold**, *emphasis*, and '
        '[a link](https://example.com/$index).',
      )
      ..writeln();
    index += 1;
  }
  return buffer.toString();
}

int _percentile(List<int> values, double percentile) {
  final sorted = [...values]..sort();
  final index = ((sorted.length - 1) * percentile).ceil();
  return sorted[index];
}

final class _StrategyReport {
  int total = 0;
  int currentPredictionExact = 0;
  int currentDivergenceResolvedLocally = 0;
  int currentDivergenceExplicitFallback = 0;
  int localEligible = 0;
  int localExact = 0;
  int stitchExact = 0;
  int expandedEligible = 0;
  int expandedExact = 0;
  final currentPredictionMismatches = <String>[];
  final localMismatches = <String>[];
  final stitchMismatches = <String>[];
  final expandedMismatches = <String>[];
  final fallbackReasons = <String, int>{};
  final localParseMicros = <int>[];
  final fullParseMicros = <int>[];

  String format(
    List<_SizeResult> sizeResults,
    List<_FullDocumentSizeResult> fullDocumentSweep,
  ) {
    final buffer = StringBuffer()
      ..writeln('flark_live_parse_prototype')
      ..writeln(
        '  corpus total=$total current_prediction_exact='
        '$currentPredictionExact current_prediction_diverged='
        '${currentPredictionMismatches.length}',
      )
      ..writeln(
        '  block_local eligible=$localEligible exact=$localExact '
        'diverged=${localMismatches.length} coverage='
        '${(100 * localEligible / total).toStringAsFixed(1)}%',
      )
      ..writeln(
        '  whole_snapshot_stitch exact=$stitchExact '
        'diverged=${stitchMismatches.length}',
      )
      ..writeln(
        '  expanded_block_reparse_probe eligible=$expandedEligible '
        'exact=$expandedExact diverged=${expandedMismatches.length}',
      )
      ..writeln(
        '  current_prediction_divergence_handling local_authoritative='
        '$currentDivergenceResolvedLocally explicit_fallback='
        '$currentDivergenceExplicitFallback',
      )
      ..writeln(
        '  small_doc_latency block_local_median='
        '${_percentile(localParseMicros, 0.5)}us block_local_p95='
        '${_percentile(localParseMicros, 0.95)}us full_median='
        '${_percentile(fullParseMicros, 0.5)}us full_p95='
        '${_percentile(fullParseMicros, 0.95)}us',
      )
      ..writeln('  fallbacks=$fallbackReasons')
      ..writeln(
        '  current_prediction_divergence_examples='
        '${currentPredictionMismatches.take(12).toList()}',
      )
      ..writeln(
        '  expanded_block_divergence_examples='
        '${expandedMismatches.take(12).toList()}',
      );
    for (final result in sizeResults) {
      buffer.writeln('  ${result.format()}');
    }
    buffer.writeln('  full_document_sync_parse_and_adopt_sweep:');
    for (final result in fullDocumentSweep) {
      buffer.writeln('    ${result.format()}');
    }
    if (localMismatches.isNotEmpty) {
      buffer.writeln('  local_mismatches:');
      for (final mismatch in localMismatches) {
        buffer.writeln('    $mismatch');
      }
    }
    if (stitchMismatches.isNotEmpty) {
      buffer.writeln('  stitch_mismatches=$stitchMismatches');
    }
    return buffer.toString();
  }
}

final class _FullDocumentSizeResult {
  const _FullDocumentSizeResult({
    required this.documentLength,
    required this.parseMedianMicros,
    required this.parseP95Micros,
    required this.adoptionMedianMicros,
    required this.adoptionP95Micros,
    required this.totalMedianMicros,
    required this.totalP95Micros,
  });

  final int documentLength;
  final int parseMedianMicros;
  final int parseP95Micros;
  final int adoptionMedianMicros;
  final int adoptionP95Micros;
  final int totalMedianMicros;
  final int totalP95Micros;

  String format() {
    return 'bytes=$documentLength '
        'parse={median:${parseMedianMicros}us,p95:${parseP95Micros}us} '
        'adopt={median:${adoptionMedianMicros}us,p95:${adoptionP95Micros}us} '
        'total={median:${totalMedianMicros}us,p95:${totalP95Micros}us}';
  }
}

final class _SizeResult {
  const _SizeResult({
    required this.label,
    required this.documentLength,
    required this.fragmentLength,
    required this.localExact,
    required this.stitchExact,
    required this.localMedianMicros,
    required this.localP95Micros,
    required this.pipelineMedianMicros,
    required this.pipelineP95Micros,
    required this.segmentPipelineMedianMicros,
    required this.segmentPipelineP95Micros,
    required this.stitchMedianMicros,
    required this.projectionMedianMicros,
    required this.renderPlanMedianMicros,
    required this.fullMedianMicros,
    required this.fullP95Micros,
  });

  final String label;
  final int documentLength;
  final int fragmentLength;
  final bool localExact;
  final bool stitchExact;
  final int localMedianMicros;
  final int localP95Micros;
  final int pipelineMedianMicros;
  final int pipelineP95Micros;
  final int segmentPipelineMedianMicros;
  final int segmentPipelineP95Micros;
  final int stitchMedianMicros;
  final int projectionMedianMicros;
  final int renderPlanMedianMicros;
  final int fullMedianMicros;
  final int fullP95Micros;

  String format() {
    return '$label doc=$documentLength fragment=$fragmentLength '
        'local_exact=$localExact stitch_exact=$stitchExact '
        'local_median=${localMedianMicros}us '
        'local_p95=${localP95Micros}us full_median=${fullMedianMicros}us '
        'full_p95=${fullP95Micros}us pipeline_median='
        '${pipelineMedianMicros}us pipeline_p95=${pipelineP95Micros}us '
        'segment_pipeline_median=${segmentPipelineMedianMicros}us '
        'segment_pipeline_p95=${segmentPipelineP95Micros}us phases_median='
        '{stitch:${stitchMedianMicros}us,projection:${projectionMedianMicros}us,'
        'render_plan:${renderPlanMedianMicros}us} '
        'parse_speedup_median='
        '${(fullMedianMicros / localMedianMicros).toStringAsFixed(1)}x';
  }
}
