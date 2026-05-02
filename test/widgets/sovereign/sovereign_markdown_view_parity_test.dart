import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/painters/tier1_painter.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

String _plainText(WidgetTester tester) {
  final richFinder = find.descendant(
    of: find.byType(SovereignMarkdownView),
    matching: find.byType(RichText),
  );
  final richText = tester.widget<RichText>(richFinder.first);
  return richText.text.toPlainText();
}

Future<void> _pumpMarkdownView(WidgetTester tester, String markdown) {
  return tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SovereignMarkdownView(markdown: markdown, selectable: false),
      ),
    ),
  );
}

void main() {
  group('SovereignMarkdownView parity', () {
    testWidgets(
      'renders core markdown semantics and suppresses link/image URLs in read text',
      (tester) async {
        const markdown = '''
# Heading
> quoted

- bullet
1. ordered
- [ ] todo
- [x] done

Inline [docs](https://example.com) and ![hero](https://example.com/hero.png)

```dart
final x = 1;
```
''';

        await _pumpMarkdownView(tester, markdown);
        await tester.pumpAndSettle();

        final text = _plainText(tester);
        expect(text, contains('Heading'));
        expect(text, contains('quoted'));
        expect(text, contains('bullet'));
        expect(text, contains('ordered'));
        expect(text, contains('todo'));
        expect(text, contains('done'));
        expect(text, contains('docs'));
        expect(text, contains('hero'));
        expect(text, contains('final x = 1;'));

        expect(text, isNot(contains('https://example.com')));
      },
    );

    testWidgets('paints quote rails and fenced code backgrounds', (
      tester,
    ) async {
      const markdown = '> quoted\n\n```dart\nfinal x = 1;\n```';

      await _pumpMarkdownView(tester, markdown);
      await tester.pumpAndSettle();

      final paintFinder = find.byWidgetPredicate(
        (widget) => widget is CustomPaint && widget.painter is Tier1Painter,
      );
      expect(paintFinder, findsOneWidget);

      final paintWidget = tester.widget<CustomPaint>(paintFinder);
      final painter = paintWidget.painter! as Tier1Painter;
      expect(painter.geometry.quoteBlocks, isNotEmpty);
      expect(painter.geometry.codeBlocks, isNotEmpty);
    });

    testWidgets('hides thematic break markers in read text', (tester) async {
      const markdown = 'before\n\n---\n\nafter';

      await _pumpMarkdownView(tester, markdown);
      await tester.pumpAndSettle();

      final text = _plainText(tester);
      expect(text, contains('before'));
      expect(text, contains('after'));
      expect(text, isNot(contains('---')));
    });

    testWidgets('keeps raw HTML literal in read text', (tester) async {
      const markdown = '''
<script>alert("x")</script>

Inline <span onclick="boom()">label</span>.
''';

      await _pumpMarkdownView(tester, markdown);
      await tester.pumpAndSettle();

      final text = _plainText(tester);
      expect(text, contains('<script>alert("x")</script>'));
      expect(text, contains('<span onclick="boom()">label</span>'));
    });

    testWidgets('uses forced strut typography for read-only parity', (
      tester,
    ) async {
      const textStyle = TextStyle(
        fontFamily: 'Ahem',
        fontSize: 19,
        height: 1.7,
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: 'Parity line',
              selectable: false,
              theme: const SovereignEditorThemeData(textStyle: textStyle),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final richFinder = find.descendant(
        of: find.byType(SovereignMarkdownView),
        matching: find.byType(RichText),
      );
      final richText = tester.widget<RichText>(richFinder.first);
      final strut = richText.strutStyle;
      expect(strut, isNotNull);
      expect(strut!.forceStrutHeight, isTrue);
      expect(strut.fontSize, equals(19));
      expect(strut.height, equals(1.7));
      expect(strut.fontFamily, equals('Ahem'));
    });
  });
}
