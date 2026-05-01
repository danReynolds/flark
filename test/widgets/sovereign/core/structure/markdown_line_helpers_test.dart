import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/core/structure/markdown_line_helpers.dart';

void main() {
  group('MarkdownLineHelpers.markdownLinkOrImageTailRangeAt', () {
    test('finds simple inline link tail range', () {
      const text = '[label](https://example.com)';
      final start = text.indexOf('](');

      final range = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
        text,
        start,
      );

      expect(range, isNotNull);
      expect(text.substring(range!.start, range.end), '](https://example.com)');
    });

    test('supports nested parentheses in URL', () {
      const text = '[label](https://example.com/path_(v1))';
      final start = text.indexOf('](');

      final range = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
        text,
        start,
      );

      expect(range, isNotNull);
      expect(
        text.substring(range!.start, range.end),
        '](https://example.com/path_(v1))',
      );
    });

    test('supports escaped closing parenthesis in URL', () {
      const text = r'[label](https://example.com/\))';
      final start = text.indexOf('](');

      final range = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
        text,
        start,
      );

      expect(range, isNotNull);
      expect(
        text.substring(range!.start, range.end),
        r'](https://example.com/\))',
      );
    });

    test('supports inline link titles containing parentheses', () {
      const text = '[label](https://example.com/path_(v1) "A (title)")';
      final start = text.indexOf('](');

      final range = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
        text,
        start,
      );

      expect(range, isNotNull);
      expect(
        text.substring(range!.start, range.end),
        '](https://example.com/path_(v1) "A (title)")',
      );
    });

    test('supports nested-bracket labels with inline link tail', () {
      const text = '[a [b]](https://example.com)';
      final start = text.lastIndexOf('](');

      final range = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
        text,
        start,
      );

      expect(range, isNotNull);
      expect(text.substring(range!.start, range.end), '](https://example.com)');
    });

    test('returns null for reference-style label boundary', () {
      const text = '[label][ref]';
      final start = text.indexOf('][');

      final range = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
        text,
        start,
      );

      expect(range, isNull);
    });

    test('returns null when tail crosses a newline', () {
      const text = '[label](https://example.com\nbroken)';
      final start = text.indexOf('](');

      final range = MarkdownLineHelpers.markdownLinkOrImageTailRangeAt(
        text,
        start,
      );

      expect(range, isNull);
    });
  });

  group('MarkdownLineHelpers.selectionCenteredEmptyInlineRanges', () {
    test('finds both sides of an empty backtick wrapper at caret', () {
      const value = TextEditingValue(
        text: '``',
        selection: TextSelection.collapsed(offset: 1),
      );

      final ranges = MarkdownLineHelpers.selectionCenteredEmptyInlineRanges(
        value,
      );

      expect(
        ranges,
        equals(const <TextRange>[
          TextRange(start: 0, end: 1),
          TextRange(start: 1, end: 2),
        ]),
      );
    });
  });
}
