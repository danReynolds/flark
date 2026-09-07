import 'dart:ui' as ui;
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  const styles = [
    ('*', Style.emphasis),
    ('**', Style.strong),
    ('~~', Style.strikethrough),
    ('`', Style.code),
    ('***', Style.strong | Style.emphasis),
  ];
  for (final (marker, bits) in styles) {
    for (final inside in [false, true]) {
      for (final backward in [false, true]) {
        for (final burst in [false, true]) {
          testWidgets(
            'paint $marker empty inside=$inside backward=$backward burst=$burst',
            (tester) async {
              final original = '${marker}t$marker';
              final caret = backward
                  ? (inside ? marker.length + 1 : original.length)
                  : (inside ? marker.length : 0);
              final c = FlarkController(
                FlarkEditor(backend, text: original, caret: caret),
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
              await tester.sendKeyEvent(
                backward
                    ? LogicalKeyboardKey.backspace
                    : LogicalKeyboardKey.delete,
              );
              expect(c.text, '');
              if (!burst) {
                await tester.pump();
                expect(paints, isNotEmpty);
                for (final p in paints) {
                  expect(p.rows, ['']);
                  expect(p.revision, c.editor.revision);
                  expect(p.caretSource, 0);
                }
                paints.clear();
              }
              tester.testTextInput.updateEditingValue(
                const TextEditingValue(
                  text: 'x',
                  selection: TextSelection.collapsed(offset: 1),
                ),
              );
              expect(c.text, inside ? '${marker}x$marker' : 'x');
              await tester.pump();
              expect(paints, isNotEmpty);
              for (final p in paints) {
                expect(p.rows, ['x']);
                expect(p.styles.single, [inside ? bits : 0]);
                final style = p.resolvedStyles.single.single;
                expect(
                  style.fontStyle == FontStyle.italic,
                  inside && bits & Style.emphasis != 0,
                );
                expect(
                  style.fontWeight == FontWeight.w700,
                  inside && bits & Style.strong != 0,
                );
                expect(
                  style.decoration == TextDecoration.lineThrough,
                  inside && bits & Style.strikethrough != 0,
                );
                expect(
                  style.fontFamily == flarkCodeFontFamily,
                  inside && bits & Style.code != 0,
                );
                expect(p.snapshot.source, c.text);
                expect(p.caretSource, c.editor.selection.extent);
                expect(p.revision, c.editor.revision);
              }
              await tester.pumpWidget(const SizedBox());
              c.dispose();
            },
          );
        }
      }
    }
  }
  testWidgets(
    'paragraph split, join, Undo and Redo paint each current generation',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend, text: '**ab**', caret: 3));
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
      final cases = <(FlarkCommand, String, List<String>)>[
        (const Newline(paragraph: true), '**a**\n\n**b**', ['a', '', 'b']),
        (const DeleteBackward(), '**a**\n**b**', ['a\nb']),
        (const DeleteBackward(), '**ab**', ['ab']),
        (const InsertText('x'), '**axb**', ['axb']),
        (const Undo(), '**ab**', ['ab']),
        (const Redo(), '**axb**', ['axb']),
      ];
      for (final (command, source, rows) in cases) {
        paints.clear();
        expect(c.command(command), isTrue);
        expect(c.text, source);
        await tester.pump();
        expect(paints, isNotEmpty);
        for (final p in paints) {
          expect(p.rows, rows);
          expect(p.snapshot.source, source);
          expect(p.revision, c.editor.revision);
        }
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  testWidgets(
    'rejected range leaves exact paint then source mode edits and restores',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(backend, text: '**ab** cd', caret: 3),
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
      c.command(const SetSelection(3, 8));
      await tester.pump();
      final before = c.editor.snapshot;
      expect(c.command(const Paste('x')), isFalse);
      await tester.pump();
      expect(c.editor.snapshot, same(before));
      expect(paints.last.rows, ['ab cd']);
      expect(paints.last.selectionRects, isNotEmpty);
      c.sourceMode(true);
      await tester.pump();
      expect(paints.last.rows, ['**ab** cd']);
      expect(c.command(const Paste('x')), isTrue);
      await tester.pump();
      expect(paints.last.rows, ['**axd']);
      expect(c.command(const Undo()), isTrue);
      c.sourceMode(false);
      await tester.pump();
      expect(paints.last.rows, ['ab cd']);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  testWidgets('task check state changes marker pixels without a symbol font', (
    tester,
  ) async {
    final c = FlarkController(FlarkEditor(backend, text: '- [ ] task'));
    final boundary = GlobalKey();
    await tester.pumpWidget(
      MaterialApp(
        home: RepaintBoundary(
          key: boundary,
          child: ColoredBox(
            color: Colors.white,
            child: FlarkEditorWidget(controller: c, showToolbar: false),
          ),
        ),
      ),
    );
    Future<int> markerInk() async {
      final render =
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final image = await render.toImage();
      final data = (await image.toByteData(
        format: ui.ImageByteFormat.rawRgba,
      ))!.buffer.asUint8List();
      var ink = 0;
      // Only the marker column: the label starts at x=38 and cannot affect
      // this observation. Test fonts intentionally lack checkbox symbols.
      for (var y = 16; y < 44; y++) {
        for (var x = 16; x < 36; x++) {
          if (data[(y * image.width + x) * 4] < 160) ink++;
        }
      }
      image.dispose();
      return ink;
    }

    final unchecked = (await tester.runAsync(markerInk))!;
    expect(unchecked, greaterThan(20));
    expect(c.command(const ToggleTask()), isTrue);
    await tester.pump();
    final checked = (await tester.runAsync(markerInk))!;
    expect(checked, greaterThan(unchecked + 5));
    expect(c.text, '- [x] task');
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
  testWidgets(
    'glyph paint has nonempty raster and style changes actual pixels',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend, text: 'Typography'));
      final boundary = GlobalKey();
      await tester.pumpWidget(
        MaterialApp(
          home: RepaintBoundary(
            key: boundary,
            child: ColoredBox(
              color: Colors.white,
              child: FlarkEditorWidget(controller: c, showToolbar: false),
            ),
          ),
        ),
      );
      Future<List<int>> pixels() async {
        final render =
            boundary.currentContext!.findRenderObject()!
                as RenderRepaintBoundary;
        final image = await render.toImage();
        final bytes = (await image.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        ))!.buffer.asUint8List();
        final result = List<int>.of(bytes);
        image.dispose();
        return result;
      }

      final before = (await tester.runAsync(pixels))!;
      expect(before.where((b) => b != 255).length, greaterThan(100));
      c.command(const SetSelection(0, 10));
      c.command(const ToggleStyle(Style.emphasis));
      c.command(const SetSelection(2, 2));
      await tester.pump();
      final after = (await tester.runAsync(pixels))!;
      expect(
        after,
        isNot(equals(before)),
        reason: 'italic must affect the raster, not just metadata',
      );
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  testWidgets(
    'table navigation, exit, next input and heading lift paint current blocks',
    (tester) async {
      const table = '| a | b |\n| - | - |\n| c | **d** |';
      final c = FlarkController(FlarkEditor(backend, text: table));
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
      c.command(SetSelection.caret(c.editor.projection.rows.first.sourceStart));
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      expect(
        c.editor.document.rowAt(c.editor.selection.extent).text.trim(),
        'b',
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(
        c.editor.document.rowAt(c.editor.selection.extent).text.trim(),
        'd',
      );
      await tester.pump();
      expect(paints.last.rows.map((r) => r.trim()), ['a', 'b', 'c', 'd']);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(c.command(const InsertText('x')), isTrue);
      await tester.pump();
      expect(paints.last.rows.map((r) => r.trim()), [
        'a',
        'b',
        'c',
        'd',
        '',
        'x',
      ]);
      expect(c.text, '$table\n\nx');
      await tester.pumpWidget(const SizedBox());
      c.dispose();

      final heading = FlarkController(
        FlarkEditor(backend, text: '**title**\n==='),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: heading,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      final headingSize = paints.last.resolvedStyles.single.single.fontSize!;
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();
      expect(heading.text, '**title**');
      expect(paints.last.rows, ['title']);
      expect(
        paints.last.resolvedStyles.single.single.fontSize,
        lessThan(headingSize),
      );
      expect(
        paints.last.resolvedStyles.single.single.fontWeight,
        FontWeight.w700,
      );
      await tester.pumpWidget(const SizedBox());
      heading.dispose();
    },
  );
  testWidgets(
    'source delimiter completion and subsequent whitespace paint atomically',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend, text: '*t', caret: 2));
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
      expect(paints.last.rows, ['*t']);
      paints.clear();
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: '*t*',
          selection: TextSelection.collapsed(offset: 3),
        ),
      );
      await tester.pump();
      expect(paints, isNotEmpty);
      for (final p in paints) {
        expect(p.rows, ['t']);
        expect(p.resolvedStyles.single.single.fontStyle, FontStyle.italic);
      }
      c.command(const SetSelection.caret(2));
      c.command(const DeleteBackward());
      c.command(const InsertText(' '));
      await tester.pump();
      expect(paints.last.rows, ['']);
      expect(c.editor.typingContext, 0);
      c.command(const InsertText('x'));
      await tester.pump();
      expect(paints.last.rows, ['x']);
      expect(paints.last.styles.single, [0]);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  testWidgets(
    'partial styles inherit usable metrics and combined formatting reaches paint',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend));
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              style: const TextStyle(color: Colors.red),
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      expect(tester.takeException(), isNull);
      expect(c.command(const ToggleStyle(Style.strong)), isTrue);
      expect(c.command(const ToggleStyle(Style.emphasis)), isTrue);
      expect(c.command(const InsertText('x')), isTrue);
      await tester.pump();
      expect(paints.last.rows, ['x']);
      final style = paints.last.resolvedStyles.single.single;
      expect(style.fontWeight, FontWeight.w700);
      expect(style.fontStyle, FontStyle.italic);
      expect(style.color, Colors.red);
      await tester.tap(find.byTooltip('Undo'));
      await tester.pump();
      expect(c.text, '');
      expect(tester.testTextInput.hasAnyClients, isTrue);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'y',
          selection: TextSelection.collapsed(offset: 1),
        ),
      );
      await tester.pump();
      expect(c.text, '***y***');
      expect(paints.last.rows, ['y']);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
