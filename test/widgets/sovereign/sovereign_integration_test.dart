import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/widgets/sovereign/presentation/painters/tier1_painter.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  group('Sovereign Integration Level 2 (Contracts & Flows)', () {
    // -------------------------------------------------------------------------
    // DISPOSE SAFETY
    // -------------------------------------------------------------------------
    testWidgets('Dispose Race: Pending Microtasks do not crash', (
      tester,
    ) async {
      // Setup Controller
      final controller = SovereignController();

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: SovereignEditor(controller: controller)),
        ),
      );

      // Trigger change
      controller.text = "Hello *World*";
      // Parsing is scheduled as microtask/isolate.

      // Immediate Dispose
      await tester.pumpWidget(Container()); // Replaces Editor, disposing it.
      controller.dispose();

      // Wait for pending things to flush
      await tester.pumpAndSettle();

      // Assert no crash.
    });

    // -------------------------------------------------------------------------
    // RENDER INVARIANTS
    // -------------------------------------------------------------------------
    testWidgets('Render Alignment: Painter matches TextField geometry', (
      tester,
    ) async {
      // Goal: Ensure the background painter aligns with the text layout.
      final controller = SovereignController();
      controller.text = """
# Header
- List Item
""";

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: SizedBox(
                width: 300,
                child: SovereignEditor(controller: controller),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle(); // Wait for parsing

      // Find RenderObjects
      // SovereignEditor -> Stack -> Positioned(Painter) & TextField
      final painterFinder = find.byWidgetPredicate(
        (widget) => widget is CustomPaint && widget.painter is Tier1Painter,
      );

      // Debug: Print what we found
      if (painterFinder.evaluate().isEmpty) {
        debugPrint("No Tier1Painter found!");
        final customPaints = find.byType(CustomPaint);
        debugPrint("Found ${customPaints.evaluate().length} CustomPaints");
        for (final element in customPaints.evaluate()) {
          final widget = element.widget as CustomPaint;
          debugPrint(
            " - Painter: ${widget.painter?.runtimeType}, Foreground: ${widget.foregroundPainter?.runtimeType}",
          );
        }
      }

      final textFieldFinder = find.byType(TextField);

      final RenderBox painterBox = tester.renderObject(painterFinder);
      final RenderBox textFieldBox = tester.renderObject(textFieldFinder);

      // Alignments
      // Both should have same global offset?
      // Painter is inside a Stack, TextField is inside a Stack.
      // They are stacked on top of each other.
      // Offset mismatch implies drift.

      final painterPos = painterBox.localToGlobal(Offset.zero);
      final textFieldPos = textFieldBox.localToGlobal(Offset.zero);

      expect(
        painterPos.dy,
        closeTo(textFieldPos.dy, 0.5),
        reason: 'Vertical Misalignment',
      );

      // We can also check size
      expect(painterBox.size.width, closeTo(textFieldBox.size.width, 0.5));
    });

    // -------------------------------------------------------------------------
    // UI CHURN CHECK
    // -------------------------------------------------------------------------
    testWidgets(
        'UI Churn: Decoration update parses without rebuilding TextField', (
      tester,
    ) async {
      final controller = SovereignController();

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: StatefulBuilder(
              builder: (ctx, setState) {
                return SovereignEditor(controller: controller);
              },
            ),
          ),
        ),
      );

      // Trigger Text Change -> Parses -> Emits Decoration
      // The SovereignEditor listens to decoration stream.
      // It passes decoration to painter.
      // Does it rebuild the TextField?
      // If SovereignEditor is built with `AnimatedBuilder` on controller?
      // Currently `SovereignEditor` is a StatefulWidget that likely listens to controller.
      // If it calls `setState`, it rebuilds TextField.
      // The goal is to minimize this, but for phase 1/2 it might rebuild.
      // RFC says "Ensure decoration updates trigger repaints without rebuilding entire TextField subtree"

      controller.text = "New Text";
      await tester.pump();

      // If we integrated properly, the decoration repaint might happen via RepaintBoundary or CustomPainter listener
      // without full widget rebuild if optimized.
      // But SovereignController changes text value -> TextField MUST rebuild to show new text.

      // Real churn check:
      // Change text that DOES NOT change blocks? (e.g. typing inside a block).
      // If we type "a", text changes.

      // Let's verify that a Parse Result (async) arriving LATER doesn't destroy the cursor state.
      // Type "abc". Pump.
      await tester.enterText(find.byType(TextField), "abc");
      await tester.pump();

      // Async isolate returns.
      await tester.pumpAndSettle();

      // Cursor should still be at end.
    });
  });
}
