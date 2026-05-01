import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  testWidgets(
    'Fenced code block highlights via auto-detect when no info string',
    (tester) async {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      // No language tag on the opening fence.
      controller.text = '```\nfinal x = 1;\n```';

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

      final keywordSpan = leaves.firstWhere(
        (s) => (s.text ?? '').contains('final'),
        orElse: () => throw StateError('Expected a span containing "final".'),
      );

      expect(keywordSpan.style, isNotNull);
      expect(
        keywordSpan.style!.color,
        isNot(equals(Colors.black)),
        reason: 'Auto-detected fenced code should apply syntax highlighting.',
      );
    },
  );

  testWidgets(
    'Fenced code auto-detect highlights common Dart function snippets',
    (tester) async {
      final controller = SovereignController();
      addTearDown(controller.dispose);

      controller.text =
          '```\nint main() {\n  final x = 2;\n  return x;\n}\n```';

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

      final highlighted = leaves.where(
        (s) => s.style?.color != null && s.style!.color != Colors.black,
      );
      expect(
        highlighted.isNotEmpty,
        isTrue,
        reason:
            'Dart-like snippets should receive auto-detected syntax colors.',
      );
    },
  );
}
