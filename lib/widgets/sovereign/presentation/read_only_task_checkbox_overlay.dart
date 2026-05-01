import 'dart:math' as math;

import 'package:flutter/rendering.dart';

class SovereignReadOnlyTaskCheckboxVisualData {
  final Rect markerRect;
  final bool isChecked;

  const SovereignReadOnlyTaskCheckboxVisualData({
    required this.markerRect,
    required this.isChecked,
  });
}

class SovereignReadOnlyTaskCheckboxVisualSnapshot {
  final int markerCount;
  final List<SovereignReadOnlyTaskCheckboxVisualData> visuals;
  final int signature;

  const SovereignReadOnlyTaskCheckboxVisualSnapshot({
    required this.markerCount,
    required this.visuals,
    required this.signature,
  });

  static const SovereignReadOnlyTaskCheckboxVisualSnapshot empty =
      SovereignReadOnlyTaskCheckboxVisualSnapshot(
    markerCount: 0,
    visuals: <SovereignReadOnlyTaskCheckboxVisualData>[],
    signature: 0,
  );
}

typedef SovereignTaskCheckboxRangeResolver = TextRange? Function(int line);

class SovereignReadOnlyTaskCheckboxOverlay {
  static SovereignReadOnlyTaskCheckboxVisualSnapshot computeSnapshot({
    required String text,
    required int lineCount,
    required RenderParagraph renderObject,
    required SovereignTaskCheckboxRangeResolver markerRangeForLine,
    required SovereignTaskCheckboxRangeResolver visualRangeForLine,
  }) {
    if (text.isEmpty || lineCount <= 0) {
      return SovereignReadOnlyTaskCheckboxVisualSnapshot.empty;
    }

    var markerCount = 0;
    final out = <SovereignReadOnlyTaskCheckboxVisualData>[];
    for (var line = 0; line < lineCount; line++) {
      final checkboxRange = markerRangeForLine(line);
      if (checkboxRange == null) continue;
      markerCount++;
      final visualRange = visualRangeForLine(line) ?? checkboxRange;
      if (visualRange.start < 0 ||
          visualRange.end > text.length ||
          visualRange.end <= visualRange.start) {
        continue;
      }

      final boxes = renderObject.getBoxesForSelection(
        TextSelection(
          baseOffset: visualRange.start,
          extentOffset: visualRange.end,
        ),
      );
      if (boxes.isEmpty) continue;

      var left = double.infinity;
      var top = double.infinity;
      var right = double.negativeInfinity;
      var bottom = double.negativeInfinity;
      for (final box in boxes) {
        left = math.min(left, box.left);
        top = math.min(top, box.top);
        right = math.max(right, box.right);
        bottom = math.max(bottom, box.bottom);
      }
      if (!left.isFinite ||
          !top.isFinite ||
          !right.isFinite ||
          !bottom.isFinite ||
          right <= left ||
          bottom <= top) {
        continue;
      }

      final markerText = text.substring(checkboxRange.start, checkboxRange.end);
      final isChecked = markerText.length >= 2 &&
          (markerText.codeUnitAt(1) == 120 || markerText.codeUnitAt(1) == 88);
      out.add(
        SovereignReadOnlyTaskCheckboxVisualData(
          markerRect: Rect.fromLTRB(left, top, right, bottom),
          isChecked: isChecked,
        ),
      );
    }

    var signature = markerCount;
    for (final visual in out) {
      signature = Object.hash(
        signature,
        visual.isChecked ? 1 : 0,
        visual.markerRect.left.round(),
        visual.markerRect.top.round(),
        visual.markerRect.right.round(),
        visual.markerRect.bottom.round(),
      );
    }

    return SovereignReadOnlyTaskCheckboxVisualSnapshot(
      markerCount: markerCount,
      visuals: out,
      signature: signature,
    );
  }
}
