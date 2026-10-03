import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final sourceMode in [false, true]) {
    for (final mac in [false, true]) {
      testWidgets(
        'document edges, selection and next key: source=$sourceMode mac=$mac',
        (tester) async {
          const source = '# First\n\n**last**';
          final e = FlarkEditor(backend, text: source, caret: 4);
          if (sourceMode) e.setSourceMode(true);
          final c = FlarkController(e);
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
          final modifier = mac
              ? LogicalKeyboardKey.metaLeft
              : LogicalKeyboardKey.controlLeft;
          final end = mac
              ? LogicalKeyboardKey.arrowDown
              : LogicalKeyboardKey.end;
          final start = mac
              ? LogicalKeyboardKey.arrowUp
              : LogicalKeyboardKey.home;
          await tester.sendKeyDownEvent(modifier);
          await tester.sendKeyEvent(end);
          await tester.sendKeyUpEvent(modifier);
          await tester.pump();
          expect(e.selection, const FlarkSelection.collapsed(source.length));
          paints.clear();
          tester.testTextInput.updateEditingValue(
            const TextEditingValue(
              text: '$source!',
              selection: TextSelection.collapsed(offset: source.length + 1),
            ),
          );
          await tester.pump();
          expect(c.text, '$source!');
          expect(paints, isNotEmpty);
          for (final paint in paints) {
            expect(paint.caretSource, source.length + 1);
            expect(paint.revision, e.revision);
            expect(
              paint.rows,
              sourceMode
                  ? ['# First', '', '**last**!']
                  : ['First', '', 'last!'],
            );
          }
          await tester.sendKeyDownEvent(modifier);
          await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
          await tester.sendKeyEvent(start);
          await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
          await tester.sendKeyUpEvent(modifier);
          await tester.pump();
          expect(e.selection, const FlarkSelection(source.length + 1, 0));
          expect(paints.last.selectionRects, isNotEmpty);
          tester.testTextInput.updateEditingValue(
            const TextEditingValue(
              text: 'replacement',
              selection: TextSelection.collapsed(offset: 11),
            ),
          );
          await tester.pump();
          expect(c.text, 'replacement');
          c.command(const Undo());
          expect(c.text, '$source!');
          expect(e.selection, const FlarkSelection(source.length + 1, 0));
          await tester.pumpWidget(const SizedBox());
          c.dispose();
        },
      );
    }
  }

  Future<FlarkController> mount(
    WidgetTester tester,
    String source,
    int caret,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: source, caret: caret));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await tester.pump();
    return c;
  }

  Future<void> chord(
    WidgetTester tester,
    List<LogicalKeyboardKey> modifiers,
    LogicalKeyboardKey key,
  ) async {
    for (final modifier in modifiers) {
      await tester.sendKeyDownEvent(modifier);
    }
    await tester.sendKeyEvent(key);
    for (final modifier in modifiers.reversed) {
      await tester.sendKeyUpEvent(modifier);
    }
  }

  // The kernel's word movement from [caret], which each platform's word key
  // must reach.
  FlarkSelection byWord(String source, int caret, List<MoveCaret> moves) {
    final c = FlarkController(FlarkEditor(backend, text: source, caret: caret));
    moves.forEach(c.command);
    final selection = c.editor.selection;
    c.dispose();
    return selection;
  }

  const words = 'alpha beta gamma';
  const forward = MoveCaret(MoveDirection.forward, unit: MoveUnit.word);
  const extendBack = MoveCaret(
    MoveDirection.backward,
    unit: MoveUnit.word,
    extend: true,
  );

  testWidgets(
    'Control and arrows move by word on Windows and Linux',
    (tester) async {
      final c = await mount(tester, words, 0);
      final control = LogicalKeyboardKey.controlLeft;
      await chord(tester, [control], LogicalKeyboardKey.arrowRight);
      expect(c.editor.selection, byWord(words, 0, [forward]));
      await chord(tester, [control], LogicalKeyboardKey.arrowRight);
      expect(c.editor.selection, byWord(words, 0, [forward, forward]));
      await chord(tester, [
        control,
        LogicalKeyboardKey.shiftLeft,
      ], LogicalKeyboardKey.arrowLeft);
      expect(
        c.editor.selection,
        byWord(words, 0, [forward, forward, extendBack]),
      );
      // Home and End reach the line edges.
      await tester.sendKeyEvent(LogicalKeyboardKey.end);
      expect(c.editor.selection, const FlarkSelection.collapsed(16));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
    variant: const TargetPlatformVariant({
      TargetPlatform.windows,
      TargetPlatform.linux,
    }),
  );

  testWidgets(
    'Option and arrows move by word and Command reaches line edges on Apple',
    (tester) async {
      final c = await mount(tester, words, 0);
      await chord(tester, [
        LogicalKeyboardKey.altLeft,
      ], LogicalKeyboardKey.arrowRight);
      expect(c.editor.selection, byWord(words, 0, [forward]));
      await chord(tester, [
        LogicalKeyboardKey.metaLeft,
      ], LogicalKeyboardKey.arrowRight);
      expect(c.editor.selection, const FlarkSelection.collapsed(16));
      await chord(tester, [
        LogicalKeyboardKey.metaLeft,
        LogicalKeyboardKey.shiftLeft,
      ], LogicalKeyboardKey.arrowLeft);
      expect(c.editor.selection, const FlarkSelection(16, 0));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
    variant: const TargetPlatformVariant({
      TargetPlatform.macOS,
      TargetPlatform.iOS,
    }),
  );

  testWidgets(
    'Command-Backspace and Command-Delete delete to the line edge as one edit',
    (tester) async {
      for (final (source, caret, forward, result) in [
        (words, 16, false, ''),
        (words, 6, true, 'alpha '),
        // The edge's hidden syntax goes with the text it encloses.
        ('- **bold** text', 15, false, '- '),
        // A caret inside a span deletes its content and keeps its syntax.
        ('- **bold** text', 6, false, '- **ld** text'),
      ]) {
        final c = await mount(tester, source, caret);
        await chord(
          tester,
          [LogicalKeyboardKey.metaLeft],
          forward ? LogicalKeyboardKey.delete : LogicalKeyboardKey.backspace,
        );
        expect(c.text, result);
        expect(c.command(const Undo()), isTrue);
        expect(c.text, source);
        expect(c.editor.selection, FlarkSelection.collapsed(caret));
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      }
    },
    variant: TargetPlatformVariant.only(TargetPlatform.macOS),
  );

  testWidgets('Cocoa line deletion selectors delete to the line edge', (
    tester,
  ) async {
    for (final (selector, caret, result) in [
      ('deleteToBeginningOfLine:', 6, 'beta gamma'),
      ('deleteToEndOfLine:', 6, 'alpha '),
    ]) {
      final c = await mount(tester, words, caret);
      final client =
          (tester.testTextInput.log
                      .lastWhere((call) => call.method == 'TextInput.setClient')
                      .arguments
                  as List)
              .first;
      await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
        SystemChannels.textInput.name,
        SystemChannels.textInput.codec.encodeMethodCall(
          MethodCall('TextInputClient.performSelectors', [
            client,
            [selector],
          ]),
        ),
        (_) {},
      );
      expect(c.text, result);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    }
  });
}
