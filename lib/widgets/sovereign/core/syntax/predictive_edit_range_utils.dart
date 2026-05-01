import 'package:flutter/services.dart';

import 'package:sovereign_editor/src/widgets/sovereign/logic/fenced_code_scanner.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_style_scanner.dart';
import '../../models/edit_op.dart';
import 'projection_range_utils.dart';

class PredictiveInlineScanResult {
  const PredictiveInlineScanResult({
    required this.hiddenRanges,
    required this.scannedChars,
  });

  final List<TextRange> hiddenRanges;
  final int scannedChars;
}

class PredictiveEditRangeUtils {
  const PredictiveEditRangeUtils._();

  static const int defaultLocalInlineScanCharCap = 8192;

  static PredictiveInlineScanResult scanInlineMarkersNearEdit({
    required String targetText,
    required int editStart,
    required int editEnd,
    required List<TextRange> excludedRanges,
    int charCap = defaultLocalInlineScanCharCap,
  }) {
    if (targetText.isEmpty) {
      return const PredictiveInlineScanResult(
        hiddenRanges: <TextRange>[],
        scannedChars: 0,
      );
    }

    final safeStart = editStart.clamp(0, targetText.length).toInt();
    final safeEnd = editEnd.clamp(safeStart, targetText.length).toInt();

    // Scan the edited line(s) plus one adjacent line on each side.
    var segmentStart = ProjectionRangeUtils.lineStartForOffset(
      targetText,
      safeStart,
    );
    if (segmentStart > 0) {
      if (segmentStart <= 1) {
        segmentStart = 0;
      } else {
        final prevBreak = targetText.lastIndexOf('\n', segmentStart - 2);
        segmentStart = prevBreak == -1 ? 0 : prevBreak + 1;
      }
    }

    var segmentEnd = targetText.length;
    if (safeEnd < targetText.length) {
      segmentEnd = FencedCodeScanner.endOfLine(targetText, safeEnd);
      if (segmentEnd < targetText.length) {
        segmentEnd = FencedCodeScanner.endOfLine(targetText, segmentEnd);
      }
    }
    if (segmentEnd <= segmentStart) {
      return const PredictiveInlineScanResult(
        hiddenRanges: <TextRange>[],
        scannedChars: 0,
      );
    }

    final originalSegmentStart = segmentStart;
    final originalSegmentEnd = segmentEnd;
    final originalSegmentLength = originalSegmentEnd - originalSegmentStart;
    if (originalSegmentLength > charCap) {
      final editLength =
          (safeEnd - safeStart).clamp(1, targetText.length).toInt();
      var windowStart = safeStart - ((charCap - editLength) ~/ 2);
      var windowEnd = windowStart + charCap;
      if (windowStart < originalSegmentStart) {
        windowStart = originalSegmentStart;
        windowEnd = windowStart + charCap;
      }
      if (windowEnd > originalSegmentEnd) {
        windowEnd = originalSegmentEnd;
        windowStart = windowEnd - charCap;
      }
      segmentStart =
          windowStart.clamp(originalSegmentStart, originalSegmentEnd).toInt();
      segmentEnd = windowEnd.clamp(segmentStart, originalSegmentEnd).toInt();
    }
    final scannedChars = segmentEnd - segmentStart;

    final hidden = <TextRange>[];
    int cursor = segmentStart;
    for (final excluded in excludedRanges) {
      if (excluded.end <= segmentStart) continue;
      if (excluded.start >= segmentEnd) break;

      final scanEnd = excluded.start.clamp(segmentStart, segmentEnd).toInt();
      if (scanEnd > cursor) {
        hidden.addAll(_scanInlineMarkersInSlice(targetText, cursor, scanEnd));
      }
      cursor = excluded.end.clamp(segmentStart, segmentEnd).toInt();
      if (cursor >= segmentEnd) break;
    }

    if (cursor < segmentEnd) {
      hidden.addAll(_scanInlineMarkersInSlice(targetText, cursor, segmentEnd));
    }

    return PredictiveInlineScanResult(
      hiddenRanges: hidden,
      scannedChars: scannedChars,
    );
  }

  static List<TextRange> shiftAndVerifyRanges(
    List<TextRange> oldRanges,
    EditOp op,
    String newText,
  ) {
    final newRanges = <TextRange>[];
    final opStart = op.replacedRange.start;
    final opEnd = op.replacedRange.end;
    final delta = op.insertedText.length - (opEnd - opStart);

    for (final range in oldRanges) {
      if (!range.isValid) continue;

      int start = range.start;
      int end = range.end;

      if (end <= opStart) {
        // Upstream: keep as-is.
      } else if (start >= opEnd) {
        // Downstream: shift by delta.
        start += delta;
        end += delta;
      } else {
        // Overlap with mutation: drop.
        continue;
      }

      if (start < 0 || end > newText.length || start >= end) continue;

      newRanges.add(TextRange(start: start, end: end));
    }

    return newRanges;
  }

  static List<TextRange> _scanInlineMarkersInSlice(
    String text,
    int start,
    int end,
  ) {
    if (end <= start || end - start < 2) return const [];

    final slice = text.substring(start, end);
    final sliceResult = SovereignStyleScanner.scan(slice);
    final sliceHidden = SovereignStyleScanner.extractHiddenRanges(
      slice,
      sliceResult.runs,
    );

    return [
      for (final range in sliceHidden)
        TextRange(start: start + range.start, end: start + range.end),
    ];
  }
}
