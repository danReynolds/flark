import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  group('SovereignEditor Phase 1 Smoke Test', () {
    late SovereignController controller;

    setUp(() {
      controller = SovereignController();
    });

    testWidgets('renders and accepts text input', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditor(controller: controller, autofocus: true),
          ),
        ),
      );

      // Verify initial state
      expect(find.byType(SovereignEditor), findsOneWidget);
      expect(controller.text, isEmpty);

      // Enter Text
      await tester.enterText(find.byType(TextField), '# Hello World\n');
      await tester.pumpAndSettle();

      // Verify State Update
      expect(controller.text, '# Hello World\n');
      expect(controller.state.revision, greaterThan(0));

      // Verify Block Parsing (Indirectly via no crash & state)
      // In a real introspection test we'd check controller.decoration.tree
    });

    testWidgets('undo/redo works for simple input', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: SovereignEditor(controller: controller)),
        ),
      );

      final field = find.byType(TextField);

      // typing 'A'
      await tester.enterText(field, 'A');
      await tester.pump(); // Fast pump

      // typing 'B' (Should merge if fast enough, but enterText replaces usually)
      // To test undo, distinct edits are better.
      // enterText acts as a full replacement diff.

      // State: "A"
      expect(controller.text, 'A');

      // Undo
      controller.undo();
      await tester.pump();

      expect(controller.text, isEmpty);

      // Redo
      controller.redo();
      await tester.pump();
      expect(controller.text, 'A');
    });

    testWidgets('scrolls and updates viewport (Culling Smoke Test)', (
      tester,
    ) async {
      // Create a long document to force scrolling
      final longText = List.generate(100, (i) => 'Line $i').join('\n');
      controller.value = TextEditingValue(text: longText);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: SovereignEditor(controller: controller)),
        ),
      );

      await tester.pumpAndSettle(); // Allow initial parse

      final scrollable = find.byType(Scrollable).first; // Vertical scrollable
      expect(scrollable, findsOneWidget);

      // Scroll Down
      await tester.drag(scrollable, const Offset(0, -500));
      await tester.pump();

      // We just verify it didn't crash and painted a frame.
      // Ideally we'd inspect the Painter's viewport property, but that requires finding the CustomPaint widget
      // and inspecting its painter, which is hard in widget tests without keys.
      // This serves as a "No Crash" smoke test for the culling logic path.
    });
  });
}
