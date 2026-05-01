import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_types.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/models/block_node.dart';
import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';

void main() {
  group('Sovereign block markdown rendering', () {
    testWidgets('Header marker is hidden and header text is emphasized', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController();
      controller.text = '# Title';
      addTearDown(controller.dispose);

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.black),
        withComposing: false,
      );
      final leaves = _leafSpans(span);

      final markerSpans = leaves.where((s) => (s.text ?? '').contains('# '));
      expect(markerSpans, isNotEmpty);
      for (final marker in markerSpans) {
        expect(marker.style?.fontSize, equals(0));
      }

      final titleSpans = leaves.where(
        (s) => (s.text ?? '').contains('Title') && (s.style?.fontSize ?? 0) > 0,
      );
      expect(titleSpans, isNotEmpty);
      for (final title in titleSpans) {
        expect(title.style?.fontWeight, equals(FontWeight.w700));
        expect((title.style?.fontSize ?? 0) > 12, isTrue);
      }
    });

    testWidgets('Heading styles can be themed by level', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController();
      controller.text = '## Subtitle';
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditorThemeScope(
              data: const SovereignEditorThemeData(
                headings: SovereignHeadingsTheme(
                  h2: TextStyle(
                    color: Colors.teal,
                    fontWeight: FontWeight.w900,
                    letterSpacing: 1.2,
                  ),
                ),
              ),
              child: Container(),
            ),
          ),
        ),
      );
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.black),
        withComposing: false,
      );
      final leaves = _leafSpans(span);

      final subtitleSpan = leaves.firstWhere(
        (s) =>
            (s.text ?? '').contains('Subtitle') && (s.style?.fontSize ?? 0) > 0,
      );
      expect(subtitleSpan.style?.color, equals(Colors.teal));
      expect(subtitleSpan.style?.fontWeight, equals(FontWeight.w900));
      expect(subtitleSpan.style?.letterSpacing, equals(1.2));
    });

    testWidgets(
      'Heading render size is capped for fixed-line editor geometry',
      (WidgetTester tester) async {
        final controller = SovereignController();
        controller.text = '# Very Big Heading';
        addTearDown(controller.dispose);

        await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
        final context = tester.element(find.byType(Container));
        await _pumpForParse(tester);

        const base = TextStyle(fontSize: 18, color: Colors.black);
        final span = controller.buildTextSpan(
          context: context,
          style: base,
          withComposing: false,
        );
        final leaves = _leafSpans(span);

        final headingSpan = leaves.firstWhere(
          (s) =>
              (s.text ?? '').contains('Very Big Heading') &&
              (s.style?.fontSize ?? 0) > 0,
        );
        final headingSize = headingSpan.style?.fontSize ?? 0;
        expect(headingSize, lessThanOrEqualTo((base.fontSize ?? 18) * 1.35));
        expect(headingSize, greaterThan(base.fontSize ?? 18));
      },
    );

    testWidgets('Blockquote marker is hidden and quote text is styled', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController();
      controller.text = '> Quoted';
      addTearDown(controller.dispose);

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.black),
        withComposing: false,
      );
      final leaves = _leafSpans(span);

      final markerSpans = leaves.where((s) => (s.text ?? '').contains('> '));
      expect(markerSpans, isNotEmpty);
      for (final marker in markerSpans) {
        expect(marker.style?.color, equals(const Color(0x00000000)));
        expect(marker.style?.fontSize, isNot(equals(0)));
      }

      final quoteSpans = leaves.where(
        (s) =>
            (s.text ?? '').contains('Quoted') && (s.style?.fontSize ?? 0) > 0,
      );
      expect(quoteSpans, isNotEmpty);
      for (final quote in quoteSpans) {
        expect(quote.style?.fontStyle, equals(FontStyle.italic));
        expect(
          quote.style?.color,
          equals(const Color(0xFFD4C5A9).withValues(alpha: 0.88)),
        );
      }
    });

    testWidgets('List markers render visible bullets/indices', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController();
      controller.text = '- alpha\n2. beta';
      addTearDown(controller.dispose);

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.black),
        withComposing: false,
      );
      final leaves = _leafSpans(span);

      final unorderedMarker = leaves.where(
        (s) => (s.text ?? '').contains('\u2022 '),
      );
      expect(unorderedMarker, isNotEmpty);
      for (final marker in unorderedMarker) {
        expect(marker.style?.fontSize, isNot(equals(0)));
      }

      final orderedMarker = leaves.where((s) => (s.text ?? '').contains('2. '));
      expect(orderedMarker, isNotEmpty);
      for (final marker in orderedMarker) {
        expect(marker.style?.fontSize, isNot(equals(0)));
      }
    });

    testWidgets('Quoted list markers stay visible as bullets/indices', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController();
      controller.text = '> - alpha\n> 2. beta';
      addTearDown(controller.dispose);

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.black),
        withComposing: false,
      );
      final leaves = _leafSpans(span);

      final quoteMarkers = leaves.where((s) => (s.text ?? '') == '> ');
      expect(quoteMarkers.length, greaterThanOrEqualTo(2));
      for (final marker in quoteMarkers) {
        expect(marker.style?.color, equals(const Color(0x00000000)));
      }

      final unorderedMarker = leaves.where((s) => (s.text ?? '') == '\u2022 ');
      expect(unorderedMarker, isNotEmpty);
      for (final marker in unorderedMarker) {
        expect(marker.style?.fontSize, isNot(equals(0)));
      }

      final orderedMarker = leaves.where((s) => (s.text ?? '') == '2. ');
      expect(orderedMarker, isNotEmpty);
      for (final marker in orderedMarker) {
        expect(marker.style?.fontSize, isNot(equals(0)));
      }
    });

    testWidgets(
      'Task list renders visible checkbox glyphs and checked item is struck through',
      (WidgetTester tester) async {
        final controller = SovereignController();
        controller.text = '- [x] done\n- [ ] todo';
        addTearDown(controller.dispose);

        await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
        final context = tester.element(find.byType(Container));
        await _pumpForParse(tester);

        final span = controller.buildTextSpan(
          context: context,
          style: const TextStyle(fontSize: 12, color: Colors.black),
          withComposing: false,
        );
        final leaves = _leafSpans(span);

        final bulletMarkers = leaves.where((s) => (s.text ?? '') == '\u2022 ');
        // Task list items should render a checkbox in place of the bullet.
        expect(bulletMarkers, isEmpty);

        final checkboxMarkers = leaves.where(
          (s) =>
              (s.text ?? '').contains('\u2611') ||
              (s.text ?? '').contains('\u2610'),
        );
        expect(checkboxMarkers.length, greaterThanOrEqualTo(2));

        final doneSpan = leaves.firstWhere(
          (s) =>
              (s.text ?? '').contains('done') && (s.style?.fontSize ?? 0) > 0,
        );
        expect(doneSpan.style?.decoration, equals(TextDecoration.lineThrough));
        expect(doneSpan.style?.color, equals(const Color(0x8AFFFFFF)));

        final todoSpan = leaves.firstWhere(
          (s) =>
              (s.text ?? '').contains('todo') && (s.style?.fontSize ?? 0) > 0,
        );
        expect(todoSpan.style?.decoration, isNot(TextDecoration.lineThrough));
      },
    );

    testWidgets('Task checkbox glyph colors can be themed', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController(
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      controller.text = '- [x] done\n- [ ] todo';
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditorThemeScope(
              data: const SovereignEditorThemeData(
                taskCheckbox: SovereignTaskCheckboxTheme(
                  useCustomOverlay: false,
                  checked: TextStyle(color: Colors.green),
                  unchecked: TextStyle(color: Colors.red),
                ),
              ),
              child: Container(),
            ),
          ),
        ),
      );
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.white),
        withComposing: false,
      );
      final leaves = _leafSpans(span);

      final checkedMarker = leaves.firstWhere(
        (s) => (s.text ?? '').contains('\u2611'),
      );
      final uncheckedMarker = leaves.firstWhere(
        (s) => (s.text ?? '').contains('\u2610'),
      );

      expect(checkedMarker.style?.color, equals(Colors.green));
      expect(uncheckedMarker.style?.color, equals(Colors.red));
    });

    testWidgets(
      'Thematic break renders divider glyphs instead of raw markers',
      (WidgetTester tester) async {
        final controller = SovereignController();
        controller.text = 'before\n\n---\n\nafter';
        addTearDown(controller.dispose);

        await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
        final context = tester.element(find.byType(Container));
        await _pumpForParse(tester);

        final span = controller.buildTextSpan(
          context: context,
          style: const TextStyle(fontSize: 12, color: Colors.black),
          withComposing: false,
        );
        final leaves = _leafSpans(span);

        final divider = leaves.where(
          (s) => (s.text ?? '') == '\u2500\u2500\u2500',
        );
        expect(divider, isNotEmpty);
        for (final d in divider) {
          expect((d.style?.fontSize ?? 0) > 0, isTrue);
        }
      },
    );

    testWidgets('Reference definition marker prefix is hidden', (
      WidgetTester tester,
    ) async {
      final controller = SovereignController();
      controller.text = '[ref]: https://example.com\nUse [ref].';
      addTearDown(controller.dispose);

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.black),
        withComposing: false,
      );
      final leaves = _leafSpans(span);

      final prefixSpan = leaves.firstWhere((s) => (s.text ?? '') == '[ref]: ');
      expect(prefixSpan.style?.fontSize, equals(0));
      final urlSpan = leaves.firstWhere(
        (s) =>
            (s.text ?? '').contains('https://example.com') &&
            (s.style?.fontSize ?? 0) > 0,
      );
      expect(urlSpan.style?.fontSize, isNot(equals(0)));
    });

    testWidgets(
      'Image markdown renders alt text placeholder and hides syntax',
      (WidgetTester tester) async {
        final controller = SovereignController();
        controller.text = '![alt text](https://img.example/x.png)';
        addTearDown(controller.dispose);

        await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
        final context = tester.element(find.byType(Container));
        await _pumpForParse(tester);

        final span = controller.buildTextSpan(
          context: context,
          style: const TextStyle(fontSize: 12, color: Colors.black),
          withComposing: false,
        );
        final leaves = _leafSpans(span);

        final altSpan = leaves.firstWhere(
          (s) =>
              (s.text ?? '').contains('alt text') &&
              (s.style?.fontSize ?? 0) > 0,
        );
        expect(
          altSpan.style?.decoration,
          isNot(equals(TextDecoration.underline)),
        );
        expect(altSpan.style?.fontStyle, isNot(equals(FontStyle.italic)));

        expect(leaves.any((s) => (s.text ?? '') == '\u25A3 '), isFalse);
        expect(
          leaves.any(
            (s) => (s.text ?? '').contains('https://img.example/x.png'),
          ),
          isFalse,
        );
      },
    );

    testWidgets('Table block renders in monospace baseline style', (
      WidgetTester tester,
    ) async {
      const text = '| a | b |\n| --- | --- |\n| c | d |';
      final controller = SovereignController(
        text: text,
        syntaxEngine: const _StaticTableSyntaxEngine(),
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));
      await _pumpForParse(tester);

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 12, color: Colors.black),
        withComposing: false,
      );
      final leaves = _leafSpans(span);
      final cellSpan = leaves.firstWhere(
        (s) => (s.text ?? '').contains('| a | b |'),
      );
      expect(cellSpan.style?.fontFamily, isNotNull);
    });
  });
}

