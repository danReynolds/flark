import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/src/widgets/sovereign/logic/markdown_marker_grammar.dart';

void main() {
  group('MarkdownMarkerGrammar', () {
    test('v1 list markers allow -/* and ordered dot only', () {
      expect(
        MarkdownMarkerGrammar.matchListMarker(
          '- item',
          dialect: MarkdownMarkerDialect.sovereignV1,
        ),
        isNotNull,
      );
      expect(
        MarkdownMarkerGrammar.matchListMarker(
          '* item',
          dialect: MarkdownMarkerDialect.sovereignV1,
        ),
        isNotNull,
      );
      expect(
        MarkdownMarkerGrammar.matchListMarker(
          '1. item',
          dialect: MarkdownMarkerDialect.sovereignV1,
        ),
        isNotNull,
      );

      expect(
        MarkdownMarkerGrammar.matchListMarker(
          '+ item',
          dialect: MarkdownMarkerDialect.sovereignV1,
        ),
        isNull,
      );
      expect(
        MarkdownMarkerGrammar.matchListMarker(
          '1) item',
          dialect: MarkdownMarkerDialect.sovereignV1,
        ),
        isNull,
      );
    });

    test('commonmark list markers allow + and ordered ) with indent <= 3', () {
      final plus = MarkdownMarkerGrammar.matchListMarker(
        '   + item',
        dialect: MarkdownMarkerDialect.commonMark,
      );
      expect(plus, isNotNull);
      expect(plus!.ordered, isFalse);

      final ordered = MarkdownMarkerGrammar.matchListMarker(
        '  3) item',
        dialect: MarkdownMarkerDialect.commonMark,
      );
      expect(ordered, isNotNull);
      expect(ordered!.ordered, isTrue);

      expect(
        MarkdownMarkerGrammar.matchListMarker(
          '    - item',
          dialect: MarkdownMarkerDialect.commonMark,
        ),
        isNull,
      );
    });

    test('blockquote marker differs by dialect', () {
      expect(
        MarkdownMarkerGrammar.matchBlockquoteMarkerEnd(
          '> quote',
          dialect: MarkdownMarkerDialect.sovereignV1,
        ),
        2,
      );
      expect(
        MarkdownMarkerGrammar.matchBlockquoteMarkerEnd(
          '>quote',
          dialect: MarkdownMarkerDialect.sovereignV1,
        ),
        isNull,
      );

      expect(
        MarkdownMarkerGrammar.matchBlockquoteMarkerEnd(
          '>quote',
          dialect: MarkdownMarkerDialect.commonMark,
        ),
        1,
      );
    });

    test('finds nested quoted list marker range', () {
      const line = '> > 2. item';
      final range =
          MarkdownMarkerGrammar.listMarkerRangeInLineAllowingQuotePrefix(
        line,
        from: 0,
        dialect: MarkdownMarkerDialect.commonMark,
      );
      expect(range, const TextRange(start: 4, end: 7));
    });

    test('matches thematic break marker range in commonmark mode', () {
      final range = MarkdownMarkerGrammar.matchThematicBreakMarkerRange(
        '  - - - ',
        dialect: MarkdownMarkerDialect.commonMark,
      );
      expect(range, const TextRange(start: 2, end: 8));
    });

    test('matches reference definition marker prefix in commonmark mode', () {
      final range = MarkdownMarkerGrammar.matchReferenceDefinitionMarkerRange(
        ' [ref-id]:   https://example.com',
        dialect: MarkdownMarkerDialect.commonMark,
      );
      expect(range, const TextRange(start: 1, end: 13));
    });
  });
}
