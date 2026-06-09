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

    test('rejects tab-indented fence markers like CommonMark', () {
      // A tab advances to the next 4-column stop, so any leading tab puts
      // the marker at column >= 4: indented code, not a fence. Comrak agrees;
      // accepting these would manufacture fences the parser never produced.
      expect(FlarkMarkdownFencedCodeScanner.fenceLine('\t```'), isNull);
      expect(FlarkMarkdownFencedCodeScanner.fenceLine(' \t```'), isNull);
      expect(FlarkMarkdownFencedCodeScanner.fenceLine('   ```'), isNotNull);
      expect(FlarkMarkdownFencedCodeScanner.fenceLine('    ```'), isNull);
    });
  });

  group('FlarkMarkdownFenceLayout', () {
    test('scans every fence region in one pass', () {
      const markdown = 'before\n```dart\nbody\n```\nmiddle\n~~~\nopen forever';
      final layout = FlarkMarkdownFenceLayout.scan(markdown);

      expect(layout.contexts, hasLength(2));
      final closed = layout.contexts.first;
      expect(closed.language, 'dart');
      expect(closed.isClosed, isTrue);
      final open = layout.contexts.last;
      expect(open.marker, '~');
      expect(open.isClosed, isFalse);
    });

    test('contextAt matches the one-shot scanner contract', () {
      const markdown = 'a\n```\nbody\n```\nafter\n```rust\ntail';
      final layout = FlarkMarkdownFenceLayout.scan(markdown);

      for (var caret = 0; caret <= markdown.length; caret++) {
        expect(
          layout.contextAt(caret)?.openingLineStart,
          FlarkMarkdownFencedCodeScanner.contextAt(
            markdown,
            caret,
          )?.openingLineStart,
          reason: 'caret $caret',
        );
      }
    });

    test('treats fence-looking lines inside a body as body text', () {
      const markdown = '````\n```dart\nfoo\n```\n````\n';
      final layout = FlarkMarkdownFenceLayout.scan(markdown);

      // The 3-backtick lines live inside the 4-backtick fence.
      expect(layout.contexts, hasLength(1));
      expect(layout.contexts.single.markerLength, 4);
      expect(layout.openerAt(markdown.indexOf('```dart')), isNull);
      expect(
        layout.lineIsInsideEarlierFence(markdown.indexOf('```dart')),
        isTrue,
      );
    });

    test('finds closed and empty fences by boundary offsets', () {
      const markdown = '```dart\n```\nafter';
      final layout = FlarkMarkdownFenceLayout.scan(markdown);
      final closingLineEnd = markdown.indexOf('\nafter');

      expect(layout.closedFenceEndingAt(closingLineEnd)?.language, 'dart');
      expect(layout.closedFenceEndingAt(closingLineEnd + 1)?.language, 'dart');
      expect(
        layout
            .emptyClosedFenceAtBodyStart('```dart\n'.length)
            ?.openingLineStart,
        0,
      );
      expect(layout.emptyClosedFenceAtBodyStart(0), isNull);
    });
  });
}
