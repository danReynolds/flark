import 'package:flutter/services.dart';

import '../../../models/line_index.dart';

class VerticalCaretMoveResult {
  const VerticalCaretMoveResult({
    required this.targetOffset,
    required this.preferredColumn,
  });

  final int targetOffset;
  final int preferredColumn;
}

class VerticalCaretNavigation {
  const VerticalCaretNavigation._();

  static VerticalCaretMoveResult? compute({
    required TextSelection selection,
    required String text,
    required LineIndex lineIndex,
    required bool forward,
    int? preferredColumn,
  }) {
    if (!selection.isValid) return null;
    final offset =
        selection.isCollapsed ? selection.baseOffset : selection.extentOffset;
    if (offset < 0 || offset > text.length) return null;
    if (lineIndex.lineCount <= 0) return null;

    final currentLine = lineIndex.lineAtOffset(offset);
    final delta = forward ? 1 : -1;
    final targetLine = (currentLine + delta).clamp(0, lineIndex.lineCount - 1);
    if (targetLine == currentLine) return null;

    final currentStart = lineIndex.offsetAtLine(currentLine);
    final currentColumn = (offset - currentStart).clamp(0, text.length);
    final desiredColumn = preferredColumn ?? currentColumn;

    final targetStart = lineIndex.offsetAtLine(targetLine);
    final targetEnd = (targetLine + 1 < lineIndex.lineCount)
        ? lineIndex.offsetAtLine(targetLine + 1)
        : text.length;
    var targetColumnMax = (targetEnd - targetStart).clamp(0, text.length);
    if (targetColumnMax > 0 &&
        text.codeUnitAt(targetStart + targetColumnMax - 1) == 10) {
      targetColumnMax -= 1;
    }

    final targetOffset =
        targetStart + desiredColumn.clamp(0, targetColumnMax).toInt();
    return VerticalCaretMoveResult(
      targetOffset: targetOffset,
      preferredColumn: desiredColumn,
    );
  }
}
