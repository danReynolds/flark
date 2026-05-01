import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/painters/tier1_painter.dart';
import 'package:sovereign_editor/sovereign_editor.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

String _readViewPlainText(WidgetTester tester) {
  final richFinder = find.descendant(
    of: find.byType(SovereignMarkdownView),
    matching: find.byType(RichText),
  );
  final richText = tester.widget<RichText>(richFinder.first);
  return richText.text.toPlainText();
}

void main() {
  group('SovereignMarkdownView rendering', () {
    testWidgets('hides markdown heading marker and renders heading text', (
      tester,
    ) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(
              markdown: '# Heading one',
              selectable: false,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final plainText = _readViewPlainText(tester);
      expect(plainText, contains('Heading one'));
    });

    testWidgets('paints quote rails and fenced code backgrounds', (
      tester,
    ) async {
      const markdown = '> quoted\n\n```dart\nfinal x = 1;\n```';

      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(markdown: markdown, selectable: false),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final paintFinder = find.byWidgetPredicate(
        (widget) => widget is CustomPaint && widget.painter is Tier1Painter,
      );
      expect(paintFinder, findsOneWidget);
      final paintWidget = tester.widget<CustomPaint>(paintFinder);
      final painter = paintWidget.painter;
      expect(painter, isA<Tier1Painter>());

      final typedPainter = painter! as Tier1Painter;
      expect(typedPainter.geometry.quoteBlocks, isNotEmpty);
      expect(typedPainter.geometry.codeBlocks, isNotEmpty);
    });

    testWidgets('does not clip fenced background horizontal insets', (
      tester,
    ) async {
      const markdown = '```dart\nfinal x = 1;\n```';

      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(markdown: markdown, selectable: false),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final layerStackFinder = find.byWidgetPredicate((widget) {
        if (widget is! Stack) return false;
        return widget.children.any((child) {
          if (child is! Positioned) return false;
          final positionedChild = child.child;
          if (positionedChild is! IgnorePointer) return false;
          return positionedChild.child is CustomPaint;
        });
      });
      expect(layerStackFinder, findsOneWidget);

      final stack = tester.widget<Stack>(layerStackFinder);
      expect(
        stack.clipBehavior,
        Clip.none,
        reason: 'Negative code-block horizontal insets need unclipped paint so '
            'fence text stays visibly inset from the background edge.',
      );
    });

    testWidgets('hides markdown task markers in read mode text', (
      tester,
    ) async {
      const markdown = '- [ ] todo\n- [x] done';

      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: SovereignMarkdownView(markdown: markdown, selectable: false),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final plainText = _readViewPlainText(tester);
      expect(plainText, contains('todo'));
      expect(plainText, contains('done'));
      expect(plainText, isNot(contains('[ ]')));
      expect(plainText, isNot(contains('[x]')));
      expect(
        find.byKey(const Key('SovereignMarkdownViewTaskCheckboxVisual')),
        findsNWidgets(2),
      );
    });

    testWidgets(
      'renders projected markdown immediately when authoritative parse fails',
      (tester) async {
        const markdown = '* bullet\n- [ ] todo';

        await tester.pumpWidget(
          const MaterialApp(
            home: Scaffold(
              body: SovereignMarkdownView(
                markdown: markdown,
                selectable: false,
                syntaxEngine: _FailingParseSyntaxEngine(),
              ),
            ),
          ),
        );
        await tester.pump();

        final plainText = _readViewPlainText(tester);
        expect(plainText, contains('bullet'));
        expect(plainText, contains('todo'));
        expect(plainText, isNot(contains('* bullet')));
        expect(plainText, isNot(contains('[ ]')));
      },
    );
  });
}

class _FailingParseSyntaxEngine implements SyntaxEngine {
  const _FailingParseSyntaxEngine();

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    return Future<SyntaxSnapshot>.error(
      StateError('Intentional parse failure for coverage'),
    );
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return const V1SyntaxEngineAdapter().predict(request);
  }
}
