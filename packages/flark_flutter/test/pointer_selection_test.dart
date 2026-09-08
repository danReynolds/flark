import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets('click back at bold word end resumes its formatting', (
    tester,
  ) async {
    const source = 'say **what** next';
    final c = FlarkController(FlarkEditor(backend, text: source, caret: 10));
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
    final surface = tester.renderObject<RenderFlarkSurface>(
      find.byType(FlarkSurface),
    );
    final edge = surface.caretRect;
    expect(c.command(const ToggleStyle(Style.strong)), isTrue);
    expect(c.editor.typingContext, 0);
    await tester.pump();
    await tester.tapAt(
      surface.localToGlobal(Offset(edge.left + .75, edge.center.dy)),
      kind: PointerDeviceKind.mouse,
    );
    expect(c.editor.selection.extent, 10);
    expect(c.editor.typingContext, Style.strong);
    paints.clear();
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'say **whatx** next',
        selection: TextSelection.collapsed(offset: 11),
      ),
    );
    await tester.pump();
    expect(c.text, 'say **whatx** next');
    expect(paints, isNotEmpty);
    for (final paint in paints) {
      expect(paint.rows, ['say whatx next']);
      expect(paint.styles.single, [0, Style.strong, 0]);
      expect(paint.caretSource, 11);
      expect(paint.revision, c.editor.revision);
    }
    await tester.pumpWidget(const SizedBox());
    await tester.pump(kDoubleTapMinTime);
    c.dispose();
  });
  for (final (open, style) in [
    ('**', Style.strong),
    ('*', Style.emphasis),
    ('***', Style.strong | Style.emphasis),
  ]) {
    for (final suffix in ['', ' next']) {
      for (final start in [false, true]) {
        for (final dx in [-.75, .75]) {
          testWidgets(
            'pointer $dx at ${start ? 'start' : 'end'} of $open word / $suffix',
            (tester) async {
              final source = 'say ${open}what$open$suffix';
              final at = 4 + open.length + (start ? 0 : 4);
              final c = FlarkController(
                FlarkEditor(backend, text: source, caret: at),
              );
              final paints = <FlarkPaintObservation>[];
              await tester.pumpWidget(
                MaterialApp(
                  home: Scaffold(
                    body: SizedBox(
                      width: 180,
                      child: FlarkEditorWidget(
                        controller: c,
                        autofocus: true,
                        onPaint: paints.add,
                      ),
                    ),
                  ),
                ),
              );
              await tester.pump();
              final surface = tester.renderObject<RenderFlarkSurface>(
                find.byType(FlarkSurface),
              );
              final edge = surface.caretRect;
              c.command(const SetSelection.caret(0));
              expect(c.editor.typingContext, 0);
              await tester.pump();
              await tester.tapAt(
                surface.localToGlobal(Offset(edge.left + dx, edge.center.dy)),
                kind: PointerDeviceKind.mouse,
              );
              expect(c.editor.selection.extent, at);
              expect(c.editor.typingContext, style);
              final expected = source.replaceRange(at, at, 'x');
              paints.clear();
              tester.testTextInput.updateEditingValue(
                TextEditingValue(
                  text: expected,
                  selection: TextSelection.collapsed(offset: at + 1),
                ),
              );
              await tester.pump();
              expect(c.text, expected);
              expect(paints, isNotEmpty);
              for (final p in paints) {
                expect(p.rows, ['say ${start ? 'xwhat' : 'whatx'}$suffix']);
                expect(p.styles.single, [0, style, if (suffix.isNotEmpty) 0]);
                expect(p.caretSource, at + 1);
                expect(identical(p.snapshot, c.editor.snapshot), isTrue);
              }
              if (suffix.isNotEmpty) {
                // Clicking after the separator is a distinct visible caret and
                // must leave the previous word's formatting behind.
                final plainAt = 4 + open.length * 2 + 5 + 1;
                c.command(SetSelection.caret(plainAt));
                await tester.pump();
                final plain = surface.caretRect;
                c.command(SetSelection.caret(at));
                await tester.pump();
                await tester.pump(kDoubleTapTimeout);
                await tester.tapAt(
                  surface.localToGlobal(
                    Offset(plain.left - .75, plain.center.dy),
                  ),
                  kind: PointerDeviceKind.mouse,
                );
                expect(c.editor.selection.extent, plainAt);
                expect(c.editor.typingContext, 0);
                paints.clear();
                tester.testTextInput.updateEditingValue(
                  TextEditingValue(
                    text: expected.replaceRange(plainAt, plainAt, 'z'),
                    selection: TextSelection.collapsed(offset: plainAt + 1),
                  ),
                );
                await tester.pump();
                expect(c.text, expected.replaceRange(plainAt, plainAt, 'z'));
                expect(paints, isNotEmpty);
                expect(paints.first.styles.single.last, 0);
              }
              await tester.pumpWidget(const SizedBox());
              await tester.pump(kDoubleTapMinTime);
              c.dispose();
            },
          );
        }
      }
    }
  }
  for (final source in [
    'A simple **bold** word.',
    'A simple [bold](https://example.com) word.',
    'A simple `bold` word.',
  ]) {
    testWidgets('double click selects the visible word: $source', (
      tester,
    ) async {
      final start = source.indexOf('bold');
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: start + 2),
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
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkSurface),
      );
      final point = surface.localToGlobal(surface.caretRect.center);
      await tester.tapAt(point, kind: PointerDeviceKind.mouse);
      await tester.pump(kDoubleTapMinTime);
      paints.clear();
      await tester.tapAt(point, kind: PointerDeviceKind.mouse);
      await tester.pump();
      expect(c.editor.selection, FlarkSelection(start, start + 4));
      expect(paints, isNotEmpty);
      expect(paints.last.selectionRects, isNotEmpty);
      paints.clear();
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text: source.replaceRange(start, start + 4, 'new'),
          selection: TextSelection.collapsed(offset: start + 3),
        ),
      );
      await tester.pump();
      expect(c.text, source.replaceRange(start, start + 4, 'new'));
      expect(paints, isNotEmpty);
      for (final paint in paints) {
        expect(paint.rows, ['A simple new word.']);
        expect(paint.caretSource, start + 3);
        expect(paint.revision, c.editor.revision);
      }
      c.command(const Undo());
      await tester.pump();
      expect(c.text, source);
      expect(c.editor.selection, FlarkSelection(start, start + 4));
      await tester.pumpWidget(const SizedBox());
      // The engine's double-tap minimum-interval timer outlives its tracker.
      // Flush it only after the proving paints and unmount assertions.
      await tester.pump(kDoubleTapMinTime);
      c.dispose();
    });
  }
}
