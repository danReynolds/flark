import '../../core/selection/sovereign_selection.dart';
import '../../core/state/sovereign_editor_state.dart';

final class SovereignSelectedLine {
  const SovereignSelectedLine({
    required this.index,
    required this.start,
    required this.end,
    required this.text,
  });

  final int index;
  final int start;
  final int end;
  final String text;
}

List<SovereignSelectedLine> selectedMarkdownLines(SovereignEditorState state) {
  final selection = state.selection;
  final buffer = state.document.buffer;
  final startLine = buffer.lineAtOffset(selection.start);
  final endOffset = _inclusiveSelectionEnd(selection);
  final endLine = buffer.lineAtOffset(endOffset);
  final lines = <SovereignSelectedLine>[];

  for (var lineIndex = startLine; lineIndex <= endLine; lineIndex += 1) {
    final start = buffer.lineStart(lineIndex);
    final end = buffer.lineEnd(lineIndex);
    lines.add(
      SovereignSelectedLine(
        index: lineIndex,
        start: start,
        end: end,
        text: state.markdown.substring(start, end),
      ),
    );
  }

  return lines;
}

int _inclusiveSelectionEnd(SovereignSelection selection) {
  if (selection.isCollapsed) return selection.extentOffset;
  return selection.end - 1;
}
