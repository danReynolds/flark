import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/presentation/sovereign_editor.dart';

void main() {
  group('List key integration', () {
    Future<void> pumpEditor(
      WidgetTester tester, {
      required SovereignController controller,
      required FocusNode focusNode,
    }) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SovereignEditor(
              controller: controller,
              focusNode: focusNode,
              enableTestShortcuts: true,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();
    }

    testWidgets('Enter continues asterisk bullet and keeps marker hidden', (
      tester,
    ) async {
      final controller = SovereignController(
        text: '* item',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await pumpEditor(tester, controller: controller, focusNode: focusNode);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals('* item\n* '));
      expect(controller.selection.baseOffset, equals('* item\n* '.length));
      expect(
        controller.decoration.hiddenRanges
            .where((r) => controller.text.substring(r.start, r.end) == '* ')
            .length,
        greaterThanOrEqualTo(2),
      );

      final span = controller.buildTextSpan(
        context: tester.element(find.byType(SovereignEditor)),
        withComposing: false,
      );
      final italicListText = _collectStyledText(
        span,
        predicate: (text, style) =>
            text.contains('item') && style?.fontStyle == FontStyle.italic,
      );
      expect(
        italicListText,
        isEmpty,
        reason:
            'List markers should not be interpreted as inline italic wrappers.',
      );
    });

    testWidgets('Enter continues ordered list and increments marker', (
      tester,
    ) async {
      final controller = SovereignController(
        text: '1. one',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await pumpEditor(tester, controller: controller, focusNode: focusNode);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals('1. one\n2. '));
      expect(controller.selection.baseOffset, equals('1. one\n2. '.length));
      expect(
        controller.decoration.hiddenRanges
            .where((r) => controller.text.substring(r.start, r.end) == '2. ')
            .length,
        greaterThanOrEqualTo(1),
      );
    });

    testWidgets('Enter continues nested list item preserving indent', (
      tester,
    ) async {
      final controller = SovereignController(
        text: '  - item',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await pumpEditor(tester, controller: controller, focusNode: focusNode);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.length,
      );
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();

      expect(controller.text, equals('  - item\n  - '));
      expect(controller.selection.baseOffset, equals('  - item\n  - '.length));
    });

    testWidgets('Tab indents list item instead of moving focus', (
      tester,
    ) async {
      final controller = SovereignController(
        text: '- item',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Column(
              children: [
                SovereignEditor(
                  controller: controller,
                  focusNode: focusNode,
                  enableTestShortcuts: true,
                ),
                const TextField(),
              ],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byType(SovereignEditor));
      await tester.pump();

      controller.selection = const TextSelection.collapsed(offset: 0);
      await tester.pump();

      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pump();

      expect(controller.text, '  - item');
      expect(focusNode.hasFocus, isTrue);
    });

    testWidgets('Shift+Tab outdents nested list item', (tester) async {
      final controller = SovereignController(
        text: '  1. one',
        syntaxEngine: const V1SyntaxEngineAdapter(),
      );
      addTearDown(controller.dispose);
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);

      await pumpEditor(tester, controller: controller, focusNode: focusNode);
      controller.selection = const TextSelection.collapsed(offset: 4);
      await tester.pump();

      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.pump();

      expect(controller.text, '1. one');
      expect(focusNode.hasFocus, isTrue);
    });
  });
}

List<String> _collectStyledText(
  InlineSpan span, {
  required bool Function(String text, TextStyle? style) predicate,
}) {
  final out = <String>[];

  void visit(InlineSpan node) {
    if (node is TextSpan) {
      final text = node.text;
      if (text != null && text.isNotEmpty && predicate(text, node.style)) {
        out.add(text);
      }
      final children = node.children;
      if (children != null) {
        for (final child in children) {
          visit(child);
        }
      }
    }
  }

  visit(span);
  return out;
}
