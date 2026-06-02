import 'package:flutter_test/flutter_test.dart';

import '../support/sovereign_markdown_fixture_loader.dart';

void main() {
  group('Flark markdown fixture loader', () {
    test('loads curated CommonMark fixtures', () {
      final fixtures = loadFlarkMarkdownFixtureLane(
        'test/fixtures/commonmark/core_cases.json',
      );

      expect(fixtures, isNotEmpty);
      expect(
        fixtures.map((fixture) => fixture.id),
        contains('heading_emphasis_inline_code'),
      );
      expect(
        fixtures
            .firstWhere(
              (fixture) => fixture.id == 'heading_emphasis_inline_code',
            )
            .markdown,
        contains('# Title'),
      );
    });

    test('loads curated GFM fixtures with extension metadata', () {
      final fixtures = loadFlarkMarkdownFixtureLane(
        'test/fixtures/commonmark/gfm_cases.json',
      );

      expect(
        fixtures.map((fixture) => fixture.category).toSet(),
        containsAll({'autolink', 'strikethrough', 'table', 'task_list'}),
      );
      expect(
        fixtures.every((fixture) => fixture.requiresGfmDifference),
        isTrue,
      );

      final strike = fixtures.firstWhere(
        (fixture) => fixture.id == 'strikethrough_extension',
      );
      expect(strike.requiresGfmDifference, isTrue);
      expect(strike.expectedContains, contains('<del>gone</del>'));
      expect(strike.expectedNotContains, contains('~~gone~~'));

      final table = fixtures.firstWhere(
        (fixture) => fixture.id == 'table_extension',
      );
      expect(table.expectedContains, contains('<table>'));
      expect(table.expectedContains, contains('<th align="right">Value</th>'));
    });
  });
}
