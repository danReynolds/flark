import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/markdown/source/sovereign_markdown_fenced_code_scanner.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  group('SovereignMarkdownFencedCodeScanner', () {
    test('extracts opening context, language, and editable body range', () {
      const markdown = '```dart meta\nfoo\n```\n';

      final context = SovereignMarkdownFencedCodeScanner.contextForOpeningLine(
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
        const SovereignSourceRange(13, 16),
      );
    });

    test('keeps trailing EOF newlines editable in unclosed fences', () {
      const markdown = '```dart\nfoo\n';

      final context = SovereignMarkdownFencedCodeScanner.contextForOpeningLine(
        markdown,
        0,
      );

      expect(context, isNotNull);
      expect(context!.isClosed, isFalse);
      expect(
        context.bodyContentRange(markdown),
        const SovereignSourceRange(8, 12),
      );
    });

    test('supports tilde fences and longer closing fence markers', () {
      const markdown = '~~~js\nx\n~~~~\n';

      final context = SovereignMarkdownFencedCodeScanner.contextAt(
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

      final context = SovereignMarkdownFencedCodeScanner.contextAt(
        markdown,
        markdown.indexOf('ok'),
      );

      expect(context, isNotNull);
      expect(context!.closingLineStart, markdown.lastIndexOf('```'));
    });

    test('rejects backtick info strings that contain backticks', () {
      final line = SovereignMarkdownFencedCodeScanner.fenceLine('``` `bad`');

      expect(line, isNull);
    });

    test('returns no body context on opening or closing fence lines', () {
      const markdown = '```dart\nfoo\n```\n';

      expect(SovereignMarkdownFencedCodeScanner.contextAt(markdown, 0), isNull);
      expect(
        SovereignMarkdownFencedCodeScanner.contextAt(
          markdown,
          markdown.indexOf('\n```') + 1,
        ),
        isNull,
      );
    });
  });
}
