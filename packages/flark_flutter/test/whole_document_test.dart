import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets('Option Backspace deletes a word and keeps the next key', (
    tester,
  ) async {
    final c = FlarkController(
      FlarkEditor(backend, text: 'one two three', caret: 13),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pump();
    expect(c.text, 'one two ');
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'one two x',
        selection: TextSelection.collapsed(offset: 9),
      ),
    );
    await tester.pump();
    expect(c.text, 'one two x');
    c.command(const Undo());
    c.command(const Undo());
    expect(c.text, 'one two three');
    expect(c.editor.selection.extent, 13);
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets('delete beside whitespace preserves style on the first paint', (
    tester,
  ) async {
    final c = FlarkController(
      FlarkEditor(backend, text: '**two x**', caret: 7),
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
    paints.clear();
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(c.text, '**two** ');
    expect(paints, isNotEmpty);
    for (final paint in paints) {
      expect(paint.rows, ['two ']);
      expect(paint.styles.single, contains(Style.strong));
      expect(paint.caretSource, 8);
      expect(paint.revision, c.editor.revision);
    }
    paints.clear();
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: '**two** y',
        selection: TextSelection.collapsed(offset: 9),
      ),
    );
    await tester.pump();
    expect(c.text, '**two** **y**');
    for (final paint in paints) {
      expect(paint.rows, ['two y']);
      expect(paint.styles.single, contains(Style.strong));
    }
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets('replace a word selected from outside its closing syntax', (
    tester,
  ) async {
    const source = 'before **bold** after';
    final c = FlarkController(
      FlarkEditor(backend, text: source, caret: source.length),
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
    for (var i = 0; i < 6; i++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    }
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    for (var i = 0; i < 4; i++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    }
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();
    expect(c.editor.selection, const FlarkSelection(15, 9));
    expect(paints.last.selectionRects, isNotEmpty);
    paints.clear();
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'before **new after',
        selection: TextSelection.collapsed(offset: 12),
      ),
    );
    await tester.pump();
    expect(c.text, 'before **new** after');
    expect(paints, isNotEmpty);
    for (final paint in paints) {
      expect(paint.rows, ['before new after']);
      expect(paint.styles.single, contains(Style.strong));
      expect(paint.caretSource, 12);
      expect(paint.revision, c.editor.revision);
    }
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'before **new!** after',
        selection: TextSelection.collapsed(offset: 13),
      ),
    );
    await tester.pump();
    expect(c.text, 'before **new!** after');
    c.command(const Undo());
    c.command(const Undo());
    await tester.pump();
    expect(c.text, source);
    expect(c.editor.selection, const FlarkSelection(15, 9));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets(
    'Select All, paste, next key and undo preserve whole document paint',
    (tester) async {
      const original = '# Heading\n\n- one\n- **two**';
      final c = FlarkController(FlarkEditor(backend, text: original));
      final paints = <FlarkPaintObservation>[];
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async => call.method == 'Clipboard.getData'
            ? {'text': 'fresh **text**'}
            : null,
      );
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );
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
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyA);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(0, original.length));
      expect(paints.last.selectionRects, isNotEmpty);
      paints.clear();
      await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
      await tester.pump();
      expect(c.text, 'fresh **text**');
      expect(paints, isNotEmpty);
      for (final p in paints) {
        expect(p.rows, ['fresh text']);
        expect(p.caretSource, c.text.length);
        expect(p.revision, c.editor.revision);
        expect(p.styles.single, contains(Style.strong));
      }
      paints.clear();
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'fresh **text**!',
          selection: TextSelection.collapsed(offset: 15),
        ),
      );
      await tester.pump();
      expect(c.text, 'fresh **text**!');
      expect(paints, isNotEmpty);
      for (final p in paints) {
        expect(p.rows, ['fresh text!']);
        expect(p.caretSource, 15);
      }
      c.command(const Undo());
      c.command(const Undo());
      await tester.pump();
      expect(c.text, original);
      expect(c.editor.selection, const FlarkSelection(0, original.length));
      expect(paints.last.selectionRects, isNotEmpty);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
