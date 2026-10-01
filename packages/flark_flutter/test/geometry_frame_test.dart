import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets(
    'admitted nested containers leave text inside a narrow viewport',
    (tester) async {
      final source = '${'> ' * 8}word';
      final c = FlarkController(
        FlarkEditor(backend, text: source, caret: source.length),
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
      expect(c.editor.sourceMode, isFalse);
      expect(paints.last.caret, isNotNull);
      expect(paints.last.caret!.right, lessThan(180));
      expect(paints.last.rows, ['word']);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  testWidgets('width-only reflow keeps the caret in the first resized frame', (
    tester,
  ) async {
    final source = 'word ' * 100;
    final c = FlarkController(
      FlarkEditor(backend, text: source, caret: source.length),
    );
    final paints = <FlarkPaintObservation>[];
    Widget app(double width) => MaterialApp(
      home: Scaffold(
        body: SizedBox(
          width: width,
          height: 300,
          child: FlarkEditorWidget(
            controller: c,
            autofocus: true,
            showToolbar: false,
            onPaint: paints.add,
          ),
        ),
      ),
    );
    await tester.pumpWidget(app(600));
    await tester.pump();
    final selection = c.editor.selection;
    for (final width in [180.0, 600.0]) {
      paints.clear();
      await tester.pumpWidget(app(width));
      expect(paints, isNotEmpty);
      expect(paints.first.caret, isNotNull);
      expect(c.text, source);
      expect(c.editor.selection, selection);
    }
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets('source edits and resizing replace cached live presentation', (
    tester,
  ) async {
    const original = '## Heading\n\n**word** and *another*';
    const edited = '> Heading\n\n*word* and **another**';
    final c = FlarkController(
      FlarkEditor(backend, text: original, caret: original.length),
    );
    final paints = <FlarkPaintObservation>[];
    Widget app(double width) => MaterialApp(
      home: Scaffold(
        body: SizedBox(
          width: width,
          child: FlarkEditorWidget(
            controller: c,
            autofocus: true,
            showToolbar: false,
            onPaint: paints.add,
          ),
        ),
      ),
    );
    await tester.pumpWidget(app(600));
    await tester.pump();
    c.sourceMode(true);
    await tester.pump();
    expect(c.command(const ReplaceRange(0, original.length, edited)), isTrue);
    await tester.pumpWidget(app(240));
    c.sourceMode(false);
    paints.clear();
    await tester.pump();
    for (final width in [240.0, 600.0]) {
      if (width == 600) {
        paints.clear();
        await tester.pumpWidget(app(width));
      }
      final paint = paints.first;
      expect(paint.snapshot.source, edited);
      expect(paint.rows, ['Heading', '', 'word and another']);
      expect(paint.resolvedStyles.first.single.fontSize, 16);
      expect(paint.resolvedStyles.last.first.fontStyle, FontStyle.italic);
      expect(paint.resolvedStyles.last.last.fontWeight, FontWeight.w700);
      expect(paint.caretSource, edited.length);
      expect(paint.caret, isNotNull);
    }
    paints.clear();
    expect(c.command(const InsertText('!')), isTrue);
    await tester.pump();
    expect(paints.first.rows.last, 'word and another!');
    expect(paints.first.caretSource, c.editor.selection.extent);
    expect(c.text, '> Heading\n\n*word* and **another**!');
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets(
    'wrapped vertical input uses each preceding unpainted selection',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(backend, text: 'word ' * 100, caret: 0),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 240,
              child: FlarkEditorWidget(controller: c, autofocus: true),
            ),
          ),
        ),
      );
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      final first = c.editor.selection.extent;
      expect(first, greaterThan(0));
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      expect(c.editor.selection.extent, greaterThan(first));
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
      expect(c.editor.selection.extent, first);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets('line edge after an unpainted insertion uses current layout', (
    tester,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: 'a', caret: 1));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 240,
            child: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      ),
    );
    await tester.pump();
    expect(c.command(InsertText(' word' * 20)), isTrue);
    await tester.sendKeyEvent(LogicalKeyboardKey.home);
    expect(c.editor.selection.extent, greaterThan(1));
    expect(c.editor.selection.extent, lessThan(c.text.length));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets('first edited paint reveals caret and scrolling can hide it', (
    tester,
  ) async {
    final source =
        '${List.generate(30, (i) => 'Paragraph $i').join('\n\n')}\n\nLast paragraph';
    final c = FlarkController(FlarkEditor(backend, text: source, caret: 0));
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 320,
            height: 240,
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
    paints.clear();
    c.command(SetSelection.caret(source.length));
    await tester.pump();
    expect(paints, isNotEmpty);
    expect(paints.first.rows.last, 'Last paragraph');
    expect(paints.first.caret, isNotNull);
    paints.clear();
    expect(c.command(InsertText(' word' * 100)), isTrue);
    await tester.pump();
    expect(paints.first.caret, isNotNull);
    final position = tester
        .state<ScrollableState>(find.byType(Scrollable))
        .position;
    expect(
      paints.first.caret!.bottom,
      lessThanOrEqualTo(position.pixels + position.viewportDimension),
    );
    position.jumpTo(0);
    paints.clear();
    await tester.pump();
    expect(position.pixels, 0);
    expect(
      paints.last.caret,
      isNull,
      reason: 'Only a drawn caret is observable',
    );
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets(
    'End and a hit past a soft-wrapped line keep the caret on that line',
    (tester) async {
      final source = 'ordinary words ' * 12;
      final c = FlarkController(FlarkEditor(backend, text: source, caret: 0));
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 240,
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
      final first = paints.last.caret!;
      // The first visual line ends where the second begins.
      const wrap = 'ordinary '.length;
      c.command(const SetSelection.caret(wrap));
      await tester.pump();
      final second = paints.last.caret!;
      expect(second.top, greaterThan(first.top));
      c.command(const SetSelection.caret(0));
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.end);
      await tester.pump();
      expect(c.editor.selection.extent, wrap);
      expect(paints.last.caret!.top, first.top);
      expect(paints.last.caret!.left, greaterThan(second.left));
      // End again stays, Home returns to the start of the same line.
      await tester.sendKeyEvent(LogicalKeyboardKey.end);
      expect(c.editor.selection.extent, wrap);
      await tester.sendKeyEvent(LogicalKeyboardKey.home);
      expect(c.editor.selection.extent, 0);
      // Down from the first line's end reaches the second line's end.
      await tester.sendKeyEvent(LogicalKeyboardKey.end);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      expect(c.editor.selection.extent, 'ordinary words '.length);
      expect(paints.last.caret!.top, second.top);
      // A click past the first line's end puts the caret there as well.
      final origin = tester.getTopLeft(find.byType(FlarkSurface));
      await tester.tapAt(
        origin + Offset(236, first.center.dy),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pump(kDoubleTapTimeout);
      expect(c.editor.selection.extent, wrap);
      expect(paints.last.caret!.top, first.top);
      // Affinity changes only where the caret is drawn, not where text goes.
      tester.testTextInput.updateEditingValue(
        TextEditingValue(
          text: source.replaceRange(wrap, wrap, 'x'),
          selection: const TextSelection.collapsed(offset: wrap + 1),
        ),
      );
      expect(c.text, source.replaceRange(wrap, wrap, 'x'));
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
