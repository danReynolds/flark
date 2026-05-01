import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';
import 'package:sovereign_editor/widgets/sovereign/theme/sovereign_editor_theme.dart';

void main() {
  testWidgets('Tapping task checkbox toggles checked state', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '- [ ] todo',
      syntaxEngine: const V1SyntaxEngineAdapter(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: SovereignEditor(controller: controller)),
      ),
    );
    await tester.pumpAndSettle();

    final targetFinder = find.bySemanticsLabel('Check task item');
    expect(targetFinder, findsOneWidget);

    await tester.tap(targetFinder);
    await tester.pump();
    await tester.pump();
    expect(controller.text, '- [x] todo');

    await tester.tap(find.bySemanticsLabel('Uncheck task item'));
    await tester.pump();
    await tester.pump();
    expect(controller.text, '- [ ] todo');
  });

  testWidgets('Custom task checkbox overlay renders with theme styling', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '- [ ] todo',
      syntaxEngine: const V1SyntaxEngineAdapter(),
    );
    addTearDown(controller.dispose);

    const fill = Color(0xFF1A1F27);
    const border = Color(0xFFB98A42);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            theme: const SovereignEditorThemeData(
              taskCheckbox: SovereignTaskCheckboxTheme(
                useCustomOverlay: true,
                uncheckedFillColor: fill,
                uncheckedBorderColor: border,
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final visual = find.byKey(const Key('SovereignTaskCheckboxVisual'));
    expect(visual, findsOneWidget);

    final decorated = tester.widget<DecoratedBox>(visual);
    final decoration = decorated.decoration as BoxDecoration;
    expect(decoration.color, fill);
    expect(decoration.border, isA<Border>());
    final boxBorder = decoration.border! as Border;
    expect(boxBorder.top.color, border);
  });

  testWidgets(
    'Custom task checkbox overlay aligns with unordered bullet marker column',
    (WidgetTester tester) async {
      final controller = SovereignController(
        text: '- [ ] todo',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: SovereignEditor(controller: controller)),
        ),
      );
      await tester.pumpAndSettle();

      final editable = tester.state<EditableTextState>(
        find.byType(EditableText),
      );
      final markerCaret = editable.renderEditable.getLocalRectForCaret(
        const TextPosition(offset: 0),
      );
      final markerCaretGlobal = editable.renderEditable.localToGlobal(
        markerCaret.topLeft,
      );

      final visualBox = tester.renderObject<RenderBox>(
        find.byKey(const Key('SovereignTaskCheckboxVisual')),
      );
      final visualGlobal = visualBox.localToGlobal(Offset.zero);

      expect(
        (visualGlobal.dx - markerCaretGlobal.dx).abs(),
        lessThanOrEqualTo(2.0),
      );
    },
  );

  testWidgets('Task list text aligns with bullet list content column', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '- bullet\n- [ ] task',
      syntaxEngine: const V1SyntaxEngineAdapter(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: SovereignEditor(controller: controller)),
      ),
    );
    await tester.pumpAndSettle();

    final editable = tester.state<EditableTextState>(find.byType(EditableText));
    final bulletCaret = editable.renderEditable.getLocalRectForCaret(
      const TextPosition(offset: 2),
    );
    final taskCaret = editable.renderEditable.getLocalRectForCaret(
      const TextPosition(offset: 15),
    );

    expect((taskCaret.left - bulletCaret.left).abs(), lessThanOrEqualTo(12.0));
  });

  testWidgets('Enter on task item creates second checkbox on next line', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(
      text: '- [ ] todo',
      syntaxEngine: const V1SyntaxEngineAdapter(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: SovereignEditor(controller: controller)),
      ),
    );
    await tester.pumpAndSettle();

    controller.value = controller.value.copyWith(
      selection: TextSelection.collapsed(offset: controller.text.length),
      composing: TextRange.empty,
    );
    await tester.pump();

    controller.handleEnter();
    await tester.pump();
    // During the relayout frame, keep the previous checkbox visible instead of
    // hiding all task checkbox overlays (prevents visible flicker).
    expect(find.bySemanticsLabel('Check task item'), findsOneWidget);
    await tester.pump();

    expect(controller.text, equals('- [ ] todo\n- [ ] '));

    final visuals = find.byKey(const Key('SovereignTaskCheckboxVisual'));
    expect(visuals, findsNWidgets(2));

    final elements = visuals.evaluate().toList(growable: false);
    final boxes = elements
        .map((e) => e.renderObject! as RenderBox)
        .toList(growable: false);
    final yPositions = boxes
        .map((b) => b.localToGlobal(Offset.zero).dy)
        .toList(growable: false)
      ..sort();
    expect(yPositions[1] - yPositions[0], greaterThan(4.0));
  });

  testWidgets(
    'Typing task content does not hide checkbox overlay for a frame',
    (WidgetTester tester) async {
      final controller = SovereignController(
        text: '- [ ] todo',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: SovereignEditor(controller: controller)),
        ),
      );
      await tester.pumpAndSettle();

      controller.value = controller.value.copyWith(
        text: '- [ ] todo!',
        selection: const TextSelection.collapsed(offset: 10),
        composing: TextRange.empty,
      );

      await tester.pump();
      expect(find.bySemanticsLabel('Check task item'), findsOneWidget);
    },
  );
}
