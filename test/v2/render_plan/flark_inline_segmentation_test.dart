import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_core.dart';

void main() {
  FlarkRenderInlineRun run(int start, int end) {
    return FlarkRenderInlineRun(
      kind: FlarkMarkdownInlineKind.emphasis,
      type: 'emphasis',
      sourceRange: FlarkSourceRange(start, end),
      displayRange: FlarkSourceRange(start, end),
      styleToken: FlarkRenderTextStyleToken.emphasis,
    );
  }

  group('flarkSegmentInlineRuns', () {
    test('segments partition the window exactly', () {
      final runs = [run(2, 8), run(4, 6), run(7, 12)];
      final segments = flarkSegmentInlineRuns(start: 0, end: 10, runs: runs);
      expect(segments.first.start, 0);
      expect(segments.last.end, 10);
      for (var i = 0; i + 1 < segments.length; i += 1) {
        expect(segments[i].end, segments[i + 1].start,
            reason: 'segments must be adjacent');
        expect(segments[i].start, lessThan(segments[i].end));
      }
    });

    test('nested runs cover their inner segments (the boldbold class)', () {
      // ***x*** shape: an italic run over [0,4) and a bold run over [1,3).
      final italic = run(0, 4);
      final bold = run(1, 3);
      final segments = flarkSegmentInlineRuns(
        start: 0,
        end: 4,
        runs: [italic, bold],
      );
      expect(
        [for (final segment in segments) (segment.start, segment.end)],
        [(0, 1), (1, 3), (3, 4)],
      );
      expect(segments[0].coveringRuns, [italic]);
      expect(segments[1].coveringRuns, [italic, bold]);
      expect(segments[2].coveringRuns, [italic]);
    });

    test('a run covers exactly the segments within its range', () {
      final runs = [run(2, 8), run(4, 6)];
      final segments = flarkSegmentInlineRuns(start: 0, end: 10, runs: runs);
      for (final segment in segments) {
        for (final candidate in runs) {
          final covers = segment.coveringRuns.contains(candidate);
          final inside = candidate.displayRange.start <= segment.start &&
              candidate.displayRange.end >= segment.end;
          expect(covers, inside,
              reason: 'coverage must be exact containment');
        }
      }
    });

    test('zero-width runs cut at their position but cover nothing', () {
      // An empty-alt image projects to a zero-width run: it contributes a
      // boundary (so positional emitters can align to it) but can never
      // cover a segment — segments are non-empty by construction.
      final zero = run(3, 3);
      final segments = flarkSegmentInlineRuns(
        start: 0,
        end: 6,
        runs: [zero],
      );
      expect(
        [for (final segment in segments) (segment.start, segment.end)],
        [(0, 3), (3, 6)],
      );
      for (final segment in segments) {
        expect(segment.coveringRuns, isEmpty);
      }
    });

    test('runs extending past the window are clamped to it', () {
      final wide = run(0, 100);
      final segments = flarkSegmentInlineRuns(
        start: 5,
        end: 10,
        runs: [wide],
      );
      expect(
        [for (final segment in segments) (segment.start, segment.end)],
        [(5, 10)],
      );
      expect(segments.single.coveringRuns, [wide]);
    });

    test('an empty window yields no segments', () {
      expect(flarkSegmentInlineRuns(start: 4, end: 4, runs: [run(0, 8)]),
          isEmpty);
    });
  });
}
