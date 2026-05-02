import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_structure_query_service.dart';
import 'package:sovereign_editor/src/widgets/sovereign/logic/sovereign_geometry_scanner.dart';
import 'package:sovereign_editor/widgets/sovereign/models/line_index.dart';

void main() {
  group('MarkdownStructureQueryService.taskCheckboxLineInfoForLine', () {
    const queries = MarkdownStructureQueryService();

    test('finds unordered task checkbox ranges', () {
      const text = '- [ ] todo';

      final info = queries.taskCheckboxLineInfoForLine(
        text: text,
        lineIndex: LineIndex.fromText(text),
        line: 0,
      );

      expect(info, isNotNull);
      expect(info!.markerStart, 0);
      expect(info.taskStart, 2);
      expect(info.contentStart, 6);
      expect(info.isOrdered, isFalse);
    });

    test('finds ordered task checkbox ranges', () {
      const text = '12. [x] done';

      final info = queries.taskCheckboxLineInfoForLine(
        text: text,
        lineIndex: LineIndex.fromText(text),
        line: 0,
      );

      expect(info, isNotNull);
      expect(info!.markerStart, 0);
      expect(info.taskStart, 4);
      expect(info.contentStart, 8);
      expect(info.isOrdered, isTrue);
    });

    test('supports quoted task list markers', () {
      const text = '> - [ ] todo';

      final info = queries.taskCheckboxLineInfoForLine(
        text: text,
        lineIndex: LineIndex.fromText(text),
        line: 0,
      );

      expect(info, isNotNull);
      expect(info!.markerStart, 2);
      expect(info.taskStart, 4);
      expect(info.contentStart, 8);
      expect(info.isOrdered, isFalse);
    });

    test('returns null for non-task lines and invalid lines', () {
      const text = '- plain\nbody';
      final lineIndex = LineIndex.fromText(text);

      expect(
        queries.taskCheckboxLineInfoForLine(
          text: text,
          lineIndex: lineIndex,
          line: 0,
        ),
        isNull,
      );
      expect(
        queries.taskCheckboxLineInfoForLine(
          text: text,
          lineIndex: lineIndex,
          line: 2,
        ),
        isNull,
      );
    });
  });

  group('MarkdownStructureQueryService fence body queries', () {
    const queries = MarkdownStructureQueryService();

    test('identifies carets inside fenced-code body only', () {
      const text = 'before\n```\ncode\n```\nafter';
      final lineIndex = LineIndex.fromText(text);
      final geometry = const SovereignGeometryScanner().scan(text, lineIndex);

      expect(
        queries.isCaretInFenceBody(
          text: text,
          caret: text.indexOf('code'),
          lineIndex: lineIndex,
          geometry: geometry,
        ),
        isTrue,
      );
      expect(
        queries.isCaretInFenceBody(
          text: text,
          caret: text.indexOf('```'),
          lineIndex: lineIndex,
          geometry: geometry,
        ),
        isFalse,
      );
      expect(
        queries.isCaretInFenceBody(
          text: text,
          caret: text.lastIndexOf('```'),
          lineIndex: lineIndex,
          geometry: geometry,
        ),
        isFalse,
      );
    });

    test('requires ranges to stay within one fenced-code body', () {
      const text = '```\none\n```\ntext\n```\ntwo\n```';
      final lineIndex = LineIndex.fromText(text);
      final geometry = const SovereignGeometryScanner().scan(text, lineIndex);
      final oneStart = text.indexOf('one');
      final twoStart = text.indexOf('two');

      expect(
        queries.isRangeInFenceBody(
          text: text,
          start: oneStart,
          end: oneStart + 3,
          lineIndex: lineIndex,
          geometry: geometry,
        ),
        isTrue,
      );
      expect(
        queries.isRangeInFenceBody(
          text: text,
          start: oneStart,
          end: twoStart + 3,
          lineIndex: lineIndex,
          geometry: geometry,
        ),
        isFalse,
      );
    });
  });
}