List<TextSpan> _leafSpans(InlineSpan root) {
  final leaves = <TextSpan>[];
  void walk(InlineSpan span) {
    if (span is! TextSpan) return;
    if (span.text != null && span.text!.isNotEmpty) {
      leaves.add(span);
    }
    final children = span.children;
    if (children == null) return;
    for (final child in children) {
      walk(child);
    }
  }

  walk(root);
  return leaves;
}

Future<void> _pumpForParse(WidgetTester tester) async {
  for (var i = 0; i < 3; i++) {
    await tester.pump(const Duration(milliseconds: 1));
  }
}

class _StaticTableSyntaxEngine implements SyntaxEngine {
  const _StaticTableSyntaxEngine();

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    return SyntaxSnapshot(
      revision: request.revision,
      blocks: [
        BlockSpan(
          type: BlockType.table,
          start: 0,
          end: request.text.length,
          payload: const {'columns': 2, 'rows': 2},
        ),
      ],
      inlineTokens: const [],
      markerRanges: const [],
      exclusionRanges: const [],
      ambiguityZones: const [],
      cursorMask: PassthroughCursorValidationMask(
        textLength: request.text.length,
      ),
      diagnostics: const [],
    );
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return SyntaxPrediction.empty(
      revision: request.revision,
      textLength: request.text.length,
    );
  }
}
