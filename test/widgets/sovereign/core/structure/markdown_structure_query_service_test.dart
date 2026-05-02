import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_structure_query_service.dart';
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
}
