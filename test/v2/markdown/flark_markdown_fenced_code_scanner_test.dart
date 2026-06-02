import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/markdown/source/flark_markdown_fenced_code_scanner.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  group('FlarkMarkdownFencedCodeScanner', () {
    test('extracts opening context, language, and editable body range', () {
      const markdown = '```dart meta\nfoo\n```\n';

      final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
        markdown,
        0,
      );

      expect(context, isNotNull);
      expect(context!.infoString, 'dart meta');
      expect(context.language, 'dart');
      expect(context.marker, '`');
      expect(context.markerLength, 3);
      expect(context.bodyStart, 13);
      expect(context.closingLineStart, 17);
      expect(
        context.bodyContentRange(markdown),
        const FlarkSourceRange(13, 16),
      );
    });

    test('keeps trailing EOF newlines editable in unclosed fences', () {
      const markdown = '```dart\nfoo\n';

      final context = FlarkMarkdownFencedCodeScanner.contextForOpeningLine(
        markdown,
        0,
      );

      expect(context, isNotNull);
      expect(context!.isClosed, isFalse);
      expect(context.bodyContentRange(markdown), const FlarkSourceRange(8, 12));
    });

    test('supports tilde fences and longer closing fence markers', () {
      const markdown = '~~~js\nx\n~~~~\n';

      final context = FlarkMarkdownFencedCodeScanner.contextAt(
        markdown,
        markdown.indexOf('x'),
      );

      expect(context, isNotNull);
      expect(context!.marker, '~');
      expect(context.markerLength, 3);
      expect(context.language, 'js');
      expect(context.closingLineStart, markdown.indexOf('~~~~'));
      expect(context.isClosed, isTrue);
    });

    test('does not treat info-string fence lines as closers', () {
      const markdown = '```\n```not close\nok\n```\n';

      final context = FlarkMarkdownFencedCodeScanner.contextAt(
        markdown,
        markdown.indexOf('ok'),
      );

      expect(context, isNotNull);
      expect(context!.closingLineStart, markdown.lastIndexOf('```'));
    });

    test('rejects backtick info strings that contain backticks', () {
      final line = FlarkMarkdownFencedCodeScanner.fenceLine('``` `bad`');

      expect(line, isNull);
    });

    test('returns no body context on opening or closing fence lines', () {
      const markdown = '```dart\nfoo\n```\n';

      expect(FlarkMarkdownFencedCodeScanner.contextAt(markdown, 0), isNull);
      expect(
        FlarkMarkdownFencedCodeScanner.contextAt(
          markdown,
          markdown.indexOf('\n```') + 1,
        ),
        isNull,
      );
    });
  });
}
