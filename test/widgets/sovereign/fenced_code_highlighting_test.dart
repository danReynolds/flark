import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/theme/sovereign_markdown_theme.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  testWidgets('Fenced code block highlights Dart keywords via info string', (
    tester,
  ) async {
    final controller = SovereignController();
    controller.text = '```dart\nfinal x = 1;\n```';

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
    final context = tester.element(find.byType(Container));

    final span = controller.buildTextSpan(
      context: context,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );

    final leaves = (span.children ?? const <InlineSpan>[])
        .whereType<TextSpan>()
        .where((s) => (s.text ?? '').isNotEmpty)
        .toList();

    final finalSpan = leaves.firstWhere(
      (s) => (s.text ?? '').contains('final'),
      orElse: () => throw StateError('Expected a span containing "final".'),
    );

    expect(finalSpan.style, isNotNull);
    expect(
      finalSpan.style!.color,
      isNot(equals(Colors.black)),
      reason: '"final" should be highlighted (not rendered in base color).',
    );
  });

  testWidgets(
    'Unknown fenced language falls back to auto-detect highlighting',
    (tester) async {
      final controller = SovereignController();
      controller.text = '```wat\nvoid main() {\n  final x = 1;\n}\n```';

      await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
      final context = tester.element(find.byType(Container));

      final span = controller.buildTextSpan(
        context: context,
        style: const TextStyle(fontSize: 10, color: Colors.black),
        withComposing: false,
      );

      final leaves = (span.children ?? const <InlineSpan>[])
          .whereType<TextSpan>()
          .where((s) => (s.text ?? '').isNotEmpty)
          .toList();

      final finalSpan = leaves.firstWhere(
        (s) => (s.text ?? '').contains('final'),
        orElse: () => throw StateError('Expected a span containing "final".'),
      );

      expect(finalSpan.style, isNotNull);
      expect(
        finalSpan.style!.color,
        isNot(equals(Colors.black)),
        reason:
            'Unknown fence tags should still attempt auto-detect highlighting.',
      );
    },
  );

  testWidgets('Plain fenced language keeps code unhighlighted', (tester) async {
    final controller = SovereignController();
    controller.text = '```plain\nvoid main() {\n  final x = 1;\n}\n```';

    await tester.pumpWidget(MaterialApp(home: Scaffold(body: Container())));
    final context = tester.element(find.byType(Container));

    final span = controller.buildTextSpan(
      context: context,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );

    final leaves = (span.children ?? const <InlineSpan>[])
        .whereType<TextSpan>()
        .where((s) => (s.text ?? '').isNotEmpty)
        .toList();

    final finalSpan = leaves.firstWhere(
      (s) => (s.text ?? '').contains('final'),
      orElse: () => throw StateError('Expected a span containing "final".'),
    );

    expect(finalSpan.style, isNotNull);
    expect(
      finalSpan.style!.color,
      equals(Colors.black),
      reason:
          'Explicit plain tags should keep code rendered without syntax colors.',
    );
  });

  testWidgets('Fenced code highlight cache invalidates on theme change', (
    tester,
  ) async {
    final controller = SovereignController();
    controller.text = '```dart\nfinal x = 1;\n```';

    final base = SovereignMarkdownTheme.standard();
    final firstTheme = base.copyWith(syntaxKeywordColor: Colors.red);
    final secondTheme = base.copyWith(syntaxKeywordColor: Colors.blue);

    await tester.pumpWidget(
      MaterialApp(
        themeAnimationDuration: Duration.zero,
        theme: ThemeData(extensions: <ThemeExtension<dynamic>>[firstTheme]),
        home: Scaffold(body: Container()),
      ),
    );
    final firstContext = tester.element(find.byType(Container));
    final firstSpan = controller.buildTextSpan(
      context: firstContext,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );
    final firstLeaves = (firstSpan.children ?? const <InlineSpan>[])
        .whereType<TextSpan>()
        .where((s) => (s.text ?? '').isNotEmpty)
        .toList();
    final firstFinal = firstLeaves.firstWhere(
      (s) => (s.text ?? '').contains('final'),
      orElse: () => throw StateError('Expected a span containing "final".'),
    );
    expect(firstFinal.style, isNotNull);
    expect(firstFinal.style!.color, equals(Colors.red));

    await tester.pumpWidget(
      MaterialApp(
        themeAnimationDuration: Duration.zero,
        theme: ThemeData(extensions: <ThemeExtension<dynamic>>[secondTheme]),
        home: Scaffold(body: Container()),
      ),
    );
    final secondContext = tester.element(find.byType(Container));
    final secondSpan = controller.buildTextSpan(
      context: secondContext,
      style: const TextStyle(fontSize: 10, color: Colors.black),
      withComposing: false,
    );
    final secondLeaves = (secondSpan.children ?? const <InlineSpan>[])
        .whereType<TextSpan>()
        .where((s) => (s.text ?? '').isNotEmpty)
        .toList();
    final secondFinal = secondLeaves.firstWhere(
      (s) => (s.text ?? '').contains('final'),
      orElse: () => throw StateError('Expected a span containing "final".'),
    );
    expect(secondFinal.style, isNotNull);
    expect(secondFinal.style!.color, equals(Colors.blue));
  });
}
