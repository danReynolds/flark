import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/theme/dune_markdown_theme.dart';
import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';

void main() {
  testWidgets('SovereignController buildTextSpan renders inline bold', (
    tester,
  ) async {
    final controller = SovereignController();

    // 1. Set text with bold
    controller.text = "Hello **Bold** World";

    // 2. Build TextSpan (Mock context)
    // We need a context, so we pump a widget
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
    final context = tester.element(find.byType(Container));

    final span = controller.buildTextSpan(
      context: context,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );

    // 3. Inspect Span
    debugPrint("Span Children: ${span.children?.length ?? 0}");
    if (span.children != null) {
      for (var child in span.children!) {
        if (child is TextSpan) {
          debugPrint(" - '${child.text}': ${child.style?.fontWeight}");
        }
      }
    }

    // 3. Assertions
    // Expected: "Hello ", WidgetSpan, "Bold" (bold), WidgetSpan, " World"
    expect(span.children, isNotNull);
    // 5 children: Text, Widget, Text, Widget, Text
    expect(span.children!.length, 5);

    // Verify content (middle text)
    final boldSpan = span.children![2] as TextSpan;
    expect(boldSpan.text, "Bold"); // Markers hidden
    expect(boldSpan.style!.fontWeight, FontWeight.bold);
  });

  testWidgets('SovereignController buildTextSpan renders inline italic', (
    tester,
  ) async {
    final controller = SovereignController();

    // 1. Set text with italic
    controller.text = "Hello _Italic_ World";

    // 2. Build TextSpan
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
    final context = tester.element(find.byType(Container));

    final span = controller.buildTextSpan(
      context: context,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );

    // 3. Assertions
    // Expected: "Hello ", WidgetSpan, "Italic", WidgetSpan, " World"
    expect(span.children!.length, 5);

    final italicSpan = span.children![2] as TextSpan;
    expect(italicSpan.text, "Italic");
    expect(italicSpan.style!.fontStyle, FontStyle.italic);
  });

  testWidgets(
    'SovereignController buildTextSpan renders inline italic with asterisk',
    (tester) async {
      final controller = SovereignController();

      // 1. Set text with *italic*
      controller.text = "Hello *Italic* World";

      // 2. Build TextSpan
      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 10, color: Colors.black),
        withComposing: false,
      );

      // 3. Assertions
      expect(span.children!.length, 5);

      final italicSpan = span.children![2] as TextSpan;
      expect(italicSpan.text, "Italic");
      expect(italicSpan.style!.fontStyle, FontStyle.italic);
    },
  );

  testWidgets('SovereignController buildTextSpan renders inline code', (
    tester,
  ) async {
    final controller = SovereignController();

    // 1. Set text with code
    controller.text = "Hello `Code` World";

    // 2. Build TextSpan
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
    final context = tester.element(find.byType(Container));

    final span = controller.buildTextSpan(
      context: context,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );

    // 3. Assertions
    // Expected: "Hello ", WidgetSpan, "Code", WidgetSpan, " World"
    expect(span.children!.length, 5);

    final codeSpan = span.children![2] as TextSpan;
    expect(codeSpan.text, "Code");
    expect(
      codeSpan.style!.fontFamily,
      DuneMarkdownTheme.of(context).monospaceFontFamily,
    );
  });

  testWidgets('SovereignController buildTextSpan renders links', (
    tester,
  ) async {
    final controller = SovereignController();
    controller.text = '[OpenAI](https://openai.com) and https://example.com';

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
    final context = tester.element(find.byType(Container));

    final span = controller.buildTextSpan(
      context: context,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );

    final leaves = _leafSpans(span);
    final visibleLeaves = leaves.where((leaf) => !_isHiddenLeaf(leaf)).toList();

    final linkTheme = DuneMarkdownTheme.of(context);
    final linkStyledLeaves = leaves.where((leaf) {
      return leaf.style?.decoration == TextDecoration.underline &&
          leaf.style?.color == linkTheme.linkColor;
    }).toList(growable: false);
    expect(linkStyledLeaves, isNotEmpty);
    expect(
      linkStyledLeaves.any((leaf) => (leaf.text ?? '').contains('OpenAI')),
      isTrue,
    );
    expect(
      visibleLeaves.any((leaf) => (leaf.text ?? '').contains('[OpenAI](')),
      isFalse,
    );
    expect(
      visibleLeaves.any(
        (leaf) => (leaf.text ?? '').contains('https://openai.com'),
      ),
      isFalse,
      reason: 'Markdown link URL should be hidden; bare URLs remain visible.',
    );
    expect(
      linkStyledLeaves.any(
        (leaf) => (leaf.text ?? '').contains('https://example.com'),
      ),
      isTrue,
    );
  });

  testWidgets(
    'SovereignController buildTextSpan renders reference-style links',
    (tester) async {
      final controller = SovereignController();
      controller.text = '[Docs][api]\n\n[api]: https://dune.ai/docs';

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 10, color: Colors.black),
        withComposing: false,
      );

      final leaves = _leafSpans(span);
      final visibleLeaves =
          leaves.where((leaf) => !_isHiddenLeaf(leaf)).toList();
      final linkTheme = DuneMarkdownTheme.of(context);
      final linkStyledLeaves = leaves.where((leaf) {
        return leaf.style?.decoration == TextDecoration.underline &&
            leaf.style?.color == linkTheme.linkColor;
      }).toList(growable: false);

      expect(
        linkStyledLeaves.any((leaf) => (leaf.text ?? '').contains('Docs')),
        isTrue,
      );
      expect(
        visibleLeaves.any((leaf) => (leaf.text ?? '').contains('[Docs][api]')),
        isFalse,
      );
    },
  );

  testWidgets('Sovereign editor theme can override inline bold styling', (
    tester,
  ) async {
    final controller = SovereignController(text: 'Hello **Bold**');
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: SovereignEditorThemeScope(
          data: const SovereignEditorThemeData(
            inlineText: SovereignInlineTextTheme(
              bold: TextStyle(color: Colors.red),
            ),
          ),
          child: const Scaffold(body: SizedBox()),
        ),
      ),
    );
    final context = tester.element(find.byType(SizedBox));

    final span = controller.buildTextSpan(
      context: context,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );

    final boldSpan = span.children![2] as TextSpan;
    expect(boldSpan.text, 'Bold');
    expect(boldSpan.style?.fontWeight, FontWeight.bold);
    expect(boldSpan.style?.color, Colors.red);
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

bool _isHiddenLeaf(TextSpan span) {
  final style = span.style;
  if (style == null) return false;
  final fontSize = style.fontSize;
  final color = style.color;
  if (fontSize == 0) return true;
  if (color == null) return false;
  final alpha = (color.a * 255.0).round().clamp(0, 255);
  return alpha == 0;
}
