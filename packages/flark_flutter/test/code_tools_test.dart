import 'package:flark_flutter/code.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final service = FlarkTreeSitter.fromAnalyzer(CodeAnalyzer());
  tearDownAll(service.dispose);
  for (final body in ['', 'x']) {
    testWidgets('undetected snippet ${body.length} shows a usable Auto label', (
      tester,
    ) async {
      final c = FlarkController(
        FlarkEditor(
          createParseBackend(),
          codeEditing: service,
          text: '```\n$body\n```',
          caret: 4,
        ),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      expect(find.text('Auto'), findsOneWidget);
      expect(find.textContaining('null'), findsNothing);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    });
  }
  testWidgets('choosing the current code language is an inert dismissal', (
    tester,
  ) async {
    final c = FlarkController(
      FlarkEditor(
        createParseBackend(),
        codeEditing: service,
        text: '```dart\nfinal x = 1;\n```',
        caret: 12,
      ),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
    final snapshot = c.editor.snapshot, revision = c.editor.revision;
    await tester.tap(find.byTooltip('Code language'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    await tester.tap(find.text('Dart').last);
    await tester.pump();
    expect(c.notice, isNull);
    expect(c.editor.snapshot, same(snapshot));
    expect(c.editor.revision, revision);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets(
    'language picker repaints unchanged text and keeps editing/history',
    (tester) async {
      const source = '```\n{"answer": 42}\n```';
      final c = FlarkController(
        FlarkEditor(
          createParseBackend(),
          codeEditing: service,
          text: source,
          caret: 7,
        ),
      );
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      expect(find.text('Auto · JSON'), findsOneWidget);
      expect(
        paints.last.resolvedStyles.single.map((s) => s.color).toSet().length,
        greaterThan(1),
      );
      await tester.tap(find.byTooltip('Code language'));
      // Finish the menu entrance before the editing action. Edited frames
      // below still use one pump and inspect every actual paint.
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      paints.clear();
      await tester.tap(find.text('Plain text').last);
      await tester.pump();
      expect(c.text, '```text\n{"answer": 42}\n```');
      expect(c.editor.selection.extent, 11);
      expect(paints, isNotEmpty);
      for (final p in paints) {
        expect(p.rows, ['{"answer": 42}']);
        expect(p.resolvedStyles.single.map((s) => s.color).toSet().length, 1);
      }
      paints.clear();
      c.command(const Undo());
      await tester.pump();
      expect((c.text, c.editor.selection.extent), (source, 7));
      for (final p in paints) {
        expect(
          p.resolvedStyles.single.map((s) => s.color).toSet().length,
          greaterThan(1),
        );
      }
      c.command(const SetSelection.caret(5));
      paints.clear();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(c.text, '```\n{\n  "answer": 42}\n```');
      expect(c.editor.selection.extent, 8);
      for (final p in paints) {
        expect(p.caretSource, 8);
      }
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '```\n{\n  x"answer": 42}\n```',
          selection: TextSelection.collapsed(offset: 9),
        ),
      );
      await tester.pump();
      expect(c.text, '```\n{\n  x"answer": 42}\n```');
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  testWidgets('Tab and Shift-Tab preserve selection through input and paints', (
    tester,
  ) async {
    const source = '```dart\n  one\n  two\n```';
    final c = FlarkController(
      FlarkEditor(
        createParseBackend(),
        codeEditing: service,
        text: source,
        caret: 8,
      ),
    );
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FlarkEditorWidget(
            controller: c,
            autofocus: true,
            onPaint: paints.add,
          ),
        ),
      ),
    );
    await tester.pump();
    c.command(const SetSelection(19, 8));
    await tester.pump();
    paints.clear();
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pump();
    expect(c.text, '```dart\n    one\n    two\n```');
    expect(c.editor.selection, const FlarkSelection(23, 10));
    for (final p in paints) {
      expect(p.selectionRects, isNotEmpty);
    }
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();
    expect((c.text, c.editor.selection), (source, const FlarkSelection(19, 8)));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
}
