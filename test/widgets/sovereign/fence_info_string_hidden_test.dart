import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  testWidgets('Fence info string (```dart) is hidden from display', (
    WidgetTester tester,
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

    final leaves = <TextSpan>[];
    void walk(InlineSpan s) {
      if (s is TextSpan) {
        if (s.text != null && s.text!.isNotEmpty) {
          leaves.add(s);
        }
        final children = s.children;
        if (children != null) {
          for (final c in children) {
            walk(c);
          }
        }
      }
    }

    walk(span);

    final dartSpans =
        leaves.where((s) => (s.text ?? '').contains('dart')).toList();
    expect(dartSpans, isNotEmpty);
    for (final s in dartSpans) {
      expect(s.style?.fontSize, equals(0));
    }
  });
}
