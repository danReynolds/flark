part of 'sovereign_text_renderer.dart';

class _SovereignBlockStyleRunsBuilder {
  const _SovereignBlockStyleRunsBuilder();

  List<_BlockStyleRun> buildRuns(String fullText, BlockTree tree) {
    if (fullText.isEmpty || tree.blocks.isEmpty) return const [];

    final runs = <_BlockStyleRun>[];
    for (final block in tree.blocks) {
      if (block.start < 0 ||
          block.end > fullText.length ||
          block.start >= block.end) {
        continue;
      }

      switch (block.type) {
        case BlockType.header:
          final level = (block.payload?['level'] as int?)?.clamp(1, 6) ?? 1;
          runs.add(
            _BlockStyleRun(
              start: block.start,
              end: block.end,
              kind: _BlockStyleKind.header,
              headerLevel: level,
            ),
          );
          break;
        case BlockType.blockquote:
          runs.add(
            _BlockStyleRun(
              start: block.start,
              end: block.end,
              kind: _BlockStyleKind.blockquote,
            ),
          );
          break;
        case BlockType.thematicBreak:
          runs.add(
            _BlockStyleRun(
              start: block.start,
              end: block.end,
              kind: _BlockStyleKind.thematicBreak,
            ),
          );
          break;
        case BlockType.table:
          runs.add(
            _BlockStyleRun(
              start: block.start,
              end: block.end,
              kind: _BlockStyleKind.table,
            ),
          );
          break;
        case BlockType.unorderedList:
          _appendTaskCheckedRunsForListBlock(fullText, block, runs);
          break;
        case BlockType.paragraph:
        case BlockType.fencedCode:
        case BlockType.orderedList:
          break;
      }
    }

    if (runs.isEmpty) return const [];
    runs.sort((a, b) => a.start.compareTo(b.start));
    return runs;
  }

  void _appendTaskCheckedRunsForListBlock(
    String text,
    BlockNode block,
    List<_BlockStyleRun> out,
  ) {
    var lineStart = block.start;
    while (lineStart < block.end) {
      final lineEndWithBreak = FencedCodeScanner.endOfLine(text, lineStart);
      final boundedLineEndWithBreak = lineEndWithBreak.clamp(
        lineStart,
        block.end,
      );
      final lineEnd = (boundedLineEndWithBreak > lineStart &&
              text.codeUnitAt(boundedLineEndWithBreak - 1) == 10)
          ? boundedLineEndWithBreak - 1
          : boundedLineEndWithBreak;

      if (lineEnd <= lineStart) {
        lineStart = boundedLineEndWithBreak > lineStart
            ? boundedLineEndWithBreak
            : lineStart + 1;
        continue;
      }

      final bulletLen = _SovereignRendererUtils.unorderedListMarkerLength(
        text,
        lineStart,
        lineEnd,
      );
      if (bulletLen > 0) {
        final taskInfo = _SovereignRendererUtils.taskMarkerInfo(
          text,
          lineStart + bulletLen,
          lineEnd,
        );
        if (taskInfo != null && taskInfo.isChecked) {
          final contentStart = taskInfo.contentStart.clamp(lineStart, lineEnd);
          if (contentStart < lineEnd) {
            out.add(
              _BlockStyleRun(
                start: contentStart,
                end: lineEnd,
                kind: _BlockStyleKind.taskChecked,
              ),
            );
          }
        }
      }

      lineStart = boundedLineEndWithBreak > lineStart
          ? boundedLineEndWithBreak
          : lineStart + 1;
    }
  }
}
