import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/sovereign_editor.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/painters/tier1_painter.dart';

void main() {
  testWidgets('Quote caret stays to the right of the painted rail', (
    WidgetTester tester,
  ) async {
    final controller = SovereignController(text: '> quote');
    addTearDown(controller.dispose);
    final focusNode = FocusNode();
    addTearDown(focusNode.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SovereignEditor(
            controller: controller,
            focusNode: focusNode,
            theme: const SovereignEditorThemeData(
              editorContentPadding: EdgeInsets.zero,
              blockquote: SovereignBlockquoteTheme(
                railInset: 0,
                railWidth: 3,
                railRadius: Radius.circular(1),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    controller.selection = const TextSelection.collapsed(offset: 2);
    await tester.pumpAndSettle();

    final editable = tester.state<EditableTextState>(find.byType(EditableText));
    final caretRect = editable.renderEditable.getLocalRectForCaret(
      const TextPosition(offset: 2),
    );

    final paints = tester.widgetList<CustomPaint>(find.byType(CustomPaint));
    final tier1 = paints.map((p) => p.painter).whereType<Tier1Painter>().first;
    final railRight = tier1.quoteRailInset + tier1.quoteRailWidth;

    expect(
      caretRect.left,
      greaterThan(railRight + 1.0),
      reason: 'Quote rail should never overlap caret start for quote content.',
    );
  });
}
