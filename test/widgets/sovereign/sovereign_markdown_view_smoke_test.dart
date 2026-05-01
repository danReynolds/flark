import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

String _readViewPlainText(WidgetTester tester) {
  final richFinder = find.descendant(
    of: find.byType(SovereignMarkdownView),
    matching: find.byType(RichText),
  );
  final richText = tester.widget<RichText>(richFinder.first);
  return richText.text.toPlainText();
}

RenderParagraph _readViewParagraph(WidgetTester tester) {
  final richFinder = find.descendant(
    of: find.byType(SovereignMarkdownView),
    matching: find.byType(RichText),
  );
  return tester.renderObject<RenderParagraph>(richFinder.first);
}

Offset _globalOffsetForSubstring(WidgetTester tester, String substring) {
  final renderParagraph = _readViewParagraph(tester);
  final plainText = renderParagraph.text.toPlainText();
  final start = plainText.indexOf(substring);
  expect(start, greaterThanOrEqualTo(0));
  final boxes = renderParagraph.getBoxesForSelection(
    TextSelection(baseOffset: start, extentOffset: start + substring.length),
  );
  expect(boxes, isNotEmpty);
  return renderParagraph.localToGlobal(boxes.first.toRect().center);
}

Offset _globalOffsetNearRightEdgeOfSubstring(
  WidgetTester tester,
  String substring, {
  double dxOffset = 6,
}) {
  final renderParagraph = _readViewParagraph(tester);
  final plainText = renderParagraph.text.toPlainText();
  final start = plainText.indexOf(substring);
  expect(start, greaterThanOrEqualTo(0));
  final boxes = renderParagraph.getBoxesForSelection(
    TextSelection(baseOffset: start, extentOffset: start + substring.length),
  );
  expect(boxes, isNotEmpty);
  final box = boxes.last.toRect();
  return renderParagraph.localToGlobal(
    Offset(box.right + dxOffset, box.center.dy),
  );
}

void main() {
  group('SovereignMarkdownView', () {
    testWidgets('renders markdown text with selectable mode enabled', (
      tester,
    ) async {
      const markdown = '# Hello\nWorld';

      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(body: SovereignMarkdownView(markdown: markdown)),
        ),
      );

      expect(find.byType(SovereignMarkdownView), findsOneWidget);
      expect(find.byType(SelectionArea), findsOneWidget);
      expect(_readViewPlainText(tester), contains('Hello'));
      expect(_readViewPlainText(tester), contains('World'));
    });

    testWidgets('supports non-selectable mode', (tester) async {
      const markdown = 'Plain text body';

      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(markdown: markdown, selectable: false),
          ),
        ),
      );

      expect(find.byType(SelectionArea), findsNothing);
      expect(_readViewPlainText(tester), contains(markdown));
    });

    testWidgets('opens markdown links directly when overlay is disabled', (
      tester,
    ) async {
      String? openedUrl;
      const markdown = '[Docs](https://example.com)';

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: markdown,
              selectable: false,
              onOpenLink: (url) async => openedUrl = url,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byType(RichText).first);
      await tester.pump();

      expect(openedUrl, 'https://example.com');
    });

    testWidgets('does not trigger link open after a drag gesture', (
      tester,
    ) async {
      String? openedUrl;
      const markdown = '[Docs](https://example.com)';

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: markdown,
              selectable: false,
              onOpenLink: (url) async => openedUrl = url,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final richFinder = find.byType(RichText).first;
      final gesture = await tester.startGesture(tester.getCenter(richFinder));
      await gesture.moveBy(const Offset(36, 0));
      await gesture.up();
      await tester.pumpAndSettle();

      expect(openedUrl, isNull);
    });

    testWidgets('shows overlay actions and invokes edit callback', (
      tester,
    ) async {
      String? editedLabel;
      String? editedUrl;
      bool? editedIsImage;
      const markdown = '[Docs](https://example.com)';

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: markdown,
              selectable: false,
              showLinkActionsOverlay: true,
              onOpenLink: (_) async {},
              onEditInlineTarget: (context, label, url, isImage) async {
                editedLabel = label;
                editedUrl = url;
                editedIsImage = isImage;
              },
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byType(RichText).first);
      await tester.pumpAndSettle();

      expect(
        find.byKey(const Key('SovereignLinkActionsOverlay')),
        findsOneWidget,
      );
      expect(find.text('Open'), findsOneWidget);
      expect(find.text('Copy'), findsOneWidget);
      expect(find.text('Edit'), findsOneWidget);

      await tester.tap(find.text('Edit'));
      await tester.pumpAndSettle();

      expect(editedLabel, 'Docs');
      expect(editedUrl, 'https://example.com');
      expect(editedIsImage, isFalse);
    });

    testWidgets('does not show overlay when tapping non-link text', (
      tester,
    ) async {
      String? openedUrl;
      const markdown = '[Docs](https://example.com) trailing text';

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: markdown,
              selectable: false,
              showLinkActionsOverlay: true,
              onOpenLink: (url) async => openedUrl = url,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final tapOffset = _globalOffsetForSubstring(tester, 'trailing');
      await tester.tapAt(tapOffset);
      await tester.pumpAndSettle();

      expect(openedUrl, isNull);
      expect(
        find.byKey(const Key('SovereignLinkActionsOverlay')),
        findsNothing,
      );
    });

    testWidgets(
      'does not show overlay when tapping just outside link boundary',
      (tester) async {
        String? openedUrl;
        const markdown = '[Docs](https://example.com) trailing text';

        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: SovereignMarkdownView(
                markdown: markdown,
                selectable: false,
                showLinkActionsOverlay: true,
                onOpenLink: (url) async => openedUrl = url,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();

        final tapOffset = _globalOffsetNearRightEdgeOfSubstring(tester, 'Docs');
        await tester.tapAt(tapOffset);
        await tester.pumpAndSettle();

        expect(openedUrl, isNull);
        expect(
          find.byKey(const Key('SovereignLinkActionsOverlay')),
          findsNothing,
        );
      },
    );

    testWidgets('dismisses overlay when action configuration changes', (
      tester,
    ) async {
      const markdown = '[Docs](https://example.com)';

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: markdown,
              selectable: false,
              showLinkActionsOverlay: true,
              onOpenLink: (_) async {},
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byType(RichText).first);
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('SovereignLinkActionsOverlay')),
        findsOneWidget,
      );

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: markdown,
              selectable: false,
              showLinkActionsOverlay: false,
              onOpenLink: (_) async {},
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('SovereignLinkActionsOverlay')),
        findsNothing,
      );
    });

    testWidgets('shows image overlay actions from markdown image target', (
      tester,
    ) async {
      const markdown = '![hero](https://example.com/image.png)';

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: markdown,
              selectable: false,
              showLinkActionsOverlay: true,
              onOpenLink: (_) async {},
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.tap(find.byType(RichText).first);
      await tester.pumpAndSettle();

      expect(
        find.byKey(const Key('SovereignInlineImagePreview')),
        findsOneWidget,
      );
      expect(find.text('Open'), findsOneWidget);
      expect(find.text('Copy'), findsOneWidget);
    });
  });
}
