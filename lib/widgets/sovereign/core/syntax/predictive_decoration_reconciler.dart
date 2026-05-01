import 'package:flutter/services.dart';

import '../../engine/syntax_engine.dart';
import '../../engine/syntax_snapshot.dart';
import '../../logic/fenced_code_scanner.dart';
import '../../models/edit_op.dart';
import 'syntax_projection_coordinator.dart';

class PredictiveDecorationProjection {
  const PredictiveDecorationProjection({
    required this.hiddenRanges,
    required this.exclusionRanges,
    required this.cursorMask,
  });

  final List<TextRange> hiddenRanges;
  final List<TextRange> exclusionRanges;
  final CursorValidationMask cursorMask;
}

class SovereignPredictiveDecorationReconciler {
  const SovereignPredictiveDecorationReconciler(this._host);

  final SovereignSyntaxSyncHost _host;

  PredictiveDecorationProjection reconcile({
    required String targetText,
    required EditOp op,
  }) {
    final opStart = op.replacedRange.start.clamp(0, targetText.length);
    final opEnd = (op.replacedRange.start + op.insertedText.length).clamp(
      0,
      targetText.length,
    );
    final prediction = _host.syntaxEngine.predict(
      SyntaxPredictRequest(
        revision: _host.revision,
        text: targetText,
        profile: _host.markdownProfile,
        editRange: TextRange(start: opStart, end: opEnd),
        previousSnapshot: _host.latestAuthoritativeSnapshot,
        timeBudgetMicros: _host.predictiveScanTimeBudgetMicros,
        spanBudget: _host.predictiveScanSpanBudget,
        charLimit: _host.predictiveScanCharLimitOverride,
      ),
    );
    final predictedHidden = _host.normalizeHiddenRanges(
      prediction.markerRanges,
      targetText.length,
    );
    final excludedRanges = _host.normalizeHiddenRanges(
      prediction.exclusionRanges,
      targetText.length,
    );
    final ambiguityZones = _host.normalizeHiddenRanges(
      prediction.ambiguityZones,
      targetText.length,
    );
    final stableExcludedRanges = _host.stabilizeRangesInAmbiguity(
      predictedRanges: excludedRanges,
      authoritativeRanges: _host.authoritativeExclusionRanges,
      ambiguityZones: ambiguityZones,
      textLength: targetText.length,
    );
    final stablePredictedHidden = _host.stabilizeRangesInAmbiguity(
      predictedRanges: predictedHidden,
      authoritativeRanges: _host.authoritativeHiddenRanges,
      ambiguityZones: ambiguityZones,
      textLength: targetText.length,
    );
    if (ambiguityZones.isNotEmpty) {
      _host.predictiveBudgetExhaustionCount++;
    }

    final shiftedRanges = _host.shiftAndVerifyRanges(
      _host.projectedHiddenRanges,
      op,
      targetText,
    );
    final inlineFallbackHidden = _host.scanInlineMarkersNearEdit(
      targetText: targetText,
      editStart: opStart,
      editEnd: opEnd,
      excludedRanges: stableExcludedRanges,
    );
    if (ambiguityZones.isNotEmpty && inlineFallbackHidden.isNotEmpty) {
      _host.predictiveLocalFallbackCount++;
    }

    final newHiddenKeys = <int>{
      for (final range in stablePredictedHidden) _host.rangeKey(range),
    };

    bool isInlineMarker(TextRange range) {
      if (range.start < 0 ||
          range.end > targetText.length ||
          range.start >= range.end) {
        return false;
      }

      final token = targetText.substring(range.start, range.end);
      return token == '*' || token == '_' || token == '`' || token == '**';
    }

    bool isShiftableBlockMarker(TextRange range) {
      if (range.start < 0 ||
          range.end > targetText.length ||
          range.start >= range.end) {
        return false;
      }
      if (_host.isFenceMarkerRange(targetText, range)) {
        return true;
      }

      final lineStart = _host.lineStartForOffset(targetText, range.start);
      if (range.start != lineStart) return false;
      final lineEndWithBreak = FencedCodeScanner.endOfLine(
        targetText,
        lineStart,
      );
      final lineEnd = (lineEndWithBreak > 0 &&
              targetText.codeUnitAt(lineEndWithBreak - 1) == 10)
          ? lineEndWithBreak - 1
          : lineEndWithBreak;
      if (lineEnd <= lineStart) return false;

      final headerLen = _host.headerMarkerLength(
        targetText,
        lineStart,
        lineEnd,
      );
      if (headerLen > 0 && range.end == lineStart + headerLen) return true;

      final blockquoteLen = _host.blockquoteMarkerLength(
        targetText,
        lineStart,
        lineEnd,
      );
      if (blockquoteLen > 0 && range.end == lineStart + blockquoteLen) {
        return true;
      }

      final unorderedLen = _host.unorderedListMarkerLength(
        targetText,
        lineStart,
        lineEnd,
      );
      if (unorderedLen > 0 && range.end == lineStart + unorderedLen) {
        return true;
      }

      final orderedLen = _host.orderedListMarkerLength(
        targetText,
        lineStart,
        lineEnd,
      );
      if (orderedLen > 0 && range.end == lineStart + orderedLen) {
        return true;
      }

      return false;
    }

    final verifiedShifted = <TextRange>[];
    final sortedShifted = [...shiftedRanges]
      ..sort((a, b) => a.start.compareTo(b.start));
    var fenceIndex = 0;
    for (final range in sortedShifted) {
      while (fenceIndex < stableExcludedRanges.length &&
          stableExcludedRanges[fenceIndex].end <= range.start) {
        fenceIndex++;
      }
      final inFence = fenceIndex < stableExcludedRanges.length &&
          range.start >= stableExcludedRanges[fenceIndex].start &&
          range.end <= stableExcludedRanges[fenceIndex].end;
      if (inFence) continue;

      if (isInlineMarker(range)) continue;
      if (!isShiftableBlockMarker(range)) continue;

      final key = _host.rangeKey(range);
      if (!newHiddenKeys.contains(key)) continue;

      verifiedShifted.add(range);
    }

    final fenceFallbackHidden = _host.fencedCodeFenceMarkers(targetText, [
      for (final range in stableExcludedRanges)
        FencedCodeBlock(range.start, range.end),
    ]);

    final merged = <int, TextRange>{};
    for (final range in stablePredictedHidden) {
      merged[_host.rangeKey(range)] = range;
    }
    for (final range in verifiedShifted) {
      merged[_host.rangeKey(range)] = range;
    }
    for (final range in inlineFallbackHidden) {
      merged[_host.rangeKey(range)] = range;
    }
    for (final range in fenceFallbackHidden) {
      merged[_host.rangeKey(range)] = range;
    }
    final mergedList = merged.values.toList()
      ..sort((a, b) {
        final byStart = a.start.compareTo(b.start);
        if (byStart != 0) return byStart;
        return a.end.compareTo(b.end);
      });

    final normalized = <TextRange>[];
    for (final range in mergedList) {
      if (normalized.isEmpty) {
        normalized.add(range);
        continue;
      }
      final last = normalized.last;
      if (range.start < last.end) {
        continue;
      }
      normalized.add(range);
    }

    final hiddenRanges = _host.overlayCanonicalBlockMarkerRanges(
      targetText,
      normalized,
    );
    final cursorMask = _host.normalizeCursorMaskToText(
      prediction.cursorMask,
      textLength: targetText.length,
      fallbackHiddenRanges: normalized,
    );
    return PredictiveDecorationProjection(
      hiddenRanges: hiddenRanges,
      exclusionRanges: stableExcludedRanges,
      cursorMask: cursorMask,
    );
  }
}
